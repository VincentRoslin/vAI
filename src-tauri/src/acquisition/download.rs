//! The download engine: streams a `reqwest` GET to `<dest>.part` with resume
//! (`Range` from the persisted offset), verifies SHA-256, atomically renames,
//! and registers the result. State lives in `model_downloads` so a hard kill is
//! recoverable (ADR-0008).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use crate::contracts::acquisition::{DownloadInfo, DownloadProgress, DownloadState};
use crate::contracts::ids::DownloadId;
use crate::contracts::model::{Device, ModelKind};
use crate::db::Db;
use crate::ipc::{AppError, AppResult};
use crate::models::{ModelDraft, ModelRegistry};

/// How a completed download becomes a registry entry.
#[derive(Debug, Clone)]
pub enum RegisterPlan {
    /// Parse the downloaded GGUF header and register an LLM.
    GgufLlm {
        /// Backend id for the registry row (e.g. `"llama.cpp"`).
        backend: String,
    },
    /// Register with a fixed draft (its `path` is filled in on completion).
    Fixed(Box<ModelDraft>),
    /// Do not register (mock tests, or a sibling file triggers registration).
    None,
}

/// A single file transfer.
#[derive(Debug, Clone)]
pub struct DownloadSpec {
    /// Fully-resolved URL to GET.
    pub url: String,
    /// Source repo (`owner/name`).
    pub repo: String,
    /// Repo revision / git ref.
    pub revision: String,
    /// File path within the repo.
    pub filename: String,
    /// What kind of model this file is.
    pub kind: ModelKind,
    /// Absolute destination (already confined to the model dir by the caller).
    pub dest_path: PathBuf,
    /// The configured model dir `dest_path` sits under (for re-confinement on
    /// registration).
    pub models_dir: PathBuf,
    /// Expected size in bytes, for the pre-transfer budget check.
    pub expected_size: Option<u64>,
    /// Expected SHA-256 (HF LFS hash).
    pub sha256_expected: Option<String>,
    /// Registration behaviour on completion.
    pub register: RegisterPlan,
}

/// Owns running transfers + the `model_downloads` table.
pub struct DownloadEngine {
    db: Arc<Db>,
    registry: Arc<ModelRegistry>,
    http: reqwest::Client,
    running: Mutex<HashMap<DownloadId, RunningHandle>>,
}

struct RunningHandle {
    abort: tokio::task::AbortHandle,
}

const UPDATE_EVERY: Duration = Duration::from_millis(250);

impl std::fmt::Debug for DownloadEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadEngine").finish_non_exhaustive()
    }
}

impl DownloadEngine {
    /// Build an engine over `db` + `registry`.
    #[must_use]
    pub fn new(db: Arc<Db>, registry: Arc<ModelRegistry>) -> Self {
        Self {
            db,
            registry,
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            running: Mutex::new(HashMap::new()),
        }
    }

    /// Start a new download. Writes a `Queued` row, spawns the worker, returns
    /// the id. `channel`, when given, receives progress ticks.
    ///
    /// # Errors
    /// A persistence error, or [`AppError::Conflict`] if this exact file is
    /// already downloading.
    pub async fn start(
        self: &Arc<Self>,
        spec: DownloadSpec,
        channel: Option<Channel<DownloadProgress>>,
    ) -> AppResult<DownloadId> {
        // One active transfer per (repo, filename).
        if self
            .list()
            .await?
            .iter()
            .any(|d| d.repo == spec.repo && d.filename == spec.filename && is_active(d.state))
        {
            return Err(AppError::Conflict(format!(
                "{}/{} is already downloading",
                spec.repo, spec.filename
            )));
        }

        let id = DownloadId::from_trusted(uuid::Uuid::new_v4().hyphenated().to_string());
        let now = now();
        let row = spec.to_row(&id, &now);
        self.db
            .write(move |tx| {
                tx.prepare_cached(INSERT_SQL)?.execute(rusqlite::params![
                    row.id,
                    row.repo,
                    row.revision,
                    row.filename,
                    row.kind,
                    row.dest_path,
                    row.total_bytes,
                    row.downloaded_bytes,
                    row.sha256_expected,
                    row.etag,
                    row.state,
                    row.error,
                    row.created_at,
                    row.updated_at,
                ])?;
                Ok(())
            })
            .await?;

        self.spawn_worker(id.clone(), spec, channel);
        Ok(id)
    }

