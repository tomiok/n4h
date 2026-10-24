use serde_json::{Value, json};
use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
    pub exit: i32,
    pub paths: Vec<String>,
    pub next_actions: Vec<String>,
    pub operation_id: Option<String>,
}

impl Error {
    pub fn new(code: &'static str, message: impl Into<String>, exit: i32) -> Self {
        Self {
            code,
            message: message.into(),
            exit,
            paths: Vec::new(),
            next_actions: Vec::new(),
            operation_id: None,
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_arguments", message, 2)
    }

    pub fn conflict(path: &str) -> Self {
        Self::new(
            "file_changed",
            "The file no longer matches the reviewed version.",
            3,
        )
        .path(path)
        .action("Run find again and review a new plan; do not overwrite external changes.")
    }

    pub fn path(mut self, path: &str) -> Self {
        if !self.paths.iter().any(|p| p == path) {
            self.paths.push(path.to_owned());
        }
        self
    }

    pub fn action(mut self, action: impl Into<String>) -> Self {
        self.next_actions.push(action.into());
        self
    }

    pub fn operation(mut self, id: &str) -> Self {
        self.operation_id = Some(id.to_owned());
        self.next_actions.push(format!(
            "Inspect with n4h recover {id}; request rollback explicitly if appropriate."
        ));
        self
    }

    pub fn json(&self) -> Value {
        // Argument parsers and OS errors can echo arbitrarily large user input.
        // Bound errors too, without cutting UTF-8 or serialized JSON.
        let short = |text: &str| text.chars().take(512).collect::<String>();
        json!({
            "schema_version": crate::SCHEMA_VERSION,
            "ok": false,
            "error": {
                "code": self.code,
                "message": self.message.chars().take(2048).collect::<String>(),
                "paths": self.paths.iter().take(4).map(|p| short(p)).collect::<Vec<_>>(),
                "paths_total": self.paths.len(),
                "paths_truncated": self.paths.len() > 4 || self.paths.iter().any(|p| p.chars().count() > 512),
                "retryable": false,
                "next_actions": self.next_actions.iter().take(4).map(|a| short(a)).collect::<Vec<_>>(),
                "operation_id": self.operation_id
            }
        })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::new("io_error", error.to_string(), 4)
    }
}

macro_rules! storage_errors {
    ($($kind:ty),+ $(,)?) => {
        $(impl From<$kind> for Error {
            fn from(error: $kind) -> Self {
                Self::new("storage_error", error.to_string(), 4)
            }
        })+
    };
}

storage_errors!(
    redb::DatabaseError,
    redb::TransactionError,
    redb::TableError,
    redb::StorageError,
    redb::CommitError,
    redb::SetDurabilityError,
);

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::new("storage_error", error.to_string(), 4)
    }
}
