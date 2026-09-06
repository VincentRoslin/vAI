//! Phase 22.C — the live image gate. The real Krea 2 sidecar (`.venv-image` plus
//! the NF4 quant cache) driven through the real `ImageOrchestrator`, with a real
//! `llama-server` LLM resident so eviction / restore is exercised against actual
//! VRAM. All `#[ignore]`d; run explicitly on the RTX 5080 box:
//!
//! ```text
//! set LOCALAI_RUN_IMAGE_LIVE=1
//! set LOCALAI_LLAMA_SERVER=C:\Users\Vincent\Desktop\vAI\runtime\llama-server\llama-server.exe
//! set LOCALAI_TEST_GGUF=C:\Users\Vincent\Desktop\vAI\models\llm\qwen2.5-0.5b-instruct-q4_k_m.gguf
//! cargo test --manifest-path src-tauri/Cargo.toml image::live_tests -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Models dir, quant cache and LoRA dir come from `<repo>/models/image/` (the
//! owner's staged assets). `.venv-image` must be built first:
//! `node scripts/setup-venv.mjs image`.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::{image_venv_python, Krea2Backend, BACKEND_KEY, KREA2_MODEL_ID};
use crate::acquisition::AcquisitionService;
use crate::blob::BlobStore;
use crate::config::{ConfigKey, ConfigManager};
use crate::contracts::ids::ModelId;
use crate::contracts::image::{ImageEvent, ImagePhase, ImageRequest, LoraSelection};
use crate::contracts::model::{Device, ModelBackend as BackendName, ModelKind, ModelState, Quant};
use crate::conversation::ConversationEngine;
use crate::db::Db;
use crate::image::orchestrator::ImageOrchestrator;
use crate::image::repo::ImageRepo;
use crate::lifecycle::backend::ModelBackend;
use crate::lifecycle::{LifecycleManager, RetryPolicy};
use crate::llm::LlamaBackend;
use crate::models::{ModelDraft, ModelRegistry};
use crate::resources::probe::NvmlProbe;
use crate::resources::ResourceManager;

const LLAMA_BACKEND: &str = "llama.cpp";

