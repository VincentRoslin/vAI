//! `image::` adapter coverage against the stdlib fake sidecar
//! (`image_gen/server_fake.py`) — no venv, no GPU, no real model.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::repo::{ImageRepo, NewGeneratedImage};
use super::server::SidecarArgs;
use super::Krea2Backend;
use crate::contracts::ids::{AssetId, ModelId};
use crate::contracts::model::{
    Device, ModelBackend as ModelBackendName, ModelCapabilities, ModelKind, ModelMetadata, Quant,
    RegisteredModel, RegistryAvailability,
};
use crate::db::Db;
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

// ---------------------------------------------------------------- ImageRepo

async fn repo() -> (ImageRepo, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(&tmp.path().join("img.db")).await.unwrap();
    db.migrate().await.unwrap();
    (ImageRepo::new(Arc::new(db)), tmp)
}

/// Write a fake safetensors file with a header containing `keys`.
fn write_lora(dir: &std::path::Path, name: &str, keys: &str) {
    let header = format!("{{{keys}}}");
    let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
    bytes.extend(header.as_bytes());
    std::fs::write(dir.join(name), bytes).unwrap();
}

#[tokio::test]
async fn seed_registers_present_loras_and_presets_idempotently() {
    let (repo, tmp) = repo().await;
    let loras = tmp.path().join("loras");
    std::fs::create_dir_all(&loras).unwrap();
    write_lora(
        &loras,
        "krea2-realism.safetensors",
        "\"transformer.x.lora_A.weight\":[]",
    );
    write_lora(
        &loras,
        "krea2-lustify-nsfw.safetensors",
        "\"x.lora_down.weight\":[]",
    );
    write_lora(&loras, "some-community.safetensors", "\"junk\":[]");

    repo.seed(&loras).await.unwrap();
    repo.seed(&loras).await.unwrap(); // idempotent

    let mut got = repo.list_loras().await.unwrap();
    got.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    let names: Vec<_> = got.iter().map(|l| l.display_name.as_str()).collect();
    assert_eq!(names, ["Lustify (NSFW)", "Realism", "Some Community"]);
    assert!(got.iter().all(|l| l.base_compat == "krea2"));
    assert_eq!(
        got.iter()
            .find(|l| l.display_name == "Realism")
            .unwrap()
            .tags,
        vec!["realism".to_owned()]
    );

    let presets = repo.list_presets().await.unwrap();
    assert_eq!(presets.len(), 3);
    assert!(presets
        .iter()
        .any(|p| p.name == "Square 1024" && p.params.width == 1024));
}

#[tokio::test]
async fn seed_skips_absent_files_and_resolve_reports_unknown() {
    let (repo, tmp) = repo().await;
    repo.seed(&tmp.path().join("does-not-exist")).await.unwrap();
    assert!(repo.list_loras().await.unwrap().is_empty());
    let err = repo
        .resolve_lora(&crate::contracts::ids::ImageLoraId::from_trusted("nope"))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn record_and_list_generations_round_trip() {
    let (repo, tmp) = repo().await;
    let loras = tmp.path().join("loras");
    std::fs::create_dir_all(&loras).unwrap();
    write_lora(
        &loras,
        "krea2-realism.safetensors",
        "\"x.lora_A.weight\":[]",
    );
    repo.seed(&loras).await.unwrap();
    let lora_id = repo.list_loras().await.unwrap()[0].id.clone();

    let asset = AssetId::from_trusted("d".repeat(64));
    repo.record_generation(NewGeneratedImage {
        asset: asset.clone(),
        byte_len: 2048,
        prompt: "a castle".to_owned(),
        negative: None,
        width: 1024,
        height: 1024,
        steps: 8,
        guidance: 0.0,
        seed: 7,
        lora: Some((lora_id.clone(), 0.9)),
        model_id: ModelId::from_trusted("krea2"),
    })
    .await
    .unwrap();

    let rows = repo.list_generated(10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].asset, asset);
    assert_eq!(rows[0].seed, 7);
    assert_eq!(rows[0].lora.as_deref(), Some("Realism"));
    assert!(repo
        .known_asset_ids()
        .await
        .unwrap()
        .contains(asset.as_str()));

    // resolve_lora feeds the orchestrator the basename + weight.
    let (file, weight) = repo.resolve_lora(&lora_id).await.unwrap();
    assert_eq!(file, "krea2-realism.safetensors");
    assert!((weight - 0.9).abs() < 1e-6);
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
