//! Phase 10 gate coverage.
//!
//! Redaction and `content_preview` are tested as pure functions. Span/level
//! behaviour is tested against a **thread-local** capture subscriber
//! (`with_default`) so tests do not fight the global subscriber or each other.
//! The one test that touches the global reload handle runs under a lock.

use std::io::Write;
use std::sync::{Arc, Mutex};

use tracing::subscriber::with_default;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::{fmt, prelude::*, Registry};

use super::{
    content_preview, enable_file_sink, operation, push_ring, recent_lines, redact_line, set_level,
    sweep_logs, Status, LOG_KEEP_FILES,
};
use crate::contracts::ids::TaskId;
use crate::ipc::AppError;

static GLOBAL_LOCK: Mutex<()> = Mutex::new(());

// ---------- a thread-local capture subscriber ----------

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Capture {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Capture {
    type Writer = Capture;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

fn capture_subscriber(cap: &Capture, directive: &str) -> impl tracing::Subscriber {
    Registry::default().with(
        fmt::layer()
            .json()
            .with_current_span(true)
            .with_writer(cap.clone())
            .with_filter(tracing_subscriber::EnvFilter::new(directive)),
    )
}

// ---------------------------------------------------------------- redaction

#[test]
fn redaction_removes_every_secret_shape() {
    let hf = format!("hf_{}", "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7");
    let raw = format!(
        r#"{{"msg":"tok {hf} auth Authorization: Bearer {hf}xyz and password=hunter2","token":"{hf}","api_key":"sk-secret-123"}}"#
    );
    let out = redact_line(&raw);

    assert!(!out.contains(&hf), "hf_ token leaked: {out}");
    assert!(!out.contains("hunter2"), "password leaked: {out}");
    assert!(!out.contains("sk-secret-123"), "api_key leaked: {out}");
    assert!(out.contains("hf_***"));
    assert!(out.contains("Bearer ***"));
    assert!(out.contains("password=***"));
    assert!(out.contains(r#""token":"***""#));
    assert!(out.contains(r#""api_key":"***""#));
}

#[test]
fn redact_line_is_idempotent() {
    let once = redact_line(r#"{"token":"hf_ABCDEFGHIJKLMNOPQRSTUVWX1234"}"#);
    assert_eq!(redact_line(&once), once);
    assert!(once.contains("***"));
}

#[test]
fn redact_line_leaves_clean_lines_untouched() {
    let clean = r#"{"level":"INFO","fields":{"message":"database ready","schema_version":1}}"#;
    assert_eq!(redact_line(clean), clean);
}

// ---------------------------------------------------------------- operations

#[test]
fn operation_events_all_carry_the_task_id() {
    let cap = Capture::default();
    let task_id = TaskId::from_trusted("task-7c1d");

    with_default(capture_subscriber(&cap, "info"), || {
        let op = operation(Some(&task_id), "demo_op");
        let span = op.span();
        let entered = span.enter();
        tracing::info!("first");
        tracing::info!("second");
        drop(entered);
        op.finish(Status::Ok);
    });

    let text = cap.text();
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    assert!(
        lines.len() >= 3,
        "want first + second + finish, got: {text}"
    );
    for line in &lines {
        assert!(line.contains("task-7c1d"), "line missing task id: {line}");
        assert!(line.contains("demo_op"), "line missing op name: {line}");
    }
    let finish = lines
        .iter()
        .find(|l| l.contains("operation finished"))
        .unwrap();
    assert!(finish.contains("elapsed_ms"));
    assert!(finish.contains(r#""status":"ok""#));
}

#[test]
fn dropped_operation_is_logged_as_failed() {
    let cap = Capture::default();
    with_default(capture_subscriber(&cap, "info"), || {
        let op = operation(None, "abandoned");
        let span = op.span();
        let entered = span.enter();
        tracing::info!("did some work");
        drop(entered);
        drop(op); // no finish()
    });

    let text = cap.text();
    let finish = text
        .lines()
        .find(|l| l.contains("operation finished"))
        .expect("a finish line for the dropped op");
    assert!(finish.contains(r#""status":"failed""#), "got: {finish}");
}

#[test]
fn content_below_info_is_not_emitted_at_default_level() {
    let cap = Capture::default();
    with_default(capture_subscriber(&cap, "info"), || {
        tracing::trace!(content = "the private message body", "content trace");
        tracing::info!("an info breadcrumb");
    });
    let text = cap.text();
    assert!(
        !text.contains("the private message body"),
        "content leaked at info: {text}"
    );
    assert!(text.contains("an info breadcrumb"));
}

#[test]
fn app_error_log_emits_a_structured_line() {
    let cap = Capture::default();
    with_default(capture_subscriber(&cap, "info"), || {
        AppError::Validation("bad nonce".to_owned()).log("app_ping");
    });
    let text = cap.text();
    assert!(text.contains(r#""kind":"Validation""#), "got: {text}");
    assert!(text.contains(r#""context":"app_ping""#));
}

// ---------------------------------------------------------------- level

#[test]
fn set_level_reloads_the_global_filter() {
    let _lock = GLOBAL_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::env::remove_var("LOCALAI_LOG");
    super::init();

    set_level("warn").expect("valid");
    let current = super::RELOAD
        .get()
        .unwrap()
        .clone_current()
        .map(|f| f.to_string())
        .unwrap_or_default();
    assert!(current.contains("warn"), "filter not reloaded: {current}");

    set_level("info").expect("restore");
    assert!(set_level("verbose").is_err(), "bare non-level accepted");
    assert!(super::validate_directive("warn,localai=debug").is_ok());
}

// ---------------------------------------------------------------- misc

#[test]
fn content_preview_never_contains_the_text() {
    let text = "the user said something private and identifying";
    let preview = content_preview(text);
    assert!(!preview.contains("private") && !text.contains(&preview));
    assert!(preview.contains("chars"));
    assert_eq!(preview, content_preview(text));
}

#[test]
fn ring_buffer_is_bounded_and_ordered() {
    // The ring is global; other parallel tests' log lines may interleave, so
    // assert the invariants, not exact contents.
    let _lock = GLOBAL_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let marker = "ringtest-8a3d";
    let total = super::RING_CAPACITY + 50;
    for i in 0..total {
        push_ring(&format!("{marker} {i}"));
    }
    let lines = recent_lines();
    assert!(lines.len() <= super::RING_CAPACITY);
    assert_eq!(lines.len(), super::RING_CAPACITY, "ring should be full");

    // Our surviving lines are a contiguous tail, still in push order.
    let mine: Vec<usize> = lines
        .iter()
        .filter_map(|l| l.strip_prefix(&format!("{marker} ")))
        .filter_map(|n| n.parse().ok())
        .collect();
    assert!(!mine.is_empty());
    assert!(mine.windows(2).all(|w| w[0] < w[1]), "ring lost push order");
    assert_eq!(*mine.last().unwrap(), total - 1);
}

// ---------------------------------------------------------------- file sink (Phase 18.5)

#[test]
fn sweep_logs_keeps_the_newest_files() {
    let dir = tempfile::tempdir().unwrap();
    for day in 1..=12 {
        std::fs::write(
            dir.path().join(format!("localai.jsonl.2026-09-{day:02}")),
            b"x",
        )
        .unwrap();
    }
    // An unrelated file is left alone.
    std::fs::write(dir.path().join("notes.txt"), b"keep me").unwrap();

    sweep_logs(dir.path());

    let mut kept: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("localai.jsonl"))
        .collect();
    kept.sort();
    assert_eq!(kept.len(), LOG_KEEP_FILES);
    assert_eq!(kept.last().unwrap(), "localai.jsonl.2026-09-12"); // newest survives
    assert_eq!(kept.first().unwrap(), "localai.jsonl.2026-09-06"); // 12 - 7 + 1
    assert!(dir.path().join("notes.txt").is_file());
}

#[test]
fn sweep_logs_caps_total_size() {
    let dir = tempfile::tempdir().unwrap();
    let big = vec![b'x'; 30 * 1024 * 1024];
    for day in 1..=3 {
        std::fs::write(
            dir.path().join(format!("localai.jsonl.2026-09-{day:02}")),
            &big,
        )
        .unwrap();
    }
    sweep_logs(dir.path()); // 90 MB > 50 MB cap, and 3 <= keep-7

    let remaining: u64 = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.metadata().unwrap().len())
        .sum();
    assert!(remaining <= 60 * 1024 * 1024, "still {remaining} bytes");
    // Never deletes the last file even if it alone exceeds the cap.
    assert!(std::fs::read_dir(dir.path()).unwrap().count() >= 1);
}

#[test]
fn enable_file_sink_creates_the_dir_and_is_idempotent() {
    // NB: writes to a process-global OnceLock — first caller wins for the whole
    // test binary, so assert only on the returned path + dir existence.
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("logs");
    let first = enable_file_sink(&target);
    assert!(first.is_ok());
    let p = first.unwrap();
    assert!(p.is_dir());
    // Second call: no error, returns a path (possibly the first process winner).
    assert!(enable_file_sink(&target).is_ok());
}