    /// Resume a paused / interrupted download.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if `id` is unknown; [`AppError::Conflict`] if it is
    /// already running or already complete.
    pub async fn resume(self: &Arc<Self>, id: &DownloadId) -> AppResult<()> {
        let row = self.row(id).await?;
        let state = state_from_str(&row.state)?;
        if is_active(state) && state != DownloadState::Paused {
            return Err(AppError::Conflict(format!("download {id} is not paused")));
        }
        if state == DownloadState::Complete {
            return Err(AppError::Conflict(format!(
                "download {id} is already complete"
            )));
        }
        let spec = row.to_spec();
        self.spawn_worker(id.clone(), spec, None);
        Ok(())
    }

    /// Stop a running download, keeping the `.part` and marking it `Paused`.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn pause(&self, id: &DownloadId) -> AppResult<()> {
        if let Some(h) = self.running.lock().unwrap().remove(id) {
            h.abort.abort();
        }
        self.set_state(id, DownloadState::Paused, None).await
    }

    /// Stop a download and delete its `.part`; the row is removed.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn cancel(&self, id: &DownloadId) -> AppResult<()> {
        if let Some(h) = self.running.lock().unwrap().remove(id) {
            h.abort.abort();
        }
        if let Ok(row) = self.row(id).await {
            let _ = tokio::fs::remove_file(part_path(&PathBuf::from(&row.dest_path))).await;
        }
        let id_s = id.to_string();
        self.db
            .write(move |tx| {
                tx.execute("DELETE FROM model_downloads WHERE id = ?1", [id_s])?;
                Ok(())
            })
            .await
            .map_err(AppError::from)
    }

    /// Every download row.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list(&self) -> AppResult<Vec<DownloadInfo>> {
        let rows: Vec<DownloadRow> = self
            .db
            .read(|conn| {
                let rows = conn
                    .prepare_cached(&format!(
                        "SELECT {COLUMNS} FROM model_downloads ORDER BY created_at"
                    ))?
                    .query_map([], DownloadRow::from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?;
        Ok(rows.into_iter().map(DownloadRow::into_info).collect())
    }

    /// On startup: any row left `Downloading` (a hard kill) becomes `Paused`.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn reconcile_on_start(&self) -> AppResult<usize> {
        let now = now();
        self.db
            .write(move |tx| {
                let n = tx.execute(
                    "UPDATE model_downloads SET state = 'Paused', updated_at = ?1 \
                     WHERE state IN ('Downloading','Queued','Verifying')",
                    [now],
                )?;
                Ok(n)
            })
            .await
            .map_err(AppError::from)
    }

    // ------------------------------------------------------------ internals

    fn spawn_worker(
        self: &Arc<Self>,
        id: DownloadId,
        spec: DownloadSpec,
        channel: Option<Channel<DownloadProgress>>,
    ) {
        let engine = Arc::clone(self);
        let id_for_task = id.clone();
        let chan = channel;
        let task = tokio::spawn(async move {
            let result = engine.run(&id_for_task, spec, chan.as_ref()).await;
            if let Err(err) = result {
                err.log("download worker");
                let _ = engine
                    .set_state(&id_for_task, DownloadState::Failed, Some(err.to_string()))
                    .await;
                if let Some(c) = &chan {
                    let _ = c.send(DownloadProgress {
                        downloaded_bytes: 0,
                        total_bytes: None,
                        done: Some(DownloadState::Failed),
                    });
                }
            }
            engine.running.lock().unwrap().remove(&id_for_task);
        });
        self.running.lock().unwrap().insert(
            id,
            RunningHandle {
                abort: task.abort_handle(),
            },
        );
    }

    async fn run(
        &self,
        id: &DownloadId,
        spec: DownloadSpec,
        channel: Option<&Channel<DownloadProgress>>,
    ) -> AppResult<()> {
        self.set_state(id, DownloadState::Downloading, None).await?;

        let part = part_path(&spec.dest_path);
        if let Some(parent) = spec.dest_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::internal("create download dir", e))?;
        }

        let mut offset = tokio::fs::metadata(&part).await.map_or(0, |m| m.len());
        let mut request = self.http.get(&spec.url);
        if offset > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|e| AppError::BackendUnavailable(format!("GET {}: {e}", spec.repo)))?;
        let status = response.status();
        if !status.is_success() {
            return Err(AppError::BackendUnavailable(format!(
                "GET {} → HTTP {status}",
                spec.filename
            )));
        }
        // Server ignored the Range → restart from 0.
        let resuming = offset > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
        if offset > 0 && !resuming {
            offset = 0;
        }
        let total = content_length(&response).map(|len| len + offset);
        self.set_total(id, total).await?;

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&part)
            .await
            .map_err(|e| AppError::internal("open .part", e))?;
        file.set_len(offset)
            .await
            .map_err(|e| AppError::internal("truncate .part", e))?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|e| AppError::internal("seek .part", e))?;

        let mut downloaded = offset;
        let mut last_flush = Instant::now();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| AppError::BackendUnavailable(format!("stream: {e}")))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| AppError::internal("write .part", e))?;
            downloaded += chunk.len() as u64;
            if last_flush.elapsed() >= UPDATE_EVERY {
                file.flush().await.ok();
                self.set_progress(id, downloaded).await?;
                if let Some(c) = channel {
                    let _ = c.send(DownloadProgress {
                        downloaded_bytes: downloaded,
                        total_bytes: total,
                        done: None,
                    });
                }
                last_flush = Instant::now();
            }
        }
        file.flush()
            .await
            .map_err(|e| AppError::internal("flush .part", e))?;
        self.set_progress(id, downloaded).await?;

        // Verify.
        self.set_state(id, DownloadState::Verifying, None).await?;
        if let Some(expected) = &spec.sha256_expected {
            let actual = sha256_file(&part).await?;
            if !actual.eq_ignore_ascii_case(expected) {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(AppError::Validation(format!(
                    "SHA-256 mismatch for {} (expected {expected}, got {actual})",
                    spec.filename
                )));
            }
        }

        tokio::fs::rename(&part, &spec.dest_path)
            .await
            .map_err(|e| AppError::internal("finalize download", e))?;
        self.set_state(id, DownloadState::Complete, None).await?;

        self.register(&spec).await?;

        if let Some(c) = channel {
            let _ = c.send(DownloadProgress {
                downloaded_bytes: downloaded,
                total_bytes: total,
                done: Some(DownloadState::Complete),
            });
        }
        Ok(())
    }

    async fn register(&self, spec: &DownloadSpec) -> AppResult<()> {
        let models_dir = spec.models_dir.clone();

        let draft = match &spec.register {
            RegisterPlan::None => return Ok(()),
            RegisterPlan::Fixed(d) => {
                let mut d = (**d).clone();
                spec.dest_path.clone_into(&mut d.path);
                d
            }
            RegisterPlan::GgufLlm { backend } => {
                let bytes = tokio::fs::read(&spec.dest_path)
                    .await
                    .map_err(|e| AppError::internal("read gguf for registration", e))?;
                let header = match super::gguf::parse_header(&bytes)? {
                    super::gguf::GgufParse::Header(h) => h,
                    super::gguf::GgufParse::Incomplete => {
                        return Err(AppError::Validation(
                            "downloaded GGUF is truncated".to_owned(),
                        ))
                    }
                };
                let size_mb = u32::try_from(bytes.len() / (1024 * 1024)).unwrap_or(u32::MAX);
                ModelDraft {
                    display_name: format!("{} ({})", spec.repo, spec.filename),
                    kind: ModelKind::Llm,
                    backend: crate::contracts::model::ModelBackend(backend.clone()),
                    quant: header.quant.map(crate::contracts::model::Quant),
                    path: spec.dest_path.clone(),
                    streaming: true,
                    context_tokens: header.context_length,
                    estimated_vram_mb: Some(size_mb.saturating_add(1024)),
                    devices: vec![Device::Cuda, Device::Cpu],
                    config: serde_json::json!({}),
                }
            }
        };

        self.registry.register(draft, &models_dir).await.map(|_| ())
    }

    async fn row(&self, id: &DownloadId) -> AppResult<DownloadRow> {
        let id_s = id.to_string();
        self.db
            .read(move |conn| {
                conn.prepare_cached(&format!(
                    "SELECT {COLUMNS} FROM model_downloads WHERE id = ?1"
                ))?
                .query_row([id_s], DownloadRow::from_row)
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => crate::db::DbError::NotFound,
                    other => other.into(),
                })
            })
            .await
            .map_err(AppError::from)
    }

    async fn set_state(
        &self,
        id: &DownloadId,
        state: DownloadState,
        error: Option<String>,
    ) -> AppResult<()> {
        let (id_s, st, now) = (id.to_string(), state_str(state).to_owned(), now());
        self.db
            .write(move |tx| {
                tx.execute(
                    "UPDATE model_downloads SET state = ?2, error = ?3, updated_at = ?4 WHERE id = ?1",
                    rusqlite::params![id_s, st, error, now],
                )?;
                Ok(())
            })
            .await
        .map_err(AppError::from)
    }

    async fn set_total(&self, id: &DownloadId, total: Option<u64>) -> AppResult<()> {
        let (id_s, now) = (id.to_string(), now());
        let total = total.and_then(|v| i64::try_from(v).ok());
        self.db
            .write(move |tx| {
                tx.execute(
                    "UPDATE model_downloads SET total_bytes = ?2, updated_at = ?3 WHERE id = ?1",
                    rusqlite::params![id_s, total, now],
                )?;
                Ok(())
            })
            .await
            .map_err(AppError::from)
    }

    async fn set_progress(&self, id: &DownloadId, downloaded: u64) -> AppResult<()> {
        let (id_s, now) = (id.to_string(), now());
        let dl = i64::try_from(downloaded).unwrap_or(i64::MAX);
        self.db
            .write(move |tx| {
                tx.execute(
                    "UPDATE model_downloads SET downloaded_bytes = ?2, updated_at = ?3 WHERE id = ?1",
                    rusqlite::params![id_s, dl, now],
                )?;
                Ok(())
            })
            .await
        .map_err(AppError::from)
    }
}

