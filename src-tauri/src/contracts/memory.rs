//! Memory contracts (Phase 21, ADR-0012). The view shape for "what is
//! remembered" (FR-52) crossing the typed IPC boundary.
//!
//! Memory *content* is untrusted data — it originated from model output and
//! user text. The context builder sanitises it before it enters a prompt
//! (`context::sanitize`, SECURITY C2). These types only carry it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::contracts::ids::{ConversationId, MemoryId};

/// What kind of thing a memory records. Kept small and fixed for v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum MemoryKind {
    /// A durable fact ("works as a nurse", "has two cats").
    Fact,
    /// A stated preference ("prefers tea", "dislikes horror films").
    Preference,
    /// Something that happened ("moved to Berlin in March").
    Event,
    /// A personality / disposition note ("gets anxious about deadlines").
    Trait,
}

/// One stored memory, as shown in the "view memories" list (FR-52).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Memory {
    /// Stable id — pass to `memory_delete`.
    pub id: MemoryId,
    /// Category.
    pub kind: MemoryKind,
    /// The remembered text.
    pub content: String,
    /// 1..5 — higher is kept longer and ranked ahead on a tie.
    pub importance: u32,
    /// The conversation this was extracted from, if it still exists.
    pub source_conversation_id: Option<ConversationId>,
    /// RFC-3339 creation timestamp.
    pub created_at: String,
}
