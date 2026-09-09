//! Conversation engine — **the one** engine that every conversational surface
//! runs on: text chat now (Phase 16 proved the flow), voice turns (18/19),
//! persona/character conversations (20/25/26). There is no second engine.
//!
//! It owns conversation + message state (via [`repo`]); the lifecycle manager
//! owns the model. Responsibilities:
//! - conversation lifecycle (`create` / `list` / `latest` / `messages`),
//! - **typed** user turns ([`ConversationEngine::add_user_turn`] — text now,
//!   audio/image references later without a schema break),
//! - one LLM generation at a time ([`ConversationEngine::generate`]) with a
//!   first-class streaming state machine ([`ConversationEngine::generation_state`])
//!   and cancellation,
//! - persisting the assistant turn (partial or complete) on the terminal frame.
//!
//! Concurrency (v1): **one generation at a time** — a second `generate` while one
//! runs returns [`AppError::Conflict`]. A queue is Phase 24.

pub mod repo;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod live_tests;

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::context::builder::{
    BuildInput, ContextBuilder, MemoryItem, RuntimeContext, TokenBudget,
};
use crate::context::persona::PersonaRepo;
use crate::contracts::conversation::{
    Conversation, ConversationKind, GenerationHandle, GenerationMeta, GenerationState, Message,
    MessageContent, Role,
};
use crate::contracts::generation::{GenerationEvent, SamplingParams, StopReason};
use crate::contracts::ids::{ConversationId, ModelId, TaskId};
use crate::db::Db;
use crate::ipc::{AppError, AppResult};
use crate::lifecycle::LifecycleManager;
use crate::memory::{MemoryScope, MemoryService};
use crate::models::ModelRegistry;
use repo::ConversationRepo;

/// Hard cap on tokens per turn for now (Phase 20 makes it configurable).
const MAX_TOKENS: u32 = 1024;

/// The in-flight generation, if any — the engine's streaming state.
struct Running {
    handle: GenerationHandle,
    cancel: CancellationToken,
}

/// The one conversation engine. Held in managed state as
/// `Arc<ConversationEngine>`.
pub struct ConversationEngine {
    repo: ConversationRepo,
    personas: PersonaRepo,
    memory: MemoryService,
    registry: Arc<ModelRegistry>,
    builder: ContextBuilder,
    lifecycle: Arc<LifecycleManager>,
    running: Mutex<Option<Running>>,
}

impl ConversationEngine {
    #[must_use]
    pub fn new(
        db: Arc<Db>,
        registry: Arc<ModelRegistry>,
        lifecycle: Arc<LifecycleManager>,
    ) -> Self {
        Self {
            repo: ConversationRepo::new(Arc::clone(&db)),
            personas: PersonaRepo::new(Arc::clone(&db)),
            memory: MemoryService::new(db, Arc::clone(&lifecycle)),
            registry,
            builder: ContextBuilder,
            lifecycle,
            running: Mutex::new(None),
        }
    }

    /// The memory service (retrieval + extraction + the FR-52/53 CRUD),
    /// reachable for IPC.
    #[must_use]
    pub fn memory(&self) -> &MemoryService {
        &self.memory
    }

    // ---------------------------------------------------------------- lifecycle

