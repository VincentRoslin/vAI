//! The HTTP client for one running image sidecar: `/health`, `/progress`,
//! `/generate`, `/unload`, with the per-launch bearer token on every request
//! and a per-call deadline. All sidecar JSON is confined here + in
//! [`protocol`](super::protocol).

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::protocol::{
    GenerateBody, GenerateResponse, HealthResponse, ProgressResponse, SidecarError,
};
use crate::ipc::{AppError, AppResult};

/// A short timeout for the cheap polling endpoints.
const POLL_TIMEOUT: Duration = Duration::from_secs(5);

/// Talks to one image sidecar instance.
pub struct ImageClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
    /// Deadline for a whole `/generate` call.
    generate_deadline: Duration,
}

impl ImageClient {
    /// `base_url` like `http://127.0.0.1:8751`; `token` is the `--token` value.
    #[must_use]
    pub fn new(
        base_url: impl Into<String>,
        token: impl Into<String>,
        generate_deadline: Duration,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            token: token.into(),
            http: reqwest::Client::new(),
            generate_deadline,
        }
    }

    /// `GET /health`.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] on any non-200 / transport failure.
    pub async fn health(&self) -> AppResult<HealthResponse> {
        let resp = self
            .http
            .get(format!("{}/health", self.base_url))
            .timeout(Duration::from_secs(3))
            .send()
            .await
            .map_err(|e| AppError::BackendUnavailable(format!("image sidecar /health: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::BackendUnavailable(format!(
                "image sidecar /health returned {}",
                resp.status()
            )));
        }
        resp.json::<HealthResponse>()
            .await
            .map_err(|e| AppError::BackendUnavailable(format!("parse /health: {e}")))
    }

    /// `GET /progress`.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] on a transport / parse failure.
    pub async fn progress(&self) -> AppResult<ProgressResponse> {
        let resp = self
            .http
            .get(format!("{}/progress", self.base_url))
            .bearer_auth(&self.token)
            .timeout(POLL_TIMEOUT)
            .send()
            .await
            .map_err(|e| AppError::BackendUnavailable(format!("image sidecar /progress: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::BackendUnavailable(format!(
                "image sidecar /progress returned {}",
                resp.status()
            )));
        }
        resp.json::<ProgressResponse>()
            .await
            .map_err(|e| AppError::BackendUnavailable(format!("parse /progress: {e}")))
    }

    /// `POST /generate`. Blocks until the sidecar has written every image.
    ///
    /// # Errors
    /// [`AppError::Timeout`] past the deadline; [`AppError::Cancelled`] on
    /// `cancel`; [`AppError::ResourceExhausted`] on a sidecar OOM (503);
    /// [`AppError::Validation`] on a 4xx; [`AppError::BackendUnavailable`]
    /// otherwise.
    pub async fn generate(
        &self,
        body: &GenerateBody,
        cancel: &CancellationToken,
    ) -> AppResult<GenerateResponse> {
        let call = self
            .http
            .post(format!("{}/generate", self.base_url))
            .bearer_auth(&self.token)
            .json(body)
            .send();

        let resp = tokio::select! {
            () = cancel.cancelled() => return Err(AppError::Cancelled),
            r = tokio::time::timeout(self.generate_deadline, call) => r
                .map_err(|_| AppError::Timeout("image sidecar /generate".to_owned()))?
                .map_err(|e| AppError::BackendUnavailable(format!("image sidecar /generate: {e}")))?,
        };

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| AppError::BackendUnavailable(format!("read /generate body: {e}")))?;
        if !status.is_success() {
            return Err(map_sidecar_error(status, &text));
        }
        serde_json::from_str::<GenerateResponse>(&text)
            .map_err(|e| AppError::BackendUnavailable(format!("parse /generate: {e}")))
    }

    /// `POST /unload` — best-effort; a failure is logged, not returned.
    pub async fn unload(&self) {
        let sent = self
            .http
            .post(format!("{}/unload", self.base_url))
            .bearer_auth(&self.token)
            .timeout(POLL_TIMEOUT)
            .send()
            .await;
        if let Err(e) = sent {
            tracing::debug!(error = %e, "image sidecar /unload failed (ignored)");
        }
    }
}

fn map_sidecar_error(status: reqwest::StatusCode, body: &str) -> AppError {
    let detail = serde_json::from_str::<SidecarError>(body)
        .map_or_else(|_| body.chars().take(200).collect::<String>(), |e| e.detail);
    match status.as_u16() {
        401 | 403 => AppError::BackendUnavailable(format!("image sidecar auth rejected: {detail}")),
        400 | 422 => AppError::Validation(format!("image sidecar rejected the request: {detail}")),
        503 => AppError::ResourceExhausted(format!("image sidecar out of resources: {detail}")),
        _ => AppError::BackendUnavailable(format!("image sidecar {status}: {detail}")),
    }
}
