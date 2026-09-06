//! Phase 16 gate coverage for the conversation service — repo round-trips,
//! ChatML rendering, and the send / stream / cancel / failure flow against a
//! **scripted** LLM instance (no real `llama-server`).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::repo::ConversationRepo;
use super::ConversationEngine;
use crate::contracts::conversation::{ConversationKind, MessageContent, Role};
use crate::contracts::generation::{GenerationEvent, SamplingParams, StopReason};
use crate::contracts::ids::ModelId;
use crate::contracts::model::{Device, ModelBackend as BackendName, ModelKind};
use crate::db::Db;
use crate::ipc::AppError;
use crate::lifecycle::backend::{
    Completion, LlmInstance, LoadRequest, LoadedInstance, ModelBackend,
};
use crate::lifecycle::{LifecycleManager, RetryPolicy};
use crate::models::{ModelDraft, ModelRegistry};
use crate::resources::probe::MockProbe;
use crate::resources::ResourceManager;

const BACKEND_KEY: &str = "scripted";

// ---------------------------------------------------------------- repo + prompt

async fn repo() -> (ConversationRepo, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(&tmp.path().join("c.db")).await.unwrap();
    db.migrate().await.unwrap();
    (ConversationRepo::new(Arc::new(db)), tmp)
}

#[tokio::test]
async fn repo_round_trips_a_conversation_with_messages() {
    let (repo, _tmp) = repo().await;
    let convo = repo.create(ConversationKind::Persona).await.unwrap();

    repo.append(&convo.id, Role::User, text("hello"), None)
        .await
        .unwrap();
    repo.append(
        &convo.id,
        Role::Assistant,
        text("hi there"),
        Some(crate::contracts::conversation::GenerationMeta {
            model: ModelId::from_trusted("m1"),
            stop_reason: StopReason::EndOfText,
            tokens: 2,
            duration_ms: 42,
        }),
    )
    .await
    .unwrap();

    let msgs = repo.messages(&convo.id).await.unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, Role::User);
    assert_eq!(msgs[1].generation.as_ref().unwrap().tokens, 2);
    assert_eq!(
        msgs[1].generation.as_ref().unwrap().stop_reason,
        StopReason::EndOfText
    );

    // `latest` + reopen persistence.
    assert_eq!(repo.latest().await.unwrap().unwrap().id, convo.id);
}

#[tokio::test]
async fn append_to_a_missing_conversation_is_not_found() {
    let (repo, _tmp) = repo().await;
    let err = repo
        .append(
            &crate::contracts::ids::ConversationId::from_trusted("nope"),
            Role::User,
            text("x"),
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn set_persona_binds_and_is_fixed_after_a_turn() {
    use crate::context::persona::{PersonaDraft, PersonaRepo};

    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&tmp.path().join("c.db")).await.unwrap());
    db.migrate().await.unwrap();
    let repo = ConversationRepo::new(Arc::clone(&db));
    let personas = PersonaRepo::new(Arc::clone(&db));

    let pid = personas
        .create(PersonaDraft {
            name: "Ada".to_owned(),
            summary: String::new(),
            personality: String::new(),
            tone: String::new(),
            style: String::new(),
            guidance: vec![],
        })
        .await
        .unwrap();

    let convo = repo.create(ConversationKind::Persona).await.unwrap();
    assert_eq!(repo.persona_id(&convo.id).await.unwrap(), None);

    // Unknown persona → NotFound.
    assert!(matches!(
        repo.set_persona(
            &convo.id,
            Some(crate::contracts::ids::PersonaId::from_trusted("ghost")),
        )
        .await,
        Err(AppError::NotFound(_))
    ));

    repo.set_persona(&convo.id, Some(pid.clone()))
        .await
        .unwrap();
    assert_eq!(repo.persona_id(&convo.id).await.unwrap(), Some(pid.clone()));
    assert_eq!(
        repo.get(&convo.id).await.unwrap().persona_id,
        Some(pid.clone())
    );

    // Once a turn exists, the persona is fixed.
    repo.append(&convo.id, Role::User, text("hi"), None)
        .await
        .unwrap();
    assert!(matches!(
        repo.set_persona(&convo.id, None).await,
        Err(AppError::Conflict(_))
    ));
}

