use crate::budget::Budget;
use crate::error::{Error, Result};
use crate::fault::{before_write, failpoint};
use crate::files;
use crate::model::{self, FileChange, FileProgress, FileSnapshot, Operation, OperationData};
use crate::output;
use crate::store::Store;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum ObservedState {
    Before,
    After,
    Conflict,
    MissingOrUnsupported,
}

#[derive(Serialize)]
struct Observation {
    state: ObservedState,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_hash: Option<String>,
}

pub fn apply(store: &Store, plan_id: &str, budget: &Budget) -> Result<Value> {
    if let Some(existing) = store.operation_by_source("apply", plan_id)? {
        return replay(&existing, budget);
    }
    store.require_clean()?;
    let data = store.plan(plan_id)?;
    Store::unexpired(data.plan.created_at, "plan_expired")?;
    let operation = Operation {
        id: model::id("op"),
        checkpoint_id: model::id("cp"),
        created_at: model::now(),
        kind: "apply".to_owned(),
        source_id: plan_id.to_owned(),
        files: data.plan.files.into_iter().map(|f| f.change).collect(),
        snapshot_version: 2,
    };
    execute(store, operation, &data.snapshots, budget)
}

pub fn undo(store: &Store, checkpoint_id: &str, budget: &Budget) -> Result<Value> {
    if !checkpoint_id.starts_with("cp_") {
        return Err(Error::invalid("undo expects a checkpoint ID."));
    }
    if let Some(existing) = store.operation_by_source("undo", checkpoint_id)? {
        return replay(&existing, budget);
    }
    store.require_clean()?;
    let original = store.operation(checkpoint_id)?;
    if original.status != "committed" {
        return Err(Error::new(
            "invalid_operation_state",
            "Only a committed operation can be undone.",
            3,
        ));
    }
    let operation = Operation {
        id: model::id("op"),
        checkpoint_id: model::id("cp"),
        created_at: model::now(),
        kind: "undo".to_owned(),
        source_id: checkpoint_id.to_owned(),
        snapshot_version: 2,
        files: original
            .operation
            .files
            .into_iter()
            .map(|f| FileChange {
                path: f.path,
                before_hash: f.after_hash,
                after_hash: f.before_hash,
                mode: f.mode,
            })
            .collect(),
    };
    let snapshots: Vec<_> = original
        .snapshots
        .into_iter()
        .map(|s| FileSnapshot {
            before: s.after,
            after: s.before,
        })
        .collect();
    execute(store, operation, &snapshots, budget)
}

fn preflight(store: &Store, targets: &[FileChange], after: bool) -> Result<()> {
    let mut error = None;
    for file in targets {
        let expected = if after {
            &file.after_hash
        } else {
            &file.before_hash
        };
        if files::verify(&store.root, &file.path, expected, Some(file.mode)).is_err() {
            error
                .get_or_insert_with(|| Error::conflict(&file.path))
                .paths
                .push(file.path.clone());
        }
    }
    if let Some(mut error) = error {
        // conflict() includes the first path already.
        error.paths.dedup();
        return Err(error);
    }
    Ok(())
}

fn execute(
    store: &Store,
    operation: Operation,
    snapshots: &[FileSnapshot],
    budget: &Budget,
) -> Result<Value> {
    model::validate_snapshots(&operation.files, snapshots, operation.snapshot_version)?;
    preflight(store, &operation.files, false)?;
    // All receipt IDs and counts are known: serialize/count before creating the
    // journal or modifying a source, and return exactly this prepared receipt.
    let receipt = budget.finish(operation_response(
        &operation,
        "committed",
        false,
        operation.files.len(),
    ))?;
    store.save_operation(&operation, snapshots)?;
    failpoint("after_checkpoint");
    store
        .set_status(&operation.id, "applying")
        .map_err(|e| e.operation(&operation.id))?;
    for (index, (file, snapshot)) in operation.files.iter().zip(snapshots).enumerate() {
        let result = before_write("apply", index).and_then(|_| {
            files::replace(
                &store.root,
                &file.path,
                &file.before_hash,
                &snapshot.after,
                file.mode,
            )
        });
        if let Err(error) = result {
            let _ = store.set_status(&operation.id, "recovery_required");
            return Err(error.path(&file.path).operation(&operation.id));
        }
        // Hashes, not recorded progress, decide recovery if termination happens here.
        failpoint("after_rename");
        failpoint(&format!("after_rename_{}", index + 1));
        store
            .mark_file(&operation.id, index, FileProgress::Applied)
            .map_err(|e| e.operation(&operation.id))?;
        failpoint(&format!("after_file_{}", index + 1));
    }
    if let Err(error) = preflight(store, &operation.files, true) {
        let _ = store.set_status(&operation.id, "recovery_required");
        return Err(error.operation(&operation.id));
    }
    store
        .set_status(&operation.id, "committed")
        .map_err(|e| e.operation(&operation.id))?;
    Ok(receipt)
}

