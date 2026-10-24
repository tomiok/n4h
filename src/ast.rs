use crate::budget::Budget;
use crate::error::{Error, Result};
use crate::model::{self, AstCapture, AstMatch, Match, SearchResult};
use crate::{search, store::Store};
use ast_grep_core::AstGrep;
use ast_grep_core::matcher::Pattern;
use ast_grep_core::meta_var::MetaVariable;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_language::{Language, Rust};
use clap::Subcommand;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum AstCommand {
    /// Find Rust AST patterns ($NAME / $$$ARGS); syntax matching, not name resolution.
    Find {
        pattern: String,
        paths: Vec<PathBuf>,
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=500))]
        page_size: u16,
    },
    /// Prepare a capture-based rewrite; apply/undo/recover use the normal commands.
    Edit {
        #[arg(long)]
        on: String,
        #[arg(long, action = clap::ArgAction::Append, conflicts_with = "scope")]
        match_id: Vec<String>,
        #[arg(long, value_enum)]
        scope: Option<crate::Scope>,
        #[arg(long, required_unless_present_any = ["replacement_file", "replacement_stdin"], conflicts_with_all = ["replacement_file", "replacement_stdin"])]
        replace: Option<String>,
        #[arg(long, conflicts_with = "replacement_stdin")]
        replacement_file: Option<PathBuf>,
        #[arg(long)]
        replacement_stdin: bool,
    },
}

pub fn parse_source(text: &str) -> Result<AstGrep<StrDoc<Rust>>> {
    let doc = StrDoc::try_new(text, Rust).map_err(|e| Error::new("ast_parse_error", e, 3))?;
    if doc.tree.root_node().has_error() {
        return Err(Error::new(
            "ast_parse_error",
            "Rust tree-sitter parser reported error or missing syntax nodes.",
            3,
        ));
    }
    Ok(AstGrep::doc(doc))
}

pub fn find(
    store: &Store,
    cwd: &Path,
    pattern: &str,
    paths: &[PathBuf],
    budget: &Budget,
) -> Result<SearchResult> {
    if pattern.trim().is_empty() || pattern.len() > 16 * 1024 || pattern.contains('\0') {
        return Err(Error::invalid(
            "AST pattern must be nonempty text without NUL, at most 16 KiB.",
        ));
    }
    let matcher = Pattern::try_new(pattern, Rust)
        .map_err(|e| Error::invalid(format!("Invalid Rust AST pattern: {e}")))?;
    if matcher.has_error() {
        return Err(Error::invalid("Rust AST pattern contains syntax errors."));
    }
    let processed = Rust.pre_process_pattern(pattern);
    let standalone = StrDoc::try_new(&processed, Rust)
        .map_err(|e| Error::invalid(format!("Cannot parse AST pattern: {e}")))?;
    if standalone.tree.root_node().has_error()
        && parse_source(&format!("fn __n4h_pattern() {{\n{processed}\n}}")).is_err()
        && parse_source(&format!("fn __n4h_pattern() {{\n{processed};\n}}")).is_err()
    {
        return Err(Error::invalid(
            "AST pattern must be a syntactically valid Rust item, statement or expression.",
        ));
    }
    if matcher.defined_vars().len() > 32 {
        return Err(Error::invalid(
            "An AST pattern can capture at most 32 variable names.",
        ));
    }
    search::scan(
        store,
        cwd,
        pattern,
        "ast_rust",
        paths,
        budget,
        |result, path, hash, text| {
            let root = match parse_source(text) {
                Ok(root) => root,
                Err(_) => {
                    result.coverage_complete = false;
                    search::skip(result, path.to_owned(), "rust_parse_error".to_owned());
                    return Ok(());
                }
            };
            let node = root.root();
            for found in node.find_all(&matcher) {
                let range = found.range();
                let mut captures = BTreeMap::new();
                let env = found.get_env();
                for variable in env.get_matched_variables() {
                    let (name, range, multiple) = match variable {
                        MetaVariable::Capture(name, _) => {
                            let captured = env.get_match(&name).ok_or_else(|| {
                                Error::new("invalid_state", "Missing captured AST node.", 4)
                            })?;
                            (name, captured.range(), false)
                        }
                        MetaVariable::MultiCapture(name) => {
                            let nodes = env.get_multiple_matches(&name);
                            let range =
                                if let (Some(first), Some(last)) = (nodes.first(), nodes.last()) {
                                    first.range().start..last.range().end
                                } else {
                                    range.start..range.start
                                };
                            (name, range, true)
                        }
                        _ => continue,
                    };
                    captures.insert(
                        name,
                        AstCapture {
                            start_byte: range.start,
                            end_byte: range.end,
                            multiple,
                        },
                    );
                }
                let (preview, truncated) = search::snippet(&text[range.clone()], 160);
                let position = found.start_pos();
                search::push_match(
                    result,
                    Match {
                        match_id: String::new(),
                        path: path.to_owned(),
                        file_hash: hash.to_owned(),
                        start_byte: range.start,
                        end_byte: range.end,
                        line: position.line() + 1,
                        column_byte: position.byte_point().1 + 1,
                        text: preview,
                        text_truncated: truncated,
                        ast: Some(AstMatch {
                            node_kind: found.kind().to_string(),
                            captures,
                        }),
                    },
                )?;
            }
            Ok(())
        },
    )
}

