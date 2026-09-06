//! Live voice-in gate (Phase 18.B) — the real faster-whisper worker + real
//! Silero VAD, fed a recorded WAV through the real `VoiceInput` path (no mic).
//!
//! Run explicitly (needs the venv + `models/stt/` + an NVIDIA GPU):
//! `LOCALAI_RUN_VOICE_LIVE=1 cargo test -p localai --lib -- --ignored voice_live`

#![allow(clippy::cast_precision_loss)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use super::{VoiceConfig, VoiceInput};
use crate::contracts::conversation::MessageContent;
use crate::contracts::worker::WorkerKind;
use crate::conversation::ConversationEngine;
use crate::db::Db;
use crate::lifecycle::{LifecycleManager, RetryPolicy};
use crate::models::ModelRegistry;
use crate::resources::probe::MockProbe;
use crate::resources::ResourceManager;
use crate::voice::vad::VadConfig;
use crate::worker::{WorkerLayout, WorkerSupervisor};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[tokio::test]
#[ignore = "needs the venv + models/stt + a GPU; set LOCALAI_RUN_VOICE_LIVE=1"]
async fn voice_live_capture_to_turn() {
    if std::env::var("LOCALAI_RUN_VOICE_LIVE").is_err() {
        eprintln!("LOCALAI_RUN_VOICE_LIVE unset — skipping");
        return;
    }

    let python = repo().join(".venv/Scripts/python.exe");
    let workers = repo().join("workers");
    let stt_model = repo().join("models/stt");
    let vad_model = repo().join("models/vad/silero_vad.onnx");
    assert!(python.is_file(), "run scripts/setup-venv.mjs");
    assert!(stt_model.is_dir(), "acquire_fixed(Stt) first");
    assert!(vad_model.is_file());

    let stt = Arc::new(
        WorkerSupervisor::new(WorkerLayout::for_test(python, &workers), WorkerKind::Stt).with_env(
            [(
                "LOCALAI_STT_MODEL_DIR".to_owned(),
                stt_model.display().to_string(),
            )],
        ),
    );

    // Minimal engine.
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&tmp.path().join("v.db")).await.unwrap());
    db.migrate().await.unwrap();
    let registry = Arc::new(ModelRegistry::new(Arc::clone(&db)));
    let resources = Arc::new(ResourceManager::new(Arc::new(MockProbe::new()), 1_500));
    let lifecycle = Arc::new(LifecycleManager::new(
        registry,
        resources,
        RetryPolicy::default(),
    ));
    let engine = Arc::new(ConversationEngine::new(db, lifecycle));
    let convo = engine.create().await.unwrap();

    let cfg = VoiceConfig {
        vad_model,
        temp_dir: tmp.path().join("seg"),
        input_device: None,
        vad: VadConfig {
            min_silence_ms: 500,
            ..VadConfig::default()
        },
        pre_roll_ms: 300,
    };
    let voice = VoiceInput::new(Arc::clone(&engine), stt, cfg);

    // Feed the recorded fixture, then silence to force the endpoint.
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hello_localai.wav");
    let mut r = hound::WavReader::open(&fixture).unwrap();
    let rate = r.spec().sample_rate;
    let samples: Vec<f32> = r
        .samples::<i32>()
        .map(|s| s.unwrap() as f32 / f32::from(i16::MAX))
        .collect();
    let audio_secs = samples.len() as f32 / rate as f32;

    let (tx, rx) = mpsc::unbounded_channel();
    for chunk in samples.chunks(rate as usize / 10) {
        tx.send(chunk.to_vec()).unwrap();
    }
    for _ in 0..20 {
        tx.send(vec![0.0_f32; rate as usize / 20]).unwrap();
    }
    drop(tx);

    let started = Instant::now();
    voice
        .start_with_frames(convo.id.clone(), rx, rate, 1)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(60), voice.wait_idle())
        .await
        .expect("voice session finished");
    let wall = started.elapsed();

    let msgs = engine.messages(&convo.id).await.unwrap();
    assert_eq!(msgs.len(), 1, "one user turn");
    let MessageContent::Text { text } = &msgs[0].content else {
        panic!("expected Text");
    };
    println!("[18.B] transcript: {text:?}");
    println!(
        "[18.B] audio {audio_secs:.2}s, wall {:.2}s (incl. model load), RTF-ish {:.3}",
        wall.as_secs_f32(),
        wall.as_secs_f32() / audio_secs
    );
    let lower = text.to_lowercase();
    assert!(
        lower.contains("quick brown fox") || lower.contains("voice input"),
        "transcript wrong: {text:?}"
    );
}