fn replay(data: &OperationData, budget: &Budget) -> Result<Value> {
    match data.status.as_str() {
        "committed" => budget.finish(operation_response(&data.operation, "committed", true, 0)),
        "rolled_back" => Err(Error::new(
            "operation_rolled_back",
            "This operation was rolled back; create a new plan instead of reapplying it.",
            3,
        )),
        _ => Err(Error::new(
            "recovery_required",
            "The previous attempt has not committed.",
            3,
        )
        .operation(&data.operation.id)),
    }
}

fn operation_response(
    operation: &Operation,
    status: &str,
    replayed: bool,
    modified: usize,
) -> Value {
    let mut value = output::success("operation");
    value["operation_id"] = json!(operation.id);
    value["checkpoint_id"] = json!(operation.checkpoint_id);
    value["operation_kind"] = json!(operation.kind);
    value["source_id"] = json!(operation.source_id);
    value["files_total"] = json!(operation.files.len());
    value["path"] = if operation.files.len() == 1 {
        json!(operation.files[0].path)
    } else {
        Value::Null
    };
    // A compact receipt must fit even after writing many very long paths.
    value["paths"] = json!(
        operation
            .files
            .iter()
            .take(1)
            .map(|f| &f.path)
            .collect::<Vec<_>>()
    );
    value["paths_truncated"] = json!(operation.files.len() > 1);
    value["status"] = json!(status);
    value["idempotent_replay"] = json!(replayed);
    value["files_modified_this_call"] = json!(modified);
    value
}

pub fn recover(
    store: &Store,
    id: Option<&str>,
    rollback: bool,
    cursor: Option<&str>,
    budget: &Budget,
) -> Result<Value> {
    let Some(id) = id else {
        if rollback || cursor.is_some() {
            return Err(Error::invalid(
                "--rollback and --cursor require an operation ID.",
            ));
        }
        return status(store, budget);
    };
    let data = store.operation(id)?;
    if !rollback {
        return observations(store, &data, cursor, budget);
    }
    if cursor.is_some() {
        return Err(Error::invalid(
            "--cursor is inspection-only and cannot be used with --rollback.",
        ));
    }
    if data.status == "rolled_back" {
        let mut value = operation_response(&data.operation, "rolled_back", true, 0);
        value["sources_modified_this_call"] = json!(false);
        return budget.finish(value);
    }
    if data.status == "committed" {
        return Err(Error::new(
            "invalid_operation_state",
            "Use undo for a committed operation; recover only rolls back interrupted operations.",
            3,
        ));
    }
    let mut to_restore = Vec::new();
    let mut conflicts = Vec::new();
    for file in &data.operation.files {
        match observe(store, file).state {
            ObservedState::Before => to_restore.push(false),
            ObservedState::After => to_restore.push(true),
            _ => {
                conflicts.push(file.path.clone());
                to_restore.push(false);
            }
        }
    }
    // Do not begin any restoration if even one target has an external version.
    if !conflicts.is_empty() {
        return Err(recovery_conflict(&data, conflicts));
    }
    let expected_modified = to_restore.iter().filter(|changed| **changed).count();
    let mut receipt = operation_response(&data.operation, "rolled_back", false, expected_modified);
    receipt["sources_modified_this_call"] = json!(expected_modified > 0);
    let receipt = budget.finish(receipt)?;
    store
        .set_status(&data.operation.id, "recovery_required")
        .map_err(|e| e.operation(&data.operation.id))?;
    let mut modified = 0usize;
    for (index, ((file, snapshot), changed)) in data
        .operation
        .files
        .iter()
        .zip(&data.snapshots)
        .zip(to_restore)
        .enumerate()
    {
        if changed {
            before_write("rollback", index)
                .and_then(|_| {
                    files::replace(
                        &store.root,
                        &file.path,
                        &file.after_hash,
                        &snapshot.before,
                        file.mode,
                    )
                })
                .map_err(|e| e.path(&file.path).operation(&data.operation.id))?;
            modified += 1;
            failpoint("rollback_after_rename");
            failpoint(&format!("rollback_after_rename_{}", index + 1));
        } else {
            files::verify(&store.root, &file.path, &file.before_hash, Some(file.mode))
                .map_err(|_| recovery_conflict(&data, vec![file.path.clone()]))?;
        }
        store
            .mark_file(&data.operation.id, index, FileProgress::Restored)
            .map_err(|e| e.operation(&data.operation.id))?;
        failpoint(&format!("rollback_after_file_{}", index + 1));
    }
    preflight(store, &data.operation.files, false)
        .map_err(|e| recovery_conflict(&data, e.paths))?;
    store
        .set_status(&data.operation.id, "rolled_back")
        .map_err(|e| e.operation(&data.operation.id))?;
    debug_assert_eq!(modified, expected_modified);
    Ok(receipt)
}

