pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Database operation failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Could not find the local data directory. You can set NOTEON_DATA_DIR.")]
    DataDirectory,
    #[error("{0}")]
    Invalid(String),
    #[error("The background worker is unavailable.")]
    WorkerUnavailable,
}
