use thiserror::Error;

/// Errors never carry user content (titles, attendees, tokens). Messages are
/// safe to log.
#[derive(Debug, Error)]
pub enum Error {
    #[error("database error: {0}")]
    Database(String),
    #[error("database could not be opened (wrong key or corrupt file)")]
    DatabaseLocked,
    #[error("secret store error: {0}")]
    Secret(String),
    #[error("invalid input: {0}")]
    Invalid(&'static str),
    #[error("provider error: {0}")]
    Provider(String),
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        // rusqlite errors can echo SQL text but never row contents.
        Error::Database(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
