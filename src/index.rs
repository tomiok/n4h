use crate::budget::Budget;
use crate::error::{Error, Result};
use crate::model::{self, IndexReference, SearchResult, Skipped};
use crate::{files, output, search, store::Store};
use clap::Subcommand;
use redb::{
    Database, Durability, MultimapTableDefinition, ReadableDatabase, ReadableTable,
    ReadableTableMetadata, TableDefinition,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata, OpenOptions};
use std::path::{Path, PathBuf};

const META: TableDefinition<&str, &str> = TableDefinition::new("meta");
const FILES: TableDefinition<&str, &str> = TableDefinition::new("files");
const CONTENT: TableDefinition<u64, &[u8]> = TableDefinition::new("content");
const POSTINGS: MultimapTableDefinition<u32, u64> = MultimapTableDefinition::new("postings");
const INDEX_SCHEMA: u32 = 1;
const DEFAULT_QUOTA: u64 = 512 * 1024 * 1024;
const MAX_POSTINGS: u64 = 8_000_000;
const HEADROOM: u64 = 1024 * 1024;
const DISK_RESERVE: u64 = 32 * 1024 * 1024;

#[derive(Subcommand)]
pub enum IndexCommand {
    /// Rebuild a whole-root snapshot; publish only when the entire build succeeds.
    Build {
        /// Physical cache budget in bytes (1 MiB-2 GiB); default 512 MiB.
        #[arg(long, value_parser = clap::value_parser!(u64).range(1_048_576..=2_147_483_648))]
        max_bytes: Option<u64>,
    },
    /// Rewalk ignore rules and paths, reading content only when metadata changed.
    Update {
        /// Read/hash all admitted source files, not just metadata changes.
        #[arg(long)]
        verify: bool,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1_048_576..=2_147_483_648))]
        max_bytes: Option<u64>,
    },
    /// Inspect cache metadata without refreshing sources.
    Status,
    /// Delete only .n4h/index.redb; never memory, results or checkpoints.
    Clear,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Stamp {
    size: u64,
    modified_ns: Option<u128>,
    #[serde(default)]
    unix: Option<UnixStamp>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct UnixStamp {
    device: u64,
    inode: u64,
    ctime: i64,
    ctime_ns: i64,
    mode: u32,
    links: u64,
}

impl Stamp {
    fn from(metadata: &Metadata) -> Self {
        #[cfg(unix)]
        let unix = {
            use std::os::unix::fs::MetadataExt;
            Some(UnixStamp {
                device: metadata.dev(),
                inode: metadata.ino(),
                ctime: metadata.ctime(),
                ctime_ns: metadata.ctime_nsec(),
                mode: metadata.mode(),
                links: metadata.nlink(),
            })
        };
        #[cfg(not(unix))]
        let unix = None;
        Self {
            size: metadata.len(),
            modified_ns: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos()),
            unix,
        }
    }

    fn reusable(&self, other: &Self) -> bool {
        self.modified_ns.is_some() && self == other
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Entry {
    id: u64,
    hash: String,
    stamp: Stamp,
}

#[derive(Default, Serialize, Deserialize)]
struct Counters {
    files_added: usize,
    files_changed: usize,
    files_reused: usize,
    files_deleted: usize,
    files_read: usize,
    bytes_read: u64,
}

#[derive(Serialize, Deserialize)]
struct Info {
    schema: u32,
    index_id: String,
    repository_id: String,
    root: String,
    generation: u64,
    indexed_at: u64,
    verification_mode: String,
    max_bytes: u64,
    next_id: u64,
    files: usize,
    source_bytes: u64,
    posting_pairs: u64,
    skipped_total: usize,
    skipped: Vec<Skipped>,
    counters: Counters,
}

fn invalid(message: &str) -> Error {
    Error::new("invalid_index", message, 3)
        .action("Inspect index status or explicitly clear and rebuild the disposable index.")
}
fn limited(message: &str) -> Error {
    Error::new("index_limit_exceeded", message, 5)
}
fn not_ready() -> Error {
    Error::new(
        "index_not_ready",
        "No committed index snapshot is available.",
        3,
    )
    .action("Run n4h index build from the initialized repository.")
}

fn path(store: &Store) -> PathBuf {
    store.root.join(".n4h/index.redb")
}

fn check_file(path: &Path) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(invalid("Index must be a regular non-symlink file."));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(invalid("Index must not have multiple hardlinks."));
        }
    }
    Ok(true)
}