enum Part {
    Text(Vec<u8>),
    Capture { name: String, multiple: bool },
}

pub struct Template {
    parts: Vec<Part>,
}

impl Template {
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > 64 * 1024 {
            return Err(Error::new(
                "ast_limit_exceeded",
                "AST templates are limited to 64 KiB before expansion.",
                5,
            ));
        }
        let bytes = text.as_bytes();
        let mut parts = Vec::new();
        let mut literal = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != b'$' {
                literal.push(bytes[index]);
                index += 1;
                continue;
            }
            let multiple = bytes[index..].starts_with(b"$$$");
            let start = index + if multiple { 3 } else { 1 };
            if !multiple && bytes[index..].starts_with(b"$$") {
                literal.push(b'$');
                index += 2;
                continue;
            }
            if start >= bytes.len() || !bytes[start].is_ascii_uppercase() {
                return Err(Error::invalid(
                    "Template variables must be $NAME or $$$NAME (uppercase); use $$ for a literal dollar sign.",
                ));
            }
            let mut end = start + 1;
            while end < bytes.len()
                && (bytes[end].is_ascii_uppercase()
                    || bytes[end].is_ascii_digit()
                    || bytes[end] == b'_')
            {
                end += 1;
            }
            if !literal.is_empty() {
                parts.push(Part::Text(std::mem::take(&mut literal)));
            }
            parts.push(Part::Capture {
                name: text[start..end].to_owned(),
                multiple,
            });
            index = end;
        }
        if !literal.is_empty() {
            parts.push(Part::Text(literal));
        }
        Ok(Self { parts })
    }

    pub fn render(&self, found: &Match, source: &[u8], budget: usize) -> Result<Vec<u8>> {
        let ast = found
            .ast
            .as_ref()
            .ok_or_else(|| Error::new("invalid_state", "Missing persisted AST captures.", 4))?;
        let mut output = Vec::new();
        for part in &self.parts {
            let bytes = match part {
                Part::Text(text) => text.as_slice(),
                Part::Capture { name, multiple } => {
                    let capture = ast.captures.get(name).ok_or_else(|| {
                        Error::new(
                            "unknown_capture",
                            format!("Template references an uncaptured variable: {name}."),
                            2,
                        )
                    })?;
                    if capture.multiple != *multiple {
                        return Err(Error::invalid(format!(
                            "Capture {name} must use the same single/multiple form as its pattern."
                        )));
                    }
                    if capture.start_byte < found.start_byte
                        || capture.end_byte > found.end_byte
                        || capture.start_byte > capture.end_byte
                    {
                        return Err(Error::new(
                            "invalid_state",
                            "Persisted AST capture is outside its match.",
                            4,
                        ));
                    }
                    source
                        .get(capture.start_byte..capture.end_byte)
                        .ok_or_else(|| {
                            Error::new("invalid_state", "Invalid persisted AST capture range.", 4)
                        })?
                }
            };
            if output.len().saturating_add(bytes.len()) > budget.min(model::MAX_PLAN_BYTES) {
                return Err(Error::new(
                    "plan_limit_exceeded",
                    "Expanded AST replacement exceeds the plan byte limit.",
                    5,
                ));
            }
            output.extend_from_slice(bytes);
        }
        Ok(output)
    }
}