fn live_enabled() -> bool {
    std::env::var("LOCALAI_RUN_IMAGE_LIVE").is_ok_and(|v| v == "1")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

struct Live {
    _tmp: tempfile::TempDir,
    orch: Arc<ImageOrchestrator>,
    lifecycle: Arc<LifecycleManager>,
    resources: Arc<ResourceManager>,
    llm_id: ModelId,
    krea2_id: ModelId,
    blob: Arc<BlobStore>,
    repo: ImageRepo,
}

/// Wire the real backends against the staged `<repo>/models/image/` assets.
#[allow(clippy::too_many_lines)]
async fn live() -> Option<Live> {
    if !live_enabled() {
        eprintln!("LOCALAI_RUN_IMAGE_LIVE != 1 — skipping");
        return None;
    }
    let llama_bin = PathBuf::from(std::env::var("LOCALAI_LLAMA_SERVER").ok()?);
    let gguf = PathBuf::from(std::env::var("LOCALAI_TEST_GGUF").ok()?);
    assert!(llama_bin.is_file(), "llama-server: {}", llama_bin.display());
    assert!(gguf.is_file(), "gguf: {}", gguf.display());

    let models_dir = repo_root().join("models");
    let image_dir = models_dir.join("image");
    let quant_cache = image_dir.join("quant_cache").join("krea2");
    let loras_dir = image_dir.join("loras");
    assert!(
        quant_cache.join("fingerprint.json").is_file(),
        "NF4 quant cache missing at {}",
        quant_cache.display()
    );

    let python = image_venv_python(&repo_root().join(".venv").join("Scripts").join("python.exe"));
    assert!(
        python.is_file(),
        "{} missing — run `node scripts/setup-venv.mjs image`",
        python.display()
    );
    let script = repo_root().join("image_gen").join("server.py");

    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&tmp.path().join("live.db")).await.unwrap());
    db.migrate().await.unwrap();
    let registry = Arc::new(ModelRegistry::new(Arc::clone(&db)));

    let llm_id = registry
        .register(
            ModelDraft {
                display_name: "Qwen 0.5B (live)".to_owned(),
                kind: ModelKind::Llm,
                backend: BackendName(LLAMA_BACKEND.to_owned()),
                quant: None,
                path: gguf.clone(),
                streaming: true,
                context_tokens: Some(4096),
                estimated_vram_mb: Some(1500),
                devices: vec![Device::Cuda, Device::Cpu],
                config: json!({}),
            },
            gguf.parent().unwrap(),
        )
        .await
        .unwrap();

    std::fs::create_dir_all(image_dir.join("krea2")).ok();
    let krea2_id = registry
        .register(
            ModelDraft {
                display_name: "Krea 2 Turbo (live)".to_owned(),
                kind: ModelKind::Image,
                backend: BackendName(BACKEND_KEY.to_owned()),
                quant: Some(Quant("nf4".to_owned())),
                path: image_dir.join("krea2"),
                streaming: false,
                context_tokens: None,
                estimated_vram_mb: Some(11_750),
                devices: vec![Device::Cuda],
                config: json!({ "model_id": KREA2_MODEL_ID }),
            },
            &models_dir,
        )
        .await
        .unwrap();

    let resources = Arc::new(ResourceManager::new(Arc::new(NvmlProbe::new()), 1_500));
    resources.observe().await;
    let lifecycle = Arc::new(LifecycleManager::new(
        Arc::clone(&registry),
        Arc::clone(&resources),
        RetryPolicy::default(),
    ));
    lifecycle.register_backend(
        LLAMA_BACKEND,
        Arc::new(LlamaBackend::new(llama_bin)) as Arc<dyn ModelBackend>,
    );
    lifecycle.register_backend(
        BACKEND_KEY,
        Arc::new(Krea2Backend::new(
            python,
            script,
            Some(quant_cache),
            Some(loras_dir.clone()),
            tmp.path().join("exchange"),
        )) as Arc<dyn ModelBackend>,
    );

    let engine = Arc::new(ConversationEngine::new(
        Arc::clone(&db),
        Arc::clone(&registry),
        Arc::clone(&lifecycle),
    ));
    let blob = Arc::new(BlobStore::new(tmp.path().join("blobs")));
    let repo = ImageRepo::new(Arc::clone(&db));
    repo.seed(&loras_dir).await.unwrap();

    let orch = Arc::new(ImageOrchestrator::new(
        Arc::clone(&lifecycle),
        Arc::clone(&resources),
        Arc::clone(&registry),
        engine,
        Arc::clone(&blob),
        repo.clone(),
        tmp.path().join("images"),
    ));

    Some(Live {
        _tmp: tmp,
        orch,
        lifecycle,
        resources,
        llm_id,
        krea2_id,
        blob,
        repo,
    })
}

fn sink() -> (
    impl Fn(ImageEvent) + Send + Sync + 'static,
    Arc<std::sync::Mutex<Vec<ImageEvent>>>,
) {
    let log = Arc::new(std::sync::Mutex::new(Vec::new()));
    let l2 = Arc::clone(&log);
    (move |ev| l2.lock().unwrap().push(ev), log)
}