fn open(store: &Store, create: bool) -> Result<Option<Database>> {
    let path = path(store);
    let exists = check_file(&path)?;
    if !exists && !create {
        return Ok(None);
    }
    let mut builder = Database::builder();
    builder.set_cache_size(8 * 1024 * 1024);
    if exists {
        return Ok(Some(builder.open(&path)?));
    }
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options.open(&path)?.sync_all()?;
    let database = builder.create(&path)?;
    #[cfg(unix)]
    File::open(
        path.parent()
            .ok_or_else(|| invalid("Missing index parent."))?,
    )?
    .sync_all()?;
    Ok(Some(database))
}

fn info(database: &Database, store: &Store) -> Result<Option<Info>> {
    let transaction = database.begin_read()?;
    let table = match transaction.open_table(META) {
        Ok(t) => t,
        Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let Some(value) = table.get("snapshot")? else {
        return Ok(None);
    };
    let info: Info =
        serde_json::from_str(value.value()).map_err(|_| invalid("Invalid index metadata."))?;
    if info.schema != INDEX_SCHEMA
        || info.repository_id != store.repository_id
        || info.root != files::path_text(&store.root)?
    {
        return Err(invalid(
            "Index schema, root or repository identity does not match.",
        ));
    }
    if info.files > model::MAX_FILES
        || info.source_bytes > model::MAX_SEARCH_BYTES
        || info.posting_pairs > MAX_POSTINGS
        || info.generation == 0
        || info.next_id == 0
        || !(HEADROOM..=2 * 1024 * 1024 * 1024).contains(&info.max_bytes)
    {
        return Err(invalid("Index metadata exceeds supported limits."));
    }
    Ok(Some(info))
}

fn capacity(store: &Store, max_bytes: u64, extra: u64) -> Result<()> {
    let current = fs::metadata(path(store)).map(|m| m.len()).unwrap_or(0);
    let reserve = extra.saturating_add(HEADROOM);
    if current.saturating_add(reserve) > max_bytes {
        return Err(Error::new("index_quota_exceeded", "Insufficient index budget for copy-on-write updates and commit headroom. No generation was published.", 5)
            .action("Increase index build/update --max-bytes, or clear and rebuild the disposable cache. Existing durable state is unaffected."));
    }
    if fs2::available_space(&store.root)?.saturating_sub(DISK_RESERVE) < reserve {
        return Err(Error::new(
            "index_quota_exceeded",
            "Insufficient disk space after reserving room for durable state/recovery.",
            5,
        ));
    }
    Ok(())
}

fn grams(bytes: &[u8]) -> BTreeSet<u32> {
    bytes
        .windows(3)
        .map(|w| ((w[0] as u32) << 16) | ((w[1] as u32) << 8) | w[2] as u32)
        .collect()
}

fn scan_failed(message: impl Into<String>) -> Error {
    Error::new("index_scan_failed", message, 3).action(
        "Resolve traversal/read errors and retry; the previous committed snapshot is unchanged.",
    )
}

fn skip(info: &mut Info, path: &str, reason: &str) {
    info.skipped_total += 1;
    if info.skipped.len() < 100 {
        info.skipped.push(Skipped {
            path: path.to_owned(),
            reason: reason.to_owned(),
        });
    }
}

pub fn run(store: &Store, command: IndexCommand, budget: &Budget) -> Result<Value> {
    match command {
        IndexCommand::Build { max_bytes } => update(store, true, true, max_bytes, budget),
        IndexCommand::Update { verify, max_bytes } => {
            update(store, false, verify, max_bytes, budget)
        }
        IndexCommand::Status => status(store, budget),
        IndexCommand::Clear => clear(store, budget),
    }
}

fn update(
    store: &Store,
    rebuild: bool,
    verify: bool,
    max_bytes: Option<u64>,
    budget: &Budget,
) -> Result<Value> {
    let database = open(store, rebuild)?.ok_or_else(not_ready)?;
    let old_info = info(&database, store)?;
    if !rebuild && old_info.is_none() {
        return Err(not_ready());
    }
    let max_bytes = max_bytes
        .or_else(|| old_info.as_ref().map(|i| i.max_bytes))
        .unwrap_or(DEFAULT_QUOTA);
    capacity(store, max_bytes, 0)?;
    let mut old = BTreeMap::<String, Entry>::new();
    if old_info.is_some() {
        let transaction = database.begin_read()?;
        let table = transaction.open_table(FILES)?;
        if table.len()? > model::MAX_FILES as u64 {
            return Err(invalid("Too many indexed files."));
        }
        let mut ids = BTreeSet::new();
        for row in table.iter()? {
            let (key, value) = row?;
            let entry: Entry = serde_json::from_str(value.value())
                .map_err(|_| invalid("Invalid file metadata."))?;
            if entry.id == 0 || !ids.insert(entry.id) {
                return Err(invalid("Duplicate indexed file identity."));
            }
            old.insert(key.value().to_owned(), entry);
        }
    }
    let mut next = Info {
        schema: INDEX_SCHEMA,
        index_id: old_info
            .as_ref()
            .map_or_else(|| model::id("idx"), |i| i.index_id.clone()),
        repository_id: store.repository_id.clone(),
        root: files::path_text(&store.root)?,
        generation: old_info
            .as_ref()
            .map_or(Some(1), |i| i.generation.checked_add(1))
            .ok_or_else(|| invalid("Index generation exhausted."))?,
        indexed_at: model::now(),
        verification_mode: if verify { "full_hash" } else { "metadata_fast" }.to_owned(),
        max_bytes,
        next_id: if rebuild {
            1
        } else {
            old_info.as_ref().map_or(1, |i| i.next_id)
        },
        files: 0,
        source_bytes: 0,
        posting_pairs: if rebuild {
            0
        } else {
            old_info.as_ref().map_or(0, |i| i.posting_pairs)
        },
        skipped_total: 0,
        skipped: Vec::new(),
        counters: Counters::default(),
    };
    // Reevaluate ignore rules and all paths on every update, including removals.
    let mut candidates = BTreeSet::new();
    for entry in search::walk(&store.root) {
        let entry = entry.map_err(|e| scan_failed(e.to_string()))?;
        if let Some(error) = entry.error() {
            return Err(scan_failed(error.to_string()));
        }
        if entry.file_type().is_some_and(|t| t.is_dir()) {
            continue;
        }
        candidates.insert(entry.path().to_path_buf());
        if candidates.len() > model::MAX_FILES {
            return Err(limited("Too many index candidates (maximum 20,000)."));
        }
    }
    let mut transaction = database.begin_write()?;
    transaction.set_durability(Durability::Immediate)?;
    if rebuild {
        transaction.delete_table(FILES)?;
        transaction.delete_table(CONTENT)?;
        transaction.delete_multimap_table(POSTINGS)?;
    }
    {
        let mut file_table = transaction.open_table(FILES)?;
        let mut content = transaction.open_table(CONTENT)?;
        let mut postings = transaction.open_multimap_table(POSTINGS)?;
        let mut seen = BTreeSet::new();
        let mut walked_bytes = 0u64;
        for full in candidates {
            let relative = full
                .strip_prefix(&store.root)
                .map_err(|_| invalid("Index candidate outside root."))?;
            let name =
                files::path_text(relative).map_err(|_| scan_failed("Non-UTF-8 index path."))?;
            let metadata =
                fs::symlink_metadata(&full).map_err(|e| scan_failed(e.to_string()).path(&name))?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                skip(&mut next, &name, "non_regular_or_symlink");
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.nlink() != 1 || metadata.mode() & 0o7000 != 0 {
                    skip(&mut next, &name, "unsupported_file");
                    continue;
                }
            }
            if metadata.len() > model::MAX_FILE_BYTES {
                skip(&mut next, &name, "file_too_large");
                continue;
            }
            walked_bytes = walked_bytes.saturating_add(metadata.len());
            if walked_bytes > model::MAX_SEARCH_BYTES {
                return Err(limited("Index source byte limit exceeded (100 MiB)."));
            }
            let stamp = Stamp::from(&metadata);
            let previous = old.get(&name);
            if !rebuild && !verify && previous.is_some_and(|p| p.stamp.reusable(&stamp)) {
                seen.insert(name);
                next.files += 1;
                next.source_bytes += stamp.size;
                next.counters.files_reused += 1;
                continue;
            }
            next.counters.files_read += 1;
            next.counters.bytes_read += metadata.len();
            let snapshot =
                match files::read_snapshot(&store.root, &name, model::MAX_FILE_BYTES as usize) {
                    Ok(s) => s,
                    Err(e) if matches!(e.code, "unsupported_file" | "file_limit_exceeded") => {
                        skip(&mut next, &name, e.code);
                        continue;
                    }
                    Err(e) => return Err(scan_failed(e.to_string()).path(&name)),
                };
            let after = Stamp::from(
                &fs::symlink_metadata(&full).map_err(|e| scan_failed(e.to_string()).path(&name))?,
            );
            if stamp != after || stamp.size != snapshot.bytes.len() as u64 {
                return Err(scan_failed(
                    "Source metadata changed during index read; retry without concurrent writers.",
                )
                .path(&name));
            }
            let hash = model::hash(&snapshot.bytes);
            let same_content = !rebuild && previous.is_some_and(|p| p.hash == hash);
            if same_content {
                let previous = previous.ok_or_else(|| invalid("Missing previous file entry."))?;
                let updated = Entry {
                    id: previous.id,
                    hash,
                    stamp: stamp.clone(),
                };
                file_table.insert(name.as_str(), serde_json::to_string(&updated)?.as_str())?;
                next.counters.files_reused += 1;
            } else {
                let id = if !rebuild {
                    if let Some(previous) = previous {
                        let bytes = content
                            .get(previous.id)?
                            .ok_or_else(|| invalid("Missing old indexed content."))?
                            .value()
                            .to_vec();
                        for gram in grams(&bytes) {
                            if !postings.remove(gram, previous.id)? {
                                return Err(invalid("Missing old posting."));
                            }
                            next.posting_pairs = next
                                .posting_pairs
                                .checked_sub(1)
                                .ok_or_else(|| invalid("Invalid posting count."))?;
                        }
                        next.counters.files_changed += 1;
                        previous.id
                    } else {
                        next.counters.files_added += 1;
                        let id = next.next_id;
                        next.next_id = next
                            .next_id
                            .checked_add(1)
                            .ok_or_else(|| invalid("Index file IDs exhausted."))?;
                        id
                    }
                } else {
                    if previous.is_some() {
                        next.counters.files_changed += 1;
                    } else {
                        next.counters.files_added += 1;
                    }
                    let id = next.next_id;
                    next.next_id = next
                        .next_id
                        .checked_add(1)
                        .ok_or_else(|| invalid("Index file IDs exhausted."))?;
                    id
                };
                let trigrams = grams(&snapshot.bytes);
                if next.posting_pairs.saturating_add(trigrams.len() as u64) > MAX_POSTINGS {
                    return Err(limited("Index posting-pair limit exceeded (8 million)."));
                }
                // Conservative COW estimate, not a claim of an OS space reservation.
                capacity(
                    store,
                    max_bytes,
                    snapshot.bytes.len() as u64 * 2 + trigrams.len() as u64 * 64,
                )?;
                for gram in trigrams {
                    postings.insert(gram, id)?;
                    next.posting_pairs += 1;
                }
                content.insert(id, snapshot.bytes.as_slice())?;
                let entry = Entry {
                    id,
                    hash,
                    stamp: stamp.clone(),
                };
                file_table.insert(name.as_str(), serde_json::to_string(&entry)?.as_str())?;
            }
            next.files += 1;
            next.source_bytes += snapshot.bytes.len() as u64;
            seen.insert(name);
            crate::fault::failpoint("index_after_file");
        }
        for (name, entry) in &old {
            if seen.contains(name) {
                continue;
            }
            next.counters.files_deleted += 1;
            if !rebuild {
                let bytes = content
                    .get(entry.id)?
                    .ok_or_else(|| invalid("Missing removed-file content."))?
                    .value()
                    .to_vec();
                for gram in grams(&bytes) {
                    if !postings.remove(gram, entry.id)? {
                        return Err(invalid("Missing removed-file posting."));
                    }
                    next.posting_pairs = next
                        .posting_pairs
                        .checked_sub(1)
                        .ok_or_else(|| invalid("Invalid posting count."))?;
                }
                content.remove(entry.id)?;
                file_table.remove(name.as_str())?;
            }
        }
        if next.files > model::MAX_FILES || next.source_bytes > model::MAX_SEARCH_BYTES {
            return Err(limited("Index file/byte limit exceeded."));
        }
        if postings.len()? != next.posting_pairs
            || file_table.len()? != next.files as u64
            || content.len()? != next.files as u64
        {
            return Err(invalid("Index counts and tables disagree."));
        }
    }
    next.indexed_at = model::now();
    transaction
        .open_table(META)?
        .insert("snapshot", serde_json::to_string(&next)?.as_str())?;
    let allocation = transaction.stats()?;
    let allocated_bytes = allocation
        .allocated_pages()
        .saturating_mul(allocation.page_size() as u64);
    if allocated_bytes.saturating_add(HEADROOM) > max_bytes {
        return Err(Error::new(
            "index_quota_exceeded",
            "Staged index pages exceed the cache budget; no generation was published.",
            5,
        ));
    }
    capacity(store, max_bytes, 0)?;
    let receipt = if budget.active() {
        Some(status_value(store, Some(&next), budget, true)?)
    } else {
        None
    };
    crate::fault::failpoint("index_before_commit");
    transaction.commit()?;
    match receipt {
        Some(value) => Ok(value),
        None => status_value(store, Some(&next), budget, false),
    }
}

