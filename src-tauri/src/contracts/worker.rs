//! Stateless-worker protocol contracts (ADR-0013).
//!
//! Workers (STT, TTS, face-embedder) speak **one JSON object per line** over
//! stdin/stdout; `stderr` is logs only. The framing below is the same for every
//! worker; the `payload` / `data` bodies are worker-specific and defined by each
//! worker's own phase (STT: 18, TTS: 19, embedder: 27), which also validates
//! them.
//!
//! Protocol version: [`crate::contracts::WORKER_PROTOCOL_VERSION`]. Bump policy
//! in `docs/contracts.md`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::contracts::ids::WorkerJobId;
use crate::ipc::AppError;

/// Which worker a message concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum WorkerKind {
    /// Speech-to-text.
    Stt,
    /// Text-to-speech.
    Tts,
    /// Embedding / identity vectors.
    Embed,
}

/// First line a worker emits on startup, before any job response. The supervisor
/// checks `protocol_version` against [`crate::contracts::WORKER_PROTOCOL_VERSION`]
/// and refuses a mismatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct WorkerHello {
    /// The protocol version the worker implements.
    pub protocol_version: u32,
    /// Which worker this process is.
    pub worker: WorkerKind,
}

/// A unit of work sent to a worker (one line on its stdin).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct WorkerRequest {
    /// Correlates the response(s) with this request.
    pub id: WorkerJobId,
    /// Target worker (a sanity check — the pipe already implies it).
    pub kind: WorkerKind,
    /// Worker-specific request body; schema defined + validated by the worker's
    /// phase.
    #[ts(type = "unknown")]
    pub payload: Value,
}

/// The outcome carried by a [`WorkerResponse`]. Adjacently tagged:
/// `{ "status": "Ok", "body": { ... } }`. `Progress` may appear zero or more
/// times before a terminal `Ok` / `Err`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "status", content = "body")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum WorkerResult {
    /// Terminal success. `data` is the worker-specific result body.
    Ok {
        /// Worker-specific result body.
        #[ts(type = "unknown")]
        data: Value,
    },
    /// Terminal failure.
    Err {
        /// The failure.
        error: AppError,
    },
    /// Non-terminal progress update.
    Progress {
        /// Fractional progress in `0.0..=1.0`.
        progress: f32,
        /// Optional human-readable detail.
        detail: Option<String>,
    },
}

/// A worker's reply to a [`WorkerRequest`] (one line on its stdout).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct WorkerResponse {
    /// The request this replies to.
    pub id: WorkerJobId,
    /// The outcome (or a progress frame).
    pub result: WorkerResult,
}
