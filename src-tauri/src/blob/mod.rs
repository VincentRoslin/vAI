//! The content-addressed blob store (`CLAUDE.md` Article I, ADR-0009).
//!
//! Binary artifacts — generated images now, audio and reference images later —
//! are stored on disk keyed by the lowercase hex SHA-256 of their bytes, at
//! `<root>/<sha[0:2]>/<sha>`. The Rust core is the only writer; the frontend
//! and subprocesses never touch these files (they get bytes over IPC / a path
//! Rust dictates).
//!
//! **Write order (ADR-0009):** [`BlobStore::put`] fsyncs the blob file *before*
//! it returns; the caller then commits the `asset` row (and whatever
//! references it) in a transaction. A crash between the two leaves an orphan
//! blob — harmless, and [`reconcile`] finds it — never a row pointing at a
//! missing file.
//!
//! `put` does **not** touch SQLite: persistence sequencing is the caller's, so
//! this module has no schema dependency and is trivially unit-testable.

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::contracts::ids::AssetId;
use crate::ipc::{AppError, AppResult};

/// A content-addressed store rooted at one directory.
#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
}

/// What [`reconcile`] found: blobs on disk with no `asset` row, and `asset`
/// rows whose blob file is missing.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    /// Blob files present on disk that no `asset` row references.
    pub orphan_blobs: Vec<AssetId>,
    /// `asset` ids with no blob file on disk.
    pub dangling_rows: Vec<AssetId>,
}

impl BlobStore {
    /// Open (creating the root if absent) a store at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Store `bytes`, returning their content address. Idempotent: storing the
    /// same bytes twice is a no-op that returns the same [`AssetId`].
    ///
    /// The blob file is written to a temp name, fsynced, atomically renamed
    /// into place, and its directory fsynced — so on return the bytes are
    /// durable. `media_type` is advisory metadata for the caller's `asset`
    /// row; it is not stored here.
    ///
    /// # Errors
    /// [`AppError::internal`] on any filesystem failure.
    pub fn put(&self, bytes: &[u8], _media_type: &str) -> AppResult<AssetId> {
        let sha = hex_sha256(bytes);
        let dir = self.root.join(&sha[0..2]);
        let final_path = dir.join(&sha);
        if final_path.is_file() {
            return Ok(AssetId::from_trusted(sha));
        }
        fs::create_dir_all(&dir).map_err(|e| AppError::internal("create blob dir", e))?;

        let tmp = dir.join(format!("{sha}.tmp"));
        {
            let mut f =
                fs::File::create(&tmp).map_err(|e| AppError::internal("create blob tmp", e))?;
            f.write_all(bytes)
                .map_err(|e| AppError::internal("write blob tmp", e))?;
            f.sync_all()
                .map_err(|e| AppError::internal("fsync blob tmp", e))?;
        }
        fs::rename(&tmp, &final_path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            AppError::internal("rename blob into place", e)
        })?;
        sync_dir(&dir);
        Ok(AssetId::from_trusted(sha))
    }

    /// The path of a stored blob.
    ///
    /// # Errors
    /// [`AppError::Validation`] if `id` is not a 64-hex-char SHA-256;
    /// [`AppError::NotFound`] if no such blob is stored.
    pub fn get(&self, id: &AssetId) -> AppResult<PathBuf> {
        let sha = validated_sha(id)?;
        let path = self.root.join(&sha[0..2]).join(sha);
        if path.is_file() {
            Ok(path)
        } else {
            Err(AppError::NotFound(format!("blob {id}")))
        }
    }

    /// Read a stored blob's bytes.
    ///
    /// # Errors
    /// As [`BlobStore::get`], plus [`AppError::internal`] on a read failure.
    pub fn read(&self, id: &AssetId) -> AppResult<Vec<u8>> {
        let path = self.get(id)?;
        fs::read(&path).map_err(|e| AppError::internal("read blob", e))
    }

    /// Whether a valid-looking blob is stored. A malformed id returns `false`
    /// (not an error) — callers that need the distinction use [`BlobStore::get`].
    #[must_use]
    pub fn contains(&self, id: &AssetId) -> bool {
        self.get(id).is_ok()
    }

    /// Every blob id currently on disk (a full walk of the two-level tree).
    ///
    /// # Errors
    /// [`AppError::internal`] on a directory read failure.
    pub fn list_ids(&self) -> AppResult<Vec<AssetId>> {
        let mut out = Vec::new();
        if !self.root.is_dir() {
            return Ok(out);
        }
        let shards =
            fs::read_dir(&self.root).map_err(|e| AppError::internal("read blob root", e))?;
        for shard in shards.flatten() {
            if !shard.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let entries =
                fs::read_dir(shard.path()).map_err(|e| AppError::internal("read blob shard", e))?;
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if is_sha256_hex(name) {
                    out.push(AssetId::from_trusted(name.to_owned()));
                }
            }
        }
        Ok(out)
    }

    /// Root directory (for tests / diagnostics).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// Compare the blobs on disk with the set of `asset` ids the database knows
/// about. Neither side is authoritative on its own; this only reports.
///
/// # Errors
/// [`AppError::internal`] if the on-disk listing fails.
pub fn reconcile<S: ::std::hash::BuildHasher>(
    store: &BlobStore,
    known_ids: &HashSet<String, S>,
) -> AppResult<ReconcileReport> {
    let on_disk: Vec<String> = store
        .list_ids()?
        .into_iter()
        .map(|id| id.as_str().to_owned())
        .collect();

    let mut orphan_blobs: Vec<AssetId> = on_disk
        .iter()
        .filter(|sha| !known_ids.contains(*sha))
        .cloned()
        .map(AssetId::from_trusted)
        .collect();
    let mut dangling_rows: Vec<AssetId> = known_ids
        .iter()
        .filter(|sha| !on_disk.contains(sha))
        .cloned()
        .map(AssetId::from_trusted)
        .collect();
    orphan_blobs.sort();
    dangling_rows.sort();
    Ok(ReconcileReport {
        orphan_blobs,
        dangling_rows,
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn validated_sha(id: &AssetId) -> AppResult<String> {
    let s = id.as_str();
    if is_sha256_hex(s) {
        Ok(s.to_owned())
    } else {
        Err(AppError::Validation(format!(
            "asset id must be a 64-char lowercase hex SHA-256, got {id}"
        )))
    }
}

/// Best-effort fsync of a directory so a rename is durable. A failure here is
/// not fatal — the rename itself already happened.
fn sync_dir(dir: &Path) {
    if let Ok(handle) = fs::File::open(dir) {
        let _ = handle.sync_all();
    }
}