fn status_value(
    store: &Store,
    info: Option<&Info>,
    budget: &Budget,
    before_commit: bool,
) -> Result<Value> {
    let mut value = output::success("index_status");
    value["repository_id"] = json!(store.repository_id);
    value["ready"] = json!(info.is_some());
    value["path"] = json!(path(store));
    value["physical_bytes"] = json!(fs::metadata(path(store)).map(|m| m.len()).unwrap_or(0));
    if let Some(info) = info {
        value["index_id"] = json!(info.index_id);
        value["generation"] = json!(info.generation);
        value["indexed_at"] = json!(info.indexed_at);
        value["verification_mode"] = json!(info.verification_mode);
        value["files_indexed"] = json!(info.files);
        value["source_bytes"] = json!(info.source_bytes);
        value["posting_pairs"] = json!(info.posting_pairs);
        value["max_bytes"] = json!(info.max_bytes);
        value["counters"] = json!(info.counters);
        value["skipped_total"] = json!(info.skipped_total);
        value["skipped"] = output::skipped_summary(&info.skipped);
        value["skipped_details_truncated"] = json!(info.skipped_total > 10);
    }
    value["scope"] = json!("indexed_snapshot");
    if before_commit {
        value["physical_bytes_observed_before_commit"] = json!(true);
    }
    value["warning"] = json!(
        "Cached content is not a live freshness guarantee. Update reevaluates paths/ignore rules; --verify rereads all admitted files. Cache is not encrypted."
    );
    if let Some(value) = budget.fit(value)? {
        return Ok(value);
    }
    let mut compact = output::success("index_status");
    compact["repository_id"] = json!(store.repository_id);
    compact["ready"] = json!(info.is_some());
    compact["scope"] = json!("indexed_snapshot");
    compact["details_truncated"] = json!(true);
    compact["next_action"] = json!(
        "Use index status with a larger budget for details; this cache is not a live freshness guarantee."
    );
    if let Some(info) = info {
        compact["index_id"] = json!(info.index_id);
        compact["generation"] = json!(info.generation);
        compact["files_indexed"] = json!(info.files);
    }
    budget.finish(compact)
}

