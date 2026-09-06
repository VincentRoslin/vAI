//! Memory (Phase 21, ADR-0012, `AI_PIPELINES.md` §9).
//!
//! `turn completes → (async, off the response path) schema-constrained LLM
//! extraction → candidates → importance + dedup + size gate → store (memory +
//! memory_fts) with provenance`. Retrieval: a BM25 keyword query from the
//! current turn, scope-filtered, top-k → the one context builder's memory slot
//! (Phase 20), token-budgeted.
//!
//! **Scope is per-Persona for Tab 1** (FR-56); Phase 26 adds `character:<id>`.
//! Retrieved memory text is **untrusted** — the context builder sanitises it.

pub mod extract;
pub mod repo;
pub mod retrieve;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::contracts::ids::{MemoryId, ModelId, PersonaId};
use crate::contracts::memory::Memory;
use crate::db::Db;
use crate::ipc::AppResult;
use crate::lifecycle::LifecycleManager;

pub use extract::Exchange;
pub use repo::{MemoryRepo, NewMemory, Scored};

/// 1..=5 importance; a candidate below this is not stored.
pub const MIN_IMPORTANCE: u32 = 3;
/// Max memories pulled into context per turn.
pub const RETRIEVE_K: u32 = 8;
/// Per-scope row cap — matches the ADR-0012 embeddings-trigger corpus size.
/// When a scope is full, the lowest-priority row is dropped on the next insert.
pub const PER_SCOPE_CAP: u32 = 500;
/// Max stored memory length (characters, after sanitising).
pub const MAX_CONTENT_CHARS: usize = 500;
/// Minimum stored memory length.
pub const MIN_CONTENT_CHARS: usize = 3;
/// A candidate whose word set overlaps an existing memory in the same scope by
/// at least this Jaccard ratio is treated as a duplicate and dropped. (Raw
/// BM25 magnitude is corpus-dependent; a set overlap is stable and readable.)
pub const DEDUP_JACCARD: f64 = 0.6;

/// Which memory store a turn reads from / writes to. Per-Persona for Tab 1
/// (FR-56); `Character` lands in Phase 26.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryScope {
    /// A Tab 1 Persona's memory.
    Persona(PersonaId),
}

impl MemoryScope {
    /// The `scope` column value — `"persona:<id>"`.
    #[must_use]
    pub fn as_key(&self) -> String {
        match self {
            Self::Persona(id) => format!("persona:{id}"),
        }
    }
}

/// One memory chosen for the current turn's context, with its rank.
#[derive(Debug, Clone)]
pub struct RetrievedMemory {
    /// The row.
    pub memory: Memory,
    /// `bm25()` — more negative is a better match.
    pub rank: f64,
}

/// The memory orchestrator. Held by the [`crate::conversation::ConversationEngine`]
/// (constructed internally, like the persona repo) and reachable for IPC via
/// `engine.memory()`.
pub struct MemoryService {
    repo: MemoryRepo,
    lifecycle: Arc<LifecycleManager>,
    /// One extraction at a time; a request while one runs is dropped.
    extract_permit: Arc<Semaphore>,
    /// Cancels an in-flight extraction on shutdown.
    cancel: CancellationToken,
}

impl MemoryService {
    #[must_use]
    pub fn new(db: Arc<Db>, lifecycle: Arc<LifecycleManager>) -> Self {
        Self {
            repo: MemoryRepo::new(db),
            lifecycle,
            extract_permit: Arc::new(Semaphore::new(1)),
            cancel: CancellationToken::new(),
        }
    }

    /// The underlying repository.
    #[must_use]
    pub fn repo(&self) -> &MemoryRepo {
        &self.repo
    }

    /// Every memory in a scope, newest first (FR-52).
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list(&self, scope: &MemoryScope) -> AppResult<Vec<Memory>> {
        self.repo.list(&scope.as_key()).await
    }

    /// Delete a memory (FR-53).
    ///
    /// # Errors
    /// [`crate::ipc::AppError::NotFound`] if unknown; a persistence error.
    pub async fn delete(&self, id: &MemoryId) -> AppResult<()> {
        self.repo.delete(id).await
    }

    /// Retrieve up to [`RETRIEVE_K`] memories relevant to `turn`, best match
    /// first. A **pure read** — safe on the response path. Logs a recall-gap
    /// sample (`target: "memory"`) when the scope is non-empty but nothing
    /// matched, to inform the deferred embeddings decision (ADR-0012).
    ///
    /// # Errors
    /// A persistence error.
    pub async fn retrieve(
        &self,
        scope: &MemoryScope,
        turn: &str,
    ) -> AppResult<Vec<RetrievedMemory>> {
        let key = scope.as_key();
        let Some(query) = retrieve::fts_query_for(turn) else {
            return Ok(Vec::new());
        };
        let hits = self.repo.search(&key, &query, RETRIEVE_K).await?;
        if hits.is_empty() && self.repo.count(&key).await? > 0 {
            tracing::info!(
                target: "memory",
                scope = %key,
                query = %query,
                "recall gap — scope non-empty but FTS5 matched nothing"
            );
        }
        Ok(hits
            .into_iter()
            .map(|s| RetrievedMemory {
                memory: s.memory,
                rank: s.rank,
            })
            .collect())
    }

    /// Kick off an extraction pass over `exchange` in the background, using the
    /// model that produced the reply. Returns immediately. At most one runs at a
    /// time; a call while one is in flight is dropped (the next turn re-covers
    /// recent context).
    pub fn spawn_extraction(&self, model_id: ModelId, scope: &MemoryScope, exchange: Exchange) {
        let Ok(permit) = Arc::clone(&self.extract_permit).try_acquire_owned() else {
            tracing::debug!(target: "memory", "extraction skipped — one already running");
            return;
        };
        let lifecycle = Arc::clone(&self.lifecycle);
        let repo = self.repo.clone();
        let cancel = self.cancel.clone();
        let key = scope.as_key();
        tokio::spawn(async move {
            let _permit = permit; // held for the duration
            let stored =
                run_extraction_task(&lifecycle, &repo, &model_id, &key, &exchange, cancel).await;
            tracing::info!(target: "memory", scope = %key, stored, "extraction pass complete");
        });
    }

    /// Cancel an in-flight extraction (called on app exit).
    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

async fn run_extraction_task(
    lifecycle: &Arc<LifecycleManager>,
    repo: &MemoryRepo,
    model_id: &ModelId,
    scope_key: &str,
    exchange: &Exchange,
    cancel: CancellationToken,
) -> usize {
    let _busy = match lifecycle.begin_use(model_id).await {
        Ok(guard) => guard,
        Err(err) => {
            tracing::debug!(target: "memory", %err, "extraction skipped — model unavailable");
            return 0;
        }
    };
    let Some(instance) = lifecycle.instance(model_id).await else {
        tracing::debug!(target: "memory", "extraction skipped — model not loaded");
        return 0;
    };
    let Some(llm) = instance.as_llm() else {
        tracing::debug!(target: "memory", "extraction skipped — model is not an LLM");
        return 0;
    };

    let candidates = extract::run_extraction(llm, exchange, cancel.clone()).await;
    let mut stored = 0usize;
    for candidate in &candidates {
        if cancel.is_cancelled() {
            break;
        }
        if let Some(row) = extract::validate(candidate, repo, scope_key, exchange).await {
            match repo.insert(row, PER_SCOPE_CAP).await {
                Ok(_) => stored += 1,
                Err(err) => tracing::warn!(target: "memory", %err, "failed to store a memory"),
            }
        }
    }
    stored
}
