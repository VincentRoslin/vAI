//! HuggingFace API access: model search, `.gguf` file listing (+ header-derived
//! metadata), and a header range fetch. Read-only; the **only** runtime network
//! egress in the app, behind an explicit user action (ADR-0008, `SECURITY.md`).
//!
//! No auth for public models. A user HF token (Settings, later) would go here as
//! a bearer header; it is never logged (`logging` redaction covers it anyway).

use std::time::Duration;

use serde::Deserialize;

use crate::acquisition::gguf::{self, GgufParse};
use crate::contracts::acquisition::{HfGgufFile, HfModelSummary};
use crate::ipc::{AppError, AppResult};

/// How many header bytes to fetch initially, and the ceiling (ADR-0008).
const HEADER_START: u64 = 1 << 20; // 1 MiB
const HEADER_MAX: u64 = 8 << 20; // 8 MiB

/// A thin HF client. `base` is `https://huggingface.co` in production; tests
/// point it at a mock server.
#[derive(Debug, Clone)]
pub struct HfClient {
    http: reqwest::Client,
    base: String,
}

impl Default for HfClient {
    fn default() -> Self {
        Self::new("https://huggingface.co")
    }
}

impl HfClient {
    /// Build a client against `base` (no trailing slash).
    #[must_use]
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(8))
                .timeout(Duration::from_secs(30))
                .user_agent("localai/0.1")
                .build()
                .unwrap_or_default(),
            base: base.into(),
        }
    }

    fn offline(context: &str, err: &reqwest::Error) -> AppError {
        AppError::BackendUnavailable(format!("HuggingFace {context}: {err}"))
    }

    /// Search models, GGUF-tagged, newest first.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] when offline or the API errors.
    pub async fn search_models(&self, query: &str, limit: u32) -> AppResult<Vec<HfModelSummary>> {
        let url = format!("{}/api/models", self.base);
        let resp = self
            .http
            .get(&url)
            .query(&[
                ("search", query),
                ("filter", "gguf"),
                ("limit", &limit.min(100).to_string()),
                ("sort", "downloads"),
                ("direction", "-1"),
            ])
            .send()
            .await
            .map_err(|e| Self::offline("search", &e))?
            .error_for_status()
            .map_err(|e| Self::offline("search", &e))?;

        let raw: Vec<RawModel> = resp.json().await.map_err(|e| Self::offline("search", &e))?;
        Ok(raw
            .into_iter()
            .map(|m| HfModelSummary {
                repo: m.id,
                downloads: m.downloads,
                likes: m.likes,
                updated: m.last_modified,
            })
            .collect())
    }

    /// List a repo's `.gguf` files with size + LFS SHA-256, then enrich each with
    /// quant / context from its GGUF header (a bounded range fetch per file).
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] when offline or the API errors.
    pub async fn list_gguf_files(&self, repo: &str) -> AppResult<Vec<HfGgufFile>> {
        let url = format!("{}/api/models/{repo}/tree/main?recursive=1", self.base);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| Self::offline("file list", &e))?
            .error_for_status()
            .map_err(|e| Self::offline("file list", &e))?;
        let entries: Vec<RawTreeEntry> = resp
            .json()
            .await
            .map_err(|e| Self::offline("file list", &e))?;

        let mut files = Vec::new();
        for entry in entries {
            let is_gguf = std::path::Path::new(&entry.path)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
            if entry.r#type != "file" || !is_gguf {
                continue;
            }
            let sha256 = entry.lfs.as_ref().and_then(|l| l.sha256.clone());
            let (quant, context_length) = match self.header_meta(repo, &entry.path).await {
                Ok((q, c)) => (q, c),
                Err(err) => {
                    tracing::warn!(%err, file = %entry.path, "could not read GGUF header");
                    (None, None)
                }
            };
            files.push(HfGgufFile {
                filename: entry.path,
                size: entry.size,
                quant,
                context_length,
                sha256,
            });
        }
        Ok(files)
    }

    /// Range-fetch and parse just enough of a GGUF header to get quant + context.
    async fn header_meta(
        &self,
        repo: &str,
        filename: &str,
    ) -> AppResult<(Option<String>, Option<u32>)> {
        let mut len = HEADER_START;
        loop {
            let buf = self.fetch_range(repo, filename, len).await?;
            match gguf::parse_header(&buf)? {
                GgufParse::Header(h) => {
                    tracing::debug!(bytes = buf.len(), file = %filename, "GGUF header parsed");
                    return Ok((h.quant, h.context_length));
                }
                GgufParse::Incomplete if len < HEADER_MAX => len = (len * 2).min(HEADER_MAX),
                GgufParse::Incomplete => return Ok((None, None)),
            }
        }
    }

    /// Fetch the first `len` bytes of a repo file via a Range request.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] on a transport / status error.
    pub async fn fetch_range(&self, repo: &str, filename: &str, len: u64) -> AppResult<Vec<u8>> {
        let url = format!("{}/{repo}/resolve/main/{filename}", self.base);
        let resp = self
            .http
            .get(&url)
            .header(reqwest::header::RANGE, format!("bytes=0-{}", len - 1))
            .send()
            .await
            .map_err(|e| Self::offline("header fetch", &e))?
            .error_for_status()
            .map_err(|e| Self::offline("header fetch", &e))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| Self::offline("header fetch", &e))?;
        Ok(bytes.to_vec())
    }
}

// ---------------------------------------------------------------- API shapes

#[derive(Deserialize)]
struct RawModel {
    id: String,
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    likes: Option<u64>,
    #[serde(default, rename = "lastModified")]
    last_modified: Option<String>,
}

#[derive(Deserialize)]
struct RawTreeEntry {
    r#type: String,
    path: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    lfs: Option<RawLfs>,
}

#[derive(Deserialize)]
struct RawLfs {
    #[serde(default)]
    sha256: Option<String>,
}
