//! Structured logging + diagnostics for the core (Phase 10; formalizes the
//! Phase 6 seed). Policy: `SECURITY.md` (no secrets / no conversation content in
//! logs), `PERFORMANCE.md` (cheap on the hot path), ADR-0015 (no telemetry).
//!
//! - JSON lines to **stdout** through a non-blocking, bounded, **lossy** writer
//!   (a token-stream burst drops lines with a count rather than blocking).
//! - Secret **redaction on every line** at the write boundary.
//! - An in-memory **ring buffer** of the last [`RING_CAPACITY`] lines
//!   ([`recent_lines`]) — for tests and the Phase 37 diagnostics bundle.
//! - Level from `LOCALAI_LOG` (dev override, wins) or config ([`set_level`]),
//!   hot-reloadable.
//! - **No network sink, ever.**

use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use regex::Regex;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::{fmt, prelude::*, reload, EnvFilter, Registry};

use crate::contracts::ids::TaskId;
use crate::ipc::{AppError, AppResult};

/// How many recent formatted log lines to keep in memory.
pub const RING_CAPACITY: usize = 256;

/// File-log rotation base name — rolled daily to `localai.jsonl.<YYYY-MM-DD>`.
const LOG_FILE_PREFIX: &str = "localai.jsonl";
/// Keep at most this many rolled log files.
const LOG_KEEP_FILES: usize = 7;
/// …and drop the oldest while the directory exceeds this.
const LOG_MAX_BYTES: u64 = 50 * 1024 * 1024;

type FilterHandle = reload::Handle<EnvFilter, Registry>;

static RING: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static RELOAD: OnceLock<FilterHandle> = OnceLock::new();
static GUARD: OnceLock<WorkerGuard> = OnceLock::new();
static ENV_OVERRIDE: OnceLock<bool> = OnceLock::new();
static INIT: OnceLock<()> = OnceLock::new();
/// The optional on-disk sink (Phase 18.5). `None` until [`enable_file_sink`].
static FILE: OnceLock<NonBlocking> = OnceLock::new();
static FILE_GUARD: OnceLock<WorkerGuard> = OnceLock::new();
static FILE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Initialize the global subscriber. Idempotent — safe to call from tests.
pub fn init() {
    if INIT.set(()).is_err() {
        return;
    }

    let (from_env, filter) = match EnvFilter::try_from_env("LOCALAI_LOG") {
        Ok(filter) => (true, filter),
        Err(_) => (false, EnvFilter::new("info")),
    };
    let _ = ENV_OVERRIDE.set(from_env);

    let (filter_layer, handle) = reload::Layer::new(filter);
    let _ = RELOAD.set(handle);

    let (non_blocking, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .lossy(true)
        .buffered_lines_limit(8192)
        .finish(io::stdout());
    let _ = GUARD.set(guard);

    let fmt_layer = fmt::layer()
        .json()
        .with_timer(fmt::time::UtcTime::rfc_3339())
        .with_target(true)
        .with_current_span(true)
        .with_span_list(false)
        .with_writer(move || RedactWriter::new(non_blocking.clone()));

    let _ = Registry::default()
        .with(filter_layer)
        .with(fmt_layer)
        .try_init();
}

// ---------------------------------------------------------------- file sink

/// Start writing the (already-redacted) log stream to a rotating JSON-lines file
/// under `dir` (Phase 18.5). Idempotent — a second call is a no-op. Runs a
/// startup sweep so the directory does not grow without bound.
///
/// # Errors
/// [`AppError::internal`] if `dir` cannot be created.
pub fn enable_file_sink(dir: &Path) -> AppResult<PathBuf> {
    if let Some(existing) = FILE_DIR.get() {
        return Ok(existing.clone());
    }
    std::fs::create_dir_all(dir).map_err(|e| AppError::internal("create log dir", e))?;
    sweep_logs(dir);

    let appender = tracing_appender::rolling::daily(dir, LOG_FILE_PREFIX);
    let (non_blocking, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .lossy(true)
        .buffered_lines_limit(8192)
        .finish(appender);
    let _ = FILE.set(non_blocking);
    let _ = FILE_GUARD.set(guard);
    let _ = FILE_DIR.set(dir.to_path_buf());

    tracing::info!(dir = %dir.display(), "log file sink enabled");
    Ok(dir.to_path_buf())
}

/// The active log directory, once [`enable_file_sink`] has run.
#[must_use]
pub fn log_dir() -> Option<PathBuf> {
    FILE_DIR.get().cloned()
}

/// Keep the newest [`LOG_KEEP_FILES`] rolled files, then drop oldest while the
/// directory still exceeds [`LOG_MAX_BYTES`].
fn sweep_logs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, u64)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            if !name.starts_with(LOG_FILE_PREFIX) {
                return None;
            }
            let len = e.metadata().ok()?.len();
            Some((path, len))
        })
        .collect();
    // Names embed the date (`localai.jsonl.2026-09-06`) → lexical sort == age.
    files.sort_by(|a, b| a.0.cmp(&b.0));

    while files.len() > LOG_KEEP_FILES {
        let (path, _) = files.remove(0);
        let _ = std::fs::remove_file(path);
    }
    let mut total: u64 = files.iter().map(|(_, n)| n).sum();
    while total > LOG_MAX_BYTES && files.len() > 1 {
        let (path, n) = files.remove(0);
        let _ = std::fs::remove_file(path);
        total = total.saturating_sub(n);
    }
}

