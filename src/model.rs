use serde::{Deserialize, Serialize};

pub const RESULT_TTL: u64 = 24 * 60 * 60;
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_PLAN_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_PLAN_FILES: usize = 50;
pub const MAX_SEARCH_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_FILES: usize = 20_000;
pub const MAX_MATCHES: usize = 20_000;
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;
pub const MAX_STORE_BYTES: u64 = 200 * 1024 * 1024;

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}

pub fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub created_at: u64,
    pub pattern: String,
    pub mode: String,
    pub paths: Vec<String>,
    pub files_scanned: usize,
    pub bytes_scanned: u64,
    pub coverage_complete: bool,
    pub skipped_total: usize,
    pub skipped: Vec<Skipped>,
    pub matches: Vec<Match>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<IndexReference>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexReference {
    pub index_id: String,
    pub generation: u64,
    pub indexed_at: u64,
    pub files_total: usize,
    pub scope_files: usize,
    pub candidate_files: usize,
    pub verification_mode: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Skipped {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Match {
    pub match_id: String,
    pub path: String,
    pub file_hash: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub line: usize,
    pub column_byte: usize,
    pub text: String,
    pub text_truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ast: Option<AstMatch>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AstMatch {
    pub node_kind: String,
    pub captures: std::collections::BTreeMap<String, AstCapture>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AstCapture {
    pub start_byte: usize,
    pub end_byte: usize,
    pub multiple: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub before_hash: String,
    pub after_hash: String,
    pub mode: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlanFile {
    #[serde(flatten)]
    pub change: FileChange,
    pub match_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(from = "PlanWire")]
pub struct Plan {
    pub id: String,
    pub created_at: u64,
    pub result_id: String,
    pub files: Vec<PlanFile>,
    pub unchanged_matches: usize,
    pub snapshot_version: u8,
}

#[derive(Clone, Debug)]
pub struct FileSnapshot {
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}

pub struct PlanData {
    pub plan: Plan,
    pub snapshots: Vec<FileSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "OperationWire")]
pub struct Operation {
    pub id: String,
    pub checkpoint_id: String,
    pub created_at: u64,
    pub kind: String,
    pub source_id: String,
    pub files: Vec<FileChange>,
    pub snapshot_version: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileProgress {
    Unknown,
    Prepared,
    Applied,
    Restored,
}

pub struct OperationData {
    pub operation: Operation,
    pub status: String,
    pub snapshots: Vec<FileSnapshot>,
    pub progress: Vec<FileProgress>,
}

fn indexed_version() -> u8 {
    2
}

// Read old one-file redb records without rewriting their snapshot keys or IDs.
// Denying unknown legacy fields prevents an invalid files array falling back to
// a legacy interpretation and silently changing an operation's scope.
#[derive(Deserialize)]
#[serde(untagged)]
enum PlanWire {
    Current(CurrentPlan),
    Legacy(LegacyPlan),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentPlan {
    id: String,
    created_at: u64,
    result_id: String,
    files: Vec<PlanFile>,
    #[serde(default)]
    unchanged_matches: usize,
    #[serde(default = "indexed_version")]
    snapshot_version: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyPlan {
    id: String,
    created_at: u64,
    result_id: String,
    path: String,
    match_ids: Vec<String>,
    before_hash: String,
    after_hash: String,
    mode: u32,
}

impl From<PlanWire> for Plan {
    fn from(wire: PlanWire) -> Self {
        match wire {
            PlanWire::Current(p) => Self {
                id: p.id,
                created_at: p.created_at,
                result_id: p.result_id,
                files: p.files,
                unchanged_matches: p.unchanged_matches,
                snapshot_version: p.snapshot_version,
            },
            PlanWire::Legacy(p) => Self {
                id: p.id,
                created_at: p.created_at,
                result_id: p.result_id,
                files: vec![PlanFile {
                    change: FileChange {
                        path: p.path,
                        before_hash: p.before_hash,
                        after_hash: p.after_hash,
                        mode: p.mode,
                    },
                    match_ids: p.match_ids,
                }],
                unchanged_matches: 0,
                snapshot_version: 1,
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OperationWire {
    Current(CurrentOperation),
    Legacy(LegacyOperation),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentOperation {
    id: String,
    checkpoint_id: String,
    created_at: u64,
    kind: String,
    source_id: String,
    files: Vec<FileChange>,
    #[serde(default = "indexed_version")]
    snapshot_version: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyOperation {
    id: String,
    checkpoint_id: String,
    created_at: u64,
    kind: String,
    source_id: String,
    path: String,
    before_hash: String,
    after_hash: String,
    mode: u32,
}

impl From<OperationWire> for Operation {
    fn from(wire: OperationWire) -> Self {
        match wire {
            OperationWire::Current(o) => Self {
                id: o.id,
                checkpoint_id: o.checkpoint_id,
                created_at: o.created_at,
                kind: o.kind,
                source_id: o.source_id,
                files: o.files,
                snapshot_version: o.snapshot_version,
            },
            OperationWire::Legacy(o) => Self {
                id: o.id,
                checkpoint_id: o.checkpoint_id,
                created_at: o.created_at,
                kind: o.kind,
                source_id: o.source_id,
                files: vec![FileChange {
                    path: o.path,
                    before_hash: o.before_hash,
                    after_hash: o.after_hash,
                    mode: o.mode,
                }],
                snapshot_version: 1,
            },
        }
    }
}

pub fn snapshot_key(id: &str, version: u8, index: usize, suffix: &str) -> String {
    if version == 1 {
        format!("{id}:{suffix}")
    } else {
        format!("{id}:{index}:{suffix}")
    }
}

pub fn validate_changes<'a>(
    files: impl IntoIterator<Item = &'a FileChange>,
    version: u8,
) -> crate::error::Result<()> {
    use crate::error::Error;
    use std::collections::BTreeSet;
    use std::path::{Component, Path, PathBuf};
    let files: Vec<_> = files.into_iter().collect();
    let bad = || {
        Error::new(
            "invalid_state",
            "Invalid stored file set or snapshot layout.",
            4,
        )
    };
    if files.is_empty()
        || files.len() > MAX_PLAN_FILES
        || !matches!(version, 1 | 2)
        || (version == 1 && files.len() != 1)
    {
        return Err(bad());
    }
    let mut seen = BTreeSet::new();
    for file in files {
        let path = Path::new(&file.path);
        let normal: PathBuf = path.components().collect();
        if file.path.is_empty()
            || normal.to_str() != Some(&file.path)
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(n) if n != ".n4h" && n != ".git"))
            || !seen.insert(&file.path)
            || file.mode & !0o777 != 0
            || file.mode & 0o222 == 0
            || file.before_hash == file.after_hash
        {
            return Err(bad());
        }
    }
    Ok(())
}

pub fn validate_snapshots<'a>(
    files: impl IntoIterator<Item = &'a FileChange>,
    snapshots: &[FileSnapshot],
    version: u8,
) -> crate::error::Result<()> {
    use crate::error::Error;
    let files: Vec<_> = files.into_iter().collect();
    validate_changes(files.iter().copied(), version)?;
    let bad = || {
        Error::new(
            "invalid_state",
            "Stored snapshots failed integrity or size validation.",
            4,
        )
    };
    if files.len() != snapshots.len() || snapshot_bytes(snapshots) > MAX_PLAN_BYTES {
        return Err(bad());
    }
    for (file, snapshot) in files.into_iter().zip(snapshots) {
        if hash(&snapshot.before) != file.before_hash
            || hash(&snapshot.after) != file.after_hash
            || std::str::from_utf8(&snapshot.before).is_err()
            || std::str::from_utf8(&snapshot.after).is_err()
            || snapshot.before.contains(&0)
            || snapshot.after.contains(&0)
        {
            return Err(bad());
        }
    }
    Ok(())
}

pub fn snapshot_bytes(snapshots: &[FileSnapshot]) -> usize {
    snapshots.iter().fold(0usize, |n, s| {
        n.saturating_add(s.before.len())
            .saturating_add(s.after.len())
    })
}
