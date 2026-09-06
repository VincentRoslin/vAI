//! All SQL for `memory` + `memory_fts` (`V0006`). Rust owns persistence
//! (`CLAUDE.md` Article I, ADR-0009, ADR-0012); nothing else touches these
//! tables.
//!
//! `memory_fts` is a **standalone** FTS5 table (content stored, no external
//! content, no sync triggers) — every write here updates both tables in the
//! same transaction.

use std::sync::Arc;

use rusqlite::Row;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::contracts::ids::{ConversationId, MemoryId};
use crate::contracts::memory::{Memory, MemoryKind};
use crate::db::Db;
use crate::ipc::{AppError, AppResult};

/// A validated memory ready to store. The caller has already sanitised
/// `content` and checked importance / length / dedup.
#[derive(Debug, Clone)]
pub struct NewMemory {
    /// Scope key — `"persona:<id>"` (later `"character:<id>"`).
    pub scope: String,
    /// Category.
    pub kind: MemoryKind,
    /// The remembered text (sanitised).
    pub content: String,
    /// 1..=5.
    pub importance: u32,
    /// Provenance — the conversation it came from.
    pub source_conversation_id: Option<ConversationId>,
    /// Provenance — the message it came from (no FK; messages may be pruned).
    pub source_message_id: Option<String>,
}

/// A memory plus its BM25 rank for a query (lower = better).
#[derive(Debug, Clone)]
pub struct Scored {
    /// The row.
    pub memory: Memory,
    /// `bm25()` — negative; more negative is a better match.
    pub rank: f64,
}

/// All `memory` + `memory_fts` SQL.
#[derive(Clone)]
pub struct MemoryRepo {
    db: Arc<Db>,
}

impl MemoryRepo {
    #[must_use]
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Store a memory. If `scope` is already at `cap`, the single
    /// lowest-priority row (importance, then oldest) is deleted first.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn insert(&self, mem: NewMemory, cap: u32) -> AppResult<MemoryId> {
        let id = MemoryId::from_trusted(Uuid::new_v4().hyphenated().to_string());
        let now = now_rfc3339();
        let key = id.to_string();
        let row = mem.clone();
        let idc = key.clone();
        self.db
            .write(move |tx| {
                let count: u32 = tx
                    .prepare_cached("SELECT count(*) FROM memory WHERE scope = ?1")?
                    .query_row([&row.scope], |r| r.get(0))?;
                if count >= cap {
                    let victim: Option<String> = tx
                        .prepare_cached(
                            "SELECT id FROM memory WHERE scope = ?1 \
                             ORDER BY importance ASC, created_at ASC, id ASC LIMIT 1",
                        )?
                        .query_row([&row.scope], |r| r.get(0))
                        .ok();
                    if let Some(v) = victim {
                        tx.prepare_cached("DELETE FROM memory WHERE id = ?1")?
                            .execute([&v])?;
                        tx.prepare_cached("DELETE FROM memory_fts WHERE mem_id = ?1")?
                            .execute([&v])?;
                    }
                }
                tx.prepare_cached(
                    "INSERT INTO memory \
                     (id, scope, kind, content, importance, \
                      source_conversation_id, source_message_id, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )?
                .execute(rusqlite::params![
                    idc,
                    row.scope,
                    kind_str(row.kind),
                    row.content,
                    row.importance,
                    row.source_conversation_id.map(|c| c.to_string()),
                    row.source_message_id,
                    now,
                ])?;
                tx.prepare_cached(
                    "INSERT INTO memory_fts (mem_id, scope, content) VALUES (?1, ?2, ?3)",
                )?
                .execute(rusqlite::params![idc, row.scope, row.content])?;
                Ok(())
            })
            .await?;
        Ok(id)
    }

