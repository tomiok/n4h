use crate::error::{Error, Result};
use crate::model::{
    self, FileProgress, FileSnapshot, Operation, OperationData, PlanData, SearchResult,
};
use fs2::FileExt;
use redb::{
    Database, Durability, ReadTransaction, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    TableDefinition, WriteTransaction,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

mod memory;
mod resume;

const META: TableDefinition<&str, &str> = TableDefinition::new("meta");
const RESULTS: TableDefinition<&str, &str> = TableDefinition::new("results");
const PLANS: TableDefinition<&str, &str> = TableDefinition::new("plans");
const OPERATIONS: TableDefinition<&str, &str> = TableDefinition::new("operations");
const CHECKPOINT_INDEX: TableDefinition<&str, &str> = TableDefinition::new("checkpoint_index");
const SOURCE_INDEX: TableDefinition<&str, &str> = TableDefinition::new("source_index");
const PENDING: TableDefinition<&str, &str> = TableDefinition::new("pending_operations");
const SNAPSHOTS: TableDefinition<&str, &[u8]> = TableDefinition::new("snapshots");
const NOTES: TableDefinition<&str, &str> = TableDefinition::new("notes");
const STORAGE_SCHEMA: &str = "6";

#[derive(Serialize, Deserialize)]
struct StoredOperation {
    operation: Operation,
    status: String,
    #[serde(default)]
    progress: Vec<FileProgress>,
}

pub struct Store {
    pub root: PathBuf,
    pub repository_id: String,
    database: Database,
    state: PathBuf,
    // Drop the database before releasing the repository-wide process lock.
    _lock: File,
}

fn invalid_state(message: &str) -> Error {
    Error::new("invalid_state", message, 4)
}

fn reject_link(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            #[cfg(unix)]
            let hardlink = {
                use std::os::unix::fs::MetadataExt;
                metadata.is_file() && metadata.nlink() != 1
            };
            #[cfg(not(unix))]
            let hardlink = false;
            if metadata.file_type().is_symlink() || hardlink {
                return Err(Error::new(
                    "invalid_state",
                    "n4h storage must not use symlinks or hardlinks.",
                    3,
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn reject_legacy(state: &Path) -> Result<()> {
    // Detect sidecars too: ignoring a SQLite journal/WAL could hide recovery data.
    for name in [
        "state.sqlite3",
        "state.sqlite3-wal",
        "state.sqlite3-shm",
        "state.sqlite3-journal",
    ] {
        match fs::symlink_metadata(state.join(name)) {
            Ok(_) => {
                return Err(Error::new(
                    "legacy_state_detected",
                    "SQLite state was detected. No conversion, deletion or new redb state was performed.",
                    3,
                ).action("Preserve the entire .n4h directory. Use the previous SQLite-backed n4h to inspect and recover pending operations before an explicit migration.")
                 .action("Automatic SQLite import is not supported; do not delete checkpoints to bypass this check."));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn pending_status(status: &str) -> Result<bool> {
    match status {
        "prepared" | "applying" | "recovery_required" => Ok(true),
        "committed" | "rolled_back" => Ok(false),
        _ => Err(invalid_state("Unknown stored operation status.")),
    }
}

fn source_key(kind: &str, source_id: &str) -> String {
    format!("{kind}:{source_id}")
}

fn put_new(
    transaction: &WriteTransaction,
    definition: TableDefinition<&str, &str>,
    key: &str,
    value: &str,
) -> Result<()> {
    let mut table = transaction.open_table(definition)?;
    if table.get(key)?.is_some() {
        return Err(invalid_state(
            "Duplicate immutable reference or operation index.",
        ));
    }
    table.insert(key, value)?;
    Ok(())
}

fn save_snapshots(
    transaction: &WriteTransaction,
    id: &str,
    version: u8,
    snapshots: &[FileSnapshot],
) -> Result<()> {
    let mut table = transaction.open_table(SNAPSHOTS)?;
    for (index, snapshot) in snapshots.iter().enumerate() {
        for (suffix, bytes) in [
            ("before", snapshot.before.as_slice()),
            ("after", snapshot.after.as_slice()),
        ] {
            let key = model::snapshot_key(id, version, index, suffix);
            if table.get(key.as_str())?.is_some() {
                return Err(invalid_state("Duplicate immutable snapshot."));
            }
            table.insert(key.as_str(), bytes)?;
        }
    }
    Ok(())
}

fn load_snapshots(
    transaction: &ReadTransaction,
    id: &str,
    version: u8,
    count: usize,
) -> Result<Vec<FileSnapshot>> {
    let table = transaction.open_table(SNAPSHOTS)?;
    let mut total_bytes = 0usize;
    let mut get = |index: usize, suffix: &str| -> Result<Vec<u8>> {
        let key = model::snapshot_key(id, version, index, suffix);
        let bytes = table
            .get(key.as_str())?
            .ok_or_else(|| invalid_state("Missing snapshot for a stored plan or operation."))?;
        total_bytes = total_bytes.saturating_add(bytes.value().len());
        if total_bytes > model::MAX_PLAN_BYTES {
            return Err(invalid_state(
                "Stored snapshots exceed the operation byte limit.",
            ));
        }
        Ok(bytes.value().to_vec())
    };
    (0..count)
        .map(|index| {
            Ok(FileSnapshot {
                before: get(index, "before")?,
                after: get(index, "after")?,
            })
        })
        .collect()
}

fn normalize_record(record: &mut StoredOperation) -> Result<()> {
    model::validate_changes(&record.operation.files, record.operation.snapshot_version)?;
    if record.progress.is_empty() && record.operation.snapshot_version == 1 {
        record.progress = vec![FileProgress::Unknown; record.operation.files.len()];
    }
    if record.progress.len() != record.operation.files.len() {
        return Err(invalid_state("Invalid per-file journal progress."));
    }
    Ok(())
}

fn load_record(transaction: &ReadTransaction, id: &str) -> Result<StoredOperation> {
    let table = transaction.open_table(OPERATIONS)?;
    let payload = table.get(id)?.ok_or_else(|| {
        Error::new(
            "operation_not_found",
            "Unknown operation or checkpoint ID.",
            3,
        )
    })?;
    let mut record: StoredOperation = serde_json::from_str(payload.value())?;
    normalize_record(&mut record)?;
    if record.operation.id != id {
        return Err(invalid_state("Operation key does not match its stored ID."));
    }
    let checkpoints = transaction.open_table(CHECKPOINT_INDEX)?;
    let sources = transaction.open_table(SOURCE_INDEX)?;
    let pending = transaction.open_table(PENDING)?;
    let source = source_key(&record.operation.kind, &record.operation.source_id);
    if checkpoints
        .get(record.operation.checkpoint_id.as_str())?
        .as_ref()
        .map(|v| v.value())
        != Some(id)
        || sources.get(source.as_str())?.as_ref().map(|v| v.value()) != Some(id)
    {
        return Err(invalid_state(
            "Operation indexes do not match the stored operation.",
        ));
    }
    let pending_entry = pending.get(id)?;
    if (pending_status(&record.status)?
        && pending_entry.as_ref().map(|v| v.value()) != Some(record.status.as_str()))
        || (!pending_status(&record.status)? && pending_entry.is_some())
    {
        return Err(invalid_state(
            "Pending-operation index does not match the journal.",
        ));
    }
    Ok(record)
}

fn load_operation(transaction: &ReadTransaction, id: &str) -> Result<OperationData> {
    let record = load_record(transaction, id)?;
    let snapshots = load_snapshots(
        transaction,
        id,
        record.operation.snapshot_version,
        record.operation.files.len(),
    )?;
    model::validate_snapshots(
        &record.operation.files,
        &snapshots,
        record.operation.snapshot_version,
    )?;
    Ok(OperationData {
        operation: record.operation,
        status: record.status,
        snapshots,
        progress: record.progress,
    })
}

impl Store {
    pub fn locate(cwd: &Path) -> Result<PathBuf> {
        for ancestor in cwd.ancestors() {
            if fs::symlink_metadata(ancestor.join(".n4h")).is_ok() {
                return Ok(ancestor.to_path_buf());
            }
        }
        Err(
            Error::new("not_initialized", "No .n4h directory was found.", 2)
                .action("Run n4h init at the repository root."),
        )
    }

    #[cfg(test)]
    pub fn init(root: &Path) -> Result<Self> {
        Self::init_checked(root, |_, _| Ok(()))
    }

    pub fn init_checked(
        root: &Path,
        preflight: impl Fn(&Path, &str) -> Result<()>,
    ) -> Result<Self> {
        if let Ok(existing) = Self::locate(root) {
            let store = Self::open(&existing)?;
            preflight(&store.root, &store.repository_id)?;
            return Ok(store);
        }
        let repository_id = model::id("repo");
        preflight(root, &repository_id)?;
        let state = root.join(".n4h");
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            fs::DirBuilder::new().mode(0o700).create(&state)?;
        }
        #[cfg(not(unix))]
        fs::create_dir(&state)?;
        let lock = Self::lock(&state)?;
        let path = state.join("state.redb");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path)?.sync_all()?;
        let database = Database::builder()
            .set_cache_size(8 * 1024 * 1024)
            .create(&path)?;
        let mut transaction = database.begin_write()?;
        transaction.set_durability(Durability::Immediate)?;
        for table in [
            META,
            RESULTS,
            PLANS,
            OPERATIONS,
            CHECKPOINT_INDEX,
            SOURCE_INDEX,
            PENDING,
            NOTES,
        ] {
            transaction.open_table(table)?;
        }
        transaction.open_table(SNAPSHOTS)?;
        {
            let mut meta = transaction.open_table(META)?;
            for (key, value) in [
                ("schema", STORAGE_SCHEMA.to_owned()),
                ("engine", "redb".to_owned()),
                ("root", crate::files::path_text(root)?),
                ("repository_id", repository_id.clone()),
                ("memory_generation", "0".to_owned()),
            ] {
                meta.insert(key, value.as_str())?;
            }
        }
        transaction.commit()?;
        #[cfg(unix)]
        {
            File::open(&state)?.sync_all()?;
            File::open(root)?.sync_all()?;
        }
        Ok(Self {
            root: root.to_path_buf(),
            repository_id,
            database,
            state,
            _lock: lock,
        })
    }

    pub fn open(root: &Path) -> Result<Self> {
        let state = root.join(".n4h");
        reject_link(&state)?;
        reject_legacy(&state)?;
        let lock = Self::lock(&state)?;
        let path = state.join("state.redb");
        reject_link(&path)?;
        // Unlike create(), open() cannot silently initialize an absent/empty database.
        let database = Database::builder()
            .set_cache_size(8 * 1024 * 1024)
            .open(&path)?;
        let repository_id = {
            let transaction = database.begin_read()?;
            let meta = transaction.open_table(META)?;
            let get = |key| -> Result<String> {
                Ok(meta
                    .get(key)?
                    .ok_or_else(|| invalid_state("Missing storage metadata."))?
                    .value()
                    .to_owned())
            };
            if !matches!(
                get("schema")?.as_str(),
                "2" | "3" | "4" | "5" | STORAGE_SCHEMA
            ) || get("engine")? != "redb"
                || get("root")? != crate::files::path_text(root)?
            {
                return Err(Error::new(
                    "invalid_state",
                    "Unsupported schema or storage copied from a different root.",
                    3,
                ));
            }
            get("repository_id")?
        };
        Ok(Self {
            root: root.to_path_buf(),
            repository_id,
            database,
            state,
            _lock: lock,
        })
    }

    fn lock(state: &Path) -> Result<File> {
        let path = state.join("lock");
        reject_link(&path)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path)?;
        file.try_lock_exclusive().map_err(|_| {
            Error::new(
                "repository_busy",
                "Another n4h process holds the repository lock.",
                3,
            )
        })?;
        Ok(file)
    }

    fn write(&self) -> Result<WriteTransaction> {
        let mut transaction = self.database.begin_write()?;
        transaction.set_durability(Durability::Immediate)?;
        // Upgrade only as part of a successful mutation. Schema-2 one-file
        // records and their snapshot keys stay readable without rewriting them.
        transaction
            .open_table(META)?
            .insert("schema", STORAGE_SCHEMA)?;
        transaction.open_table(NOTES)?;
        {
            let mut meta = transaction.open_table(META)?;
            if meta.get("memory_generation")?.is_none() {
                meta.insert("memory_generation", "0")?;
            }
        }
        Ok(transaction)
    }

    pub fn capacity(&self, required: u64) -> Result<()> {
        let mut used = 0u64;
        for entry in fs::read_dir(&self.state)? {
            let entry = entry?;
            // The disposable index has its own quota; never charge cache growth
            // against durable checkpoints. Filesystem free space is still shared.
            if entry.file_name() == "index.redb" {
                continue;
            }
            used = used.saturating_add(entry.metadata()?.len());
        }
        let reserve = required.saturating_add(1024 * 1024);
        if used.saturating_add(reserve) > model::MAX_STORE_BYTES
            || fs2::available_space(&self.state)? < reserve
        {
            return Err(Error::new(
                "storage_limit_exceeded",
                "Insufficient storage capacity for state and recovery.",
                5,
            ));
        }
        Ok(())
    }

    pub fn save_result(&self, result: &SearchResult) -> Result<()> {
        let payload = serde_json::to_string(result)?;
        self.capacity(payload.len() as u64 * 2)?;
        let transaction = self.write()?;
        put_new(&transaction, RESULTS, &result.id, &payload)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn result(&self, id: &str) -> Result<SearchResult> {
        let transaction = self.database.begin_read()?;
        let table = transaction.open_table(RESULTS)?;
        let payload = table.get(id)?.ok_or_else(|| {
            Error::new(
                "result_not_found",
                "Unknown result ID in this repository.",
                3,
            )
        })?;
        let result: SearchResult = serde_json::from_str(payload.value())?;
        if result.id != id {
            return Err(invalid_state("Result key does not match its stored ID."));
        }
        Self::unexpired(result.created_at, "result_expired")?;
        Ok(result)
    }

    pub fn save_plan(&self, data: &PlanData) -> Result<()> {
        model::validate_snapshots(
            data.plan.files.iter().map(|f| &f.change),
            &data.snapshots,
            data.plan.snapshot_version,
        )?;
        let payload = serde_json::to_string(&data.plan)?;
        self.capacity((model::snapshot_bytes(&data.snapshots) + payload.len()) as u64 * 2)?;
        let transaction = self.write()?;
        put_new(&transaction, PLANS, &data.plan.id, &payload)?;
        save_snapshots(
            &transaction,
            &data.plan.id,
            data.plan.snapshot_version,
            &data.snapshots,
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn plan(&self, id: &str) -> Result<PlanData> {
        let transaction = self.database.begin_read()?;
        let table = transaction.open_table(PLANS)?;
        let payload = table
            .get(id)?
            .ok_or_else(|| Error::new("plan_not_found", "Unknown plan ID.", 3))?;
        let plan: model::Plan = serde_json::from_str(payload.value())?;
        if plan.id != id {
            return Err(invalid_state("Plan key does not match its stored ID."));
        }
        model::validate_changes(plan.files.iter().map(|f| &f.change), plan.snapshot_version)?;
        let snapshots = load_snapshots(&transaction, id, plan.snapshot_version, plan.files.len())?;
        model::validate_snapshots(
            plan.files.iter().map(|f| &f.change),
            &snapshots,
            plan.snapshot_version,
        )?;
        Ok(PlanData { plan, snapshots })
    }

    pub fn unexpired(created: u64, code: &'static str) -> Result<()> {
        if model::now().saturating_sub(created) >= model::RESULT_TTL {
            return Err(Error::new(code, "This reference has expired.", 3)
                .action("Run find again and review a new plan."));
        }
        Ok(())
    }

    pub fn pending(&self) -> Result<Vec<(Operation, String)>> {
        let transaction = self.database.begin_read()?;
        let table = transaction.open_table(PENDING)?;
        let mut operations = Vec::new();
        for entry in table.iter()? {
            let (id, _) = entry?;
            let record = load_record(&transaction, id.value())?;
            if !pending_status(&record.status)? {
                return Err(invalid_state("Terminal operation in the pending index."));
            }
            operations.push((record.operation, record.status));
        }
        operations.sort_by(|(a, _), (b, _)| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
        Ok(operations)
    }

    pub fn require_clean(&self) -> Result<()> {
        if let Some((operation, _)) = self.pending()?.first() {
            return Err(Error::new(
                "recovery_required",
                "An earlier operation requires recovery before another edit.",
                3,
            )
            .operation(&operation.id));
        }
        Ok(())
    }

    pub fn operation_count(&self) -> Result<u64> {
        let transaction = self.database.begin_read()?;
        Ok(transaction.open_table(OPERATIONS)?.len()?)
    }

    pub fn operation_by_source(
        &self,
        kind: &str,
        source_id: &str,
    ) -> Result<Option<OperationData>> {
        let transaction = self.database.begin_read()?;
        let table = transaction.open_table(SOURCE_INDEX)?;
        let key = source_key(kind, source_id);
        let Some(id) = table.get(key.as_str())? else {
            return Ok(None);
        };
        let data = load_operation(&transaction, id.value())?;
        if data.operation.kind != kind || data.operation.source_id != source_id {
            return Err(invalid_state(
                "Source index points to a different operation.",
            ));
        }
        Ok(Some(data))
    }

    pub fn operation(&self, id: &str) -> Result<OperationData> {
        let transaction = self.database.begin_read()?;
        if id.starts_with("cp_") {
            let table = transaction.open_table(CHECKPOINT_INDEX)?;
            let operation_id = table.get(id)?.ok_or_else(|| {
                Error::new(
                    "operation_not_found",
                    "Unknown operation or checkpoint ID.",
                    3,
                )
            })?;
            let data = load_operation(&transaction, operation_id.value())?;
            if data.operation.checkpoint_id != id {
                return Err(invalid_state(
                    "Checkpoint index points to a different operation.",
                ));
            }
            Ok(data)
        } else {
            load_operation(&transaction, id)
        }
    }

    pub fn save_operation(&self, operation: &Operation, snapshots: &[FileSnapshot]) -> Result<()> {
        if operation.kind != "apply" && operation.kind != "undo" {
            return Err(invalid_state("Unsupported operation kind."));
        }
        model::validate_snapshots(&operation.files, snapshots, operation.snapshot_version)?;
        self.capacity(model::snapshot_bytes(snapshots) as u64 * 3)?;
        let record = StoredOperation {
            operation: operation.clone(),
            status: "prepared".to_owned(),
            progress: vec![FileProgress::Prepared; operation.files.len()],
        };
        let transaction = self.write()?;
        put_new(
            &transaction,
            OPERATIONS,
            &operation.id,
            &serde_json::to_string(&record)?,
        )?;
        put_new(
            &transaction,
            CHECKPOINT_INDEX,
            &operation.checkpoint_id,
            &operation.id,
        )?;
        put_new(
            &transaction,
            SOURCE_INDEX,
            &source_key(&operation.kind, &operation.source_id),
            &operation.id,
        )?;
        put_new(&transaction, PENDING, &operation.id, "prepared")?;
        save_snapshots(
            &transaction,
            &operation.id,
            operation.snapshot_version,
            snapshots,
        )?;
        crate::fault::failpoint("before_checkpoint_commit");
        transaction.commit()?;
        Ok(())
    }

    pub fn set_status(&self, id: &str, status: &str) -> Result<()> {
        pending_status(status)?;
        self.update_record(id, |record| {
            let permitted = record.status == status
                || matches!(
                    (record.status.as_str(), status),
                    ("prepared", "applying" | "recovery_required")
                        | ("applying", "committed" | "recovery_required")
                        | ("recovery_required", "rolled_back")
                );
            if !permitted {
                return Err(invalid_state("Invalid journal status transition."));
            }
            record.status = status.to_owned();
            Ok(())
        })
    }

    pub fn mark_file(&self, id: &str, index: usize, progress: FileProgress) -> Result<()> {
        self.update_record(id, |record| {
            if !matches!(
                (&progress, record.status.as_str()),
                (FileProgress::Applied, "applying") | (FileProgress::Restored, "recovery_required")
            ) {
                return Err(invalid_state("Invalid file progress transition."));
            }
            let slot = record
                .progress
                .get_mut(index)
                .ok_or_else(|| invalid_state("Unknown journal file index."))?;
            *slot = progress;
            Ok(())
        })
    }

    fn update_record(
        &self,
        id: &str,
        change: impl FnOnce(&mut StoredOperation) -> Result<()>,
    ) -> Result<()> {
        let transaction = self.write()?;
        let status = {
            let mut table = transaction.open_table(OPERATIONS)?;
            let mut record: StoredOperation = serde_json::from_str(
                table
                    .get(id)?
                    .ok_or_else(|| Error::new("operation_not_found", "Unknown operation ID.", 3))?
                    .value(),
            )?;
            if record.operation.id != id {
                return Err(invalid_state("Operation key does not match its stored ID."));
            }
            normalize_record(&mut record)?;
            change(&mut record)?;
            table.insert(id, serde_json::to_string(&record)?.as_str())?;
            record.status
        };
        {
            let mut pending = transaction.open_table(PENDING)?;
            if pending_status(&status)? {
                pending.insert(id, status.as_str())?;
            } else {
                pending.remove(id)?;
            }
        }
        if status == "committed" {
            crate::fault::failpoint("before_committed_status_commit");
        }
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operation() -> Operation {
        Operation {
            id: model::id("op"),
            checkpoint_id: model::id("cp"),
            created_at: model::now(),
            kind: "apply".to_owned(),
            source_id: model::id("p"),
            files: vec![model::FileChange {
                path: "a.txt".to_owned(),
                before_hash: model::hash(b"old"),
                after_hash: model::hash(b"new"),
                mode: 0o644,
            }],
            snapshot_version: 2,
        }
    }

    fn snapshots() -> Vec<FileSnapshot> {
        vec![FileSnapshot {
            before: b"old".to_vec(),
            after: b"new".to_vec(),
        }]
    }

    fn assert_one_complete_operation(store: &Store) {
        let transaction = store.database.begin_read().unwrap();
        for table in [OPERATIONS, CHECKPOINT_INDEX, SOURCE_INDEX, PENDING] {
            assert_eq!(transaction.open_table(table).unwrap().len().unwrap(), 1);
        }
        assert_eq!(transaction.open_table(SNAPSHOTS).unwrap().len().unwrap(), 2);
    }

    #[test]
    fn operation_snapshots_and_indexes_survive_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let original = operation();
        {
            let store = Store::init(directory.path()).unwrap();
            store.save_operation(&original, &snapshots()).unwrap();
            assert_one_complete_operation(&store);
        }
        let store = Store::open(directory.path()).unwrap();
        for id in [&original.id, &original.checkpoint_id] {
            let data = store.operation(id).unwrap();
            assert_eq!(data.operation.id, original.id);
            assert_eq!(data.status, "prepared");
            assert_eq!(data.snapshots[0].before, b"old");
            assert_eq!(data.snapshots[0].after, b"new");
        }
        assert_eq!(
            store
                .operation_by_source("apply", &original.source_id)
                .unwrap()
                .unwrap()
                .operation
                .id,
            original.id
        );
        assert_eq!(store.pending().unwrap()[0].0.id, original.id);
        assert_one_complete_operation(&store);
    }

    #[test]
    fn duplicate_checkpoint_or_source_aborts_every_staged_record() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::init(directory.path()).unwrap();
        let original = operation();
        store.save_operation(&original, &snapshots()).unwrap();
        for checkpoint_collision in [true, false] {
            let mut candidate = operation();
            if checkpoint_collision {
                candidate.checkpoint_id = original.checkpoint_id.clone();
            } else {
                candidate.source_id = original.source_id.clone();
            }
            assert_eq!(
                store
                    .save_operation(&candidate, &snapshots())
                    .unwrap_err()
                    .code,
                "invalid_state"
            );
            assert_eq!(
                store.operation(&candidate.id).err().unwrap().code,
                "operation_not_found"
            );
            assert_one_complete_operation(&store);
            if !checkpoint_collision {
                assert_eq!(
                    store
                        .operation(&candidate.checkpoint_id)
                        .err()
                        .unwrap()
                        .code,
                    "operation_not_found"
                );
            }
        }
        assert_eq!(store.operation_count().unwrap(), 1);
    }

    #[test]
    fn status_and_pending_index_transition_together() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::init(directory.path()).unwrap();
        let original = operation();
        store.save_operation(&original, &snapshots()).unwrap();
        store.set_status(&original.id, "applying").unwrap();
        assert_eq!(store.pending().unwrap()[0].1, "applying");
        store.set_status(&original.id, "committed").unwrap();
        assert!(store.pending().unwrap().is_empty());
        assert_eq!(
            store.operation(&original.checkpoint_id).unwrap().status,
            "committed"
        );
        assert_eq!(
            store.set_status(&original.id, "applying").unwrap_err().code,
            "invalid_state"
        );
        assert!(store.pending().unwrap().is_empty());
        let other = operation();
        store.save_operation(&other, &snapshots()).unwrap();
        store.set_status(&other.id, "recovery_required").unwrap();
        assert_eq!(store.pending().unwrap()[0].1, "recovery_required");
        store.set_status(&other.id, "rolled_back").unwrap();
        assert!(store.pending().unwrap().is_empty());
        assert_eq!(store.operation(&other.id).unwrap().status, "rolled_back");
    }

    #[test]
    fn uncommitted_status_change_does_not_remove_a_pending_operation() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::init(directory.path()).unwrap();
        let original = operation();
        store.save_operation(&original, &snapshots()).unwrap();
        {
            let transaction = store.write().unwrap();
            {
                let mut operations = transaction.open_table(OPERATIONS).unwrap();
                let mut record: StoredOperation = serde_json::from_str(
                    operations
                        .get(original.id.as_str())
                        .unwrap()
                        .unwrap()
                        .value(),
                )
                .unwrap();
                record.status = "committed".to_owned();
                operations
                    .insert(
                        original.id.as_str(),
                        serde_json::to_string(&record).unwrap().as_str(),
                    )
                    .unwrap();
            }
            transaction
                .open_table(PENDING)
                .unwrap()
                .remove(original.id.as_str())
                .unwrap();
            // Drop without commit, as with a failed operation before durability.
        }
        assert_eq!(store.operation(&original.id).unwrap().status, "prepared");
        assert_eq!(store.pending().unwrap()[0].1, "prepared");
        assert_one_complete_operation(&store);
    }

    #[test]
    fn an_existing_plan_and_its_snapshots_cannot_be_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::init(directory.path()).unwrap();
        let mut data = PlanData {
            plan: model::Plan {
                id: model::id("p"),
                created_at: model::now(),
                result_id: model::id("r"),
                files: vec![model::PlanFile {
                    change: model::FileChange {
                        path: "a.txt".to_owned(),
                        before_hash: model::hash(b"old"),
                        after_hash: model::hash(b"new"),
                        mode: 0o644,
                    },
                    match_ids: vec!["m_000001".to_owned()],
                }],
                unchanged_matches: 0,
                snapshot_version: 2,
            },
            snapshots: snapshots(),
        };
        store.save_plan(&data).unwrap();
        data.snapshots[0].after = b"different".to_vec();
        data.plan.files[0].change.after_hash = model::hash(&data.snapshots[0].after);
        assert_eq!(store.save_plan(&data).unwrap_err().code, "invalid_state");
        let saved = store.plan(&data.plan.id).unwrap();
        assert_eq!(saved.snapshots[0].after, b"new");
        assert_eq!(saved.plan.files[0].change.after_hash, model::hash(b"new"));
        let transaction = store.database.begin_read().unwrap();
        assert_eq!(transaction.open_table(SNAPSHOTS).unwrap().len().unwrap(), 2);
    }

    #[test]
    fn missing_snapshot_and_inconsistent_indexes_are_integrity_errors() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::init(directory.path()).unwrap();
        let original = operation();
        store.save_operation(&original, &snapshots()).unwrap();
        let transaction = store.write().unwrap();
        let key = model::snapshot_key(&original.id, 2, 0, "after");
        transaction
            .open_table(SNAPSHOTS)
            .unwrap()
            .remove(key.as_str())
            .unwrap();
        transaction.commit().unwrap();
        assert_eq!(
            store.operation(&original.id).err().unwrap().code,
            "invalid_state"
        );
        let transaction = store.write().unwrap();
        transaction
            .open_table(CHECKPOINT_INDEX)
            .unwrap()
            .remove(original.checkpoint_id.as_str())
            .unwrap();
        transaction.commit().unwrap();
        assert_eq!(store.pending().err().unwrap().code, "invalid_state");
        assert_eq!(store.require_clean().unwrap_err().code, "invalid_state");
    }
}
