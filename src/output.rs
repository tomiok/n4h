use crate::budget::Budget;
use crate::error::{Error, Result};
use crate::files;
use crate::model::{self, PlanData, SearchResult};
use crate::search::snippet;
use crate::store::Store;
use serde_json::{Value, json};

pub fn success(kind: &str) -> Value {
    json!({"schema_version": crate::SCHEMA_VERSION, "ok": true, "kind": kind})
}

pub(crate) fn cursor(id: &str, offset: usize) -> String {
    format!("c_{id}_{offset:x}")
}

pub(crate) fn offset(id: &str, token: Option<&str>, max: usize) -> Result<usize> {
    let Some(token) = token else {
        return Ok(0);
    };
    let prefix = format!("c_{id}_");
    let parsed = token
        .strip_prefix(&prefix)
        .and_then(|s| usize::from_str_radix(s, 16).ok());
    match parsed {
        Some(value) if value > 0 && value < max => Ok(value),
        _ => Err(Error::new(
            "invalid_cursor",
            "Cursor does not identify a remaining page of this snapshot.",
            3,
        )),
    }
}

pub fn result_page(
    store: &Store,
    result: &SearchResult,
    token: Option<&str>,
    size: usize,
    budget: &Budget,
) -> Result<Value> {
    if !(1..=500).contains(&size) {
        return Err(Error::invalid("page-size must be between 1 and 500."));
    }
    let start = offset(&result.id, token, result.matches.len())?;
    let mut end = (start + size).min(result.matches.len());
    let mut skipped = skipped_summary(&result.skipped);
    loop {
        let mut value = success("search_result");
        value["repository_id"] = json!(store.repository_id);
        value["result_id"] = json!(result.id);
        value["mode"] = json!(result.mode);
        let (pattern, pattern_truncated) = snippet(&result.pattern, 1024);
        value["pattern"] = json!(pattern);
        value["pattern_truncated"] = json!(pattern_truncated);
        value["expires_at"] = json!(result.created_at + model::RESULT_TTL);
        value["scan_complete"] = json!(true);
        value["coverage_complete"] = json!(result.coverage_complete);
        value["scope"] = json!("persisted_result");
        if let Some(index) = &result.index {
            value["scope"] = json!("indexed_snapshot");
            value["index"] = json!(index);
            value["index_generation"] = json!(index.generation);
            value["indexed_at"] = json!(index.indexed_at);
            value["snapshot_warning"] = json!(
                "This result searches cached content, not the current working tree. Run index update (or update --verify) to refresh. Edits verify selected live file hashes."
            );
            value["skipped_scope"] = json!("whole_index_build");
        }
        value["policy"] = json!({"hidden": false, "follow_symlinks": false, "max_file_bytes": model::MAX_FILE_BYTES, "encoding": "utf-8", "line_based": true});
        if result.mode == "ast_rust" {
            value["policy"]["line_based"] = json!(false);
            value["policy"]["language"] = json!("rust");
            value["policy"]["engine"] = json!("ast-grep/tree-sitter");
            value["policy"]["file_extension"] = json!("rs");
            value["policy"]["symbol_resolution"] = json!(false);
            value["policy"]["parse_errors_block_edit"] = json!(true);
        }
        value["files_scanned"] = json!(result.files_scanned);
        value["bytes_scanned"] = json!(result.bytes_scanned);
        value["total_matches"] = json!(result.matches.len());
        value["shown_matches"] = json!(end - start);
        value["truncated"] = json!(end < result.matches.len());
        value["cursor_next"] = if end < result.matches.len() {
            json!(cursor(&result.id, end))
        } else {
            Value::Null
        };
        value["skipped_total"] = json!(result.skipped_total);
        value["skipped"] = skipped.clone();
        value["skipped_details_truncated"] =
            json!(result.skipped_total > skipped.as_array().map_or(0, Vec::len));
        value["matches"] = json!(&result.matches[start..end]);
        if let Some(value) = budget.fit(value)? {
            return Ok(value);
        }
        if let Some(skipped) = skipped.as_array_mut()
            && !skipped.is_empty()
        {
            skipped.truncate(skipped.len() / 2);
            continue;
        }
        if end <= start + 1 {
            return Err(budget.too_small());
        }
        end = start + (end - start) / 2;
    }
}

