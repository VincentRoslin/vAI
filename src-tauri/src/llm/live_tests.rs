//! Phase 15 deferred gate (plan 15.D) — the real `llama-server` binary + a real
//! GGUF. `#[ignore]`d; run explicitly with the two env vars set:
//!
//! ```text
//! LOCALAI_LLAMA_SERVER=<...>/runtime/llama-server/llama-server.exe \
//! LOCALAI_TEST_GGUF=<...>/models/qwen2.5-0.5b-instruct-q4_k_m.gguf \
//! cargo test --manifest-path src-tauri/Cargo.toml llm::live_tests -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::LlamaBackend;
use crate::contracts::generation::{GenerationEvent, SamplingParams, StopReason};
use crate::contracts::model::{Device, ModelBackend as BackendName, ModelKind};
use crate::db::Db;
use crate::lifecycle::backend::{LoadRequest, LoadedInstance, ModelBackend};
use crate::models::{ModelDraft, ModelRegistry};

fn env_paths() -> Option<(PathBuf, PathBuf)> {
    let bin = std::env::var("LOCALAI_LLAMA_SERVER").ok()?;
    let gguf = std::env::var("LOCALAI_TEST_GGUF").ok()?;
    Some((PathBuf::from(bin), PathBuf::from(gguf)))
}

