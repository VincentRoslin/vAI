//! `VoiceInput` orchestration — capture is bypassed (pre-recorded frames), VAD
//! is real (needs `models/vad/silero_vad.onnx`), the STT worker is the stdlib
//! fake. Verifies the capture→VAD→STT→turn path end to end.

#![allow(clippy::cast_precision_loss)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use super::{SttResult, VoiceConfig, VoiceInput, VoiceState};
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

fn vad_model() -> PathBuf {
    repo().join("models/vad/silero_vad.onnx")
}

fn which_python() -> Option<PathBuf> {
    let venv = repo()
        .join(".venv")
        .join(if cfg!(windows) { "Scripts" } else { "bin" })
        .join(if cfg!(windows) {
            "python.exe"
        } else {
            "python"
        });
    venv.is_file().then_some(venv)
}

async fn engine() -> Arc<ConversationEngine> {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&dir.path().join("t.db")).await.unwrap());
    db.migrate().await.unwrap();
    std::mem::forget(dir);
    let registry = Arc::new(ModelRegistry::new(Arc::clone(&db)));
    let resources = Arc::new(ResourceManager::new(Arc::new(MockProbe::new()), 1_500));
    let lifecycle = Arc::new(LifecycleManager::new(
        registry,
        resources,
        RetryPolicy::default(),
    ));
    Arc::new(ConversationEngine::new(db, lifecycle))
}

fn fake_stt(tmp: &std::path::Path) -> Option<Arc<WorkerSupervisor>> {
    let python = which_python()?;
    std::fs::copy(repo().join("workers/stt_fake.py"), tmp.join("stt.py")).unwrap();
    Some(Arc::new(WorkerSupervisor::new(
        WorkerLayout::for_test(python, tmp),
        crate::contracts::worker::WorkerKind::Stt,
    )))
}

fn cfg(tmp: &std::path::Path) -> VoiceConfig {
    VoiceConfig {
        vad_model: vad_model(),
        temp_dir: tmp.join("segments"),
        input_device: None,
        vad: VadConfig {
            min_silence_ms: 300,
            ..VadConfig::default()
        },
        pre_roll_ms: 200,
    }
}

fn load_fixture_16k() -> (Vec<f32>, u32) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hello_localai.wav");
    let mut r = hound::WavReader::open(path).unwrap();
    let spec = r.spec();
    let raw: Vec<f32> = r
        .samples::<i32>()
        .map(|s| s.unwrap() as f32 / f32::from(i16::MAX))
        .collect();
    (raw, spec.sample_rate)
}

#[test]
fn low_confidence_policy() {
    let ok = SttResult {
        text: "hello there".into(),
        language: Some("en".into()),
        duration_s: Some(1.0),
        avg_logprob: Some(-0.3),
        no_speech_prob: Some(0.02),
    };
    assert!(ok.is_confident());
    let empty = SttResult {
        text: "   ".into(),
        ..ok.clone()
    };
    assert!(!empty.is_confident());
    let nonspeech = SttResult {
        no_speech_prob: Some(0.9),
        ..ok.clone()
    };
    assert!(!nonspeech.is_confident());
    let garbled = SttResult {
        avg_logprob: Some(-2.5),
        ..ok.clone()
    };
    assert!(!garbled.is_confident());
}

#[tokio::test]
async fn utterance_becomes_one_user_turn() {
    if !vad_model().is_file() {
        eprintln!("no silero_vad.onnx — skipping");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let Some(stt) = fake_stt(tmp.path()) else {
        eprintln!("no venv python — skipping");
        return;
    };
    let engine = engine().await;
    let convo = engine.create().await.unwrap();
    let voice = VoiceInput::new(Arc::clone(&engine), stt, cfg(tmp.path()));

    let (tx, rx) = mpsc::unbounded_channel();
    let (samples, rate) = load_fixture_16k();
    // Feed speech, then ~0.6 s of silence so the VAD endpoints.
    for chunk in samples.chunks(1600) {
        tx.send(chunk.to_vec()).unwrap();
    }
    for _ in 0..12 {
        tx.send(vec![0.0_f32; 800]).unwrap();
    }
    drop(tx);

    voice
        .start_with_frames(convo.id.clone(), rx, rate, 1)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), voice.wait_idle())
        .await
        .expect("session finished");

    let msgs = engine.messages(&convo.id).await.unwrap();
    assert_eq!(msgs.len(), 1, "exactly one turn");
    match &msgs[0].content {
        crate::contracts::conversation::MessageContent::Text { text } => {
            assert!(text.contains("fake transcript"), "got {text:?}");
        }
        other => panic!("expected Text, got {other:?}"),
    }
}

#[tokio::test]
async fn empty_transcript_creates_no_turn() {
    if !vad_model().is_file() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let Some(_) = which_python() else { return };
    // Fake in "empty" mode: build a supervisor whose script forces it.
    let python = which_python().unwrap();
    std::fs::write(
        tmp.path().join("stt.py"),
        r#"import json,sys
print(json.dumps({"protocol_version":1,"worker":"Stt"})); sys.stdout.flush()
for line in sys.stdin:
    if not line.strip(): continue
    r=json.loads(line)
    print(json.dumps({"id":r["id"],"result":{"status":"Ok","body":{"data":{"text":"","no_speech_prob":0.95,"avg_logprob":-0.2}}}}))
    sys.stdout.flush()
"#,
    )
    .unwrap();
    let stt = Arc::new(WorkerSupervisor::new(
        WorkerLayout::for_test(python, tmp.path()),
        crate::contracts::worker::WorkerKind::Stt,
    ));
    let engine = engine().await;
    let convo = engine.create().await.unwrap();
    let voice = VoiceInput::new(Arc::clone(&engine), stt, cfg(tmp.path()));

    let (tx, rx) = mpsc::unbounded_channel();
    let (samples, rate) = load_fixture_16k();
    for chunk in samples.chunks(1600) {
        tx.send(chunk.to_vec()).unwrap();
    }
    for _ in 0..12 {
        tx.send(vec![0.0_f32; 800]).unwrap();
    }
    drop(tx);
    voice
        .start_with_frames(convo.id.clone(), rx, rate, 1)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), voice.wait_idle())
        .await
        .unwrap();

    assert!(engine.messages(&convo.id).await.unwrap().is_empty());
    assert_eq!(voice.state(), VoiceState::Error); // transient "didn't catch that"
}

#[tokio::test]
async fn second_start_while_listening_conflicts() {
    if !vad_model().is_file() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let Some(stt) = fake_stt(tmp.path()) else {
        return;
    };
    let engine = engine().await;
    let convo = engine.create().await.unwrap();
    let voice = VoiceInput::new(engine, stt, cfg(tmp.path()));

    let (_tx, rx) = mpsc::unbounded_channel();
    voice
        .start_with_frames(convo.id.clone(), rx, 16_000, 1)
        .await
        .unwrap();
    let (_tx2, rx2) = mpsc::unbounded_channel();
    let err = voice
        .start_with_frames(convo.id.clone(), rx2, 16_000, 1)
        .await
        .expect_err("conflict");
    assert_eq!(err.kind_str(), "Conflict");
    voice.cancel_listening().await;
}
