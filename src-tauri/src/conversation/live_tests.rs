//! Phase 16 gate — the real vertical slice, Rust-side: register the local GGUF,
//! load it through the lifecycle manager (real `llama-server`), and run the
//! conversation service end to end (persist → stream → persist → cancel →
//! restart recovery). `#[ignore]`d; needs the same two env vars as
//! `llm::live_tests`:
//!
//! ```text
//! LOCALAI_LLAMA_SERVER=<repo>/runtime/llama-server/llama-server.exe \
//! LOCALAI_TEST_GGUF=<repo>/models/qwen2.5-0.5b-instruct-q4_k_m.gguf \
//! cargo test --manifest-path src-tauri/Cargo.toml conversation::live_tests -- --ignored --nocapture --test-threads=1
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use super::ConversationEngine;
use crate::acquisition::AcquisitionService;
use crate::config::ConfigManager;
use crate::contracts::conversation::Role;
use crate::contracts::generation::{GenerationEvent, StopReason};
use crate::db::Db;
use crate::lifecycle::{LifecycleManager, RetryPolicy};
use crate::llm::{LlamaBackend, BACKEND_KEY};
use crate::models::ModelRegistry;
use crate::resources::probe::NvmlProbe;
use crate::resources::ResourceManager;

struct Rig {
    _tmp: tempfile::TempDir,
    service: Arc<ConversationEngine>,
    lifecycle: Arc<LifecycleManager>,
    registry: Arc<ModelRegistry>,
    model: crate::contracts::ids::ModelId,
    db_path: PathBuf,
}

async fn rig() -> Rig {
    let bin = PathBuf::from(std::env::var("LOCALAI_LLAMA_SERVER").expect("LOCALAI_LLAMA_SERVER"));
    let gguf = PathBuf::from(std::env::var("LOCALAI_TEST_GGUF").expect("LOCALAI_TEST_GGUF"));
    assert!(bin.is_file() && gguf.is_file());
    let models_dir = gguf.parent().unwrap().to_path_buf();
    let filename = gguf.file_name().unwrap().to_string_lossy().into_owned();

    let tmp = tempfile::tempdir().unwrap();
    let cfg_dir = tmp.path().join("cfg");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    // Point models.dir at the real folder so `register_local_gguf` resolves it.
    std::fs::write(
        cfg_dir.join("config.json"),
        serde_json::json!({ "version": 1, "models": { "dir": models_dir } }).to_string(),
    )
    .unwrap();
    let config = Arc::new(ConfigManager::load(&cfg_dir, tmp.path()).unwrap());

    let db_path = tmp.path().join("live.db");
    let db = Arc::new(Db::open(&db_path).await.unwrap());
    db.migrate().await.unwrap();

    let registry = Arc::new(ModelRegistry::new(Arc::clone(&db)));
    let acquisition =
        AcquisitionService::new(Arc::clone(&db), Arc::clone(&registry), Arc::clone(&config));
    let model = acquisition.register_local_gguf(&filename).await.unwrap();

    let resources = Arc::new(ResourceManager::new(Arc::new(NvmlProbe::new()), 1_500));
    resources.observe().await;
    let lifecycle = Arc::new(LifecycleManager::new(
        Arc::clone(&registry),
        resources,
        RetryPolicy::default(),
    ));
    lifecycle.register_backend(BACKEND_KEY, Arc::new(LlamaBackend::new(bin)));
    lifecycle.load(&model).await.expect("real model loads");

    let service = Arc::new(ConversationEngine::new(
        db,
        Arc::clone(&registry),
        Arc::clone(&lifecycle),
    ));
    Rig {
        _tmp: tmp,
        service,
        lifecycle,
        registry,
        model,
        db_path,
    }
}

fn drain(rx: &mut mpsc::UnboundedReceiver<GenerationEvent>) -> Vec<GenerationEvent> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        out.push(ev);
    }
    out
}

