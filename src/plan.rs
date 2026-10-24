use crate::ast::Template;
use crate::budget::Budget;
use crate::error::{Error, Result};
use crate::files;
use crate::model::{self, FileChange, FileSnapshot, Plan, PlanData, PlanFile};
use crate::store::Store;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

pub fn prepare(
    store: &Store,
    result_id: &str,
    match_ids: &[String],
    all: bool,
    replacement: &[u8],
    budget: &Budget,
) -> Result<PlanData> {
    prepare_impl(store, result_id, match_ids, all, replacement, false, budget)
}

pub fn prepare_ast(
    store: &Store,
    result_id: &str,
    match_ids: &[String],
    all: bool,
    replacement: &[u8],
    budget: &Budget,
) -> Result<PlanData> {
    prepare_impl(store, result_id, match_ids, all, replacement, true, budget)
}

fn limit() -> Error {
    Error::new(
        "plan_limit_exceeded",
        "Before and after bytes exceed the 20 MiB plan limit.",
        5,
    )
}

fn prepare_impl(
    store: &Store,
    result_id: &str,
    match_ids: &[String],
    all: bool,
    replacement: &[u8],
    ast_mode: bool,
    budget: &Budget,
) -> Result<PlanData> {
    store.require_clean()?;
    if (match_ids.is_empty() && !all) || (!match_ids.is_empty() && all) {
        return Err(Error::new(
            "scope_required",
            "Specify repeated --match-id values OR --scope all, never both.",
            2,
        ));
    }
    let template_text = std::str::from_utf8(replacement)
        .map_err(|_| Error::new("unsupported_file", "Replacement must be UTF-8.", 5))?;
    if replacement.len() > model::MAX_PLAN_BYTES || replacement.contains(&0) {
        return Err(Error::new(
            "unsupported_file",
            "Replacement must be bounded UTF-8 text without NUL bytes.",
            5,
        ));
    }
    let result = store.result(result_id)?;
    if ast_mode && result.mode != "ast_rust" {
        return Err(Error::new(
            "ast_result_required",
            "ast edit requires a Rust AST result from ast find.",
            2,
        ));
    }
    if !ast_mode && result.mode == "ast_rust" {
        return Err(Error::new(
            "ast_edit_required",
            "Use ast edit for AST results; normal edit is literal and does not expand captures.",
            2,
        ));
    }
    let template = ast_mode
        .then(|| Template::parse(template_text))
        .transpose()?;
    if !result.coverage_complete {
        return Err(Error::new(
            "incomplete_coverage",
            "This search had read, traversal or parse errors and is not editable.",
            3,
        ));
    }
    let selected_ids: BTreeSet<&str> = match_ids.iter().map(String::as_str).collect();
    if selected_ids.len() != match_ids.len() {
        return Err(Error::invalid("Duplicate match IDs are not allowed."));
    }
    let selected: Vec<_> = result
        .matches
        .iter()
        .filter(|m| all || selected_ids.contains(m.match_id.as_str()))
        .collect();
    if selected.is_empty() || (!all && selected.len() != selected_ids.len()) {
        return Err(Error::invalid(
            "Selection is empty or contains unknown match IDs.",
        ));
    }
    let mut groups = BTreeMap::<&str, Vec<&model::Match>>::new();
    for found in selected {
        groups.entry(&found.path).or_default().push(found);
    }
    if groups.len() > model::MAX_PLAN_FILES {
        return Err(Error::new(
            "plan_limit_exceeded",
            "A plan can select at most 50 files.",
            5,
        ));
    }
    let mut targets = Vec::new();
    let mut snapshots = Vec::new();
    let mut total_bytes = 0usize;
    let mut unchanged_matches = 0usize;
    for (path, mut selected) in groups {
        let snapshot = files::verify(&store.root, path, &selected[0].file_hash, None)?;
        if snapshot.mode & 0o222 == 0 {
            return Err(
                Error::new("unsupported_file", "Read-only files are not edited.", 5).path(path),
            );
        }
        let text = std::str::from_utf8(&snapshot.bytes)
            .map_err(|_| Error::new("invalid_state", "Invalid source encoding.", 4))?;
        if ast_mode {
            crate::ast::parse_source(text).map_err(|e| e.path(path))?;
        }
        selected.sort_by_key(|m| m.start_byte);
        let mut last_end = 0;
        let mut removed = 0usize;
        for found in &selected {
            if found.start_byte >= found.end_byte
                || found.end_byte > snapshot.bytes.len()
                || !text.is_char_boundary(found.start_byte)
                || !text.is_char_boundary(found.end_byte)
                || found.file_hash != selected[0].file_hash
            {
                return Err(Error::new(
                    "invalid_state",
                    "Invalid stored match range or hash.",
                    4,
                ));
            }
            if found.start_byte < last_end {
                return Err(Error::new(
                    "overlapping_matches",
                    "Selected matches overlap or nest. Select explicit non-overlapping match IDs.",
                    3,
                )
                .path(path));
            }
            last_end = found.end_byte;
        }
        let mut inserted = 0usize;
        let mut edits = Vec::new();
        for found in &selected {
            let bytes = match &template {
                Some(template) => Cow::Owned(template.render(
                    found,
                    &snapshot.bytes,
                    model::MAX_PLAN_BYTES.saturating_sub(inserted),
                )?),
                None => Cow::Borrowed(replacement),
            };
            if snapshot.bytes[found.start_byte..found.end_byte] == *bytes {
                unchanged_matches += 1;
                continue;
            }
            inserted = inserted.checked_add(bytes.len()).ok_or_else(limit)?;
            if inserted > model::MAX_PLAN_BYTES {
                return Err(limit());
            }
            removed += found.end_byte - found.start_byte;
            edits.push((*found, bytes));
        }
        if edits.is_empty() {
            continue;
        }
        let base = snapshot.bytes.len() - removed;
        if total_bytes
            .saturating_add(snapshot.bytes.len())
            .saturating_add(base)
            .saturating_add(inserted)
            > model::MAX_PLAN_BYTES
        {
            return Err(limit());
        }
        let mut after = Vec::with_capacity(base + inserted);
        let mut offset = 0;
        let mut changed_ids = Vec::new();
        for (found, bytes) in edits {
            after.extend_from_slice(&snapshot.bytes[offset..found.start_byte]);
            after.extend_from_slice(&bytes);
            changed_ids.push(found.match_id.clone());
            offset = found.end_byte;
        }
        after.extend_from_slice(&snapshot.bytes[offset..]);
        if ast_mode {
            let text = std::str::from_utf8(&after)
                .map_err(|_| Error::new("invalid_state", "Capture splits UTF-8.", 4))?;
            crate::ast::parse_source(text).map_err(|_| {
                Error::new(
                    "ast_rewrite_invalid",
                    "Expanded replacement produces Rust syntax errors; no plan was saved.",
                    3,
                )
                .path(path)
            })?;
        }
        total_bytes += snapshot.bytes.len() + after.len();
        targets.push(PlanFile {
            change: FileChange {
                path: path.to_owned(),
                before_hash: model::hash(&snapshot.bytes),
                after_hash: model::hash(&after),
                mode: snapshot.mode,
            },
            match_ids: changed_ids,
        });
        snapshots.push(FileSnapshot {
            before: snapshot.bytes,
            after,
        });
    }
    if targets.is_empty() {
        return Err(Error::new(
            "no_changes",
            "The replacement makes no changes.",
            3,
        ));
    }
    let data = PlanData {
        plan: Plan {
            id: model::id("p"),
            created_at: model::now(),
            result_id: result.id,
            files: targets,
            unchanged_matches,
            snapshot_version: 2,
        },
        snapshots,
    };
    // Refuse an unreviewable bounded preview before persisting a new plan.
    if budget.active() {
        crate::output::plan_page(&data, None, budget)?;
    }
    store.save_plan(&data)?;
    Ok(data)
}