// ---------------------------------------------------------------- helpers

fn is_active(state: DownloadState) -> bool {
    matches!(
        state,
        DownloadState::Queued
            | DownloadState::Downloading
            | DownloadState::Paused
            | DownloadState::Verifying
    )
}

fn part_path(dest: &std::path::Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

fn now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

fn content_length(response: &reqwest::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)?
        .to_str()
        .ok()?
        .parse()
        .ok()
}

async fn sha256_file(path: &std::path::Path) -> AppResult<String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| AppError::internal("read for hashing", e))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn state_str(state: DownloadState) -> &'static str {
    match state {
        DownloadState::Queued => "Queued",
        DownloadState::Downloading => "Downloading",
        DownloadState::Paused => "Paused",
        DownloadState::Verifying => "Verifying",
        DownloadState::Complete => "Complete",
        DownloadState::Failed => "Failed",
    }
}

fn state_from_str(raw: &str) -> AppResult<DownloadState> {
    Ok(match raw {
        "Queued" => DownloadState::Queued,
        "Downloading" => DownloadState::Downloading,
        "Paused" => DownloadState::Paused,
        "Verifying" => DownloadState::Verifying,
        "Complete" => DownloadState::Complete,
        "Failed" => DownloadState::Failed,
        other => {
            return Err(AppError::internal(
                "download state",
                format!("unknown {other:?}"),
            ))
        }
    })
}