pub(crate) fn skipped_summary(skipped: &[model::Skipped]) -> Value {
    json!(skipped.iter().take(10).map(|s| {
        let (path, path_truncated) = snippet(&s.path, 512);
        let (reason, reason_truncated) = snippet(&s.reason, 240);
        json!({"path": path, "reason": reason, "path_truncated": path_truncated, "reason_truncated": reason_truncated})
    }).collect::<Vec<_>>())
}

pub fn context(
    store: &Store,
    result: &SearchResult,
    match_id: &str,
    radius: usize,
    budget: &Budget,
) -> Result<Value> {
    if radius > 20 {
        return Err(Error::invalid("context must be at most 20 lines."));
    }
    let found = result
        .matches
        .iter()
        .find(|m| m.match_id == match_id)
        .ok_or_else(|| Error::invalid("Unknown match ID for this result."))?;
    let snapshot = files::verify(&store.root, &found.path, &found.file_hash, None)?;
    let text = std::str::from_utf8(&snapshot.bytes)
        .map_err(|_| Error::new("unsupported_file", "Invalid UTF-8.", 5))?;
    let first = found.line.saturating_sub(radius + 1);
    let count = found.line.saturating_add(radius).saturating_sub(first);
    let mut lines: Vec<Value> = text
        .lines()
        .enumerate()
        .skip(first)
        .take(count)
        .map(|(index, line)| {
            let (text, truncated) = snippet(line, 240);
            json!({"line": index + 1, "text": text, "text_truncated": truncated})
        })
        .collect();
    let requested_lines = lines.len();
    loop {
        let mut value = success("match_context");
        value["result_id"] = json!(result.id);
        value["match"] = json!(found);
        value["context"] = json!(lines);
        value["context_truncated"] = json!(lines.len() < requested_lines);
        value["requested_context_radius"] = json!(radius);
        if let Some(value) = budget.fit(value)? {
            return Ok(value);
        }
        if lines.len() <= 1 {
            return Err(budget.too_small());
        }
        let first_distance = lines[0]["line"]
            .as_u64()
            .unwrap_or(0)
            .abs_diff(found.line as u64);
        let last_distance = lines[lines.len() - 1]["line"]
            .as_u64()
            .unwrap_or(0)
            .abs_diff(found.line as u64);
        if first_distance > last_distance {
            lines.remove(0);
        } else {
            lines.pop();
        }
    }
}

