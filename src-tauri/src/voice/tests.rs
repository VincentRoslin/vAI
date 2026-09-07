//! `VoiceInput` orchestration — capture is bypassed (pre-recorded frames), VAD
//! is real (needs `models/vad/silero_vad.onnx`), the STT/TTS workers are the
//! stdlib fakes, and (Phase 19) the LLM is a scripted `LlmInstance`. No venv,
//! no GPU (beyond the ~2 MB ONNX VAD).

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::tts::TtsOutput;
use super::{SttResult, VoiceConfig, VoiceInput, VoiceState};
use crate::contracts::conversation::MessageContent;
use crate::contracts::generation::{GenerationEvent, SamplingParams, StopReason};
use crate::contracts::model::{Device, ModelBackend as BackendName, ModelKind};
use crate::contracts::worker::WorkerKind;
use crate::conversation::ConversationEngine;
use crate::db::Db;
use crate::lifecycle::backend::{
    Completion, LlmInstance, LoadRequest, LoadedInstance, ModelBackend,
};
use crate::lifecycle::{LifecycleManager, RetryPolicy};
use crate::models::{ModelDraft, ModelRegistry};
use crate::resources::probe::MockProbe;
use crate::resources::ResourceManager;
use crate::voice::vad::VadConfig;
use crate::worker::{WorkerLayout, WorkerSupervisor};

const BACKEND_KEY: &str = "scripted";

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
fn have_deps() -> bool {
    vad_model().is_file() && which_python().is_some()
}

// ---------------------------------------------------------------- fakes

/// STT / TTS supervisor whose script is `<repo>/workers/<from>` copied into
/// `tmp` (so `<tmp>/stt.py` etc.).
fn worker_from(tmp: &Path, from: &str, kind: WorkerKind) -> Arc<WorkerSupervisor> {
    let name = match kind {
        WorkerKind::Stt => "stt.py",
        WorkerKind::Tts => "tts.py",
        WorkerKind::Embed => "embedder.py",
    };
    std::fs::copy(repo().join("workers").join(from), tmp.join(name)).unwrap();
    Arc::new(WorkerSupervisor::new(
        WorkerLayout::for_test(which_python().unwrap(), tmp),
        kind,
    ))
}

/// STT supervisor whose `stt.py` is `content` written into `tmp`.
fn stt_inline(tmp: &Path, content: &str) -> Arc<WorkerSupervisor> {
    std::fs::write(tmp.join("stt.py"), content).unwrap();
    Arc::new(WorkerSupervisor::new(
        WorkerLayout::for_test(which_python().unwrap(), tmp),
        WorkerKind::Stt,
    ))
}

/// A scripted `LlmInstance` — streams `tokens` then `Done` (or `Error`).
struct ScriptedBackend {
    tokens: Vec<String>,
    error_end: bool,
    per_token_ms: u64,
}
#[async_trait]
impl ModelBackend for ScriptedBackend {
    async fn load(
        &self,
        _req: &LoadRequest,
        _cancel: CancellationToken,
    ) -> crate::ipc::AppResult<Box<dyn LoadedInstance>> {
        Ok(Box::new(ScriptedInstance {
            tokens: self.tokens.clone(),
            error_end: self.error_end,
            per_token_ms: self.per_token_ms,
        }))
    }
}
struct ScriptedInstance {
    tokens: Vec<String>,
    error_end: bool,
    per_token_ms: u64,
}
#[async_trait]
impl LoadedInstance for ScriptedInstance {
    async fn health(&self) -> crate::ipc::AppResult<()> {
        Ok(())
    }
    fn measured_vram_mb(&self) -> Option<u32> {
        Some(1024)
    }
    async fn shutdown(&self) {}
    fn as_llm(&self) -> Option<&dyn LlmInstance> {
        Some(self)
    }
    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}
