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

// ---------------------------------------------------------------- orchestrator

use std::sync::Mutex as StdMutex;
use std::time::Duration;

use super::orchestrator::ImageOrchestrator;
use crate::contracts::ids::ImageLoraId;
use crate::contracts::image::{ImageEvent, ImagePhase, ImageRequest, LoraSelection};
use crate::contracts::model::ModelState;
use crate::conversation::ConversationEngine;
use crate::lifecycle::backend::FakeBackend;
use crate::lifecycle::{LifecycleManager, RetryPolicy};
use crate::models::{ModelDraft, ModelRegistry};
use crate::resources::probe::MockProbe;
use crate::resources::ResourceManager;

const FAKE_LLM_BACKEND: &str = "fake-llm";

struct Harness {
    _tmp: tempfile::TempDir,
    orch: Arc<ImageOrchestrator>,
    lifecycle: Arc<LifecycleManager>,
    llm_id: ModelId,
    blob: Arc<crate::blob::BlobStore>,
    repo: ImageRepo,
}

async fn harness() -> Option<Harness> {
    let python = which_python()?;
    let tmp = tempfile::tempdir().unwrap();
    let models_dir = tmp.path().join("models");
    let loras_dir = tmp.path().join("loras");
    let exchange = tmp.path().join("exchange");
    std::fs::create_dir_all(models_dir.join("image").join("krea2")).unwrap();
    std::fs::create_dir_all(&loras_dir).unwrap();
    write_lora(
        &loras_dir,
        "krea2-realism.safetensors",
        "\"x.lora_A.weight\":[]",
    );
    std::fs::write(models_dir.join("llm.gguf"), b"GGUF").unwrap();

    let db = Arc::new(Db::open(&tmp.path().join("o.db")).await.unwrap());
    db.migrate().await.unwrap();

    let registry = Arc::new(ModelRegistry::new(Arc::clone(&db)));
    let llm_id = registry
        .register(
            ModelDraft {
                display_name: "Fake LLM".to_owned(),
                kind: ModelKind::Llm,
                backend: ModelBackendName(FAKE_LLM_BACKEND.to_owned()),
                quant: None,
                path: models_dir.join("llm.gguf"),
                streaming: true,
                context_tokens: Some(4096),
                estimated_vram_mb: Some(4096),
                devices: vec![Device::Cuda],
                config: serde_json::json!({}),
            },
            &models_dir,
        )
        .await
        .unwrap();
    let krea2_id = registry
        .register(
            ModelDraft {
                display_name: "Krea 2 Turbo".to_owned(),
                kind: ModelKind::Image,
                backend: ModelBackendName(super::BACKEND_KEY.to_owned()),
                quant: Some(Quant("nf4".to_owned())),
                path: models_dir.join("image").join("krea2"),
                streaming: false,
                context_tokens: None,
                estimated_vram_mb: Some(11_750),
                devices: vec![Device::Cuda],
                config: serde_json::json!({}),
            },
            &models_dir,
        )
        .await
        .unwrap();

    let resources = Arc::new(ResourceManager::new(Arc::new(MockProbe::new()), 1_500));
    resources.observe().await;
    let lifecycle = Arc::new(LifecycleManager::new(
        Arc::clone(&registry),
        Arc::clone(&resources),
        RetryPolicy::default(),
    ));
    lifecycle.register_backend(
        FAKE_LLM_BACKEND,
        Arc::new(FakeBackend::new(Duration::from_millis(5))) as Arc<dyn ModelBackend>,
    );
    lifecycle.register_backend(
        super::BACKEND_KEY,
        Arc::new(Krea2Backend::new(
            python,
            fake_script(),
            None,
            Some(loras_dir.clone()),
            exchange,
        )) as Arc<dyn ModelBackend>,
    );

    let engine = Arc::new(ConversationEngine::new(
        Arc::clone(&db),
        Arc::clone(&registry),
        Arc::clone(&lifecycle),
    ));
    let blob = Arc::new(crate::blob::BlobStore::new(tmp.path().join("blobs")));
    let repo = ImageRepo::new(Arc::clone(&db));
    repo.seed(&loras_dir).await.unwrap();

    let orch = Arc::new(ImageOrchestrator::new(
        Arc::clone(&lifecycle),
        resources,
        registry,
        engine,
        Arc::clone(&blob),
        repo.clone(),
    ));
    let _ = krea2_id;
    Some(Harness {
        _tmp: tmp,
        orch,
        lifecycle,
        llm_id,
        blob,
        repo,
    })
}

fn collect_sink() -> (
    impl Fn(ImageEvent) + Send + Sync + 'static,
    Arc<StdMutex<Vec<ImageEvent>>>,
) {
    let log = Arc::new(StdMutex::new(Vec::new()));
    let l2 = Arc::clone(&log);
    (move |ev| l2.lock().unwrap().push(ev), log)
}

