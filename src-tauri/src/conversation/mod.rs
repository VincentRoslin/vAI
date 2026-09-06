//! Conversation service — the **thin** orchestration for the Phase 16 vertical
//! slice: persist a user message, run one generation against the loaded model,
//! stream the tokens out, persist the assistant message.
//!
//! This is **not** the shared conversation engine (Phase 17 generalizes it).
//! It owns conversation/message state (via [`repo`]); the lifecycle manager owns
//! the model.
//!
//! Concurrency (v1): **one generation at a time** — a second `send` while one is
//! running returns [`AppError::Conflict`]. A queue is Phase 24.

pub mod prompt;
pub mod repo;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod live_tests;

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::contracts::conversation::{
    Conversation, ConversationKind, GenerationMeta, Message, MessageContent, Role,
};
use crate::contracts::generation::{GenerationEvent, SamplingParams, StopReason};
use crate::contracts::ids::{ConversationId, ModelId, TaskId};
use crate::db::Db;
use crate::ipc::{AppError, AppResult};
use crate::lifecycle::LifecycleManager;
use repo::ConversationRepo;

/// Hard cap on tokens per turn for the slice (Phase 20 makes it configurable).
const MAX_TOKENS: u32 = 1024;

/// The single in-flight generation, if any.
struct InFlight {
    task_id: TaskId,
    cancel: CancellationToken,
}

/// Conversation + generation orchestration. Held in managed state as
/// `Arc<ConversationService>`.
pub struct ConversationService {
    repo: ConversationRepo,
    lifecycle: Arc<LifecycleManager>,
    in_flight: Mutex<Option<InFlight>>,
}

impl ConversationService {
    #[must_use]
    pub fn new(db: Arc<Db>, lifecycle: Arc<LifecycleManager>) -> Self {
        Self {
            repo: ConversationRepo::new(db),
            lifecycle,
            in_flight: Mutex::new(None),
        }
    }

    /// Create a new (Persona) conversation.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn create(&self) -> AppResult<Conversation> {
        self.repo.create(ConversationKind::Persona).await
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

    /// Whether a generation is currently running.
    pub async fn is_generating(&self) -> bool {
        self.in_flight.lock().await.is_some()
    }

    /// Send `text` as a user message and start generating a reply. The user
    /// message is persisted before this returns; `sink` receives each
    /// [`GenerationEvent`] as the reply streams; the assistant message is
    /// persisted when the stream terminates.
    ///
    /// # Errors
    /// - [`AppError::Conflict`] — a generation is already running.
    /// - [`AppError::Validation`] — empty `text`.
    /// - [`AppError::NotFound`] — no such conversation.
    /// - a persistence error.
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
        if text.trim().is_empty() {
            return Err(AppError::Validation("message text is empty".to_owned()));
        }
        let mut guard = self.in_flight.lock().await;
        if guard.is_some() {
            return Err(AppError::Conflict(
                "a generation is already running".to_owned(),
            ));
        }

        // Persist the user message first — it is durable even if generation fails.
        self.repo
            .append(
                &conversation_id,
                Role::User,
                MessageContent::Text { text },
                None,
            )
            .await?;

        let task_id = TaskId::from_trusted(uuid::Uuid::new_v4().hyphenated().to_string());
        let cancel = CancellationToken::new();
        *guard = Some(InFlight {
            task_id: task_id.clone(),
            cancel: cancel.clone(),
        });
        drop(guard);

        let this = Arc::clone(self);
        let tid = task_id.clone();
        tokio::spawn(async move {
            this.run_generation(conversation_id, model_id, tid, cancel, sink)
                .await;
        });

        Ok(task_id)
    }

    /// Cancel the in-flight generation identified by `task_id`.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if no generation with that id is running.
    pub async fn cancel(&self, task_id: &TaskId) -> AppResult<()> {
        let guard = self.in_flight.lock().await;
        match guard.as_ref() {
            Some(f) if &f.task_id == task_id => {
                f.cancel.cancel();
                Ok(())
            }
            _ => Err(AppError::NotFound(format!("generation {task_id}"))),
        }
    }

    /// Cancel any in-flight generation (called on app exit).
    pub async fn shutdown(&self) {
        if let Some(f) = self.in_flight.lock().await.as_ref() {
            f.cancel.cancel();
        }
    }

    async fn run_generation<F>(
        self: Arc<Self>,
        conversation_id: ConversationId,
        model_id: ModelId,
        task_id: TaskId,
        cancel: CancellationToken,
        sink: F,
    ) where
        F: Fn(GenerationEvent) + Send + Sync + 'static,
    {
        let op = crate::logging::operation(Some(&task_id), "chat_generation");
        let status = match self
            .generate(&conversation_id, &model_id, &cancel, &sink)
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
                if let Err(err) = self
                    .repo
                    .append(
                        &conversation_id,
                        Role::Assistant,
                        MessageContent::Text { text },
                        Some(meta),
                    )
                    .await
                {
                    err.log("persist assistant message");
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

        *self.in_flight.lock().await = None;
        op.finish(status);
    }

    /// The generation proper: resolve the loaded instance, render the prompt,
    /// stream, accumulate. Returns the accumulated text + its `GenerationMeta`.
    async fn generate<F>(
        &self,
        conversation_id: &ConversationId,
        model_id: &ModelId,
        cancel: &CancellationToken,
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

        let history = self.repo.messages(conversation_id).await?;
        let rendered = prompt::render_chatml(&history, prompt::DEFAULT_SYSTEM);
        let params = SamplingParams {
            temperature: Some(0.7),
            top_p: Some(0.95),
            top_k: Some(40),
            max_tokens: Some(MAX_TOKENS),
            stop: vec!["<|im_end|>".to_owned()],
            seed: None,
        };

        let (tx, mut rx) = mpsc::channel::<GenerationEvent>(256);
        let started = Instant::now();
        let stream_fut = llm.stream(rendered, params, tx, cancel.clone());

        let mut text = String::new();
        let mut stop_reason = StopReason::Error;
        let mut tokens = 0u32;
        // Drain events, forwarding each to `sink`, until a terminal frame. We
        // stop on the terminal (not on channel close) so `join!` can drop the
        // completed `stream_fut` — and its `tx` — without a stall.
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
