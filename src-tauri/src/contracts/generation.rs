//! LLM generation request + streaming-event contracts.
//!
//! [`GenerationEvent`] is the payload of the per-request Tauri Channel
//! (ADR-0002). It is adjacently tagged so the TypeScript side is a discriminated
//! union that narrows on `.type`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::contracts::ids::{ModelId, TaskId};
use crate::ipc::AppError;

/// Sampling knobs for one generation. All optional on the wire; the adapter
/// applies backend defaults for absent fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SamplingParams {
    /// Softmax temperature. `>= 0.0`; `0.0` = greedy.
    pub temperature: Option<f32>,
    /// Nucleus sampling cutoff in `0.0..=1.0`.
    pub top_p: Option<f32>,
    /// Top-k cutoff. `0` disables.
    pub top_k: Option<u32>,
    /// Hard cap on generated tokens.
    pub max_tokens: Option<u32>,
    /// Stop sequences; generation halts before emitting any of them.
    #[serde(default)]
    pub stop: Vec<String>,
    /// RNG seed for reproducibility. 32-bit to match common LLM runtimes.
    pub seed: Option<u32>,
}

impl SamplingParams {
    /// # Errors
    /// [`AppError::Validation`] if a supplied value is outside its valid range.
    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(t) = self.temperature {
            if !t.is_finite() || t < 0.0 {
                return Err(AppError::Validation(format!(
                    "temperature must be >= 0.0, got {t}"
                )));
            }
        }
        if let Some(p) = self.top_p {
            if !p.is_finite() || !(0.0..=1.0).contains(&p) {
                return Err(AppError::Validation(format!(
                    "top_p must be within 0.0..=1.0, got {p}"
                )));
            }
        }
        if let Some(0) = self.max_tokens {
            return Err(AppError::Validation("max_tokens must be >= 1".to_owned()));
        }
        Ok(())
    }
}

/// A request to generate text. The `prompt` is already fully assembled by the
/// context builder (Phase 20) — this contract does not know about personas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct GenerationRequest {
    /// Correlates the request with its [`crate::contracts::task::TaskStatus`] and
    /// its cancellation.
    pub task_id: TaskId,
    /// Which model to run.
    pub model: ModelId,
    /// The fully-rendered prompt.
    pub prompt: String,
    /// Sampling configuration.
    pub params: SamplingParams,
}

/// Why a generation stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum StopReason {
    /// The model emitted its end-of-text token.
    EndOfText,
    /// `max_tokens` was reached.
    MaxTokens,
    /// A stop sequence was hit.
    StopSequence,
    /// The caller cancelled.
    Cancelled,
    /// Generation failed; see the accompanying [`GenerationEvent::Error`].
    Error,
}

/// One frame of a streaming generation. Adjacently tagged:
/// `{ "type": "TokenDelta", "data": { ... } }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum GenerationEvent {
    /// A chunk of newly generated text.
    TokenDelta {
        /// 0-based index of this delta within the stream.
        index: u32,
        /// The text of this chunk (may be a partial word).
        text: String,
    },
    /// The stream finished normally.
    Done {
        /// Why it stopped.
        stop_reason: StopReason,
        /// Total tokens generated.
        tokens: u32,
    },
    /// The stream failed. No further frames follow.
    Error {
        /// The failure.
        error: AppError,
    },
    /// The stream was cancelled. No further frames follow.
    Cancelled,
}