#[async_trait]
impl LlmInstance for ScriptedInstance {
    async fn generate(
        &self,
        _prompt: String,
        _params: SamplingParams,
        _cancel: CancellationToken,
    ) -> crate::ipc::AppResult<Completion> {
        Ok(Completion {
            text: self.tokens.concat(),
            tokens: self.tokens.len() as u32,
            stop_reason: StopReason::EndOfText,
        })
    }
    async fn stream(
        &self,
        _prompt: String,
        _params: SamplingParams,
        tx: mpsc::Sender<GenerationEvent>,
        cancel: CancellationToken,
    ) {
        for (i, tok) in self.tokens.iter().enumerate() {
            if cancel.is_cancelled() {
                let _ = tx.send(GenerationEvent::Cancelled).await;
                return;
            }
            let _ = tx
                .send(GenerationEvent::TokenDelta {
                    index: i as u32,
                    text: tok.clone(),
                })
                .await;
            tokio::time::sleep(Duration::from_millis(self.per_token_ms)).await;
        }
        let ev = if self.error_end {
            GenerationEvent::Error {
                error: crate::ipc::AppError::WorkerCrashed("scripted".to_owned()),
            }
        } else {
            GenerationEvent::Done {
                stop_reason: StopReason::EndOfText,
                tokens: self.tokens.len() as u32,
            }
        };
        let _ = tx.send(ev).await;
    }
}

// ---------------------------------------------------------------- fixtures

struct Fixture {
    engine: Arc<ConversationEngine>,
    voice: Arc<VoiceInput>,
    convo_id: crate::contracts::ids::ConversationId,
    model_id: Option<crate::contracts::ids::ModelId>,
    _tmp: tempfile::TempDir,
}

/// Build an engine + voice service. `tokens = Some` registers + loads a scripted
/// LLM (conversational loop); `stt` is the STT supervisor to use.
async fn build(tokens: Option<&[&str]>, stt: impl Fn(&Path) -> Arc<WorkerSupervisor>) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&tmp.path().join("t.db")).await.unwrap());
    db.migrate().await.unwrap();
    let registry = Arc::new(ModelRegistry::new(Arc::clone(&db)));
    let resources = Arc::new(ResourceManager::new(Arc::new(MockProbe::new()), 1_500));
    resources.observe().await;
    let lifecycle = Arc::new(LifecycleManager::new(
        Arc::clone(&registry),
        resources,
        RetryPolicy::default(),
    ));

    let model_id = if let Some(toks) = tokens {
        let file = tmp.path().join("m.gguf");
        std::fs::write(&file, b"GGUF").unwrap();
        let model_id = registry
            .register(
                ModelDraft {
                    display_name: "Scripted".into(),
                    kind: ModelKind::Llm,
                    backend: BackendName(BACKEND_KEY.to_owned()),
                    quant: None,
                    path: file,
                    streaming: true,
                    context_tokens: Some(4096),
                    estimated_vram_mb: Some(1024),
                    devices: vec![Device::Cuda],
                    config: serde_json::json!({}),
                },
                tmp.path(),
            )
            .await
            .unwrap();
        lifecycle.register_backend(
            BACKEND_KEY,
            Arc::new(ScriptedBackend {
                tokens: toks.iter().map(|s| (*s).to_owned()).collect(),
                error_end: false,
                per_token_ms: 15,
            }) as Arc<dyn ModelBackend>,
        );
        lifecycle.load(&model_id).await.unwrap();
        Some(model_id)
    } else {
        None
    };

    let engine = Arc::new(ConversationEngine::new(
        db,
        Arc::clone(&registry),
        lifecycle,
    ));
    let convo = engine.create().await.unwrap();

    let stt = stt(tmp.path());
    let tts_worker = worker_from(tmp.path(), "tts_fake.py", WorkerKind::Tts);
    let tts = Arc::new(TtsOutput::new(
        tts_worker,
        tmp.path().join("tts-cache"),
        None,
        None,
    ));
    let voice = VoiceInput::new(Arc::clone(&engine), stt, tts, cfg(tmp.path()));

    Fixture {
        engine,
        voice,
        convo_id: convo.id,
        model_id,
        _tmp: tmp,
    }
}

fn fake_stt(tmp: &Path) -> Arc<WorkerSupervisor> {
    worker_from(tmp, "stt_fake.py", WorkerKind::Stt)
}

fn cfg(tmp: &Path) -> VoiceConfig {
    VoiceConfig {
        vad_model: vad_model(),
        temp_dir: tmp.join("segments"),
        input_device: None,
        output_device: None,
        vad: VadConfig {
            min_silence_ms: 300,
            ..VadConfig::default()
        },
        pre_roll_ms: 200,
        playback_duck: 0.2,
    }
}

fn fixture_16k() -> (Vec<f32>, u32) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hello_localai.wav");
    let mut r = hound::WavReader::open(path).unwrap();
    let spec = r.spec();
    let raw: Vec<f32> = r
        .samples::<i32>()
        .map(|s| s.unwrap() as f32 / f32::from(i16::MAX))
        .collect();
    (raw, spec.sample_rate)
}

