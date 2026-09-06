//! The HTTP client for one running `llama-server`: `/health` + `/completion`
//! (non-stream and SSE), with the per-launch bearer token on every request and
//! a per-call deadline. Cancellation drops the response so the server sees the
//! disconnect and frees the slot.

use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::protocol::{parse_chunk, parse_sse_line, CompletionRequest, SseEvent};
use crate::contracts::generation::{GenerationEvent, StopReason};
use crate::ipc::{AppError, AppResult};

/// Talks to one `llama-server` instance.
pub struct LlamaClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
    /// Deadline for a whole non-streaming call / for stream inactivity.
    deadline: Duration,
}

/// The result of a non-streaming `/completion`.
#[derive(Debug, Clone)]
pub struct Completion {
    pub text: String,
    pub tokens: u32,
    pub stop_reason: StopReason,
}

impl LlamaClient {
    /// `base_url` like `http://127.0.0.1:8080`; `token` is the `--api-key`.
    #[must_use]
    pub fn new(base_url: impl Into<String>, token: impl Into<String>, deadline: Duration) -> Self {
        Self {
            base_url: base_url.into(),
            token: token.into(),
            http: reqwest::Client::new(),
            deadline,
        }
    }

    /// `GET /health` → `Ok(())` on 200.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] on any non-200 / transport failure.
    pub async fn health(&self) -> AppResult<()> {
        let resp = self
            .http
            .get(format!("{}/health", self.base_url))
            .bearer_auth(&self.token)
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .map_err(|e| AppError::BackendUnavailable(format!("llama-server /health: {e}")))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(AppError::BackendUnavailable(format!(
                "llama-server /health returned {}",
                resp.status()
            )))
        }
    }

    /// `POST /completion` without streaming.
    ///
    /// # Errors
    /// [`AppError::Timeout`] past the deadline; [`AppError::Cancelled`] on
    /// `cancel`; [`AppError::BackendUnavailable`] on a transport / protocol
    /// failure; the mapped server error on a 4xx.
    pub async fn complete(
        &self,
        req: &CompletionRequest,
        cancel: &CancellationToken,
    ) -> AppResult<Completion> {
        let call = self
            .http
            .post(format!("{}/completion", self.base_url))
            .bearer_auth(&self.token)
            .json(req)
            .send();

        let resp = tokio::select! {
            () = cancel.cancelled() => return Err(AppError::Cancelled),
            r = tokio::time::timeout(self.deadline, call) => r
                .map_err(|_| AppError::Timeout("llama-server /completion".to_owned()))?
                .map_err(|e| AppError::BackendUnavailable(format!("llama-server /completion: {e}")))?,
        };

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| AppError::BackendUnavailable(format!("read /completion body: {e}")))?;
        if !status.is_success() {
            return Err(map_server_error(status, &body));
        }
        let chunk = parse_chunk(&body)?;
        Ok(Completion {
            text: chunk.content.clone(),
            tokens: chunk.tokens_predicted.unwrap_or(0),
            stop_reason: chunk.stop_reason(),
        })
    }

    /// `POST /completion` with `stream: true`. Pushes [`GenerationEvent`]s into
    /// `tx` — ordered `TokenDelta`s then exactly one terminal
    /// (`Done` / `Cancelled` / `Error`). Returns after the terminal is sent.
    pub async fn stream(
        &self,
        req: &CompletionRequest,
        tx: &mpsc::Sender<GenerationEvent>,
        cancel: &CancellationToken,
    ) {
        if let Err(err) = self.stream_inner(req, tx, cancel).await {
            let terminal = match err {
                AppError::Cancelled => GenerationEvent::Cancelled,
                other => GenerationEvent::Error { error: other },
            };
            let _ = tx.send(terminal).await;
        }
    }

    async fn stream_inner(
        &self,
        req: &CompletionRequest,
        tx: &mpsc::Sender<GenerationEvent>,
        cancel: &CancellationToken,
    ) -> AppResult<()> {
        let send = self
            .http
            .post(format!("{}/completion", self.base_url))
            .bearer_auth(&self.token)
            .json(req)
            .send();
        let resp = tokio::select! {
            () = cancel.cancelled() => return Err(AppError::Cancelled),
            r = tokio::time::timeout(self.deadline, send) => r
                .map_err(|_| AppError::Timeout("llama-server stream connect".to_owned()))?
                .map_err(|e| AppError::BackendUnavailable(format!("llama-server stream: {e}")))?,
        };
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(map_server_error(status, &body));
        }

        let mut body = resp.bytes_stream();
        let mut buf = String::new();
        let mut index: u32 = 0;

        loop {
            let next = tokio::select! {
                () = cancel.cancelled() => return Err(AppError::Cancelled),
                n = tokio::time::timeout(self.deadline, body.next()) => n
                    .map_err(|_| AppError::Timeout("llama-server stream idle".to_owned()))?,
            };
            let Some(bytes) = next else { break };
            let bytes =
                bytes.map_err(|e| AppError::BackendUnavailable(format!("stream chunk: {e}")))?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(nl) = buf.find('\n') {
                let line: String = buf.drain(..=nl).collect();
                match parse_sse_line(&line) {
                    Some(SseEvent::Done) => {
                        let _ = tx
                            .send(GenerationEvent::Done {
                                stop_reason: StopReason::EndOfText,
                                tokens: index,
                            })
                            .await;
                        return Ok(());
                    }
                    Some(SseEvent::Data(json)) => {
                        let chunk = parse_chunk(&json)?;
                        if !chunk.content.is_empty() {
                            let _ = tx
                                .send(GenerationEvent::TokenDelta {
                                    index,
                                    text: chunk.content.clone(),
                                })
                                .await;
                            index += 1;
                        }
                        if chunk.stop {
                            let _ = tx
                                .send(GenerationEvent::Done {
                                    stop_reason: chunk.stop_reason(),
                                    tokens: chunk.tokens_predicted.unwrap_or(index),
                                })
                                .await;
                            return Ok(());
                        }
                    }
                    None => {}
                }
            }
        }

        // Stream ended without an explicit stop object.
        let _ = tx
            .send(GenerationEvent::Done {
                stop_reason: StopReason::EndOfText,
                tokens: index,
            })
            .await;
        Ok(())
    }
}

fn map_server_error(status: reqwest::StatusCode, body: &str) -> AppError {
    let detail = serde_json::from_str::<super::protocol::ServerError>(body).map_or_else(
        |_| body.chars().take(200).collect::<String>(),
        |e| e.error.message,
    );
    match status.as_u16() {
        401 | 403 => AppError::BackendUnavailable(format!("llama-server auth rejected: {detail}")),
        400 | 422 => AppError::Validation(format!("llama-server rejected the request: {detail}")),
        _ => AppError::BackendUnavailable(format!("llama-server {status}: {detail}")),
    }
}