async fn wait_terminal(log: &Arc<std::sync::Mutex<Vec<ImageEvent>>>, secs: u64) -> ImageEvent {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(t) = log.lock().unwrap().iter().rev().find(|e| {
            matches!(
                e,
                ImageEvent::Done { .. } | ImageEvent::Error { .. } | ImageEvent::Cancelled
            )
        }) {
            return t.clone();
        }
        assert!(
            Instant::now() < deadline,
            "no terminal event within {secs}s"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn print_vram(r: &ResourceManager, tag: &str) {
    let s = r.observe().await; // re-probe NVML; snapshot() returns the last observation
    let gpu = s.gpu.map_or_else(
        || "n/a".to_owned(),
        |g| format!("{}/{} MB used", g.used_mb, g.total_mb),
    );
    println!(
        "[22.C] VRAM {tag}: {gpu}, reserved {} MB",
        s.reserved_gpu_mb
    );
}

/// Poll whole-GPU VRAM (NVML) every 250 ms until stopped, tracking the peak.
fn spawn_vram_peak(r: Arc<ResourceManager>) -> (CancellationToken, Arc<AtomicU64>) {
    let stop = CancellationToken::new();
    let peak = Arc::new(AtomicU64::new(0));
    let (s2, p2) = (stop.clone(), Arc::clone(&peak));
    tokio::spawn(async move {
        loop {
            tokio::select! {
                () = s2.cancelled() => break,
                () = tokio::time::sleep(Duration::from_millis(250)) => {}
            }
            if let Some(g) = r.observe().await.gpu {
                p2.fetch_max(g.used_mb, Ordering::Relaxed);
            }
        }
    });
    (stop, peak)
}

fn req(prompt: &str, batch: u32, loras: Vec<LoraSelection>) -> ImageRequest {
    ImageRequest {
        prompt: prompt.to_owned(),
        negative: None,
        width: 1024,
        height: 1024,
        steps: Some(8),
        guidance: None,
        seed: Some(42),
        batch_count: batch,
        loras,
    }
}

// -------------------------------------------------------------- acquisition

#[tokio::test]
#[ignore = "Phase 22.C — needs .venv-image + staged Krea 2 assets"]
async fn live_acquire_image_registers_without_downloading() {
    if !live_enabled() {
        eprintln!("LOCALAI_RUN_IMAGE_LIVE != 1 — skipping");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&tmp.path().join("a.db")).await.unwrap());
    db.migrate().await.unwrap();
    let registry = Arc::new(ModelRegistry::new(Arc::clone(&db)));
    let cfg = ConfigManager::load(tmp.path(), tmp.path()).unwrap();
    cfg.set_session(
        ConfigKey::ModelsDir,
        &repo_root().join("models").display().to_string(),
    )
    .unwrap();
    let svc = AcquisitionService::new(db, Arc::clone(&registry), Arc::new(cfg));

    let started = Instant::now();
    let id = svc.acquire_image().await.expect("acquire_image");
    println!("[22.C] acquire_image → {id} in {:?}", started.elapsed());

    let model = registry.get(&id).await.unwrap();
    assert_eq!(model.metadata.kind, ModelKind::Image);
    assert_eq!(model.metadata.backend.0, BACKEND_KEY);
    // Idempotent.
    assert_eq!(svc.acquire_image().await.unwrap(), id);
}

// -------------------------------------------------------------- the gate

#[tokio::test]
#[ignore = "Phase 22.C — RTX 5080 live gate"]
#[allow(clippy::too_many_lines)]
async fn live_generate_evicts_the_llm_and_restores_it() {
    let Some(h) = live().await else { return };
    h.lifecycle.load(&h.llm_id).await.expect("llm loads");
    assert_eq!(h.lifecycle.state(&h.llm_id).await, ModelState::Loaded);
    print_vram(&h.resources, "llm resident").await;

    // Base model.
    let (s, log) = sink();
    let load_started = Instant::now();
    let (stop, peak) = spawn_vram_peak(Arc::clone(&h.resources));
    h.orch
        .generate(req("a lighthouse at dawn, photorealistic", 1, vec![]), s)
        .await
        .expect("started");
    let terminal = wait_terminal(&log, 240).await;
    stop.cancel();
    println!(
        "[22.C] base 1024^2/8-step: {:?} total; peak whole-GPU VRAM {} MB (of 16303)",
        load_started.elapsed(),
        peak.load(Ordering::Relaxed)
    );
    let ImageEvent::Done { images } = terminal else {
        panic!("expected Done, got {terminal:?}");
    };
    assert_eq!(images.len(), 1);
    assert!(h.blob.contains(&images[0].asset), "blob missing");
    print_vram(&h.resources, "after base generate (restoring)").await;

    let phases: Vec<ImagePhase> = log
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            ImageEvent::Progress(p) => Some(p.phase),
            _ => None,
        })
        .collect();
    for want in [
        ImagePhase::Evicting,
        ImagePhase::Loading,
        ImagePhase::Generating,
        ImagePhase::Restoring,
    ] {
        assert!(
            phases.contains(&want),
            "missing phase {want:?} in {phases:?}"
        );
    }
    assert_eq!(
        h.lifecycle.state(&h.llm_id).await,
        ModelState::Loaded,
        "LLM restored"
    );
    print_vram(&h.resources, "llm restored").await;

    // Realism LoRA + batch of 2.
    let lora = h.repo.list_loras().await.unwrap();
    let realism = lora
        .iter()
        .find(|l| l.display_name == "Realism")
        .expect("Realism LoRA");
    let (s2, log2) = sink();
    let started = Instant::now();
    let (stop2, peak2) = spawn_vram_peak(Arc::clone(&h.resources));
    h.orch
        .generate(
            req(
                "portrait of a woman, freckles, natural light",
                2,
                vec![LoraSelection {
                    id: realism.id.clone(),
                    weight: 0.9,
                }],
            ),
            s2,
        )
        .await
        .expect("started");
    let ImageEvent::Done { images } = wait_terminal(&log2, 240).await else {
        panic!("lora generate did not finish Done");
    };
    stop2.cancel();
    println!(
        "[22.C] realism-LoRA batch=2: {:?} total; peak whole-GPU VRAM {} MB",
        started.elapsed(),
        peak2.load(Ordering::Relaxed)
    );
    assert_eq!(images.len(), 2);
    assert_eq!(images[0].lora.as_deref(), Some("Realism"));
    assert_eq!(h.repo.list_generated(10).await.unwrap().len(), 3);
    assert_eq!(h.lifecycle.state(&h.llm_id).await, ModelState::Loaded);

    h.lifecycle.unload_all().await;
}

