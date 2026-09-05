//! SQLite persistence — the Rust-owned data layer (`CLAUDE.md` Article I,
//! ADR-0009). The frontend and subprocesses never touch the database.
//!
//! Topology: a **writer pool of one** (every write serialized — no `SQLITE_BUSY`
//! from our own code) plus a **reader pool of four** (`query_only`). WAL,
//! `foreign_keys`, `busy_timeout`, `synchronous = NORMAL` on every connection.
//!
//! Migrations are `refinery`, forward-only, grouped in one transaction, with a
//! verified `VACUUM INTO` backup taken before any pending migration runs.

pub mod error;
#[cfg(test)]
mod tests;

use std::fs;
use std::path::{Path, PathBuf};

use deadpool_sqlite::{Config as PoolConfig, Runtime};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub use error::{DbError, DbResult};

/// How many pre-migration backups to keep.
const KEEP_BACKUPS: usize = 5;
/// Reader pool size.
const READER_POOL: usize = 4;

mod embedded {
    refinery::embed_migrations!("src/db/migrations");
}

/// The persistence layer. Held in Tauri managed state.
#[derive(Debug)]
pub struct Db {
    writer: deadpool_sqlite::Pool,
    reader: deadpool_sqlite::Pool,
    backup_dir: PathBuf,
}

fn apply_pragmas(conn: &Connection, read_only: bool) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "wal_autocheckpoint", 1000)?;
    if read_only {
        conn.pragma_update(None, "query_only", "ON")?;
    }
    Ok(())
}

fn build_pool(db_path: &Path, max_size: usize, read_only: bool) -> DbResult<deadpool_sqlite::Pool> {
    let cfg = PoolConfig::new(db_path);
    let pool = cfg
        .builder(Runtime::Tokio1)
        .map_err(|e| DbError::Other(format!("pool builder: {e}")))?
        .max_size(max_size)
        .post_create(deadpool_sqlite::Hook::async_fn(move |obj, _| {
            Box::pin(async move {
                obj.interact(move |conn| apply_pragmas(conn, read_only))
                    .await
                    .map_err(|e| deadpool_sqlite::HookError::message(e.to_string()))?
                    .map_err(|e| deadpool_sqlite::HookError::message(e.to_string()))?;
                Ok(())
            })
        }))
        .build()
        .map_err(|e| DbError::Other(format!("build pool: {e}")))?;
    Ok(pool)
}

impl Db {
    /// Open (creating if absent) the database at `db_path`, run a pre-flight
    /// integrity check on an existing file, and build the pools. Call
    /// [`Db::migrate`] next.
    ///
    /// # Errors
    /// [`DbError::Corruption`] if an existing file fails `quick_check`;
    /// [`DbError::Other`] on a pool build failure.
    pub async fn open(db_path: &Path) -> DbResult<Self> {
        if let Ok(meta) = fs::metadata(db_path) {
            if meta.len() > 0 {
                preflight_integrity(db_path).await?;
            }
        }
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let writer = build_pool(db_path, 1, false)?;
        let reader = build_pool(db_path, READER_POOL, true)?;
        let backup_dir = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("db-backups");

        Ok(Self {
            writer,
            reader,
            backup_dir,
        })
    }