// ---------------------------------------------------------------- level

/// Known bare level names.
const LEVELS: [&str; 6] = ["trace", "debug", "info", "warn", "error", "off"];

/// Parse a `tracing` filter directive (e.g. `info`, `warn,localai=debug`).
///
/// `EnvFilter` is permissive — a bare unknown word parses as a target at the
/// default level. So a directive with no per-target part (`,` / `=`) is
/// additionally required to be one of [`LEVELS`].
///
/// # Errors
/// [`AppError::Validation`] naming `logging.level` if the directive is invalid.
pub fn validate_directive(directive: &str) -> AppResult<EnvFilter> {
    let directive = directive.trim();
    if directive.is_empty() {
        return Err(AppError::Validation(
            "logging.level must not be empty".to_owned(),
        ));
    }
    if !directive.contains([',', '=']) && !LEVELS.contains(&directive.to_ascii_lowercase().as_str())
    {
        return Err(AppError::Validation(format!(
            "logging.level: unknown level {directive:?} (expected one of {LEVELS:?})"
        )));
    }
    EnvFilter::builder()
        .parse(directive)
        .map_err(|err| AppError::Validation(format!("logging.level: {err}")))
}

/// Swap the active level filter. A no-op when `LOCALAI_LOG` is set (the dev
/// override wins).
///
/// # Errors
/// [`AppError::Validation`] for an invalid directive.
pub fn set_level(directive: &str) -> AppResult<()> {
    if ENV_OVERRIDE.get().copied().unwrap_or(false) {
        tracing::debug!("LOCALAI_LOG is set — ignoring configured logging.level");
        return Ok(());
    }
    let filter = validate_directive(directive)?;
    if let Some(handle) = RELOAD.get() {
        handle
            .reload(filter)
            .map_err(|err| AppError::internal("reload log filter", err))?;
    }
    Ok(())
}

// ---------------------------------------------------------------- ring buffer

/// The most recent formatted (already-redacted) log lines, oldest first.
#[must_use]
pub fn recent_lines() -> Vec<String> {
    RING.lock()
        .map(|ring| ring.iter().cloned().collect())
        .unwrap_or_default()
}

fn push_ring(line: &str) {
    if let Ok(mut ring) = RING.lock() {
        if ring.len() >= RING_CAPACITY {
            ring.pop_front();
        }
        ring.push_back(line.to_owned());
    }
}

// ---------------------------------------------------------------- redaction

