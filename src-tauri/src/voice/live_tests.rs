//! Live voice gates:
//! - **18.B** `voice_live_capture_to_turn` — real faster-whisper + real Silero
//!   VAD, a recorded WAV through the real `VoiceInput` path (no mic).
//! - **19.B** `voice_live_tts_speaks` — real Chatterbox worker + real `cpal`
//!   playback: clauses in → audio out, time-to-first-audio recorded.
//!
//! Run explicitly (needs the venv + models + an NVIDIA GPU):
//! `LOCALAI_RUN_VOICE_LIVE=1 cargo test -p localai --lib -- --ignored voice_live`

#![allow(clippy::cast_precision_loss)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::tts::TtsOutput;
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

fn live_enabled() -> bool {
    std::env::var("LOCALAI_RUN_VOICE_LIVE").is_ok()
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[tokio::test]
#[ignore = "needs the venv + models/stt + a GPU; set LOCALAI_RUN_VOICE_LIVE=1"]
async fn voice_live_capture_to_turn() {
    if !live_enabled() {
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
        Arc::clone(&registry),
        resources,
        RetryPolicy::default(),
    ));
    let engine = Arc::new(ConversationEngine::new(db, registry, lifecycle));
    let convo = engine.create().await.unwrap();

    let tts = Arc::new(TtsOutput::new(
        Arc::new(WorkerSupervisor::new(
            WorkerLayout::for_test(
                repo().join(".venv/Scripts/python.exe"),
                repo().join("workers"),
            ),
            WorkerKind::Tts,
        )),
        tmp.path().join("tts"),
        None,
        None,
    ));
    let cfg = VoiceConfig {
        vad_model,
        temp_dir: tmp.path().join("seg"),
        input_device: None,
        output_device: None,
        vad: VadConfig {
            min_silence_ms: 500,
            ..VadConfig::default()
        },
        pre_roll_ms: 300,
        playback_duck: 0.2,
    };
    let voice = VoiceInput::new(Arc::clone(&engine), stt, tts, cfg);

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
        .start_with_frames(convo.id.clone(), None, rx, rate, 1)
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

/// 19.B — real Chatterbox worker + real `cpal` playback. Clauses in → audio out.
#[tokio::test]
#[ignore = "needs the venv + models/tts + a GPU + an output device; LOCALAI_RUN_VOICE_LIVE=1"]
async fn voice_live_tts_speaks() {
    if !live_enabled() {
        eprintln!("LOCALAI_RUN_VOICE_LIVE unset — skipping");
        return;
    }
    let tts_model = repo().join("models/tts");
    assert!(tts_model.is_dir(), "acquire_fixed(Tts) first");

    let tmp = tempfile::tempdir().unwrap();
    let worker = Arc::new(
        WorkerSupervisor::new(
            WorkerLayout::for_test(
                repo().join(".venv/Scripts/python.exe"),
                repo().join("workers"),
            ),
            WorkerKind::Tts,
        )
        .with_env([(
            "LOCALAI_TTS_MODEL_DIR".to_owned(),
            tts_model.display().to_string(),
        )]),
    );
    let out = Arc::new(TtsOutput::new(
        worker,
        tmp.path().join("clauses"),
        None,
        None,
    ));

    let devices = super::playback::list_output_devices();
    eprintln!("[19.B] output devices: {devices:?}");

    let (tx, rx) = mpsc::channel::<String>(8);
    let cancel = CancellationToken::new();
    let o2 = Arc::clone(&out);
    let c2 = cancel.clone();
    let started = Instant::now();
    let handle = tokio::spawn(async move {
        let r = o2.speak_stream(rx, c2).await;
        if let Err(e) = &r {
            eprintln!("[19.B] speak_stream error: {e:?}");
        }
        r
    });

    for clause in [
        "Hello, this is the Chatterbox turbo voice.",
        "It streams one clause at a time so the reply starts quickly.",
        "Barge in any time to stop it.",
    ] {
        tx.send(clause.to_owned()).await.unwrap();
    }
    drop(tx);

    // First-audio latency.
    let mut first_audio = None;
    while first_audio.is_none() && started.elapsed() < Duration::from_secs(30) {
        if out.queued_ms().await > 0 {
            first_audio = Some(started.elapsed());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    println!(
        "[19.B] time-to-first-audio: {:?}",
        first_audio.expect("some audio was produced")
    );

    let summary = tokio::time::timeout(Duration::from_secs(60), handle)
        .await
        .expect("speak_stream finished")
        .unwrap()
        .expect("speak ok");
    assert_eq!(summary.clauses, 3, "all clauses spoken");
    println!(
        "[19.B] spoke {} clauses / {} chars",
        summary.clauses, summary.chars
    );

    // Barge-in: stop mid-playback → silent fast.
    let (tx2, rx2) = mpsc::channel::<String>(4);
    let cancel2 = CancellationToken::new();
    let o3 = Arc::clone(&out);
    let c3 = cancel2.clone();
    let h2 = tokio::spawn(async move { o3.speak_stream(rx2, c3).await });
    tx2.send(
        "This is a long sentence that we will interrupt partway through, testing barge in latency."
            .to_owned(),
    )
    .await
    .unwrap();
    while out.queued_ms().await == 0 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let t = Instant::now();
    cancel2.cancel();
    out.stop().await;
    let _ = h2.await;
    while !out.playback_idle().await && t.elapsed() < Duration::from_millis(500) {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    println!("[19.B] barge-in trigger->silence: {:?}", t.elapsed());
    assert!(out.playback_idle().await, "playback stopped");
    assert!(
        t.elapsed() < Duration::from_millis(300),
        "barge-in within budget"
    );
    drop(tx2);
}