    /// BM25 keyword search within one scope. `fts_query` is a ready FTS5 MATCH
    /// string (see `retrieve::fts_query_for`). Best matches first.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn search(&self, scope: &str, fts_query: &str, k: u32) -> AppResult<Vec<Scored>> {
        if fts_query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let (scope, q) = (scope.to_owned(), fts_query.to_owned());
        let rows: Vec<ScoredRaw> = self
            .db
            .read(move |conn| {
                Ok(conn
                    .prepare_cached(
                        "SELECT m.id, m.kind, m.content, m.importance, \
                                m.source_conversation_id, m.created_at, bm25(memory_fts) AS rank \
                         FROM memory_fts \
                         JOIN memory m ON m.id = memory_fts.mem_id \
                         WHERE memory_fts MATCH ?1 AND memory_fts.scope = ?2 \
                         ORDER BY rank LIMIT ?3",
                    )?
                    .query_map(rusqlite::params![q, scope, k], scored_from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await?;
        rows.into_iter().map(Scored::try_convert).collect()
    }

    /// Every memory in a scope, newest first.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list(&self, scope: &str) -> AppResult<Vec<Memory>> {
        let scope = scope.to_owned();
        let raws: Vec<RawMemory> = self
            .db
            .read(move |conn| {
                Ok(conn
                    .prepare_cached(
                        "SELECT id, kind, content, importance, source_conversation_id, created_at \
                         FROM memory WHERE scope = ?1 ORDER BY created_at DESC, id DESC",
                    )?
                    .query_map([scope], RawMemory::from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await?;
        raws.into_iter().map(Memory::try_from).collect()
    }

    /// One memory.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if `id` is unknown; a persistence error otherwise.
    pub async fn get(&self, id: &MemoryId) -> AppResult<Memory> {
        let key = id.to_string();
        let raw: Option<RawMemory> = self
            .db
            .read(move |conn| {
                Ok(conn
                    .prepare_cached(
                        "SELECT id, kind, content, importance, source_conversation_id, created_at \
                         FROM memory WHERE id = ?1",
                    )?
                    .query_map([key], RawMemory::from_row)?
                    .next()
                    .transpose()?)
            })
            .await?;
        raw.ok_or_else(|| AppError::NotFound(format!("memory {id}")))
            .and_then(Memory::try_from)
    }

    /// Delete a memory from both tables.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if `id` is unknown; a persistence error otherwise.
    pub async fn delete(&self, id: &MemoryId) -> AppResult<()> {
        let key = id.to_string();
        let write = self
            .db
            .write(move |tx| {
                let n = tx
                    .prepare_cached("DELETE FROM memory WHERE id = ?1")?
                    .execute([&key])?;
                tx.prepare_cached("DELETE FROM memory_fts WHERE mem_id = ?1")?
                    .execute([&key])?;
                if n == 0 {
                    return Err(crate::db::DbError::NotFound);
                }
                Ok(())
            })
            .await;
        match write {
            Ok(()) => Ok(()),
            Err(crate::db::DbError::NotFound) => Err(AppError::NotFound(format!("memory {id}"))),
            Err(e) => Err(e.into()),
        }
    }

    /// How many memories a scope holds.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn count(&self, scope: &str) -> AppResult<u32> {
        let scope = scope.to_owned();
        Ok(self
            .db
            .read(move |conn| {
                Ok(conn
                    .prepare_cached("SELECT count(*) FROM memory WHERE scope = ?1")?
                    .query_row([scope], |r| r.get::<_, u32>(0))?)
            })
            .await?)
    }
}

// ---------------------------------------------------------------- row mapping

struct RawMemory {
    id: String,
    kind: String,
    content: String,
    importance: u32,
    source_conversation_id: Option<String>,
    created_at: String,
}

impl RawMemory {
    fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            kind: r.get(1)?,
            content: r.get(2)?,
            importance: r.get(3)?,
            source_conversation_id: r.get(4)?,
            created_at: r.get(5)?,
        })
    }
}

impl TryFrom<RawMemory> for Memory {
    type Error = AppError;

    fn try_from(r: RawMemory) -> Result<Self, Self::Error> {
        Ok(Self {
            id: MemoryId::from_trusted(r.id),
            kind: kind_from_str(&r.kind)?,
            content: r.content,
            importance: r.importance,
            source_conversation_id: r.source_conversation_id.map(ConversationId::from_trusted),
            created_at: r.created_at,
        })
    }
}

/// `search`'s row carries an extra `rank` column, so it has its own reader.
struct ScoredRaw {
    raw: RawMemory,
    rank: f64,
}

fn scored_from_row(r: &Row<'_>) -> rusqlite::Result<ScoredRaw> {
    Ok(ScoredRaw {
        raw: RawMemory {
            id: r.get(0)?,
            kind: r.get(1)?,
            content: r.get(2)?,
            importance: r.get(3)?,
            source_conversation_id: r.get(4)?,
            created_at: r.get(5)?,
        },
        rank: r.get(6)?,
    })
}

impl Scored {
    fn try_convert(s: ScoredRaw) -> AppResult<Self> {
        Ok(Self {
            memory: Memory::try_from(s.raw)?,
            rank: s.rank,
        })
    }
}

// ---------------------------------------------------------------- enum <-> text

fn kind_str(k: MemoryKind) -> &'static str {
    match k {
        MemoryKind::Fact => "Fact",
        MemoryKind::Preference => "Preference",
        MemoryKind::Event => "Event",
        MemoryKind::Trait => "Trait",
    }
}

fn kind_from_str(s: &str) -> AppResult<MemoryKind> {
    match s {
        "Fact" => Ok(MemoryKind::Fact),
        "Preference" => Ok(MemoryKind::Preference),
        "Event" => Ok(MemoryKind::Event),
        "Trait" => Ok(MemoryKind::Trait),
        other => Err(AppError::internal(
            "memory kind",
            format!("unknown {other:?}"),
        )),
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}