fn redactors() -> &'static [(Regex, &'static str)] {
    static SET: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    SET.get_or_init(|| {
        vec![
            // Bearer credentials (before the hf_ rule, which would otherwise
            // shorten an `hf_`-prefixed bearer token below the match threshold).
            (
                Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{8,}").unwrap(),
                "Bearer ***",
            ),
            // HuggingFace-style access tokens.
            (Regex::new(r"hf_[A-Za-z0-9]{20,}").unwrap(), "hf_***"),
            // Secret-ish JSON fields, whatever the value.
            (
                Regex::new(
                    r#"(?i)"(token|secret|password|api_?key|authorization|hf_token)"\s*:\s*"[^"]*""#,
                )
                .unwrap(),
                r#""$1":"***""#,
            ),
            // Secret-ish `key=value` pairs embedded in free text (not the quoted
            // JSON `"key":"value"` form — that is handled above).
            (
                Regex::new(r#"(?i)\b(password|passwd|secret|api_?key|token)\s*=\s*[^\s",}]+"#)
                    .unwrap(),
                "$1=***",
            ),
        ]
    })
}

/// Redact known secret shapes from one log line.
#[must_use]
pub fn redact_line(line: &str) -> String {
    let mut out = line.to_owned();
    for (re, repl) in redactors() {
        out = re.replace_all(&out, *repl).into_owned();
    }
    out
}

/// A `Write` that redacts each line, tees it into the ring buffer, then forwards
/// to the non-blocking stdout writer. The `fmt` layer creates one per event and
/// drops it after writing, so the flush-to-inner happens on `Drop`.
struct RedactWriter {
    inner: NonBlocking,
    buf: Vec<u8>,
}

impl RedactWriter {
    fn new(inner: NonBlocking) -> Self {
        Self {
            inner,
            buf: Vec::with_capacity(512),
        }
    }
}

impl Write for RedactWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Drop for RedactWriter {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        let raw = String::from_utf8_lossy(&self.buf);
        let redacted = redact_line(&raw);
        push_ring(redacted.trim_end_matches(['\n', '\r']));
        let _ = self.inner.write_all(redacted.as_bytes());
        if let Some(file) = FILE.get() {
            let _ = file.clone().write_all(redacted.as_bytes());
        }
    }
}

// ---------------------------------------------------------------- operations

/// Terminal status of an [`Operation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Completed successfully.
    Ok,
    /// Cancelled by the caller.
    Cancelled,
    /// Failed.
    Failed,
}

impl Status {
    #[must_use]
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

/// An instrumented unit of work. Open one with [`operation`], attach its
/// [`Operation::span`] to the async work, then call [`Operation::finish`]. A
/// dropped-without-finish operation is logged as `failed`.
#[must_use = "call `.finish(status)` when the operation completes"]
pub struct Operation {
    span: tracing::Span,
    started: Instant,
    finished: bool,
}

/// Begin an instrumented operation. Every event emitted within
/// [`Operation::span`] carries `op` and `task_id`.
pub fn operation(task_id: Option<&TaskId>, name: &'static str) -> Operation {
    let task_id = task_id.map_or("-", TaskId::as_str);
    let span = tracing::info_span!("operation", op = name, task_id = task_id);
    Operation {
        span,
        started: Instant::now(),
        finished: false,
    }
}

impl Operation {
    /// The span to attach to the operation's (async) work via
    /// `.instrument(op.span())` or `let _g = op.span().enter();`.
    #[must_use]
    pub fn span(&self) -> tracing::Span {
        self.span.clone()
    }

    /// Log completion with an elapsed time and status.
    pub fn finish(mut self, status: Status) {
        self.emit(status);
        self.finished = true;
    }

    fn emit(&self, status: Status) {
        let _guard = self.span.enter();
        tracing::info!(
            elapsed_ms = self.started.elapsed().as_secs_f64() * 1000.0,
            status = status.as_str(),
            "operation finished"
        );
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        if !self.finished {
            self.emit(Status::Failed);
        }
    }
}

// ---------------------------------------------------------------- content policy

/// A safe breadcrumb for user text (message / prompt / transcript): its length
/// and a non-cryptographic hash — **never the text itself**. The full text may
/// only be logged at `trace!`.
#[must_use]
pub fn content_preview(text: &str) -> String {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{} chars (h{:08x})", text.chars().count(), hasher.finish())
}

#[cfg(test)]
mod tests;