#[tokio::test]
#[ignore = "needs a real llama-server + GGUF (Phase 17 gate — re-hosted chat)"]
#[allow(clippy::too_many_lines)]
async fn live_send_streams_persists_and_recovers_on_restart() {
    let rig = rig().await;
    let convo = rig.service.create().await.unwrap();

    // --- gate 1 + 8: send → stream → persist; measure end-to-end TTFT ---
    let (tx, mut rx) = mpsc::unbounded_channel();
    let started = Instant::now();
    let first_delta: Arc<std::sync::Mutex<Option<Duration>>> =
        Arc::new(std::sync::Mutex::new(None));
    let fd = Arc::clone(&first_delta);
    rig.service
        .send(
            convo.id.clone(),
            rig.model.clone(),
            "Reply with exactly: pong".to_owned(),
            move |ev| {
                if matches!(ev, GenerationEvent::TokenDelta { .. }) {
                    let mut g = fd.lock().unwrap();
                    if g.is_none() {
                        *g = Some(started.elapsed());
                    }
                }
                let _ = tx.send(ev);
            },
        )
        .await
        .expect("send accepted");

    // state machine: it should report Generating for this conversation
    let st = rig.service.generation_state().await;
    assert_eq!(
        st.generating.as_ref().map(|h| &h.conversation_id),
        Some(&convo.id)
    );

    // wait for completion
    for _ in 0..600 {
        if !rig.service.is_generating().await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!rig.service.is_generating().await, "generation finished");
    assert_eq!(rig.service.generation_state().await.generating, None);
    let events = drain(&mut rx);
    let deltas = events
        .iter()
        .filter(|e| matches!(e, GenerationEvent::TokenDelta { .. }))
        .count();
    assert!(deltas > 0, "streamed at least one token");
    assert!(matches!(events.last(), Some(GenerationEvent::Done { .. })));
    let ttft = first_delta.lock().unwrap().expect("a first token");
    println!("[17] end-to-end TTFT (send → first delta) {ttft:?} · {deltas} deltas");

    let msgs = rig.service.messages(&convo.id).await.unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].role, Role::Assistant);
    let m1 = msgs[1].generation.as_ref().unwrap();
    assert_eq!(m1.stop_reason, StopReason::EndOfText);
    assert!(m1.tokens > 0);
    if let crate::contracts::conversation::MessageContent::Text { text } = &msgs[1].content {
        println!("[17] assistant said: {:?}", text.trim());
        assert!(!text.trim().is_empty());
    }

    // --- gate 4: a fresh service over the same DB sees the transcript ---
    drop(rig.service);
    let db2 = Arc::new(Db::open(&rig.db_path).await.unwrap());
    db2.migrate().await.unwrap();
    let reopened =
        ConversationEngine::new(db2, Arc::clone(&rig.registry), Arc::clone(&rig.lifecycle));
    let restored = reopened.messages(&convo.id).await.unwrap();
    assert_eq!(restored.len(), 2);
    assert_eq!(reopened.latest().await.unwrap().unwrap().id, convo.id);
    println!(
        "[17] restart recovery: transcript restored ({} messages)",
        restored.len()
    );

    // --- gate 3: cancel mid-generation, partial persisted, model reusable ---
    let cvo = reopened.create().await.unwrap();
    let (tx2, mut rx2) = mpsc::unbounded_channel();
    let cancel_svc: Arc<ConversationEngine> = Arc::new(reopened);
    let task = cancel_svc
        .send(
            cvo.id.clone(),
            rig.model.clone(),
            "Write a long detailed 500-word essay about the ocean.".to_owned(),
            move |ev| {
                let _ = tx2.send(ev);
            },
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(120)).await;
    cancel_svc.cancel(&task).await.expect("cancel accepted");
    for _ in 0..300 {
        if !cancel_svc.is_generating().await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let ev2 = drain(&mut rx2);
    assert!(matches!(ev2.last(), Some(GenerationEvent::Cancelled)));
    let cm = cancel_svc.messages(&cvo.id).await.unwrap();
    assert_eq!(
        cm[1].generation.as_ref().unwrap().stop_reason,
        StopReason::Cancelled
    );
    println!("[17] cancel: partial turn persisted as Cancelled");

    // model still usable
    let (tx3, mut rx3) = mpsc::unbounded_channel();
    cancel_svc
        .send(
            cvo.id.clone(),
            rig.model.clone(),
            "Say hi".to_owned(),
            move |ev| {
                let _ = tx3.send(ev);
            },
        )
        .await
        .expect("model still usable after cancel");
    for _ in 0..600 {
        if !cancel_svc.is_generating().await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(matches!(
        drain(&mut rx3).last(),
        Some(GenerationEvent::Done { .. })
    ));
    println!("[17] model reusable after cancel — OK");

    rig.lifecycle.unload_all().await;
}