async fn load_model() -> (Arc<dyn LoadedInstance>, PathBuf) {
    let (binary, gguf) = env_paths().expect("set LOCALAI_LLAMA_SERVER + LOCALAI_TEST_GGUF");
    assert!(binary.is_file(), "binary missing: {}", binary.display());
    assert!(gguf.is_file(), "gguf missing: {}", gguf.display());
    let models_dir = gguf.parent().unwrap().to_path_buf();

    let tmp = std::env::temp_dir().join(format!("localai-live-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let db = Db::open(&tmp.join("live.db")).await.unwrap();
    db.migrate().await.unwrap();
    let registry = Arc::new(ModelRegistry::new(Arc::new(db)));
    let id = registry
        .register(
            ModelDraft {
                display_name: "Qwen2.5 0.5B Instruct (live)".to_owned(),
                kind: ModelKind::Llm,
                backend: BackendName("llama.cpp".to_owned()),
                quant: None,
                path: gguf,
                streaming: true,
                context_tokens: Some(4096),
                estimated_vram_mb: Some(1500),
                devices: vec![Device::Cuda, Device::Cpu],
                config: json!({}),
            },
            &models_dir,
        )
        .await
        .unwrap();
    let model = registry.get(&id).await.unwrap();

    let backend = LlamaBackend::new(binary);
    let started = Instant::now();
    let instance: Arc<dyn LoadedInstance> = Arc::from(
        backend
            .load(&LoadRequest { model }, CancellationToken::new())
            .await
            .expect("llama-server loads a real GGUF"),
    );
    println!(
        "[15.D] model load + /health ready in {:?}",
        started.elapsed()
    );
    instance.health().await.expect("healthy after load");
    (instance, tmp)
}

fn prompt() -> String {
    "<|im_start|>user\nName three primary colors.<|im_end|>\n<|im_start|>assistant\n".to_owned()
}

fn params(max: u32) -> SamplingParams {
    SamplingParams {
        temperature: Some(0.7),
        top_p: Some(0.9),
        top_k: Some(40),
        max_tokens: Some(max),
        stop: vec!["<|im_end|>".to_owned()],
        seed: Some(1),
    }
}

// ---- gate 1 + 2 (non-stream) ----

#[tokio::test]
#[ignore = "needs a real llama-server + GGUF (plan 15.D)"]
async fn live_startup_and_non_streaming_generation() {
    let (instance, _tmp) = load_model().await;
    let llm = instance.as_llm().expect("LlmInstance");
    let out = llm
        .generate(prompt(), params(64), CancellationToken::new())
        .await
        .expect("generated");
    println!(
        "[15.D] non-stream: {:?} ({} tok, {:?})",
        out.text.trim(),
        out.tokens,
        out.stop_reason
    );
    assert!(!out.text.trim().is_empty());
    assert!(out.tokens > 0);
    instance.shutdown().await;
}

// ---- gate 2 (stream) + gate 8 (TTFT + tokens/sec) ----

#[tokio::test]
#[ignore = "needs a real llama-server + GGUF (plan 15.D)"]
async fn live_streaming_generation_with_ttft_and_throughput() {
    let (instance, _tmp) = load_model().await;
    let llm = instance.as_llm().expect("LlmInstance");

    let (tx, mut rx) = mpsc::channel(256);
    let started = Instant::now();

    let stream_fut = llm.stream(prompt(), params(128), tx, CancellationToken::new());
    let mut ttft = None;
    let mut deltas = 0u32;
    let mut done = None;
    let recv_fut = async {
        while let Some(ev) = rx.recv().await {
            match ev {
                GenerationEvent::TokenDelta { .. } => {
                    if ttft.is_none() {
                        ttft = Some(started.elapsed());
                    }
                    deltas += 1;
                }
                GenerationEvent::Done {
                    stop_reason,
                    tokens,
                } => {
                    done = Some((stop_reason, tokens, started.elapsed()));
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    };
    tokio::join!(stream_fut, recv_fut);

    let ttft = ttft.expect("at least one token");
    let (stop, tokens, total) = done.expect("Done frame");
    let tok_per_s = f64::from(tokens) / total.as_secs_f64();
    println!(
        "[15.D] stream: TTFT {ttft:?} · {tokens} tok in {total:?} · {tok_per_s:.1} tok/s · {deltas} deltas · {stop:?}"
    );
    assert!(deltas > 0);
    assert!(tok_per_s > 1.0, "throughput implausibly low: {tok_per_s}");
    assert!(matches!(
        stop,
        StopReason::EndOfText | StopReason::MaxTokens | StopReason::StopSequence
    ));
    instance.shutdown().await;
}

// ---- gate 3 (cancel frees the slot) ----

#[tokio::test]
#[ignore = "needs a real llama-server + GGUF (plan 15.D)"]
async fn live_cancel_frees_the_slot() {
    let (instance, _tmp) = load_model().await;
    let llm = instance.as_llm().expect("LlmInstance");

    // Start a long generation, cancel it after the first tokens.
    let cancel = CancellationToken::new();
    let (tx, mut rx) = mpsc::channel(256);
    let stream_fut = llm.stream(prompt(), params(512), tx, cancel.clone());
    let mut saw_cancelled = false;
    let control_fut = async {
        let first = rx.recv().await;
        assert!(matches!(first, Some(GenerationEvent::TokenDelta { .. })));
        cancel.cancel();
        while let Some(ev) = rx.recv().await {
            if matches!(ev, GenerationEvent::Cancelled) {
                saw_cancelled = true;
            }
        }
    };
    tokio::join!(stream_fut, control_fut);
    assert!(saw_cancelled, "stream ended with Cancelled");

    // The slot must be free again — a fresh non-stream generation succeeds.
    let llm2 = instance.as_llm().expect("LlmInstance");
    let out = llm2
        .generate(prompt(), params(32), CancellationToken::new())
        .await
        .expect("slot is free, generation works after cancel");
    assert!(!out.text.trim().is_empty());
    println!("[15.D] post-cancel generation ok: {:?}", out.text.trim());
    instance.shutdown().await;
}

// ---- gate 5 (external kill detected) + gate 6 (no orphan) ----

#[tokio::test]
#[ignore = "needs a real llama-server + GGUF (plan 15.D)"]
async fn live_external_kill_is_detected_and_no_orphan_on_shutdown() {
    let (instance, _tmp) = load_model().await;
    let server = instance
        .as_any()
        .downcast_ref::<super::LlamaServer>()
        .expect("LlamaServer");
    let pid = server.child_id().await.expect("running child");

    // Kill it from outside.
    let killed = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output()
        .expect("taskkill");
    assert!(killed.status.success(), "taskkill: {killed:?}");
    tokio::time::sleep(Duration::from_millis(500)).await;

    // health() must now report the instance dead.
    assert!(
        instance.health().await.is_err(),
        "dead child detected by health()"
    );

    // Clean shutdown leaves no orphan.
    instance.shutdown().await;
    let list = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}")])
        .output()
        .expect("tasklist");
    let out = String::from_utf8_lossy(&list.stdout);
    assert!(
        !out.contains(&pid.to_string()),
        "no orphan llama-server for pid {pid}: {out}"
    );
    println!("[15.D] external kill detected; no orphan after shutdown");
}
