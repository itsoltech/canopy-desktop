//! Electron-compatible ordinary preferences. Secrets and unknown keys are never exposed or rewritten.
mod contract;
mod database;
mod worker;
pub use contract::*;
pub use database::{Access, ImportReport};
use std::fmt;
pub use worker::SettingsClient;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    DataDirectory,
    UnsupportedSchema,
    InvalidSchema,
    InvalidProjectState,
    InvalidToolCatalog,
    InvalidIntegrations,
    InvalidTaskDrafts,
    UnsupportedKey,
    InvalidValue(PreferenceKey),
    InvalidToolId,
    DestinationExists,
    BackupTimeout,
    QueueFull,
    WorkerStopped,
    ReadOnly,
    BatchTooLarge,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SQLite errors can contain trigger messages supplied by the database.
        // Keep display text free of raw values and database-authored messages.
        match self {
            Self::Sqlite(_) => write!(f, "SQLite operation failed"),
            Self::Io(e) => write!(f, "File operation failed: {}", e.kind()),
            Self::DataDirectory => write!(
                f,
                "Application data directory is unavailable or could not be protected"
            ),
            Self::UnsupportedSchema => {
                write!(f, "Unsupported database schema; no migrations were applied")
            }
            Self::InvalidTaskDrafts => write!(
                f,
                "Task drafts are invalid or from a newer version; existing data was preserved."
            ),
            Self::InvalidIntegrations => {
                write!(f, "Unsupported or invalid integrations configuration")
            }
            Self::InvalidToolCatalog => write!(f, "Unsupported or invalid tool catalog"),
            Self::InvalidProjectState => write!(f, "Unsupported or invalid project state"),
            Self::InvalidSchema => write!(f, "Database does not match the preferences contract"),
            Self::UnsupportedKey => write!(
                f,
                "Preference key is not in the ordinary-settings allowlist"
            ),
            Self::InvalidValue(k) => write!(f, "Invalid value for {}", k.as_str()),
            Self::InvalidToolId => write!(f, "Tool ID cannot be empty"),
            Self::DestinationExists => {
                write!(f, "Destination already exists; refusing to overwrite it")
            }
            Self::BackupTimeout => write!(f, "SQLite backup timed out"),
            Self::QueueFull => write!(f, "Settings queue is full; retry later"),
            Self::WorkerStopped => write!(f, "Settings worker stopped"),
            Self::ReadOnly => write!(f, "Settings connection is read-only"),
            Self::BatchTooLarge => write!(f, "Settings batch exceeds the limit of 64 changes"),
        }
    }
}
impl std::error::Error for Error {}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