// ChatML assembly is now the context builder's responsibility — see
// `context::builder::tests` (Phase 20). The scripted `stream` below still
// asserts it receives a ChatML prompt.

fn text(s: &str) -> MessageContent {
    MessageContent::Text { text: s.to_owned() }
}

// ---------------------------------------------------------------- scripted LLM

/// What the scripted stream should do after emitting its tokens.
#[derive(Clone, Copy)]
enum Ending {
    Done,
    Error,
    /// Emit one token, then block until cancelled.
    StallThenCancel,
}

type PromptLog = Arc<std::sync::Mutex<Vec<String>>>;

struct ScriptedBackend {
    tokens: Vec<String>,
    ending: Ending,
    loads: Arc<AtomicU32>,
    prompts: PromptLog,
}

#[async_trait]
impl ModelBackend for ScriptedBackend {
    async fn load(
        &self,
        _req: &LoadRequest,
        _cancel: CancellationToken,
    ) -> crate::ipc::AppResult<Box<dyn LoadedInstance>> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(ScriptedInstance {
            tokens: self.tokens.clone(),
            ending: self.ending,
            prompts: Arc::clone(&self.prompts),
        }))
    }
}

struct ScriptedInstance {
    tokens: Vec<String>,
    ending: Ending,
    prompts: PromptLog,
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
            tokens: u32::try_from(self.tokens.len()).unwrap_or(0),
            stop_reason: StopReason::EndOfText,
        })
    }

    async fn stream(
        &self,
        prompt: String,
        _params: SamplingParams,
        tx: mpsc::Sender<GenerationEvent>,
        cancel: CancellationToken,
    ) {
        assert!(prompt.contains("<|im_start|>user\n"), "prompt is ChatML");
        self.prompts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(prompt);
        for (i, tok) in self.tokens.iter().enumerate() {
            if cancel.is_cancelled() {
                let _ = tx.send(GenerationEvent::Cancelled).await;
                return;
            }
            let _ = tx
                .send(GenerationEvent::TokenDelta {
                    index: u32::try_from(i).unwrap_or(0),
                    text: tok.clone(),
                })
                .await;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        match self.ending {
            Ending::Done => {
                let _ = tx
                    .send(GenerationEvent::Done {
                        stop_reason: StopReason::EndOfText,
                        tokens: u32::try_from(self.tokens.len()).unwrap_or(0),
                    })
                    .await;
            }
            Ending::Error => {
                let _ = tx
                    .send(GenerationEvent::Error {
                        error: AppError::WorkerCrashed("scripted backend died".to_owned()),
                    })
                    .await;
            }
            Ending::StallThenCancel => {
                cancel.cancelled().await;
                let _ = tx.send(GenerationEvent::Cancelled).await;
            }
        }
    }
}

// ---------------------------------------------------------------- service fixture

struct Fixture {
    _tmp: tempfile::TempDir,
    service: Arc<ConversationEngine>,
    model: ModelId,
    loads: Arc<AtomicU32>,
    db: Arc<Db>,
    prompts: PromptLog,
}

async fn fixture(tokens: &[&str], ending: Ending) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let models_dir = tmp.path().join("models");
    std::fs::create_dir_all(&models_dir).unwrap();
    let file = models_dir.join("m.gguf");
    std::fs::write(&file, b"GGUF").unwrap();

    let db = Arc::new(Db::open(&tmp.path().join("c.db")).await.unwrap());
    db.migrate().await.unwrap();

    let registry = Arc::new(ModelRegistry::new(Arc::clone(&db)));
    let model = registry
        .register(
            ModelDraft {
                display_name: "Scripted".to_owned(),
                kind: ModelKind::Llm,
                backend: BackendName(BACKEND_KEY.to_owned()),
                quant: None,
                path: file,
                streaming: true,
                context_tokens: Some(4096),
                estimated_vram_mb: Some(1024),
                devices: vec![Device::Cuda],
                config: json!({}),
            },
            &models_dir,
        )
        .await
        .unwrap();

    let probe = Arc::new(MockProbe::new());
    let resources = Arc::new(ResourceManager::new(probe, 1_500));
    resources.observe().await;
    let lifecycle = Arc::new(LifecycleManager::new(
        Arc::clone(&registry),
        resources,
        RetryPolicy::default(),
    ));
    let loads = Arc::new(AtomicU32::new(0));
    let prompts: PromptLog = Arc::new(std::sync::Mutex::new(Vec::new()));
    lifecycle.register_backend(
        BACKEND_KEY,
        Arc::new(ScriptedBackend {
            tokens: tokens.iter().map(|s| (*s).to_owned()).collect(),
            ending,
            loads: Arc::clone(&loads),
            prompts: Arc::clone(&prompts),
        }) as Arc<dyn ModelBackend>,
    );
    lifecycle.load(&model).await.expect("model loads");

    let service = Arc::new(ConversationEngine::new(
        Arc::clone(&db),
        Arc::clone(&registry),
        lifecycle,
    ));
    Fixture {
        _tmp: tmp,
        service,
        model,
        loads,
        db,
        prompts,
    }
}