/// Feed one spoken utterance + `silence_chunks` of trailing silence.
fn feed_utterance(tx: &mpsc::UnboundedSender<Vec<f32>>, samples: &[f32], silence_chunks: usize) {
    for chunk in samples.chunks(1600) {
        let _ = tx.send(chunk.to_vec());
    }
    for _ in 0..silence_chunks {
        let _ = tx.send(vec![0.0_f32; 800]);
    }
}

// ---------------------------------------------------------------- tests

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
    assert!(!SttResult {
        text: "   ".into(),
        ..ok.clone()
    }
    .is_confident());
    assert!(!SttResult {
        no_speech_prob: Some(0.9),
        ..ok.clone()
    }
    .is_confident());
    assert!(!SttResult {
        avg_logprob: Some(-2.5),
        ..ok.clone()
    }
    .is_confident());
}

#[tokio::test]
async fn one_utterance_becomes_a_user_turn_no_model() {
    if !have_deps() {
        eprintln!("no venv / vad model — skipping");
        return;
    }
    let f = build(None, fake_stt).await;
    let (tx, rx) = mpsc::unbounded_channel();
    let (samples, rate) = fixture_16k();
    feed_utterance(&tx, &samples, 12);
    drop(tx);

    f.voice
        .start_with_frames(f.convo_id.clone(), None, rx, rate, 1)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), f.voice.wait_idle())
        .await
        .expect("session finished");

    let msgs = f.engine.messages(&f.convo_id).await.unwrap();
    assert_eq!(msgs.len(), 1);
    let MessageContent::Text { text } = &msgs[0].content else {
        panic!("expected Text");
    };
    assert!(text.contains("fake transcript"), "{text:?}");
}

#[tokio::test]
async fn empty_transcript_creates_no_turn() {
    if !have_deps() {
        return;
    }
    let empty = r#"import json,sys
print(json.dumps({"protocol_version":1,"worker":"Stt"})); sys.stdout.flush()
for line in sys.stdin:
    if not line.strip(): continue
    r=json.loads(line)
    print(json.dumps({"id":r["id"],"result":{"status":"Ok","body":{"data":{"text":"","no_speech_prob":0.95,"avg_logprob":-0.2}}}}))
    sys.stdout.flush()
"#;
    let f = build(None, |tmp| stt_inline(tmp, empty)).await;

    let (tx, rx) = mpsc::unbounded_channel();
    let (samples, rate) = fixture_16k();
    feed_utterance(&tx, &samples, 12);
    drop(tx);
    f.voice
        .start_with_frames(f.convo_id.clone(), None, rx, rate, 1)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30), f.voice.wait_idle())
        .await
        .unwrap();

    assert!(f.engine.messages(&f.convo_id).await.unwrap().is_empty());
    assert_eq!(f.voice.state(), VoiceState::Error);
}

#[tokio::test]
async fn second_start_conflicts() {
    if !have_deps() {
        return;
    }
    let f = build(None, fake_stt).await;
    let (_tx, rx) = mpsc::unbounded_channel();
    f.voice
        .start_with_frames(f.convo_id.clone(), None, rx, 16_000, 1)
        .await
        .unwrap();
    let (_tx2, rx2) = mpsc::unbounded_channel();
    let err = f
        .voice
        .start_with_frames(f.convo_id.clone(), None, rx2, 16_000, 1)
        .await
        .expect_err("conflict");
    assert_eq!(err.kind_str(), "Conflict");
    f.voice.cancel_listening().await;
}

