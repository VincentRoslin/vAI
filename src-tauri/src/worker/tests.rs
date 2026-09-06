//! `WorkerSupervisor` coverage against the stdlib fake worker
//! (`workers/stt_fake.py`) — no venv, no GPU.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::layout::WorkerLayout;
use super::{RestartPolicy, WorkerSupervisor};
use crate::contracts::worker::WorkerKind;

/// The repo's `workers/` dir (this crate is `src-tauri/`).
fn repo_workers() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("workers")
}

/// A layout whose `stt.py` is really the fake, run by whichever `python` is on
/// PATH. Returns `None` when no interpreter is available (skips the test).
fn fake_layout(dir: &std::path::Path) -> Option<WorkerLayout> {
    let python = which_python()?;
    std::fs::copy(repo_workers().join("stt_fake.py"), dir.join("stt.py")).unwrap();
    Some(WorkerLayout::for_test(python, dir))
}

/// An **absolute** python path (`script_for` requires `python.is_file()`). Tries
/// the repo venv, then `where`/`which`.
fn which_python() -> Option<PathBuf> {
    let venv = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".venv")
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(if cfg!(windows) {
            "python.exe"
        } else {
            "python"
        });
    if venv.is_file() {
        return Some(venv);
    }
    let finder = if cfg!(windows) { "where" } else { "which" };
    for cand in ["python", "python3", "py"] {
        if let Ok(out) = std::process::Command::new(finder).arg(cand).output() {
            if out.status.success() {
                if let Some(first) = String::from_utf8_lossy(&out.stdout).lines().next() {
                    let p = PathBuf::from(first.trim());
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

#[tokio::test]
async fn handshake_then_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let Some(layout) = fake_layout(dir.path()) else {
        eprintln!("no python on PATH — skipping");
        return;
    };
    let sup = WorkerSupervisor::new(layout, WorkerKind::Stt);
    let cancel = CancellationToken::new();

    let out = sup
        .request(json!({ "audio_path": "x.wav" }), &cancel, None)
        .await
        .expect("request ok");
    assert_eq!(
        out["text"], "hello local AI this is a fake transcript",
        "canned transcript"
    );
    assert_eq!(out["language"], "en");
    sup.shutdown().await;
}

#[tokio::test]
async fn progress_frames_are_forwarded() {
    let dir = tempfile::tempdir().unwrap();
    let Some(layout) = fake_layout(dir.path()) else {
        return;
    };
    let sup = WorkerSupervisor::new(layout, WorkerKind::Stt);
    let cancel = CancellationToken::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let out = sup
        .request(
            json!({ "audio_path": "x", "mode": "progress" }),
            &cancel,
            Some(tx),
        )
        .await
        .unwrap();
    assert_eq!(out["text"], "hello local AI this is a fake transcript");

    let mut seen = Vec::new();
    while let Ok((p, _)) = rx.try_recv() {
        seen.push(p);
    }
    assert_eq!(seen, vec![0.25, 0.5, 0.75]);
}

#[tokio::test]
async fn worker_error_is_returned_as_apperror() {
    let dir = tempfile::tempdir().unwrap();
    let Some(layout) = fake_layout(dir.path()) else {
        return;
    };
    let sup = WorkerSupervisor::new(layout, WorkerKind::Stt);
    let cancel = CancellationToken::new();

    // `crash` mode exits the process mid-job → the in-flight request fails.
    let err = sup
        .request(json!({ "audio_path": "x", "mode": "crash" }), &cancel, None)
        .await
        .expect_err("crash → error");
    assert_eq!(err.kind_str(), "BackendUnavailable");

    // Next request restarts the worker and succeeds.
    let out = sup
        .request(json!({ "audio_path": "x" }), &cancel, None)
        .await
        .expect("recovered");
    assert_eq!(out["text"], "hello local AI this is a fake transcript");
    sup.shutdown().await;
}

#[tokio::test]
async fn protocol_version_mismatch_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let Some(python) = which_python() else { return };
    std::fs::write(
        dir.path().join("stt.py"),
        "import json,sys\nprint(json.dumps({'protocol_version': 999, 'worker': 'Stt'}))\nsys.stdout.flush()\nfor _ in sys.stdin: pass\n",
    )
    .unwrap();
    let sup = WorkerSupervisor::with_policy(
        WorkerLayout::for_test(python, dir.path()),
        WorkerKind::Stt,
        RestartPolicy {
            max_restarts: 0,
            backoff: Duration::from_millis(1),
        },
    );
    let err = sup
        .request(json!({}), &CancellationToken::new(), None)
        .await
        .expect_err("mismatch");
    assert_eq!(err.kind_str(), "BackendUnavailable");
}

#[tokio::test]
async fn cancel_abandons_the_wait() {
    let dir = tempfile::tempdir().unwrap();
    let Some(python) = which_python() else { return };
    // A worker that says hello then never answers.
    std::fs::write(
        dir.path().join("stt.py"),
        "import json,sys,time\nprint(json.dumps({'protocol_version': 1, 'worker': 'Stt'}))\nsys.stdout.flush()\nfor _ in sys.stdin:\n    time.sleep(30)\n",
    )
    .unwrap();
    let sup = WorkerSupervisor::new(WorkerLayout::for_test(python, dir.path()), WorkerKind::Stt);
    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        c2.cancel();
    });
    let err = sup
        .request(json!({}), &cancel, None)
        .await
        .expect_err("cancelled");
    assert_eq!(err.kind_str(), "Cancelled");
    sup.shutdown().await;
}

#[tokio::test]
async fn missing_interpreter_is_a_clean_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("stt.py"), "print('x')").unwrap();
    let sup = WorkerSupervisor::with_policy(
        WorkerLayout::for_test(dir.path().join("nope.exe"), dir.path()),
        WorkerKind::Stt,
        RestartPolicy {
            max_restarts: 0,
            backoff: Duration::from_millis(1),
        },
    );
    let err = sup
        .request(json!({}), &CancellationToken::new(), None)
        .await
        .expect_err("no interpreter");
    assert_eq!(err.kind_str(), "BackendUnavailable");
}