// ---------------------------------------------------------------- row plumbing

const COLUMNS: &str = "id, repo, revision, filename, kind, dest_path, total_bytes, \
    downloaded_bytes, sha256_expected, etag, state, error, created_at, updated_at";

const INSERT_SQL: &str = "INSERT INTO model_downloads \
    (id, repo, revision, filename, kind, dest_path, total_bytes, downloaded_bytes, \
     sha256_expected, etag, state, error, created_at, updated_at) \
    VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)";

#[allow(dead_code)] // some columns are carried but not all are read back
struct DownloadRow {
    id: String,
    repo: String,
    revision: String,
    filename: String,
    kind: String,
    dest_path: String,
    total_bytes: Option<i64>,
    downloaded_bytes: i64,
    sha256_expected: Option<String>,
    etag: Option<String>,
    state: String,
    error: Option<String>,
    created_at: String,
    updated_at: String,
}

impl DownloadRow {
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            repo: row.get(1)?,
            revision: row.get(2)?,
            filename: row.get(3)?,
            kind: row.get(4)?,
            dest_path: row.get(5)?,
            total_bytes: row.get(6)?,
            downloaded_bytes: row.get(7)?,
            sha256_expected: row.get(8)?,
            etag: row.get(9)?,
            state: row.get(10)?,
            error: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    }

    fn into_info(self) -> DownloadInfo {
        DownloadInfo {
            id: DownloadId::from_trusted(self.id),
            repo: self.repo,
            filename: self.filename,
            kind: kind_from_str(&self.kind),
            state: state_from_str(&self.state).unwrap_or(DownloadState::Failed),
            total_bytes: self.total_bytes.and_then(|v| u64::try_from(v).ok()),
            downloaded_bytes: u64::try_from(self.downloaded_bytes).unwrap_or(0),
            error: self.error,
        }
    }

    fn to_spec(&self) -> DownloadSpec {
        DownloadSpec {
            url: hf_resolve_url(&self.repo, &self.revision, &self.filename),
            repo: self.repo.clone(),
            revision: self.revision.clone(),
            filename: self.filename.clone(),
            kind: kind_from_str(&self.kind),
            dest_path: PathBuf::from(&self.dest_path),
            models_dir: PathBuf::from(&self.dest_path)
                .parent()
                .and_then(std::path::Path::parent)
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf(),
            expected_size: self.total_bytes.and_then(|v| u64::try_from(v).ok()),
            sha256_expected: self.sha256_expected.clone(),
            // Resume of a plain file; the GGUF re-registration on completion is
            // safe to skip (a resumed picker download re-registers via a fresh
            // start; a resumed fixed-model file's sibling triggers it).
            register: RegisterPlan::None,
        }
    }
}

