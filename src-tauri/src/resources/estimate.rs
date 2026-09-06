//! VRAM estimation for an LLM load, plus a learned per-model correction.
//!
//! The closed form (ADR-0007):
//! `weights + kv_cache + cuda_context + compute_buffers`.
//! `weights ≈ file_size_mb` (a GGUF is almost entirely quantised weights);
//! `kv_cache` from the GGUF header dims × context. The caller adds the
//! configured safety margin on top — it is *not* baked in here.
//!
//! [`Calibration`] multiplies the raw estimate by a factor learned from
//! `measured / raw` pairs (EMA). Keyed by model id, falling back to a backend
//! default. In-memory for Phase 13; it becomes durable when the ledger does
//! (Phase 33).

use std::collections::HashMap;

/// Fixed CUDA context overhead per loaded model, in MB (ADR-0007, ≈600).
pub const CUDA_CONTEXT_MB: u32 = 600;
/// Fixed compute/scratch buffer allowance, in MB (ADR-0007, ≈300).
pub const COMPUTE_BUFFERS_MB: u32 = 300;

/// Inputs to [`estimate_llm_vram`]. Dimensions come from the GGUF header
/// (`acquisition::gguf`); `kv_bytes` is 2 for f16 cache, 1 for q8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EstimateInput {
    /// On-disk size of the weights file, in MB.
    pub file_size_mb: u32,
    /// Transformer block count (`*.block_count`).
    pub n_layers: u32,
    /// KV head count (`*.attention.head_count_kv`).
    pub n_kv_heads: u32,
    /// Per-head dimension (`embedding_length / head_count`).
    pub head_dim: u32,
    /// Context window the model will be loaded with, in tokens.
    pub context_tokens: u32,
    /// Bytes per KV cache element (2 = f16, 1 = q8_0).
    pub kv_bytes: u32,
}

impl EstimateInput {
    /// KV-cache size for these dims, in MB: `layers × 2 (K+V) × kv_heads ×
    /// head_dim × context × kv_bytes`.
    #[must_use]
    pub fn kv_cache_mb(&self) -> u32 {
        let bytes = u64::from(self.n_layers)
            * 2
            * u64::from(self.n_kv_heads)
            * u64::from(self.head_dim)
            * u64::from(self.context_tokens)
            * u64::from(self.kv_bytes);
        u32::try_from(bytes / (1024 * 1024)).unwrap_or(u32::MAX)
    }
}

/// The raw closed-form VRAM estimate for an LLM load, in MB. The safety margin
/// is added by the caller, not here.
#[must_use]
pub fn estimate_llm_vram(input: &EstimateInput) -> u32 {
    input
        .file_size_mb
        .saturating_add(input.kv_cache_mb())
        .saturating_add(CUDA_CONTEXT_MB)
        .saturating_add(COMPUTE_BUFFERS_MB)
}

/// EMA weight given to each new observation (0..1). Small = slow to trust one
/// sample.
const EMA_ALPHA: f32 = 0.3;
/// Clamp the learned factor to a sane band — a wild measurement (a shared GPU,
/// a bad reading) can't send future estimates to zero or the moon.
const FACTOR_MIN: f32 = 0.5;
const FACTOR_MAX: f32 = 2.0;

/// Per-key multiplicative correction on the raw estimate, learned over time.
#[derive(Debug, Default)]
pub struct Calibration {
    factors: HashMap<String, f32>,
}

impl Calibration {
    /// A fresh table (every factor defaults to 1.0).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The factor for `key`, or `fallback`'s factor, or 1.0.
    #[must_use]
    pub fn factor(&self, key: &str, fallback: &str) -> f32 {
        self.factors
            .get(key)
            .or_else(|| self.factors.get(fallback))
            .copied()
            .unwrap_or(1.0)
    }

    /// Apply the learned factor to a raw estimate.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    pub fn apply(&self, raw_mb: u32, key: &str, fallback: &str) -> u32 {
        let scaled = raw_mb as f32 * self.factor(key, fallback);
        scaled.round().max(0.0) as u32
    }

    /// Fold a new `(measured, raw)` observation into `key`'s factor by EMA.
    #[allow(clippy::cast_precision_loss)]
    pub fn record(&mut self, key: &str, measured_mb: u32, raw_mb: u32) {
        if raw_mb == 0 {
            return;
        }
        let observed = (measured_mb as f32 / raw_mb as f32).clamp(FACTOR_MIN, FACTOR_MAX);
        let entry = self.factors.entry(key.to_owned()).or_insert(1.0);
        *entry = (EMA_ALPHA * observed + (1.0 - EMA_ALPHA) * *entry).clamp(FACTOR_MIN, FACTOR_MAX);
    }
}