fn recovery_conflict(data: &OperationData, paths: Vec<String>) -> Error {
    let mut error = Error::new("recovery_conflict", "Current bytes, permissions or file type do not match a supported checkpoint state. No conflicting file was overwritten.", 3)
        .operation(&data.operation.id);
    error.paths = paths;
    error
}

fn observe(store: &Store, file: &FileChange) -> Observation {
    match files::read_snapshot(&store.root, &file.path, model::MAX_PLAN_BYTES) {
        Ok(snapshot) => {
            let hash = model::hash(&snapshot.bytes);
            Observation {
                state: if snapshot.mode != file.mode {
                    ObservedState::Conflict
                } else if hash == file.before_hash {
                    ObservedState::Before
                } else if hash == file.after_hash {
                    ObservedState::After
                } else {
                    ObservedState::Conflict
                },
                current_hash: Some(hash),
            }
        }
        Err(_) => Observation {
            state: ObservedState::MissingOrUnsupported,
            current_hash: None,
        },
    }
}

fn observations(
    store: &Store,
    data: &OperationData,
    token: Option<&str>,
    budget: &Budget,
) -> Result<Value> {
    let start = output::offset(&data.operation.id, token, data.operation.files.len())?;
    let mut rows = Vec::new();
    for (index, file) in data.operation.files.iter().enumerate().skip(start) {
        let mut row = serde_json::to_value(observe(store, file))?;
        row["path"] = json!(file.path);
        row["before_hash"] = json!(file.before_hash);
        row["after_hash"] = json!(file.after_hash);
        row["recorded_progress"] = json!(data.progress[index]);
        rows.push(row);
    }
    loop {
        let end = start + rows.len();
        let mut value = operation_response(&data.operation, &data.status, true, 0);
        value["observations"] = json!(rows);
        value["shown_files"] = json!(rows.len());
        value["truncated"] = json!(end < data.operation.files.len());
        value["cursor_next"] = if end < data.operation.files.len() {
            json!(output::cursor(&data.operation.id, end))
        } else {
            Value::Null
        };
        if data.operation.files.len() == 1 {
            value["observation"] = rows[0].clone();
        }
        if let Some(value) = budget.fit(value)? {
            return Ok(value);
        }
        if rows.len() <= 1 {
            return Err(budget.too_small());
        }
        rows.pop();
    }
}

pub fn status(store: &Store, budget: &Budget) -> Result<Value> {
    let pending = store.pending()?;
    let mut operations: Vec<_> = pending.iter().take(50).map(|(op, state)| {
        json!({"operation_id": op.id, "checkpoint_id": op.checkpoint_id, "status": state,
            "files_total": op.files.len(), "path": if op.files.len() == 1 { json!(op.files[0].path) } else { Value::Null }})
    }).collect();
    loop {
        let mut value = output::success("status");
        value["repository_id"] = json!(store.repository_id);
        value["root"] = json!(store.root);
        value["pending_operations"] = json!(operations);
        value["pending_operations_total"] = json!(pending.len());
        value["pending_operations_truncated"] = json!(operations.len() < pending.len());
        value["recovery_required"] = json!(!pending.is_empty());
        value["operations_recorded"] = json!(store.operation_count()?);
        if let Some(value) = budget.fit(value)? {
            return Ok(value);
        }
        if operations.len() <= 1 {
            return Err(budget.too_small());
        }
        operations.pop();
    }
}