#[tokio::test]
async fn conversation_loop_speaks_a_reply_then_listens_again() {
    if !have_deps() {
        return;
    }
    let f = build(
        Some(&["Hello ", "there, ", "how can I help? ", "Anything else?"]),
        fake_stt,
    )
    .await;
    let mut states = f.voice.subscribe();
    let seen: Arc<std::sync::Mutex<Vec<VoiceState>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen2 = Arc::clone(&seen);
    tokio::spawn(async move {
        while states.changed().await.is_ok() {
            seen2.lock().unwrap().push(*states.borrow_and_update());
        }
    });

    let (tx, rx) = mpsc::unbounded_channel();
    let (samples, rate) = fixture_16k();
    feed_utterance(&tx, &samples, 14);
    // keep the channel open so the session loops; end it after the reply window
    f.voice
        .start_with_frames(f.convo_id.clone(), f.model_id.clone(), rx, rate, 1)
        .await
        .unwrap();

    // Wait for the assistant turn to be persisted.
    let ok = tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            let msgs = f.engine.messages(&f.convo_id).await.unwrap();
            if msgs
                .iter()
                .any(|m| m.role == crate::contracts::conversation::Role::Assistant)
            {
                return msgs;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("assistant turn persisted");

    let assistant = ok
        .iter()
        .find(|m| m.role == crate::contracts::conversation::Role::Assistant)
        .unwrap();
    let MessageContent::Text { text } = &assistant.content else {
        panic!()
    };
    // Whole reply spoken (not truncated) and normal stop reason.
    assert_eq!(text, "Hello there, how can I help? Anything else?");
    assert_eq!(
        assistant.generation.as_ref().unwrap().stop_reason,
        StopReason::EndOfText
    );

    drop(tx);
    f.voice.stop_listening().await;

    // We passed through an answering state (watch may collapse fast hops, so
    // accept any of them).
    let states = seen.lock().unwrap().clone();
    assert!(
        states.iter().any(|s| matches!(
            s,
            VoiceState::Transcribing | VoiceState::Thinking | VoiceState::Speaking
        )),
        "no answering state seen: {states:?}"
    );
}

#[tokio::test]
async fn barge_in_cancels_the_reply_and_persists_a_truncated_turn() {
    if !have_deps() {
        return;
    }
    // Many slow tokens so we can interrupt mid-generation.
    let toks: Vec<&str> = vec!["word "; 40];
    let f = build(Some(&toks), fake_stt).await;

    let (tx, rx) = mpsc::unbounded_channel();
    let (samples, rate) = fixture_16k();
    feed_utterance(&tx, &samples, 14); // first utterance → generate + speak
    f.voice
        .start_with_frames(f.convo_id.clone(), f.model_id.clone(), rx, rate, 1)
        .await
        .unwrap();

    // Wait until we're Thinking/Speaking, then speak over it (barge-in).
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if matches!(f.voice.state(), VoiceState::Thinking | VoiceState::Speaking) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("reached Thinking/Speaking");

    feed_utterance(&tx, &samples, 14); // second utterance = barge-in trigger
    drop(tx);

    tokio::time::timeout(Duration::from_secs(30), f.voice.wait_idle())
        .await
        .expect("session finished");

    let msgs = f.engine.messages(&f.convo_id).await.unwrap();
    if let Some(a) = msgs
        .iter()
        .find(|m| m.role == crate::contracts::conversation::Role::Assistant)
    {
        if let Some(g) = &a.generation {
            assert_eq!(g.stop_reason, StopReason::Cancelled, "truncated via cancel");
        }
        let MessageContent::Text { text } = &a.content else {
            panic!()
        };
        assert!(
            text.len() < "word ".len() * 40,
            "spoken prefix, not the whole reply"
        );
    }
    assert_ne!(f.voice.state(), VoiceState::Speaking, "playback stopped");
}

/// The chunker → TTS → playback path in isolation (fake TTS worker).
#[tokio::test]
async fn tts_speaks_clauses_then_cancel_stops() {
    let Some(python) = which_python() else { return };
    let tmp = tempfile::tempdir().unwrap();
    std::fs::copy(
        repo().join("workers/tts_fake.py"),
        tmp.path().join("tts.py"),
    )
    .unwrap();
    let worker = Arc::new(WorkerSupervisor::new(
        WorkerLayout::for_test(python, tmp.path()),
        WorkerKind::Tts,
    ));
    let out = Arc::new(TtsOutput::new(worker, tmp.path().join("cache"), None, None));

    let (tx, rx) = mpsc::channel::<String>(8);
    let cancel = CancellationToken::new();
    let o2 = Arc::clone(&out);
    let c2 = cancel.clone();
    let handle = tokio::spawn(async move { o2.speak_stream(rx, c2).await });

    tx.send("This is the first clause of the reply.".into())
        .await
        .unwrap();
    tx.send("And a second, slightly longer clause here.".into())
        .await
        .unwrap();

    // Wait (up to 8 s) for the fake worker to produce some audio.
    let mut queued = false;
    for _ in 0..160 {
        if out.queued_ms().await > 0 {
            queued = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(queued, "the fake TTS produced audio");

    cancel.cancel();
    out.stop().await;
    drop(tx);
    let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(out.playback_idle().await, "playback stopped after cancel");
}
