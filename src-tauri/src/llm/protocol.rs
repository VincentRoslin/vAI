//! The `llama-server` `/completion` wire shapes + its SSE framing.
//!
//! This is the **only** place that knows llama.cpp's JSON. Everything outside
//! `llm/` speaks the Phase 7 [`crate::contracts::generation`] vocabulary.

use serde::{Deserialize, Serialize};

use crate::contracts::generation::{SamplingParams, StopReason};
use crate::ipc::AppError;

/// A `POST /completion` body. `stream` toggles SSE.
#[derive(Debug, Clone, Serialize)]
pub struct CompletionRequest {
    pub prompt: String,
    pub n_predict: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    pub stream: bool,
    /// Keep the prompt KV cache warm between calls in one slot.
    pub cache_prompt: bool,
}

impl CompletionRequest {
    /// Build from the Phase 7 contract. `n_predict = -1` (unbounded) when the
    /// caller set no `max_tokens`.
    #[must_use]
    pub fn from_params(prompt: String, params: &SamplingParams, stream: bool) -> Self {
        Self {
            prompt,
            n_predict: params
                .max_tokens
                .map_or(-1, |n| i32::try_from(n).unwrap_or(i32::MAX)),
            temperature: params.temperature,
            top_p: params.top_p,
            top_k: params.top_k,
            seed: params.seed,
            stop: params.stop.clone(),
            stream,
            cache_prompt: true,
        }
    }
}

/// One `/completion` response object — the whole body when `stream=false`, or
/// the payload of each `data:` line when `stream=true`. Fields absent on
/// non-final streaming chunks are `Option`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CompletionChunk {
    /// Newly generated text for this chunk (or the whole text, non-stream).
    #[serde(default)]
    pub content: String,
    /// `true` on the final object.
    #[serde(default)]
    pub stop: bool,
    /// Newer llama-server: `"eos" | "word" | "limit" | "none"`.
    #[serde(default)]
    pub stop_type: Option<String>,
    /// Older llama-server booleans (kept for compatibility).
    #[serde(default)]
    pub stopped_eos: Option<bool>,
    #[serde(default)]
    pub stopped_word: Option<bool>,
    #[serde(default)]
    pub stopped_limit: Option<bool>,
    /// Total tokens generated (present on the final object).
    #[serde(default)]
    pub tokens_predicted: Option<u32>,
}

impl CompletionChunk {
    /// The [`StopReason`] the final object implies.
    #[must_use]
    pub fn stop_reason(&self) -> StopReason {
        if let Some(kind) = self.stop_type.as_deref() {
            return match kind {
                "word" => StopReason::StopSequence,
                "limit" => StopReason::MaxTokens,
                // "eos" and anything unrecognised
                _ => StopReason::EndOfText,
            };
        }
        if self.stopped_word == Some(true) {
            StopReason::StopSequence
        } else if self.stopped_limit == Some(true) {
            StopReason::MaxTokens
        } else {
            StopReason::EndOfText
        }
    }
}

/// One parsed SSE event from a `/completion` stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEvent {
    /// A `data:` line carrying a JSON completion chunk.
    Data(String),
    /// The `data: [DONE]` sentinel (OpenAI-compat mode; native mode omits it).
    Done,
}

/// Parse one line of an SSE stream. Returns `None` for blank lines, comments
/// (`:`), and non-`data:` fields — the caller skips them.
#[must_use]
pub fn parse_sse_line(line: &str) -> Option<SseEvent> {
    let line = line.trim_end_matches(['\r', '\n']);
    let rest = line.strip_prefix("data:")?;
    let rest = rest.strip_prefix(' ').unwrap_or(rest);
    if rest == "[DONE]" {
        Some(SseEvent::Done)
    } else if rest.is_empty() {
        None
    } else {
        Some(SseEvent::Data(rest.to_owned()))
    }
}

/// Decode a `data:` JSON payload into a [`CompletionChunk`].
///
/// # Errors
/// [`AppError::BackendUnavailable`] when the payload is not the expected shape.
pub fn parse_chunk(json: &str) -> Result<CompletionChunk, AppError> {
    serde_json::from_str(json)
        .map_err(|e| AppError::BackendUnavailable(format!("malformed llama-server chunk: {e}")))
}

/// An error object llama-server returns on a bad request (`{"error":{...}}`).
#[derive(Debug, Clone, Deserialize)]
pub struct ServerError {
    pub error: ServerErrorBody,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerErrorBody {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub code: Option<i64>,
}