fn status(store: &Store, budget: &Budget) -> Result<Value> {
    let database = open(store, false)?;
    let info = database
        .as_ref()
        .map(|db| info(db, store))
        .transpose()?
        .flatten();
    status_value(store, info.as_ref(), budget, false)
}

fn clear(store: &Store, budget: &Budget) -> Result<Value> {
    let path = path(store);
    let removed = check_file(&path)?;
    let mut value = output::success("index_cleared");
    value["removed"] = json!(removed);
    value["path"] = json!(path);
    value["durable_state_modified"] = json!(false);
    value["secure_erasure"] = json!(false);
    let receipt = budget.finish(value)?;
    if removed {
        fs::remove_file(&path)?;
        #[cfg(unix)]
        File::open(
            path.parent()
                .ok_or_else(|| invalid("Missing index parent."))?,
        )?
        .sync_all()?;
    }
    Ok(receipt)
}

pub fn find(
    store: &Store,
    cwd: &Path,
    pattern: &str,
    paths: &[PathBuf],
    budget: &Budget,
) -> Result<SearchResult> {
    let matcher = search::matcher(pattern, false)?;
    let database = open(store, false)?.ok_or_else(not_ready)?;
    let info = info(&database, store)?.ok_or_else(not_ready)?;
    let default = [PathBuf::from(".")];
    let paths = if paths.is_empty() {
        &default[..]
    } else {
        paths
    };
    let scopes = paths
        .iter()
        .map(|p| files::relative_scope(&store.root, cwd, p))
        .collect::<Result<Vec<_>>>()?;
    let transaction = database.begin_read()?;
    let table = transaction.open_table(FILES)?;
    let content = transaction.open_table(CONTENT)?;
    let postings = transaction.open_multimap_table(POSTINGS)?;
    if table.len()? != info.files as u64
        || content.len()? != info.files as u64
        || postings.len()? != info.posting_pairs
    {
        return Err(invalid("Index counts and tables disagree."));
    }
    let mut files_in_scope = BTreeMap::new();
    for row in table.iter()? {
        let (name, payload) = row?;
        if !scopes
            .iter()
            .any(|scope| Path::new(name.value()).starts_with(scope))
        {
            continue;
        }
        let entry: Entry = serde_json::from_str(payload.value())
            .map_err(|_| invalid("Invalid indexed file metadata."))?;
        if entry.id == 0 || entry.stamp.size > model::MAX_FILE_BYTES {
            return Err(invalid("Invalid indexed file identity or size."));
        }
        files_in_scope.insert(name.value().to_owned(), entry);
    }
    let trigrams = grams(pattern.as_bytes());
    let mut counts = Vec::new();
    for gram in &trigrams {
        counts.push((postings.get(*gram)?.len(), *gram));
    }
    counts.sort_unstable();
    let mut candidate_ids: Option<BTreeSet<u64>> = None;
    for (_, gram) in counts {
        let ids: BTreeSet<_> = postings
            .get(gram)?
            .map(|id| id.map(|v| v.value()).map_err(Error::from))
            .collect::<Result<_>>()?;
        candidate_ids = Some(match candidate_ids {
            None => ids,
            Some(previous) => previous.intersection(&ids).copied().collect(),
        });
        if candidate_ids.as_ref().is_some_and(BTreeSet::is_empty) {
            break;
        }
    }
    let scope_files = files_in_scope.len();
    let candidates: Vec<_> = files_in_scope
        .into_iter()
        .filter(|(_, entry)| {
            candidate_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&entry.id))
        })
        .collect();
    let mut result = SearchResult {
        id: model::id("r"),
        created_at: model::now(),
        pattern: pattern.to_owned(),
        mode: "literal".to_owned(),
        paths: scopes
            .iter()
            .map(|p| {
                if p.as_os_str().is_empty() {
                    Ok(".".to_owned())
                } else {
                    files::path_text(p)
                }
            })
            .collect::<Result<_>>()?,
        files_scanned: 0,
        bytes_scanned: 0,
        coverage_complete: true,
        skipped_total: info.skipped_total,
        skipped: info.skipped,
        matches: Vec::new(),
        index: Some(IndexReference {
            index_id: info.index_id,
            generation: info.generation,
            indexed_at: info.indexed_at,
            files_total: info.files,
            scope_files,
            candidate_files: candidates.len(),
            verification_mode: info.verification_mode,
        }),
    };
    for (name, entry) in candidates {
        let bytes = content
            .get(entry.id)?
            .ok_or_else(|| invalid("Missing indexed content."))?;
        if bytes.value().len() as u64 != entry.stamp.size
            || model::hash(bytes.value()) != entry.hash
        {
            return Err(invalid("Indexed bytes do not match the file manifest."));
        }
        let text =
            std::str::from_utf8(bytes.value()).map_err(|_| invalid("Invalid UTF-8 in index."))?;
        result.files_scanned += 1;
        result.bytes_scanned += bytes.value().len() as u64;
        search::collect_matches(&mut result, &name, &entry.hash, text, &matcher)?;
    }
    if budget.active() {
        output::result_page(store, &result, None, 1, budget)?;
    }
    store.save_result(&result)?;
    Ok(result)
}
