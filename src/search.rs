use crate::budget::Budget;
use crate::error::{Error, Result};
use crate::files::{self, path_text};
use crate::model::{self, Match, SearchResult, Skipped};
use crate::store::Store;
use ignore::WalkBuilder;
use regex::RegexBuilder;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub fn find(
    store: &Store,
    cwd: &Path,
    pattern: &str,
    regex: bool,
    paths: &[PathBuf],
    budget: &Budget,
) -> Result<SearchResult> {
    let matcher = matcher(pattern, regex)?;
    scan(
        store,
        cwd,
        pattern,
        if regex { "regex" } else { "literal" },
        paths,
        budget,
        |result, path, hash, text| collect_matches(result, path, hash, text, &matcher),
    )
}

pub(crate) fn matcher(pattern: &str, regex: bool) -> Result<regex::Regex> {
    if pattern.contains(['\n', '\r']) || pattern.len() > 16 * 1024 {
        return Err(Error::invalid(
            "Patterns must be single-line and no longer than 16 KiB.",
        ));
    }
    let expression = if regex {
        pattern.to_owned()
    } else {
        regex::escape(pattern)
    };
    let matcher = RegexBuilder::new(&expression)
        .size_limit(10 * 1024 * 1024)
        .build()
        .map_err(|e| Error::invalid(format!("Invalid regex: {e}")))?;
    if matcher.find("").is_some_and(|m| m.is_empty()) {
        return Err(Error::invalid("Zero-length matches are not supported."));
    }
    Ok(matcher)
}

pub(crate) fn collect_matches(
    result: &mut SearchResult,
    path: &str,
    file_hash: &str,
    text: &str,
    matcher: &regex::Regex,
) -> Result<()> {
    let mut offset = 0;
    for (index, raw_line) in text.split_inclusive('\n').enumerate() {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let (line, bom) = if index == 0 && line.starts_with('\u{feff}') {
            (&line[3..], 3)
        } else {
            (line, 0)
        };
        for found in matcher.find_iter(line) {
            if found.is_empty() {
                return Err(Error::invalid("This regex produces zero-length matches."));
            }
            let (preview, truncated) = snippet(found.as_str(), 160);
            push_match(
                result,
                Match {
                    match_id: String::new(),
                    path: path.to_owned(),
                    file_hash: file_hash.to_owned(),
                    start_byte: offset + bom + found.start(),
                    end_byte: offset + bom + found.end(),
                    line: index + 1,
                    column_byte: bom + found.start() + 1,
                    text: preview,
                    text_truncated: truncated,
                    ast: None,
                },
            )?;
        }
        offset += raw_line.len();
    }
    Ok(())
}

pub(crate) fn walk(path: &Path) -> ignore::Walk {
    let mut builder = WalkBuilder::new(path);
    builder
        .follow_links(false)
        .hidden(true)
        .filter_entry(|entry| entry.file_name() != ".n4h" && entry.file_name() != ".git");
    builder.build()
}