    /// Apply pending migrations (forward-only, grouped in one transaction). Takes
    /// a verified `VACUUM INTO` backup first when anything is pending. Idempotent:
    /// a call with nothing pending is a no-op. Returns the schema version now in
    /// effect.
    ///
    /// # Errors
    /// [`DbError::Migration`] if the DB is newer than this build, the backup
    /// cannot be verified, or a migration fails (the group rolls back).
    pub async fn migrate(&self) -> DbResult<u32> {
        let backup_dir = self.backup_dir.clone();
        let conn = self.writer.get().await?;
        conn.interact(move |conn| -> DbResult<u32> {
            let runner = embedded::migrations::runner().set_grouped(true);

            let target = runner
                .get_migrations()
                .iter()
                .map(refinery::Migration::version)
                .max()
                .unwrap_or(0);

            // `get_last_applied_migration` errors if refinery's history table
            // doesn't exist yet (a fresh DB) — treat that as "nothing applied".
            let history_exists = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name='refinery_schema_history'",
                    [],
                    |_| Ok(true),
                )
                .optional()?
                .unwrap_or(false);
            let current = if history_exists {
                runner
                    .get_last_applied_migration(conn)?
                    .map_or(0, |m| m.version())
            } else {
                0
            };

            if current > target {
                return Err(DbError::Migration(format!(
                    "database schema version {current} is newer than this build supports ({target})"
                )));
            }
            if current < target {
                back_up(conn, &backup_dir)?;
            }

            runner.run(conn)?;
            Ok(u32::try_from(target).unwrap_or(0))
        })
        .await?
    }

    /// Run `f` inside an `IMMEDIATE` transaction: commit on `Ok`, roll back on
    /// `Err` (or a panic). One level only — do not open a nested transaction.
    ///
    /// # Errors
    /// Whatever `f` returns, or a pool / driver error.
    pub async fn write<T, F>(&self, f: F) -> DbResult<T>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> DbResult<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.writer.get().await?;
        conn.interact(move |conn| -> DbResult<T> {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let out = f(&tx)?;
            tx.commit()?;
            Ok(out)
        })
        .await?
    }

    /// Run `f` against a read-only connection.
    ///
    /// # Errors
    /// Whatever `f` returns, or a pool / driver error.
    pub async fn read<T, F>(&self, f: F) -> DbResult<T>
    where
        F: FnOnce(&Connection) -> DbResult<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.reader.get().await?;
        conn.interact(move |conn| f(conn)).await?
    }

    /// `PRAGMA quick_check` against a reader connection.
    ///
    /// # Errors
    /// [`DbError::Corruption`] if the check does not return `ok`.
    pub async fn integrity_check(&self) -> DbResult<()> {
        self.read(|conn| {
            let result: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
            if result == "ok" {
                Ok(())
            } else {
                Err(DbError::Corruption(format!("quick_check: {result}")))
            }
        })
        .await
    }

    /// Best-effort housekeeping for a clean shutdown: checkpoint the WAL and let
    /// SQLite refresh its query plans.
    pub async fn checkpoint_and_optimize(&self) {
        let _ = self
            .write(|tx| {
                tx.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA optimize;")?;
                Ok(())
            })
            .await;
    }

    /// The `core_app_meta` repository.
    #[must_use]
    pub fn app_meta(&self) -> AppMetaRepo<'_> {
        AppMetaRepo { db: self }
    }
}

async fn preflight_integrity(db_path: &Path) -> DbResult<()> {
    let path = db_path.to_path_buf();
    tokio::task::spawn_blocking(move || -> DbResult<()> {
        let conn = Connection::open(&path)?;
        let result: String = conn
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(DbError::from)?;
        if result == "ok" {
            Ok(())
        } else {
            Err(DbError::Corruption(format!("quick_check: {result}")))
        }
    })
    .await
    .map_err(|e| DbError::Other(format!("join: {e}")))?
}

fn back_up(conn: &Connection, backup_dir: &Path) -> DbResult<PathBuf> {
    fs::create_dir_all(backup_dir)?;
    let stamp = OffsetDateTime::now_utc().unix_timestamp();
    let path = backup_dir.join(format!("pre-migrate-{stamp}.db"));
    conn.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])?;

    let size = fs::metadata(&path)?.len();
    if size == 0 {
        return Err(DbError::Migration(
            "pre-migration backup is empty — refusing to migrate".to_owned(),
        ));
    }
    prune_backups(backup_dir)?;
    Ok(path)
}

fn prune_backups(backup_dir: &Path) -> DbResult<()> {
    let mut backups: Vec<PathBuf> = fs::read_dir(backup_dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let is_backup_name = p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("pre-migrate-"));
            let is_db = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("db"));
            is_backup_name && is_db
        })
        .collect();
    backups.sort();
    while backups.len() > KEEP_BACKUPS {
        let oldest = backups.remove(0);
        let _ = fs::remove_file(oldest);
    }
    Ok(())
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

// ---------------------------------------------------------------- repository

/// Typed CRUD over `core_app_meta`.
pub struct AppMetaRepo<'a> {
    db: &'a Db,
}

impl AppMetaRepo<'_> {
    /// The value for `key`, or `None` if it is not set (not an error).
    ///
    /// # Errors
    /// A pool / driver error.
    pub async fn get(&self, key: &str) -> DbResult<Option<String>> {
        let key = key.to_owned();
        self.db
            .read(move |conn| {
                let value = conn
                    .prepare_cached("SELECT value FROM core_app_meta WHERE key = ?1")?
                    .query_row([key], |row| row.get::<_, String>(0))
                    .optional()?;
                Ok(value)
            })
            .await
    }

    /// Insert or update `key`, stamping `updated_at`.
    ///
    /// # Errors
    /// A pool / driver error.
    pub async fn set(&self, key: &str, value: &str) -> DbResult<()> {
        let (key, value, now) = (key.to_owned(), value.to_owned(), now_rfc3339());
        self.db
            .write(move |tx| {
                tx.prepare_cached(
                    "INSERT INTO core_app_meta (key, value, updated_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                )?
                .execute(rusqlite::params![key, value, now])?;
                Ok(())
            })
            .await
    }

    /// Every key/value pair, ordered by key.
    ///
    /// # Errors
    /// A pool / driver error.
    pub async fn all(&self) -> DbResult<Vec<(String, String)>> {
        self.db
            .read(|conn| {
                let rows = conn
                    .prepare_cached("SELECT key, value FROM core_app_meta ORDER BY key")?
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
    }
}