pub fn plan_page(data: &PlanData, token: Option<&str>, budget: &Budget) -> Result<Value> {
    let files_id = format!("{}_files", data.plan.id);
    let files_mode = token.is_some_and(|t| t.starts_with(&format!("c_{files_id}_")));
    let files_start = if files_mode {
        offset(&files_id, token, data.plan.files.len())?
    } else {
        0
    };
    let (mut summaries, _) = file_summaries(data, files_start)?;
    if files_mode {
        loop {
            let mut value = plan_base(data, &summaries, files_start, true);
            value["cursor_next"] = value["files_cursor_next"].clone();
            if let Some(value) = budget.fit(value)? {
                return Ok(value);
            }
            if summaries.len() <= 1 {
                return Err(budget.too_small());
            }
            summaries.truncate(summaries.len().div_ceil(2));
        }
    }
    let mut diff = String::new();
    for (file, snapshot) in data.plan.files.iter().zip(&data.snapshots) {
        let before = std::str::from_utf8(&snapshot.before)
            .map_err(|_| Error::new("invalid_state", "Invalid plan bytes.", 4))?;
        let after = std::str::from_utf8(&snapshot.after)
            .map_err(|_| Error::new("invalid_state", "Invalid plan bytes.", 4))?;
        diff.push_str(
            &similar::TextDiff::from_lines(before, after)
                .unified_diff()
                .context_radius(3)
                .header(&file.change.path, &file.change.path)
                .to_string(),
        );
    }
    let start = offset(&data.plan.id, token, diff.len())?;
    if !diff.is_char_boundary(start) {
        return Err(Error::new(
            "invalid_cursor",
            "Cursor is not a UTF-8 boundary.",
            3,
        ));
    }
    let mut end = (start + 24 * 1024).min(diff.len());
    if budget.active() {
        end = line_end(&diff, start, end);
    }
    loop {
        while !diff.is_char_boundary(end) {
            end -= 1;
        }
        let mut value = plan_base(data, &summaries, files_start, false);
        value["diff"] = json!(&diff[start..end]);
        value["diff_start_byte"] = json!(start);
        value["diff_truncated"] = json!(end < diff.len());
        value["diff_chunk_boundary"] = json!(if budget.active() {
            "line_end"
        } else {
            "utf8_byte"
        });
        value["cursor_next"] = if end < diff.len() {
            json!(cursor(&data.plan.id, end))
        } else {
            Value::Null
        };
        if let Some(value) = budget.fit(value)? {
            return Ok(value);
        }
        if summaries.len() > 1 {
            summaries.truncate(summaries.len().div_ceil(2));
            continue;
        }
        if end <= start + 4 {
            return Err(budget.too_small());
        }
        let guess = start + (end - start) / 2;
        let next = if budget.active() {
            line_end(&diff, start, guess)
        } else {
            guess
        };
        if next >= end {
            return Err(budget.too_small());
        }
        end = next;
    }
}

fn line_end(diff: &str, start: usize, guess: usize) -> usize {
    diff.as_bytes()[start..guess]
        .iter()
        .rposition(|b| *b == b'\n')
        .map(|i| start + i + 1)
        .or_else(|| {
            diff.as_bytes()[start..]
                .iter()
                .position(|b| *b == b'\n')
                .map(|i| start + i + 1)
        })
        .unwrap_or(diff.len())
}

fn plan_base(data: &PlanData, summaries: &[Value], start: usize, files_mode: bool) -> Value {
    let end = start + summaries.len();
    let mut value = success(if files_mode {
        "plan_files"
    } else {
        "edit_plan"
    });
    value["plan_id"] = json!(data.plan.id);
    value["result_id"] = json!(data.plan.result_id);
    value["files_total"] = json!(data.plan.files.len());
    value["files"] = json!(summaries);
    value["shown_files"] = json!(summaries.len());
    value["files_truncated"] = json!(end < data.plan.files.len());
    value["files_cursor_next"] = if end < data.plan.files.len() {
        json!(cursor(&format!("{}_files", data.plan.id), end))
    } else {
        Value::Null
    };
    let changed: usize = data.plan.files.iter().map(|f| f.match_ids.len()).sum();
    value["selected_matches"] = json!(changed + data.plan.unchanged_matches);
    value["changed_matches"] = json!(changed);
    value["unchanged_matches"] = json!(data.plan.unchanged_matches);
    value["expires_at"] = json!(data.plan.created_at + model::RESULT_TTL);
    value["sources_modified"] = json!(false);
    if data.plan.files.len() == 1 {
        let file = &data.plan.files[0].change;
        value["path"] = json!(file.path);
        value["before_hash"] = json!(file.before_hash);
        value["after_hash"] = json!(file.after_hash);
    }
    value
}

fn file_summaries(data: &PlanData, start: usize) -> Result<(Vec<Value>, usize)> {
    let mut rows = Vec::new();
    for file in data.plan.files.iter().skip(start) {
        let mut row = serde_json::to_value(&file.change)?;
        row["selected_matches"] = json!(file.match_ids.len());
        rows.push(row);
        if rows.len() > 1 && serde_json::to_vec(&rows)?.len() > model::MAX_OUTPUT_BYTES / 3 {
            rows.pop();
            break;
        }
    }
    let end = start + rows.len();
    Ok((rows, end))
}