    /// Create a new (Persona) conversation.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn create(&self) -> AppResult<Conversation> {
        self.repo.create(ConversationKind::Persona).await
    }

    /// Create a new Persona conversation, optionally bound to `persona_id`.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if `persona_id` is unknown; a persistence error.
    pub async fn create_with_persona(
        &self,
        persona_id: Option<crate::contracts::ids::PersonaId>,
    ) -> AppResult<Conversation> {
        let convo = self.repo.create(ConversationKind::Persona).await?;
        if persona_id.is_some() {
            self.repo.set_persona(&convo.id, persona_id.clone()).await?;
        }
        Ok(Conversation {
            persona_id,
            ..convo
        })
    }

    /// Bind (or clear) the Persona for a conversation. Rejected once the
    /// conversation has a turn (FR-17).
    ///
    /// # Errors
    /// [`AppError::NotFound`] / [`AppError::Conflict`]; a persistence error.
    pub async fn set_persona(
        &self,
        conversation_id: &ConversationId,
        persona_id: Option<crate::contracts::ids::PersonaId>,
    ) -> AppResult<()> {
        self.repo.set_persona(conversation_id, persona_id).await
    }

    /// The persona repository (persona CRUD lives here, shared with the builder).
    #[must_use]
    pub fn personas(&self) -> &PersonaRepo {
        &self.personas
    }

    /// Every conversation, newest activity first.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list(&self) -> AppResult<Vec<Conversation>> {
        self.repo.list().await
    }

    /// The most-recently-active conversation.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn latest(&self) -> AppResult<Option<Conversation>> {
        self.repo.latest().await
    }

    /// A conversation's messages, in order.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn messages(&self, id: &ConversationId) -> AppResult<Vec<Message>> {
        self.repo.messages(id).await
    }

    // ---------------------------------------------------------------- state

    /// The engine's streaming state: whether a generation is running and, if so,
    /// which task / conversation.
    pub async fn generation_state(&self) -> GenerationState {
        GenerationState {
            generating: self.running.lock().await.as_ref().map(|r| r.handle.clone()),
        }
    }

    /// Convenience: whether a generation is currently running.
    pub async fn is_generating(&self) -> bool {
        self.running.lock().await.is_some()
    }

    // ---------------------------------------------------------------- turns

    /// Persist one user turn with **typed** content. `Text` must be non-empty;
    /// `Audio` / `Image` are accepted as-is (Phase 18/22).
    ///
    /// # Errors
    /// [`AppError::Validation`] for empty text; [`AppError::NotFound`] for an
    /// unknown conversation; a persistence error.
    pub async fn add_user_turn(
        &self,
        conversation_id: &ConversationId,
        content: MessageContent,
    ) -> AppResult<Message> {
        if let MessageContent::Text { text } = &content {
            if text.trim().is_empty() {
                return Err(AppError::Validation("message text is empty".to_owned()));
            }
        }
        self.repo
            .append(conversation_id, Role::User, content, None)
            .await
    }

    /// Run one LLM generation over the conversation's history. `sink` receives
    /// each [`GenerationEvent`]; the assistant turn is persisted (partial or
    /// complete) when the stream terminates. Returns immediately with the
    /// generation's task id.
    ///
    /// # Errors
    /// - [`AppError::Conflict`] — a generation is already running.
    /// - [`AppError::NotFound`] — no such conversation.
    /// - [`AppError::BackendUnavailable`] / [`AppError::Validation`] — the model
    ///   is not loaded / not an LLM.
    /// - a persistence error.
    pub async fn generate<F>(
        self: &Arc<Self>,
        conversation_id: ConversationId,
        model_id: ModelId,
        sink: F,
    ) -> AppResult<TaskId>
    where
        F: Fn(GenerationEvent) + Send + Sync + 'static,
    {
        self.generate_inner(conversation_id, model_id, false, sink)
            .await
    }

    /// Like [`Self::generate`], but the prompt includes spoken-register
    /// instructions and a shorter token cap — used by the voice loop.
    pub async fn generate_spoken<F>(
        self: &Arc<Self>,
        conversation_id: ConversationId,
        model_id: ModelId,
        sink: F,
    ) -> AppResult<TaskId>
    where
        F: Fn(GenerationEvent) + Send + Sync + 'static,
    {
        self.generate_inner(conversation_id, model_id, true, sink)
            .await
    }

    async fn generate_inner<F>(
        self: &Arc<Self>,
        conversation_id: ConversationId,
        model_id: ModelId,
        spoken: bool,
        sink: F,
    ) -> AppResult<TaskId>
    where
        F: Fn(GenerationEvent) + Send + Sync + 'static,
    {
        let mut guard = self.running.lock().await;
        if guard.is_some() {
            return Err(AppError::Conflict(
                "a generation is already running".to_owned(),
            ));
        }
        // Fail fast on a missing conversation before we mark ourselves busy.
        self.repo.get(&conversation_id).await?;

        let task_id = TaskId::from_trusted(uuid::Uuid::new_v4().hyphenated().to_string());
        let cancel = CancellationToken::new();
        *guard = Some(Running {
            handle: GenerationHandle {
                task_id: task_id.clone(),
                conversation_id: conversation_id.clone(),
            },
            cancel: cancel.clone(),
        });
        drop(guard);
        tracing::info!(task_id = %task_id, conversation = %conversation_id, "generation started");

        let this = Arc::clone(self);
        let tid = task_id.clone();
        tokio::spawn(async move {
            this.run_generation(conversation_id, model_id, tid, cancel, spoken, sink)
                .await;
        });

        Ok(task_id)
    }

    /// Add a text user turn and start generating a reply — the convenience the
    /// text chat UI uses.
    ///
    /// # Errors
    /// As [`ConversationEngine::add_user_turn`] + [`ConversationEngine::generate`].
    pub async fn send<F>(
        self: &Arc<Self>,
        conversation_id: ConversationId,
        model_id: ModelId,
        text: String,
        sink: F,
    ) -> AppResult<TaskId>
    where
        F: Fn(GenerationEvent) + Send + Sync + 'static,
    {
        if self.is_generating().await {
            return Err(AppError::Conflict(
                "a generation is already running".to_owned(),
            ));
        }
        self.add_user_turn(&conversation_id, MessageContent::Text { text })
            .await?;
        self.generate(conversation_id, model_id, sink).await
    }

    /// Cancel the running generation identified by `task_id`.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if no generation with that id is running.
    pub async fn cancel(&self, task_id: &TaskId) -> AppResult<()> {
        let guard = self.running.lock().await;
        match guard.as_ref() {
            Some(r) if &r.handle.task_id == task_id => {
                r.cancel.cancel();
                Ok(())
            }
            _ => Err(AppError::NotFound(format!("generation {task_id}"))),
        }
    }

    /// Cancel any running generation + in-flight memory extraction (app exit).
    pub async fn shutdown(&self) {
        if let Some(r) = self.running.lock().await.as_ref() {
            r.cancel.cancel();
        }
        self.memory.shutdown();
    }

    // ---------------------------------------------------------------- internals

    async fn run_generation<F>(
        self: Arc<Self>,
        conversation_id: ConversationId,
        model_id: ModelId,
        task_id: TaskId,
        cancel: CancellationToken,
        spoken: bool,
        sink: F,
    ) where
        F: Fn(GenerationEvent) + Send + Sync + 'static,
    {
        let op = crate::logging::operation(Some(&task_id), "conversation_generate");
        let status = match self
            .stream_once(&conversation_id, &model_id, &cancel, spoken, &sink)
            .await
        {
            // The stream ran (possibly ending in Error/Cancelled) — persist the
            // assistant turn, partial or not, so the UI never loses it.
            Ok((text, meta)) => {
                let status = match meta.stop_reason {
                    StopReason::Error => crate::logging::Status::Failed,
                    StopReason::Cancelled => crate::logging::Status::Cancelled,
                    _ => crate::logging::Status::Ok,
                };
                if status == crate::logging::Status::Failed {
                    // The backend itself misbehaved mid-stream (a timeout or a
                    // transport failure — the only ways `stream_once` reaches
                    // `StopReason::Error` once streaming has started). `/health`
                    // may still report fine (a WDDM hang can freeze the compute
                    // path without killing the process), so without this the
                    // *next* message would hit the same wedged backend and
                    // stall again. Force a fresh backend on the next `load`.
                    self.lifecycle
                        .report_unhealthy(&model_id, "generation ended in StopReason::Error")
                        .await;
                }
                let persisted = self
                    .repo
                    .append(
                        &conversation_id,
                        Role::Assistant,
                        MessageContent::Text { text },
                        Some(meta.clone()),
                    )
                    .await;
                if let Err(err) = &persisted {
                    err.log("persist assistant message");
                }
                // A clean completion → mine the exchange for memories
                // (per-Persona, off the response path).
                if persisted.is_ok()
                    && matches!(
                        meta.stop_reason,
                        StopReason::EndOfText | StopReason::MaxTokens | StopReason::StopSequence
                    )
                {
                    self.maybe_extract_memory(&conversation_id, &model_id).await;
                }
                status
            }
            // Setup failed before any streaming — surface it, persist nothing
            // (no half-turn to pollute the transcript).
            Err(err) => {
                sink(GenerationEvent::Error { error: err });
                crate::logging::Status::Failed
            }
        };

        *self.running.lock().await = None;
        op.finish(status);
    }

    /// Assemble the exact prompt the LLM adapter will receive for the next
    /// generation on `conversation_id` with `model_id` — the one
    /// [`ContextBuilder`] call [`Self::stream_once`] makes. Exposed for the
    /// FR-34 "show prompt" surface (`chat_prompt_preview`).
    ///
    /// # Errors
    /// [`AppError::NotFound`] for an unknown conversation or model; a
    /// persistence error otherwise.
    pub async fn preview_prompt(
        &self,
        conversation_id: &ConversationId,
        model_id: &ModelId,
    ) -> AppResult<crate::context::builder::BuiltPrompt> {
        self.build_prompt(conversation_id, model_id, false).await
    }

    /// The deterministic context assembly shared by `stream_once` and
    /// `preview_prompt`: batch the history + persona + model reads, then call
    /// the one [`ContextBuilder`].
    async fn build_prompt(
        &self,
        conversation_id: &ConversationId,
        model_id: &ModelId,
        spoken: bool,
    ) -> AppResult<crate::context::builder::BuiltPrompt> {
        let history = self.repo.messages(conversation_id).await?;
        let persona_id = self.repo.persona_id(conversation_id).await?;
        let persona = match &persona_id {
            Some(pid) => match self.personas.get(pid).await {
                Ok(p) => Some(p),
                // The row was deleted between the two reads (races only). Treat
                // as no persona.
                Err(AppError::NotFound(_)) => None,
                Err(e) => return Err(e),
            },
            None => None,
        };
        let model = self.registry.get(model_id).await?;
        let budget = TokenBudget::from_context_window(model.metadata.capabilities.context_tokens);

        // Memory is per-Persona (FR-56): a conversation with no persona has no
        // memory scope. The query is the latest user turn.
        let memory: Vec<MemoryItem> = match &persona_id {
            Some(pid) => {
                let scope = MemoryScope::Persona(pid.clone());
                let query = last_user_text(&history).unwrap_or_default();
                self.memory
                    .retrieve(&scope, &query)
                    .await
                    .unwrap_or_else(|err| {
                        err.log("memory retrieval");
                        Vec::new()
                    })
                    .into_iter()
                    .map(|r| MemoryItem {
                        text: r.memory.content,
                    })
                    .collect()
            }
            None => Vec::new(),
        };

        Ok(self.builder.build(&BuildInput {
            system: if persona.is_some() {
                crate::context::builder::PERSONA_SYSTEM
            } else {
                crate::context::builder::DEFAULT_SYSTEM
            },
            persona: persona.as_ref(),
            character: None,
            memory: &memory,
            history: &history,
            runtime: RuntimeContext {
                now: RuntimeContext::now().now,
                spoken,
            },
            budget,
        }))
    }

    /// Fire a background memory-extraction pass over the conversation's latest
    /// exchange, if it has a Persona scope. Off the response path.
    async fn maybe_extract_memory(&self, conversation_id: &ConversationId, model_id: &ModelId) {
        let Ok(Some(pid)) = self.repo.persona_id(conversation_id).await else {
            return;
        };
        let Ok(history) = self.repo.messages(conversation_id).await else {
            return;
        };
        let (Some(user_text), Some(assistant)) = (
            last_user_text(&history),
            history.iter().rev().find(|m| m.role == Role::Assistant),
        ) else {
            return;
        };
        let MessageContent::Text {
            text: assistant_text,
        } = &assistant.content
        else {
            return;
        };
        self.memory.spawn_extraction(
            model_id.clone(),
            &MemoryScope::Persona(pid),
            crate::memory::Exchange {
                conversation_id: conversation_id.clone(),
                assistant_message_id: Some(assistant.id.to_string()),
                user_text,
                assistant_text: assistant_text.clone(),
            },
        );
    }

    /// Resolve the loaded instance, render the prompt, stream, accumulate.
    async fn stream_once<F>(
        &self,
        conversation_id: &ConversationId,
        model_id: &ModelId,
        cancel: &CancellationToken,
        spoken: bool,
        sink: &F,
    ) -> AppResult<(String, GenerationMeta)>
    where
        F: Fn(GenerationEvent) + Send + Sync,
    {
        let _busy = self.lifecycle.begin_use(model_id).await?;
        let instance = self.lifecycle.instance(model_id).await.ok_or_else(|| {
            AppError::BackendUnavailable(format!("model {model_id} is not loaded"))
        })?;
        let llm = instance
            .as_llm()
            .ok_or_else(|| AppError::Validation(format!("model {model_id} is not an LLM")))?;

        // `build_prompt` -> `ContextBuilder::build` logs the provenance at
        // `target: "context"`.
        let built = self.build_prompt(conversation_id, model_id, spoken).await?;
        tracing::debug!(
            target: "context",
            conversation = %conversation_id,
            total_tokens = built.provenance.total_tokens,
            "prompt assembled for generation"
        );
        let rendered = built.text;
        let params = SamplingParams {
            temperature: Some(0.7),
            top_p: Some(0.95),
            top_k: Some(40),
            max_tokens: Some(if spoken { 256 } else { MAX_TOKENS }),
            stop: vec!["<|im_end|>".to_owned()],
            seed: None,
        };

        let (tx, mut rx) = mpsc::channel::<GenerationEvent>(256);
        let started = Instant::now();
        let stream_fut = llm.stream(rendered, params, tx, cancel.clone());

        let mut text = String::new();
        let mut stop_reason = StopReason::Error;
        let mut tokens = 0u32;
        // Drain events to `sink` until the terminal frame. We stop on the
        // terminal (not on channel close) so `join!` can drop the completed
        // `stream_fut` — and its `tx` — without a stall.
        let recv_fut = async {
            while let Some(ev) = rx.recv().await {
                let terminal = match &ev {
                    GenerationEvent::TokenDelta { text: delta, .. } => {
                        text.push_str(delta);
                        false
                    }
                    GenerationEvent::Done {
                        stop_reason: sr,
                        tokens: n,
                    } => {
                        stop_reason = *sr;
                        tokens = *n;
                        true
                    }
                    GenerationEvent::Cancelled => {
                        stop_reason = StopReason::Cancelled;
                        true
                    }
                    GenerationEvent::Error { .. } => {
                        stop_reason = StopReason::Error;
                        true
                    }
                };
                sink(ev);
                if terminal {
                    break;
                }
            }
        };
        tokio::join!(stream_fut, recv_fut);

        let meta = GenerationMeta {
            model: model_id.clone(),
            stop_reason,
            tokens,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        };
        Ok((text, meta))
    }
}

/// The most recent user turn's text (typed message or a transcribed voice
/// turn) — the memory query + the extraction exchange.
fn last_user_text(history: &[Message]) -> Option<String> {
    history.iter().rev().find_map(|m| {
        if m.role != Role::User {
            return None;
        }
        match &m.content {
            MessageContent::Text { text } => Some(text.clone()),
            MessageContent::Audio {
                transcript: Some(t),
                ..
            } => Some(t.clone()),
            _ => None,
        }
    })
}