impl DownloadSpec {
    fn to_row(&self, id: &DownloadId, now: &str) -> InsertRow {
        InsertRow {
            id: id.to_string(),
            repo: self.repo.clone(),
            revision: self.revision.clone(),
            filename: self.filename.clone(),
            kind: kind_str(self.kind).to_owned(),
            dest_path: self.dest_path.to_string_lossy().into_owned(),
            total_bytes: self.expected_size.and_then(|v| i64::try_from(v).ok()),
            downloaded_bytes: 0,
            sha256_expected: self.sha256_expected.clone(),
            etag: None,
            state: "Queued".to_owned(),
            error: None,
            created_at: now.to_owned(),
            updated_at: now.to_owned(),
        }
    }
}

struct InsertRow {
    id: String,
    repo: String,
    revision: String,
    filename: String,
    kind: String,
    dest_path: String,
    total_bytes: Option<i64>,
    downloaded_bytes: i64,
    sha256_expected: Option<String>,
    etag: Option<String>,
    state: String,
    error: Option<String>,
    created_at: String,
    updated_at: String,
}

fn kind_str(kind: ModelKind) -> &'static str {
    match kind {
        ModelKind::Llm => "Llm",
        ModelKind::Stt => "Stt",
        ModelKind::Tts => "Tts",
        ModelKind::Image => "Image",
        ModelKind::Embedder => "Embedder",
    }
}

fn kind_from_str(raw: &str) -> ModelKind {
    match raw {
        "Stt" => ModelKind::Stt,
        "Tts" => ModelKind::Tts,
        "Image" => ModelKind::Image,
        "Embedder" => ModelKind::Embedder,
        _ => ModelKind::Llm,
    }
}

/// The HuggingFace `resolve` URL for a file.
#[must_use]
pub fn hf_resolve_url(repo: &str, revision: &str, filename: &str) -> String {
    format!("https://huggingface.co/{repo}/resolve/{revision}/{filename}")
}