fn sink_channel() -> (
    mpsc::UnboundedSender<GenerationEvent>,
    mpsc::UnboundedReceiver<GenerationEvent>,
) {
    mpsc::unbounded_channel()
}

async fn collect(mut rx: mpsc::UnboundedReceiver<GenerationEvent>) -> Vec<GenerationEvent> {
    let mut out = Vec::new();
    while let Some(ev) = rx.recv().await {
        out.push(ev);
    }
    out
}

// ---------------------------------------------------------------- state machine + split API

#[tokio::test]
async fn generation_state_tracks_idle_then_generating_then_idle() {
    let f = fixture(&["a", "b", "c"], Ending::Done).await;
    let convo = f.service.create().await.unwrap();
    assert_eq!(f.service.generation_state().await.generating, None);

    let (tx, rx) = sink_channel();
    let task = f
        .service
        .send(
            convo.id.clone(),
            f.model.clone(),
            "hi".to_owned(),
            move |ev| {
                let _ = tx.send(ev);
            },
        )
        .await
        .unwrap();

    // Generating — with the right task + conversation.
    let st = f
        .service
        .generation_state()
        .await
        .generating
        .expect("generating");
    assert_eq!(st.task_id, task);
    assert_eq!(st.conversation_id, convo.id);

    let _ = collect(rx).await;
    wait_for_idle(&f.service).await;
    assert_eq!(f.service.generation_state().await.generating, None);
}

#[tokio::test]
async fn add_user_turn_then_generate_streams_and_persists() {
    let f = fixture(&["Hi ", "there"], Ending::Done).await;
    let convo = f.service.create().await.unwrap();

    // A typed turn added on its own (this is the seam voice/STT uses).
    f.service
        .add_user_turn(&convo.id, text("question"))
        .await
        .expect("turn persisted");
    assert_eq!(f.service.messages(&convo.id).await.unwrap().len(), 1);

    let (tx, rx) = sink_channel();
    f.service
        .generate(convo.id.clone(), f.model.clone(), move |ev| {
            let _ = tx.send(ev);
        })
        .await
        .expect("generate accepted");
    assert!(matches!(
        collect(rx).await.last(),
        Some(GenerationEvent::Done { .. })
    ));
    wait_for_idle(&f.service).await;

    let msgs = f.service.messages(&convo.id).await.unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(
        msgs[1].content,
        MessageContent::Text {
            text: "Hi there".to_owned()
        }
    );
}

#[tokio::test]
async fn add_user_turn_rejects_empty_text_and_a_missing_conversation() {
    let f = fixture(&["x"], Ending::Done).await;
    let convo = f.service.create().await.unwrap();
    assert!(matches!(
        f.service.add_user_turn(&convo.id, text("  ")).await,
        Err(AppError::Validation(_))
    ));
    assert!(matches!(
        f.service
            .add_user_turn(
                &crate::contracts::ids::ConversationId::from_trusted("nope"),
                text("hi"),
            )
            .await,
        Err(AppError::NotFound(_))
    ));
}

// ---------------------------------------------------------------- send / stream

