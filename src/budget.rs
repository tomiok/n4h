use crate::error::{Error, Result};
use crate::model::MAX_OUTPUT_BYTES;
use clap::ValueEnum;
use serde_json::{Value, json};
use tiktoken_rs::CoreBPE;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Tokenizer {
    #[value(name = "o200k_base")]
    O200kBase,
    #[value(name = "cl100k_base")]
    Cl100kBase,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_self_inclusive_and_finishing_is_idempotent() {
        for tokenizer in [Tokenizer::O200kBase, Tokenizer::Cl100kBase] {
            let budget = Budget::new(Some(10000), Some(tokenizer)).unwrap();
            for length in [0, 1, 30, 100, 300] {
                let value =
                    json!({"ok": true, "text": "日本語🙂\"\\\n<|endoftext|>".repeat(length)});
                let value = budget.finish(value).unwrap();
                let serialized = format!("{}\n", serde_json::to_string(&value).unwrap());
                assert_eq!(
                    value["budget"]["output_tokens"],
                    tokenizer.encoder().encode_ordinary(&serialized).len()
                );
                assert_eq!(budget.finish(value.clone()).unwrap(), value);
            }
        }
    }

    #[test]
    fn corrupted_oversized_recovery_ids_cannot_bypass_the_emergency_byte_or_token_bound() {
        let error = Error::new("invalid_state", "Corrupted local state.", 4)
            .operation(&"x".repeat(100_000));
        for tokenizer in [Tokenizer::O200kBase, Tokenizer::Cl100kBase] {
            let budget = Budget::new(Some(256), Some(tokenizer)).unwrap();
            let value = budget.error(&error);
            let serialized = format!("{}\n", serde_json::to_string(&value).unwrap());
            assert!(serialized.len() <= MAX_OUTPUT_BYTES);
            assert!(tokenizer.encoder().encode_ordinary(&serialized).len() <= 256);
            assert_eq!(value["error"]["invalid_identifier_omitted"], true);
        }
    }
}

impl Tokenizer {
    pub fn name(self) -> &'static str {
        match self {
            Self::O200kBase => "o200k_base",
            Self::Cl100kBase => "cl100k_base",
        }
    }

    fn encoder(self) -> &'static CoreBPE {
        match self {
            Self::O200kBase => tiktoken_rs::o200k_base_singleton(),
            Self::Cl100kBase => tiktoken_rs::cl100k_base_singleton(),
        }
    }
}

#[derive(Default)]
pub struct Budget {
    tokens: Option<(usize, Tokenizer)>,
}

impl Budget {
    pub fn new(max_tokens: Option<u32>, tokenizer: Option<Tokenizer>) -> Result<Self> {
        match (max_tokens, tokenizer) {
            (None, None) => Ok(Self::default()),
            (Some(limit), Some(tokenizer)) if (256..=1_000_000).contains(&limit) => Ok(Self {
                tokens: Some((limit as usize, tokenizer)),
            }),
            _ => Err(Error::invalid(
                "Use --max-tokens (256-1000000) together with an explicit --tokenizer.",
            )),
        }
    }

    pub fn active(&self) -> bool {
        self.tokens.is_some()
    }

    pub fn too_small(&self) -> Error {
        Error::new("output_budget_too_small", "The minimum complete response unit does not fit the output budget. Nothing was cut mid-JSON or mid-UTF-8.", 5)
            .action("Increase --max-tokens, request less context, or omit token flags. A failed preview never authorizes apply.")
    }

    /// Includes metadata, JSON escaping and the final newline. No provider/model
    /// inference: tokens mean only the explicitly selected ordinary encoding.
    pub fn fit(&self, mut value: Value) -> Result<Option<Value>> {
        let Some((limit, tokenizer)) = self.tokens else {
            return Ok((serde_json::to_vec(&value)?.len() < MAX_OUTPUT_BYTES).then_some(value));
        };
        value["budget"] = json!({"max_tokens": limit, "tokenizer": tokenizer.name(),
            "max_bytes": MAX_OUTPUT_BYTES, "output_tokens": 0,
            "count_scope": "serialized_json_plus_newline", "special_tokens": "ordinary_text"});
        // The count itself is part of the response. Recount until the field is
        // a fixed point; verify that exact serialization again at final emission.
        for _ in 0..16 {
            let mut serialized = serde_json::to_string(&value)?;
            serialized.push('\n');
            if serialized.len() > MAX_OUTPUT_BYTES {
                return Ok(None);
            }
            let count = tokenizer.encoder().encode_ordinary(&serialized).len();
            if value["budget"]["output_tokens"].as_u64() == Some(count as u64) {
                return Ok((count <= limit).then_some(value));
            }
            value["budget"]["output_tokens"] = json!(count);
        }
        Err(Error::new(
            "token_count_unstable",
            "Could not stabilize the self-inclusive token count; no bounded response was emitted.",
            4,
        ))
    }

    pub fn finish(&self, value: Value) -> Result<Value> {
        self.fit(value)?.ok_or_else(|| self.too_small())
    }

    /// Keep the original error code and recovery ID even when diagnostics need
    /// compacting. Invalid CLI flags are handled before any Budget is accepted.
    pub fn error(&self, error: &Error) -> Value {
        if let Ok(Some(value)) = self.fit(error.json()) {
            return value;
        }
        let compact = json!({"schema_version": crate::SCHEMA_VERSION, "ok": false,
            "error": {"code": error.code, "operation_id": error.operation_id,
                "message": "Diagnostics shortened to honor the output budget.",
                "retryable": false, "paths": [], "paths_total": error.paths.len(),
                "paths_truncated": !error.paths.is_empty(), "details_truncated": true,
                "next_actions": if error.operation_id.is_some() {
                    vec!["Inspect operation_id with recover before further source edits; increase output budget for diagnostics."]
                } else { vec!["Increase output budget for full diagnostics."] }}});
        if let Ok(Some(value)) = self.fit(compact) {
            return value;
        }
        let minimal = json!({"schema_version": crate::SCHEMA_VERSION, "ok": false,
            "error": {"code": error.code, "operation_id": error.operation_id, "details_truncated": true}});
        // This envelope fits both encodings at the minimum accepted budget.
        // If counting itself fails, preserve a valid bounded diagnostic instead
        // of panicking or losing the operation ID after a filesystem failure.
        self.finish(minimal).unwrap_or_else(|_| {
            // Never fall back to an unbounded value, even if local state was
            // corrupted to contain an enormous/nonstandard operation ID.
            let recovery_id = error.operation_id.as_deref().filter(|id| {
                id.strip_prefix("op_").is_some_and(|suffix| {
                    suffix.len() == 32 && suffix.bytes().all(|b| b.is_ascii_hexdigit())
                })
            });
            json!({"schema_version": crate::SCHEMA_VERSION, "ok": false,
                "error": {"code": "token_count_unstable", "operation_id": recovery_id,
                    "details_truncated": true, "invalid_identifier_omitted": error.operation_id.is_some() && recovery_id.is_none()}})
        })
    }
}
