//! Model-acquisition contracts (Phase 12): HF search results, GGUF file listings,
//! and download queue / progress state.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::contracts::ids::DownloadId;
use crate::contracts::model::ModelKind;

/// One model from an HF search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct HfModelSummary {
    /// `owner/name`.
    pub repo: String,
    /// Download count, when the API reports it.
    pub downloads: Option<u64>,
    /// Like count, when the API reports it.
    pub likes: Option<u64>,
    /// RFC-3339 last-modified, when the API reports it.
    pub updated: Option<String>,
}

/// One `.gguf` file in a repo, with header metadata when it could be read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct HfGgufFile {
    /// Path within the repo.
    pub filename: String,
    /// Size in bytes, when the API reports it.
    pub size: Option<u64>,
    /// Quant label from the GGUF header (`Q4_K_M`, …).
    pub quant: Option<String>,
    /// Context length from the GGUF header.
    pub context_length: Option<u32>,
    /// SHA-256 (HF LFS hash), when available — used for integrity verification.
    pub sha256: Option<String>,
}

/// Lifecycle of a download. `Complete` and `Failed` are terminal (a `Failed`
/// download can be retried, which creates a fresh row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum DownloadState {
    /// Accepted, not yet transferring.
    Queued,
    /// Bytes are moving.
    Downloading,
    /// Stopped by the user; `.part` kept.
    Paused,
    /// Transfer done, hashing / checking.
    Verifying,
    /// Verified and in place.
    Complete,
    /// Aborted with an error.
    Failed,
}

/// A download as the picker sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DownloadInfo {
    /// Stable id.
    pub id: DownloadId,
    /// Source repo.
    pub repo: String,
    /// File within the repo.
    pub filename: String,
    /// What kind of model this file is.
    pub kind: ModelKind,
    /// Current state.
    pub state: DownloadState,
    /// Total size in bytes, once known.
    pub total_bytes: Option<u64>,
    /// Bytes transferred so far.
    pub downloaded_bytes: u64,
    /// Last error message, for a `Failed` download.
    pub error: Option<String>,
}

/// A progress tick, delivered over the per-download Tauri Channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct DownloadProgress {
    /// Bytes transferred so far.
    pub downloaded_bytes: u64,
    /// Total size, once known.
    pub total_bytes: Option<u64>,
    /// Terminal state reached, if any (the last tick of a finished download).
    pub done: Option<DownloadState>,
}