#[tokio::test]
async fn send_streams_deltas_and_persists_the_assistant_message() {
    let f = fixture(&["Hel", "lo", "!"], Ending::Done).await;
    let convo = f.service.create().await.unwrap();

    let (tx, rx) = sink_channel();
    let task = f
        .service
        .send(
            convo.id.clone(),
            f.model.clone(),
            "hi".to_owned(),
            move |ev| {
                let _ = tx.send(ev);
            },
        )
        .await
        .expect("send accepted");
    assert!(!task.to_string().is_empty());

    let events = collect(rx).await;
    let deltas: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            GenerationEvent::TokenDelta { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, ["Hel", "lo", "!"]);
    assert!(matches!(events.last(), Some(GenerationEvent::Done { .. })));

    // Wait for persistence (the spawn finishes just after the terminal frame).
    wait_for_idle(&f.service).await;
    let msgs = f.service.messages(&convo.id).await.unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1].role, Role::Assistant);
    assert_eq!(
        msgs[1].content,
        MessageContent::Text {
            text: "Hello!".to_owned()
        }
    );
    assert_eq!(
        msgs[1].generation.as_ref().unwrap().stop_reason,
        StopReason::EndOfText
    );
}

#[tokio::test]
async fn a_second_send_while_generating_is_a_conflict() {
    let f = fixture(&["a", "b", "c", "d"], Ending::StallThenCancel).await;
    let convo = f.service.create().await.unwrap();

    let (tx, _rx) = sink_channel();
    f.service
        .send(
            convo.id.clone(),
            f.model.clone(),
            "one".to_owned(),
            move |ev| {
                let _ = tx.send(ev);
            },
        )
        .await
        .expect("first accepted");

    tokio::time::sleep(Duration::from_millis(20)).await;
    let err = f
        .service
        .send(convo.id.clone(), f.model.clone(), "two".to_owned(), |_| {})
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));

    // clean up the stalled generation
    f.service.shutdown().await;
    wait_for_idle(&f.service).await;
}

#[tokio::test]
async fn cancel_mid_stream_persists_the_partial_turn() {
    let toks = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let f = fixture(&toks, Ending::StallThenCancel).await;
    let convo = f.service.create().await.unwrap();

    let (tx, rx) = sink_channel();
    let task = f
        .service
        .send(
            convo.id.clone(),
            f.model.clone(),
            "go".to_owned(),
            move |ev| {
                let _ = tx.send(ev);
            },
        )
        .await
        .unwrap();

    // Tokens land every ~20 ms; cancel well before all 8 have been emitted.
    tokio::time::sleep(Duration::from_millis(30)).await;
    f.service.cancel(&task).await.expect("cancel accepted");

    let events = collect(rx).await;
    assert!(matches!(events.last(), Some(GenerationEvent::Cancelled)));
    let deltas = events
        .iter()
        .filter(|e| matches!(e, GenerationEvent::TokenDelta { .. }))
        .count();
    assert!(
        deltas > 0 && deltas < toks.len(),
        "cancelled mid-stream: {deltas} deltas"
    );

    wait_for_idle(&f.service).await;
    let msgs = f.service.messages(&convo.id).await.unwrap();
    assert_eq!(msgs.len(), 2);
    let meta = msgs[1].generation.as_ref().unwrap();
    assert_eq!(meta.stop_reason, StopReason::Cancelled);
    if let MessageContent::Text { text } = &msgs[1].content {
        assert!(!text.is_empty() && text.len() < toks.concat().len());
    } else {
        panic!("expected text");
    }

    // The model is still usable — a second send is accepted (this fixture's
    // stream stalls, so cancel it to let the test finish).
    let (tx2, rx2) = sink_channel();
    let task2 = f
        .service
        .send(
            convo.id.clone(),
            f.model.clone(),
            "again".to_owned(),
            move |ev| {
                let _ = tx2.send(ev);
            },
        )
        .await
        .expect("model still usable");
    tokio::time::sleep(Duration::from_millis(10)).await;
    f.service.cancel(&task2).await.ok();
    let _ = collect(rx2).await;
    wait_for_idle(&f.service).await;
}

