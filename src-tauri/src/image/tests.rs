//! `image::` adapter coverage against the stdlib fake sidecar
//! (`image_gen/server_fake.py`) — no venv, no GPU, no real model.

use std::path::PathBuf;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::server::SidecarArgs;
use super::Krea2Backend;
use crate::contracts::ids::ModelId;
use crate::contracts::model::{
    Device, ModelBackend as ModelBackendName, ModelCapabilities, ModelKind, ModelMetadata, Quant,
    RegisteredModel, RegistryAvailability,
};
use crate::ipc::AppError;
use crate::lifecycle::backend::{ImageGenerateArgs, LoadRequest, ModelBackend};

/// An absolute `python` path — repo venv first, then `where`/`which`.
fn which_python() -> Option<PathBuf> {
    let venv = repo_root().join(".venv").join(if cfg!(windows) {
        "Scripts/python.exe"
    } else {
        "bin/python"
    });
    if venv.is_file() {
        return Some(venv);
    }
    let finder = if cfg!(windows) { "where" } else { "which" };
    for cand in ["python3", "python", "py"] {
        if let Ok(out) = std::process::Command::new(finder).arg(cand).output() {
            if out.status.success() {
                if let Some(line) = String::from_utf8_lossy(&out.stdout).lines().next() {
                    let p = PathBuf::from(line.trim());
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fake_script() -> PathBuf {
    repo_root().join("image_gen").join("server_fake.py")
}

fn krea2_model() -> RegisteredModel {
    RegisteredModel {
        metadata: ModelMetadata {
            id: ModelId::from_trusted("krea2"),
            display_name: "Krea 2 Turbo".to_owned(),
            kind: ModelKind::Image,
            backend: ModelBackendName(super::BACKEND_KEY.to_owned()),
            quant: Some(Quant("nf4".to_owned())),
            capabilities: ModelCapabilities {
                streaming: false,
                context_tokens: None,
            },
            estimated_vram_mb: Some(11_750),
        },
        path: "C:/models/image/krea2".into(),
        availability: RegistryAvailability::Ready,
        devices: vec![Device::Cuda],
    }
}

fn backend(exchange: &std::path::Path) -> Option<Krea2Backend> {
    Some(Krea2Backend::new(
        which_python()?,
        fake_script(),
        None,
        None,
        exchange.to_path_buf(),
    ))
}

#[tokio::test]
async fn to_argv_carries_the_flags_and_extras() {
    let args = SidecarArgs {
        python: "py".into(),
        script: "server.py".into(),
        port: 8751,
        token: "tok".into(),
        model_path: Some("C:/m".into()),
        quant_cache: Some("C:/q".into()),
        loras_dir: Some("C:/l".into()),
        extra: vec!["--protocol".into(), "9".into()],
    };
    let flags = args.to_argv();
    assert_eq!(flags[0], "server.py");
    assert!(flags.windows(2).any(|w| w == ["--port", "8751"]));
    assert!(flags.windows(2).any(|w| w == ["--token", "tok"]));
    assert!(flags.windows(2).any(|w| w == ["--model-path", "C:/m"]));
    assert!(flags.windows(2).any(|w| w == ["--protocol", "9"]));
    assert_eq!(args.base_url(), "http://127.0.0.1:8751");
}

#[tokio::test]
async fn backend_loads_generates_and_shuts_down_cleanly() {
    let exchange = tempfile::tempdir().unwrap();
    let Some(backend) = backend(exchange.path()) else {
        eprintln!("no python — skipping");
        return;
    };
    let cancel = CancellationToken::new();
    let instance = backend
        .load(
            &LoadRequest {
                model: krea2_model(),
            },
            cancel,
        )
        .await
        .expect("load");

    instance.health().await.expect("healthy");
    assert_eq!(instance.measured_vram_mb(), Some(11_750));

    let img = instance.as_image().expect("as_image");
    let (tx, mut rx) = mpsc::unbounded_channel();
    let pngs = img
        .generate(
            ImageGenerateArgs {
                prompt: "a lighthouse".to_owned(),
                negative: None,
                width: 1024,
                height: 1024,
                // Enough fake work (~1.2 s) to outlast a few 250 ms progress polls.
                steps: 60,
                guidance: 0.0,
                seed: 100,
                batch_count: 2,
                lora: None,
            },
            tx,
            CancellationToken::new(),
        )
        .await
        .expect("generate");

    assert_eq!(pngs.len(), 2);
    assert_eq!(pngs[0].seed, 100);
    assert_eq!(pngs[1].seed, 101);
    assert_eq!(&pngs[0].bytes[1..4], b"PNG", "not a PNG");
    // At least one progress frame arrived.
    let mut saw_progress = false;
    while rx.try_recv().is_ok() {
        saw_progress = true;
    }
    assert!(saw_progress, "no progress frames forwarded");

    // The per-call exchange dir was cleaned up.
    let leftovers: Vec<_> = std::fs::read_dir(exchange.path())
        .unwrap()
        .flatten()
        .collect();
    assert!(
        leftovers.is_empty(),
        "exchange dir not cleaned: {leftovers:?}"
    );

    instance.shutdown().await;
    // The child is gone — /health no longer answers.
    assert!(
        instance.health().await.is_err(),
        "sidecar still alive after shutdown"
    );
}

#[tokio::test]
async fn backend_refuses_a_protocol_mismatch() {
    let exchange = tempfile::tempdir().unwrap();
    let Some(backend) = backend(exchange.path())
        .map(|b| b.with_extra_args(["--protocol".to_owned(), "99".to_owned()]))
    else {
        eprintln!("no python — skipping");
        return;
    };
    let Err(err) = backend
        .load(
            &LoadRequest {
                model: krea2_model(),
            },
            CancellationToken::new(),
        )
        .await
    else {
        panic!("expected the load to be refused");
    };
    assert!(
        matches!(&err, AppError::BackendUnavailable(m) if m.contains("protocol")),
        "unexpected: {err:?}"
    );
}

#[tokio::test]
async fn load_is_cancellable() {
    let exchange = tempfile::tempdir().unwrap();
    let Some(backend) = backend(exchange.path()) else {
        return;
    };
    let cancel = CancellationToken::new();
    cancel.cancel();
    let Err(err) = backend
        .load(
            &LoadRequest {
                model: krea2_model(),
            },
            cancel,
        )
        .await
    else {
        panic!("expected cancellation");
    };
    assert!(matches!(err, AppError::Cancelled));
}
