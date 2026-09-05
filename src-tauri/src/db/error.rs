//! Structured persistence error. Raw `rusqlite` / pool errors never escape the
//! `db` module — they are mapped here, and `db` methods return `DbError`. At the
//! IPC boundary `DbError` maps into [`AppError`] (`docs/decisions/0002`, `0009`).

use crate::ipc::AppError;

/// A persistence-layer failure.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// A row that was required does not exist.
    #[error("not found")]
    NotFound,

    /// A uniqueness / foreign-key / check constraint was violated.
    #[error("conflict: {0}")]
    Conflict(String),

    /// The database was locked past `busy_timeout`.
    #[error("database busy")]
    Busy,

    /// The database file is corrupt or is not a database.
    #[error("database corruption: {0}")]
    Corruption(String),

    /// A migration could not be applied (or the DB is newer than this build).
    #[error("migration failed: {0}")]
    Migration(String),

    /// A filesystem error around the DB (backup, directory creation).
    #[error("db io: {0}")]
    Io(String),

    /// Anything else; the full detail is logged, not shown.
    #[error("db error: {0}")]
    Other(String),
}

impl From<rusqlite::Error> for DbError {
    fn from(err: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode;
        match &err {
            rusqlite::Error::QueryReturnedNoRows => Self::NotFound,
            rusqlite::Error::SqliteFailure(e, msg) => match e.code {
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => Self::Busy,
                ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase => {
                    Self::Corruption(msg.clone().unwrap_or_else(|| err.to_string()))
                }
                ErrorCode::ConstraintViolation => {
                    Self::Conflict(msg.clone().unwrap_or_else(|| err.to_string()))
                }
                _ => Self::Other(err.to_string()),
            },
            _ => Self::Other(err.to_string()),
        }
    }
}

impl From<deadpool_sqlite::PoolError> for DbError {
    fn from(err: deadpool_sqlite::PoolError) -> Self {
        Self::Other(format!("pool: {err}"))
    }
}

impl From<deadpool_sqlite::InteractError> for DbError {
    fn from(err: deadpool_sqlite::InteractError) -> Self {
        Self::Other(format!("interact: {err}"))
    }
}

impl From<std::io::Error> for DbError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl From<refinery::Error> for DbError {
    fn from(err: refinery::Error) -> Self {
        Self::Migration(err.to_string())
    }
}

impl From<DbError> for AppError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::NotFound => Self::NotFound("database row".to_owned()),
            DbError::Conflict(m) => Self::Conflict(m),
            DbError::Busy => Self::ResourceExhausted("database is busy".to_owned()),
            DbError::Corruption(m) => {
                tracing::error!(detail = %m, "database corruption");
                Self::Internal
            }
            DbError::Migration(m) => {
                tracing::error!(detail = %m, "database migration failure");
                Self::Internal
            }
            DbError::Io(m) => {
                tracing::error!(detail = %m, "database io failure");
                Self::Internal
            }
            DbError::Other(m) => {
                tracing::error!(detail = %m, "database error");
                Self::Internal
            }
        }
    }
}

/// Convenience alias for `db` module results.
pub type DbResult<T> = Result<T, DbError>;