#[tokio::test]
async fn a_backend_error_persists_a_truncated_turn_and_recovers() {
    let f = fixture(&["partial "], Ending::Error).await;
    let convo = f.service.create().await.unwrap();

    let (tx, rx) = sink_channel();
    f.service
        .send(
            convo.id.clone(),
            f.model.clone(),
            "hi".to_owned(),
            move |ev| {
                let _ = tx.send(ev);
            },
        )
        .await
        .unwrap();
    let events = collect(rx).await;
    assert!(events
        .iter()
        .any(|e| matches!(e, GenerationEvent::Error { .. })));

    wait_for_idle(&f.service).await;
    let msgs = f.service.messages(&convo.id).await.unwrap();
    assert_eq!(
        msgs[1].generation.as_ref().unwrap().stop_reason,
        StopReason::Error
    );
    assert_eq!(
        msgs[1].content,
        MessageContent::Text {
            text: "partial ".to_owned()
        }
    );

    // A following send works (the scripted instance stays alive).
    let (tx2, rx2) = sink_channel();
    f.service
        .send(
            convo.id.clone(),
            f.model.clone(),
            "retry".to_owned(),
            move |ev| {
                let _ = tx2.send(ev);
            },
        )
        .await
        .expect("recovered");
    let _ = collect(rx2).await;
    wait_for_idle(&f.service).await;
    assert_eq!(f.loads.load(Ordering::SeqCst), 1);
}

// ---------------------------------------------------------------- persona → prompt (Phase 20)

#[tokio::test]
async fn the_prompt_the_adapter_receives_carries_the_active_persona() {
    use crate::context::persona::{PersonaDraft, PersonaRepo};

    let f = fixture(&["ok"], Ending::Done).await;
    let personas = PersonaRepo::new(Arc::clone(&f.db));
    let repo = ConversationRepo::new(Arc::clone(&f.db));

    let pid = personas
        .create(PersonaDraft {
            name: "Mar* the Cartographer".to_owned(),
            summary: "keeper of forgotten coastlines".to_owned(),
            personality: "meticulous, wry, allergic to rounding errors".to_owned(),
            tone: String::new(),
            style: String::new(),
            guidance: vec![],
        })
        .await
        .unwrap();

    let convo = f.service.create().await.unwrap();
    repo.set_persona(&convo.id, Some(pid)).await.unwrap();

    let (tx, rx) = sink_channel();
    f.service
        .send(
            convo.id.clone(),
            f.model.clone(),
            "where".to_owned(),
            move |ev| {
                let _ = tx.send(ev);
            },
        )
        .await
        .unwrap();
    let _ = collect(rx).await;
    wait_for_idle(&f.service).await;

    let seen = f.prompts.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    let prompt = &seen[0];
    assert!(
        prompt.contains("keeper of forgotten coastlines"),
        "{prompt}"
    );
    assert!(
        prompt.contains("meticulous, wry, allergic to rounding errors"),
        "{prompt}"
    );
    assert!(
        prompt.contains("You are Mar* the Cartographer."),
        "{prompt}"
    );

    // `preview_prompt` returns the same assembly (FR-34 surface).
    let preview = f.service.preview_prompt(&convo.id, &f.model).await.unwrap();
    assert!(preview.text.contains("keeper of forgotten coastlines"));
    assert!(preview.provenance.total_tokens > 0);
}

#[tokio::test]
async fn no_persona_prompt_is_bare_system_plus_history() {
    let f = fixture(&["ok"], Ending::Done).await;
    let convo = f.service.create().await.unwrap();

    let (tx, rx) = sink_channel();
    f.service
        .send(
            convo.id.clone(),
            f.model.clone(),
            "hello there".to_owned(),
            move |ev| {
                let _ = tx.send(ev);
            },
        )
        .await
        .unwrap();
    let _ = collect(rx).await;
    wait_for_idle(&f.service).await;

    let prompt = f.prompts.lock().unwrap()[0].clone();
    assert!(prompt.starts_with("<|im_start|>system\nYou are a helpful assistant."));
    assert!(prompt.contains("<|im_start|>user\nhello there<|im_end|>"));
    assert!(!prompt.contains("Personality:"));
}

// ---------------------------------------------------------------- helpers

async fn wait_for_idle(service: &ConversationEngine) {
    for _ in 0..200 {
        if !service.is_generating().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("generation never finished");
}