pub(crate) fn scan(
    store: &Store,
    cwd: &Path,
    pattern: &str,
    mode: &str,
    paths: &[PathBuf],
    budget: &Budget,
    mut scan_file: impl FnMut(&mut SearchResult, &str, &str, &str) -> Result<()>,
) -> Result<SearchResult> {
    let default_paths = [PathBuf::from(".")];
    let inputs = if paths.is_empty() {
        &default_paths[..]
    } else {
        paths
    };
    let scopes = inputs
        .iter()
        .map(|p| files::relative_input(&store.root, cwd, p))
        .collect::<Result<Vec<_>>>()?;
    let mut result = SearchResult {
        id: model::id("r"),
        created_at: model::now(),
        pattern: pattern.to_owned(),
        mode: mode.to_owned(),
        paths: scopes
            .iter()
            .map(|p| {
                if p.as_os_str().is_empty() {
                    Ok(".".to_owned())
                } else {
                    path_text(p)
                }
            })
            .collect::<Result<_>>()?,
        files_scanned: 0,
        bytes_scanned: 0,
        coverage_complete: true,
        skipped_total: 0,
        skipped: Vec::new(),
        matches: Vec::new(),
        index: None,
    };
    let mut candidates = BTreeSet::new();
    for scope in scopes {
        for entry in walk(&store.root.join(scope)) {
            match entry {
                Ok(entry) => {
                    if let Some(error) = entry.error() {
                        result.coverage_complete = false;
                        skip(&mut result, "<ignore-rules>".to_owned(), error.to_string());
                    }
                    if entry.file_type().is_some_and(|t| t.is_dir()) {
                        continue;
                    }
                    candidates.insert(entry.path().to_path_buf());
                    if candidates.len() > model::MAX_FILES {
                        return Err(limit("Too many candidate files."));
                    }
                }
                Err(error) => {
                    result.coverage_complete = false;
                    skip(&mut result, "<traversal>".to_owned(), error.to_string());
                }
            }
        }
    }
    for full in candidates {
        let path = match full.strip_prefix(&store.root).ok().and_then(|p| p.to_str()) {
            Some(path) => path.to_owned(),
            None => {
                result.coverage_complete = false;
                skip(
                    &mut result,
                    "<non-utf8-path>".to_owned(),
                    "unsupported_path".to_owned(),
                );
                continue;
            }
        };
        let metadata = match std::fs::symlink_metadata(&full) {
            Ok(metadata) => metadata,
            Err(error) => {
                result.coverage_complete = false;
                skip(&mut result, path, error.to_string());
                continue;
            }
        };
        if mode == "ast_rust" && full.extension().is_none_or(|e| e != "rs") {
            skip(&mut result, path, "not_rust_source".to_owned());
            continue;
        }
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            skip(&mut result, path, "non_regular_or_symlink".to_owned());
            continue;
        }
        if metadata.len() > model::MAX_FILE_BYTES {
            skip(&mut result, path, "file_too_large".to_owned());
            continue;
        }
        result.bytes_scanned += metadata.len();
        if result.bytes_scanned > model::MAX_SEARCH_BYTES {
            return Err(limit("Search byte limit exceeded."));
        }
        let snapshot =
            match files::read_snapshot(&store.root, &path, model::MAX_FILE_BYTES as usize) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    if error.code != "unsupported_file" && error.code != "file_limit_exceeded" {
                        result.coverage_complete = false;
                    }
                    skip(&mut result, path, error.code.to_owned());
                    continue;
                }
            };
        result.bytes_scanned =
            result.bytes_scanned.saturating_sub(metadata.len()) + snapshot.bytes.len() as u64;
        if result.bytes_scanned > model::MAX_SEARCH_BYTES {
            return Err(limit("Search byte limit exceeded."));
        }
        result.files_scanned += 1;
        let file_hash = model::hash(&snapshot.bytes);
        let text = std::str::from_utf8(&snapshot.bytes)
            .map_err(|_| Error::new("unsupported_file", "Invalid UTF-8.", 5))?;
        scan_file(&mut result, &path, &file_hash, text)?;
    }
    if budget.active() {
        crate::output::result_page(store, &result, None, 1, budget)?;
    }
    store.save_result(&result)?;
    Ok(result)
}

pub(crate) fn push_match(result: &mut SearchResult, mut found: Match) -> Result<()> {
    if result.matches.len() >= model::MAX_MATCHES {
        return Err(limit("Match limit exceeded."));
    }
    found.match_id = format!("m_{:06}", result.matches.len() + 1);
    result.matches.push(found);
    Ok(())
}

fn limit(message: &str) -> Error {
    Error::new("search_limit_exceeded", message, 5)
        .action("Narrow the search paths or pattern; no editable result was saved.")
}

pub(crate) fn skip(result: &mut SearchResult, path: String, reason: String) {
    result.skipped_total += 1;
    if result.skipped.len() < 100 {
        result.skipped.push(Skipped { path, reason });
    }
}

pub fn snippet(text: &str, chars: usize) -> (String, bool) {
    let cut = text
        .char_indices()
        .nth(chars)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    (text[..cut].to_owned(), cut < text.len())
}
