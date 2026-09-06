//! The HTTP wire vocabulary between the Rust core and the image sidecar
//! (`image_gen/server.py`, and `server_fake.py` in tests). **Nothing outside
//! `image/` references these types** (`ARCHITECTURE.md` §2) — the sidecar's
//! JSON shape is confined here and in [`client`](super::client).
//!
//! Binary exchange (Article I): the sidecar never returns image bytes inline —
//! it writes each PNG into an `out_dir` the Rust core dictates and returns the
//! paths; the core reads, blob-stores, and deletes them.

use serde::{Deserialize, Serialize};

/// The `/health` (and `ready`-handshake) protocol version. Bump on a breaking
/// change to any shape in this file; the backend refuses a mismatch.
pub const PROTOCOL_VERSION: u32 = 1;

/// `POST /generate` body.
#[derive(Debug, Clone, Serialize)]
pub struct GenerateBody {
    /// Positive prompt (untrusted, opaque to the sidecar).
    pub prompt: String,
    /// Negative prompt (inert at `guidance 0`).
    pub negative: Option<String>,
    /// Width in px (a multiple of 16).
    pub width: u32,
    /// Height in px (a multiple of 16).
    pub height: u32,
    /// Denoising steps.
    pub steps: u32,
    /// `guidance_scale`.
    pub guidance: f32,
    /// Seed of the first image; image `i` uses `seed + i`.
    pub seed: i64,
    /// How many images to generate.
    pub batch_count: u32,
    /// Directory the sidecar writes PNGs into (created by the core, per call).
    pub out_dir: String,
    /// The single LoRA to apply, or `None` for the base model.
    pub lora: Option<LoraBody>,
}

/// One LoRA in a [`GenerateBody`].
#[derive(Debug, Clone, Serialize)]
pub struct LoraBody {
    /// Bare `*.safetensors` filename inside the sidecar's `--loras-dir`.
    pub file: String,
    /// Adapter weight.
    pub weight: f32,
}

/// `POST /generate` response.
#[derive(Debug, Clone, Deserialize)]
pub struct GenerateResponse {
    /// One entry per generated image, in batch order.
    pub images: Vec<GeneratedEntry>,
}

/// One generated image the sidecar wrote to `out_dir`.
#[derive(Debug, Clone, Deserialize)]
pub struct GeneratedEntry {
    /// Absolute path of the PNG the sidecar wrote.
    pub path: String,
    /// The seed that produced it.
    pub seed: i64,
}

/// `GET /health` response.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    /// `"ok"` when the sidecar is serving.
    pub status: String,
    /// The sidecar's protocol version.
    #[serde(default)]
    pub protocol: u32,
    /// Whether a model is currently resident.
    #[serde(default)]
    pub loaded: bool,
    /// Measured VRAM in MB, if the sidecar reports it (`0` = unknown).
    #[serde(default)]
    pub vram_mb: u32,
}

/// `GET /progress` response.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProgressResponse {
    /// Whether a generation is currently running.
    #[serde(default)]
    pub active: bool,
    /// Completed steps for the current image.
    #[serde(default)]
    pub step: u32,
    /// Total steps for the current image.
    #[serde(default)]
    pub total_steps: u32,
    /// 0-based index of the current image within the batch.
    #[serde(default)]
    pub image_index: u32,
    /// Batch size.
    #[serde(default)]
    pub batch_count: u32,
}

/// The sidecar's error body (`{ "detail": "..." }`, FastAPI-style).
#[derive(Debug, Clone, Deserialize)]
pub struct SidecarError {
    /// Human-readable failure detail.
    pub detail: String,
}