async fn wait_for_terminal(log: &Arc<StdMutex<Vec<ImageEvent>>>) -> ImageEvent {
    for _ in 0..300 {
        if let Some(t) = log.lock().unwrap().iter().rev().find(|e| {
            matches!(
                e,
                ImageEvent::Done { .. } | ImageEvent::Error { .. } | ImageEvent::Cancelled
            )
        }) {
            return t.clone();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no terminal event");
}

#[tokio::test]
async fn generate_evicts_the_llm_generates_and_restores() {
    let Some(h) = harness().await else {
        eprintln!("no python — skipping");
        return;
    };
    h.lifecycle.load(&h.llm_id).await.expect("llm loads");
    assert_eq!(h.lifecycle.state(&h.llm_id).await, ModelState::Loaded);

    let (sink, log) = collect_sink();
    let req = ImageRequest {
        prompt: "a red door".to_owned(),
        negative: None,
        width: 1024,
        height: 1024,
        steps: Some(40),
        guidance: None,
        seed: Some(3),
        batch_count: 2,
        loras: vec![LoraSelection {
            id: h.repo.list_loras().await.unwrap()[0].id.clone(),
            weight: 0.8,
        }],
    };
    h.orch.generate(req, sink).await.expect("started");

    let terminal = wait_for_terminal(&log).await;
    let ImageEvent::Done { images } = terminal else {
        panic!("expected Done, got {terminal:?}");
    };
    assert_eq!(images.len(), 2);
    for row in &images {
        assert!(h.blob.contains(&row.asset), "image blob missing");
    }
    assert_eq!(images[0].lora.as_deref(), Some("Realism"));

    // Phases seen, in order.
    let phases: Vec<ImagePhase> = log
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            ImageEvent::Progress(p) => Some(p.phase),
            _ => None,
        })
        .collect();
    assert!(phases.contains(&ImagePhase::Evicting));
    assert!(phases.contains(&ImagePhase::Loading));
    assert!(phases.contains(&ImagePhase::Generating));
    assert!(phases.contains(&ImagePhase::Restoring));

    // LLM restored; image model unloaded.
    assert_eq!(h.lifecycle.state(&h.llm_id).await, ModelState::Loaded);
    assert_eq!(h.repo.list_generated(10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn cancel_mid_generation_restores_the_llm_and_writes_nothing() {
    let Some(h) = harness().await else {
        return;
    };
    h.lifecycle.load(&h.llm_id).await.unwrap();

    let (sink, log) = collect_sink();
    let req = ImageRequest {
        prompt: "a slow castle".to_owned(),
        negative: None,
        width: 1024,
        height: 1024,
        steps: Some(50),
        guidance: None,
        seed: Some(1),
        batch_count: 8, // ~4s of fake work — long enough to cancel
        loras: vec![],
    };
    let task_id = h.orch.generate(req, sink).await.expect("started");
    tokio::time::sleep(Duration::from_millis(400)).await;
    h.orch.cancel(&task_id).await.expect("cancel accepted");

    let terminal = wait_for_terminal(&log).await;
    assert!(
        matches!(terminal, ImageEvent::Cancelled),
        "got {terminal:?}"
    );
    assert_eq!(h.lifecycle.state(&h.llm_id).await, ModelState::Loaded);
    assert!(h.repo.list_generated(10).await.unwrap().is_empty());
    assert!(
        h.blob.list_ids().unwrap().is_empty(),
        "a partial blob was written"
    );
}

#[tokio::test]
async fn a_second_generate_is_rejected_while_one_runs() {
    let Some(h) = harness().await else {
        return;
    };
    let req = || ImageRequest {
        prompt: "x".to_owned(),
        negative: None,
        width: 512,
        height: 512,
        steps: Some(50),
        guidance: None,
        seed: Some(1),
        batch_count: 8,
        loras: vec![],
    };
    let (s1, _l1) = collect_sink();
    h.orch.generate(req(), s1).await.expect("first starts");
    let (s2, _l2) = collect_sink();
    let err = h.orch.generate(req(), s2).await.unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
}

#[tokio::test]
async fn generate_rejects_an_invalid_request() {
    let Some(h) = harness().await else {
        return;
    };
    let (sink, _log) = collect_sink();
    let bad = ImageRequest {
        prompt: String::new(),
        negative: None,
        width: 1024,
        height: 1024,
        steps: None,
        guidance: None,
        seed: None,
        batch_count: 1,
        loras: vec![],
    };
    let err = h.orch.generate(bad, sink).await.unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
    let _ = ImageLoraId::from_trusted("x"); // keep the import used across cfgs
}
