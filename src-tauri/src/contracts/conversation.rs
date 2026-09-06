//! Conversation + message contracts.
//!
//! One shape for both the Persona tab and the Character tab (`ConversationKind`
//! distinguishes them). Timestamps are RFC-3339 strings — the contract carries
//! no date library; the core parses them where it needs to.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::contracts::generation::StopReason;
use crate::contracts::ids::{AssetId, ConversationId, MessageId, ModelId, PersonaId, TaskId};

/// Who authored a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum Role {
    /// The system prompt / instructions.
    System,
    /// The human user.
    User,
    /// The model.
    Assistant,
}

/// The body of a message. Adjacently tagged:
/// `{ "type": "Text", "data": { "text": "..." } }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum MessageContent {
    /// Plain text.
    Text {
        /// The text body.
        text: String,
    },
    /// A voice turn: the stored audio blob plus its transcript once available.
    Audio {
        /// Content-addressed audio blob.
        asset: AssetId,
        /// Transcript text, when transcription has completed.
        transcript: Option<String>,
    },
    /// A generated or referenced image.
    Image {
        /// Content-addressed image blob.
        asset: AssetId,
        /// Optional caption / prompt echo.
        caption: Option<String>,
    },
}

/// Provenance for an assistant message produced by a generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct GenerationMeta {
    /// Model that produced the message.
    pub model: ModelId,
    /// Why generation stopped.
    pub stop_reason: StopReason,
    /// Tokens generated.
    pub tokens: u32,
    /// Wall-clock generation time in milliseconds.
    pub duration_ms: u64,
}

/// One message in a conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Message {
    /// Stable identifier.
    pub id: MessageId,
    /// The conversation this message belongs to.
    pub conversation_id: ConversationId,
    /// Author.
    pub role: Role,
    /// Body.
    pub content: MessageContent,
    /// RFC-3339 creation timestamp.
    pub created_at: String,
    /// Generation provenance, for assistant messages.
    pub generation: Option<GenerationMeta>,
}

/// Which tab / entity a conversation is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ConversationKind {
    /// Chat / Voice tab — bound to a Persona.
    Persona,
    /// Discovery tab — bound to a persistent Character.
    Character,
}

/// The running generation, when the engine has one (Phase 17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct GenerationHandle {
    /// The generation's task id — pass it to `chat_cancel`.
    pub task_id: TaskId,
    /// The conversation the reply is being generated for.
    pub conversation_id: ConversationId,
}

/// The conversation engine's streaming state. `generating` is `Some` while a
/// reply is being produced, `None` at rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct GenerationState {
    /// The running generation, if any.
    pub generating: Option<GenerationHandle>,
}

/// A conversation header (messages are fetched separately).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Conversation {
    /// Stable identifier.
    pub id: ConversationId,
    /// Persona vs Character.
    pub kind: ConversationKind,
    /// Display title; may be absent until the first turn names it.
    pub title: Option<String>,
    /// The bound Persona (Tab 1), if any. `None` = the default assistant.
    /// Fixed once the conversation has a turn (FR-17).
    pub persona_id: Option<PersonaId>,
    /// RFC-3339 creation timestamp.
    pub created_at: String,
    /// RFC-3339 timestamp of the most recent activity.
    pub updated_at: String,
}