#[tokio::test]
#[ignore = "Phase 22.C — RTX 5080 live gate"]
async fn live_cancel_mid_generate_restores_llm_and_writes_nothing() {
    let Some(h) = live().await else { return };
    h.lifecycle.load(&h.llm_id).await.expect("llm loads");

    let (s, log) = sink();
    let tid = h
        .orch
        .generate(req("an intricate cathedral, wide shot", 4, vec![]), s)
        .await
        .expect("started");

    // Let it get into the load/generate, then cancel.
    tokio::time::sleep(Duration::from_secs(6)).await;
    h.orch.cancel(&tid).await.expect("cancel accepted");

    let terminal = wait_terminal(&log, 120).await;
    assert!(
        matches!(terminal, ImageEvent::Cancelled),
        "got {terminal:?}"
    );
    assert!(
        h.repo.list_generated(10).await.unwrap().is_empty(),
        "wrote a row"
    );
    assert!(
        h.blob.list_ids().unwrap().is_empty(),
        "wrote a partial blob"
    );
    assert_eq!(
        h.lifecycle.state(&h.llm_id).await,
        ModelState::Loaded,
        "LLM restored"
    );
    print_vram(&h.resources, "after cancel").await;
    h.lifecycle.unload_all().await;
}

#[tokio::test]
#[ignore = "Phase 22.C — RTX 5080 live gate"]
async fn live_sidecar_crash_is_a_typed_error_and_llm_restores() {
    let Some(h) = live().await else { return };
    h.lifecycle.load(&h.llm_id).await.expect("llm loads");

    let (s, log) = sink();
    h.orch
        .generate(req("a field of tulips", 2, vec![]), s)
        .await
        .expect("started");

    // Wait for the sidecar to be up, then kill it from outside.
    tokio::time::sleep(Duration::from_secs(8)).await;
    let killed = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"Name='python.exe'\" | \
             Where-Object { $_.CommandLine -like '*image_gen*server.py*--port*' } | \
             ForEach-Object { Stop-Process -Id $_.ProcessId -Force }",
        ])
        .output()
        .expect("powershell");
    println!("[22.C] taskkill sidecar: {}", killed.status);

    let terminal = wait_terminal(&log, 120).await;
    assert!(
        matches!(terminal, ImageEvent::Error { .. }),
        "got {terminal:?}"
    );
    assert_eq!(
        h.lifecycle.state(&h.llm_id).await,
        ModelState::Loaded,
        "LLM restored after crash"
    );
    print_vram(&h.resources, "after sidecar crash").await;
    // The crashed image model must not be left Loaded.
    assert_ne!(h.lifecycle.state(&h.krea2_id).await, ModelState::Loaded);
    h.lifecycle.unload_all().await;
}
