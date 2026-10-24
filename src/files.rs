use crate::error::{Error, Result};
use crate::model::hash;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

pub struct Snapshot {
    pub bytes: Vec<u8>,
    pub mode: u32,
}

pub fn relative_input(root: &Path, cwd: &Path, input: &Path) -> Result<PathBuf> {
    let relative = relative_scope(root, cwd, input)?;
    checked_path(root, &relative)?;
    Ok(relative)
}

/// Snapshot scopes are lexical: querying deleted paths must not touch the disk.
pub fn relative_scope(root: &Path, cwd: &Path, input: &Path) -> Result<PathBuf> {
    let candidate = if input.is_absolute() {
        input.to_path_buf()
    } else {
        cwd.join(input)
    };
    let mut normalized = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    let relative = normalized.strip_prefix(root).map_err(|_| {
        Error::invalid("Search paths must remain inside the initialized repository.")
    })?;
    if relative
        .components()
        .any(|c| matches!(c, Component::Normal(n) if n == ".git" || n == ".n4h"))
    {
        return Err(Error::new(
            "unsupported_file",
            "Git and n4h state are not source targets.",
            5,
        ));
    }
    Ok(relative.to_path_buf())
}

pub fn path_text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new("unsupported_file", "Non-UTF-8 paths are not supported.", 5))
}

pub fn checked_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut full = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(Error::invalid(
                "Only repository-relative normal path components are allowed.",
            ));
        };
        if name == ".git" || name == ".n4h" {
            return Err(Error::new(
                "unsupported_file",
                "Git and n4h state are not source targets.",
                5,
            ));
        }
        full.push(name);
        if fs::symlink_metadata(&full)?.file_type().is_symlink() {
            return Err(Error::new(
                "unsupported_file",
                "Symlink paths are not supported.",
                5,
            ));
        }
    }
    Ok(full)
}

pub fn read_snapshot(root: &Path, relative: &str, limit: usize) -> Result<Snapshot> {
    let full = checked_path(root, Path::new(relative))?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(full)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(
            Error::new("unsupported_file", "Only regular files are supported.", 5).path(relative),
        );
    }
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 || metadata.mode() & 0o7000 != 0 {
            return Err(Error::new(
                "unsupported_file",
                "Hardlinks and special permission bits are unsupported.",
                5,
            )
            .path(relative));
        }
        metadata.mode() & 0o777
    };
    #[cfg(not(unix))]
    let mode = if metadata.permissions().readonly() {
        0o444
    } else {
        0o644
    };
    if metadata.len() > limit as u64 {
        return Err(Error::new(
            "file_limit_exceeded",
            "File exceeds the configured byte limit.",
            5,
        )
        .path(relative));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(
            Error::new("file_limit_exceeded", "File grew beyond the byte limit.", 5).path(relative),
        );
    }
    if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
        return Err(Error::new(
            "unsupported_file",
            "Only UTF-8 text without NUL bytes is supported.",
            5,
        )
        .path(relative));
    }
    Ok(Snapshot { bytes, mode })
}

pub fn verify(root: &Path, path: &str, expected: &str, mode: Option<u32>) -> Result<Snapshot> {
    let snapshot = read_snapshot(root, path, crate::model::MAX_PLAN_BYTES)
        .map_err(|_| Error::conflict(path))?;
    if hash(&snapshot.bytes) != expected || mode.is_some_and(|m| m != snapshot.mode) {
        return Err(Error::conflict(path));
    }
    Ok(snapshot)
}

pub fn replace(root: &Path, path: &str, expected: &str, bytes: &[u8], mode: u32) -> Result<()> {
    if mode & 0o222 == 0 {
        return Err(
            Error::new("unsupported_file", "Read-only files are not edited.", 5).path(path),
        );
    }
    let full = checked_path(root, Path::new(path))?;
    let parent = full
        .parent()
        .ok_or_else(|| Error::invalid("Missing parent directory."))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(mode))?;
    }
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    verify(root, path, expected, Some(mode))?;
    temporary.persist(&full).map_err(|e| Error::from(e.error))?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}
