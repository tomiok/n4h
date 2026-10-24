use super::*;
use crate::memory::{MAX_MEMORY_BYTES, MAX_NOTES, Note, validate_key, validate_text};

fn generation(transaction: &ReadTransaction) -> Result<u64> {
    let meta = transaction.open_table(META)?;
    let value = meta.get("memory_generation")?;
    match value {
        Some(value) => value
            .value()
            .parse()
            .map_err(|_| invalid_state("Invalid memory generation.")),
        None if meta
            .get("schema")?
            .is_some_and(|v| matches!(v.value(), "2" | "3")) =>
        {
            Ok(0)
        }
        None => Err(invalid_state("Missing memory generation.")),
    }
}

fn load_notes(transaction: &ReadTransaction, generation: u64) -> Result<Vec<Note>> {
    let table = match transaction.open_table(NOTES) {
        Ok(table) => table,
        Err(redb::TableError::TableDoesNotExist(_)) => {
            let meta = transaction.open_table(META)?;
            if generation == 0
                && meta
                    .get("schema")?
                    .is_some_and(|v| matches!(v.value(), "2" | "3"))
            {
                return Ok(Vec::new());
            }
            return Err(invalid_state("Missing memory table."));
        }
        Err(error) => return Err(error.into()),
    };
    if table.len()? > MAX_NOTES as u64 {
        return Err(invalid_state("Too many stored notes."));
    }
    let mut notes = Vec::new();
    let mut bytes = 0usize;
    for row in table.iter()? {
        let (key, payload) = row?;
        // Validate individual records before exposing agent-authored data.
        let note: Note = serde_json::from_str(payload.value())?;
        if note.key != key.value()
            || validate_key(&note.key).is_err()
            || validate_text(&note.text).is_err()
            || note.origin != "agent_note"
            || note.revision == 0
            || note.revision > generation
            || note.updated_at < note.created_at
        {
            return Err(invalid_state("Invalid memory note or revision."));
        }
        bytes = bytes
            .saturating_add(note.key.len())
            .saturating_add(note.text.len());
        if bytes > MAX_MEMORY_BYTES {
            return Err(invalid_state("Stored memory exceeds its byte limit."));
        }
        notes.push(note);
    }
    Ok(notes)
}

fn missing_note() -> Error {
    Error::new(
        "note_not_found",
        "No note with this key exists in this repository.",
        3,
    )
}

fn revision_conflict(current: u64) -> Error {
    Error::new(
        "revision_conflict",
        format!("The note has revision {current}; the requested revision is stale."),
        3,
    )
    .action("Read mem get for this key, reconcile the text and retry with its current revision.")
}

impl Store {
    pub fn memory_snapshot(&self) -> Result<(u64, Vec<Note>)> {
        let transaction = self.database.begin_read()?;
        let generation = generation(&transaction)?;
        Ok((generation, load_notes(&transaction, generation)?))
    }

    pub fn note(&self, key: &str) -> Result<Note> {
        validate_key(key)?;
        self.memory_snapshot()?
            .1
            .into_iter()
            .find(|n| n.key == key)
            .ok_or_else(missing_note)
    }

    pub fn put_note(
        &self,
        key: &str,
        text: &str,
        expected: Option<u64>,
        preflight: impl Fn(&Note) -> Result<()>,
    ) -> Result<Note> {
        validate_key(key)?;
        validate_text(text)?;
        let (generation, notes) = self.memory_snapshot()?;
        let existing = notes.iter().find(|n| n.key == key);
        match (existing, expected) {
            (Some(_), None) => {
                return Err(Error::new(
                    "note_exists",
                    "Creating a note cannot overwrite an existing key.",
                    3,
                )
                .action("Use mem get, then update with --if-revision and the current revision."));
            }
            (Some(note), Some(revision)) if note.revision != revision => {
                return Err(revision_conflict(note.revision));
            }
            (None, Some(_)) => return Err(missing_note()),
            _ => {}
        }
        if existing.is_none() && notes.len() >= MAX_NOTES {
            return Err(Error::new(
                "memory_limit_exceeded",
                "A repository can contain at most 1024 notes.",
                5,
            ));
        }
        let used: usize = notes.iter().map(|n| n.key.len() + n.text.len()).sum();
        let old = existing.map_or(0, |n| n.key.len() + n.text.len());
        if used - old + key.len() + text.len() > MAX_MEMORY_BYTES {
            return Err(Error::new(
                "memory_limit_exceeded",
                "Combined note keys and text exceed 4 MiB.",
                5,
            ));
        }
        let revision = generation
            .checked_add(1)
            .ok_or_else(|| invalid_state("Memory revision counter exhausted."))?;
        let now = model::now();
        let note = Note {
            key: key.to_owned(),
            text: text.to_owned(),
            created_at: existing.map_or(now, |n| n.created_at),
            updated_at: existing.map_or(now, |n| now.max(n.updated_at)),
            revision,
            origin: "agent_note".to_owned(),
        };
        let payload = serde_json::to_string(&note)?;
        preflight(&note)?;
        self.capacity(payload.len() as u64 * 2)?;
        // The process-wide repository lock protects the read/check/write sequence.
        // A monotonic generation also avoids revision reuse after delete/recreate.
        let transaction = self.write()?;
        transaction
            .open_table(NOTES)?
            .insert(key, payload.as_str())?;
        transaction
            .open_table(META)?
            .insert("memory_generation", revision.to_string().as_str())?;
        crate::fault::failpoint("before_memory_commit");
        transaction.commit()?;
        Ok(note)
    }

    pub fn forget_note(
        &self,
        key: &str,
        expected: u64,
        preflight: impl Fn(u64) -> Result<()>,
    ) -> Result<u64> {
        validate_key(key)?;
        let (generation, notes) = self.memory_snapshot()?;
        let note = notes
            .iter()
            .find(|n| n.key == key)
            .ok_or_else(missing_note)?;
        if note.revision != expected {
            return Err(revision_conflict(note.revision));
        }
        let next = generation
            .checked_add(1)
            .ok_or_else(|| invalid_state("Memory revision counter exhausted."))?;
        preflight(note.revision)?;
        // Do not enforce the repository quota on logical deletion: forgetting can
        // release live data at the quota, though the commit still needs disk space.
        let transaction = self.write()?;
        transaction.open_table(NOTES)?.remove(key)?;
        transaction
            .open_table(META)?
            .insert("memory_generation", next.to_string().as_str())?;
        crate::fault::failpoint("before_memory_commit");
        transaction.commit()?;
        Ok(note.revision)
    }
}
