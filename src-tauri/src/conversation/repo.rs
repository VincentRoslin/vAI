//! All SQL for conversations + messages (V0004). Rust owns persistence
//! (`CLAUDE.md` Article I, ADR-0009); nothing else touches these tables.

use std::sync::Arc;

use rusqlite::Row;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::contracts::conversation::{
    Conversation, ConversationKind, GenerationMeta, Message, MessageContent, Role,
};
use crate::contracts::generation::StopReason;
use crate::contracts::ids::{ConversationId, MessageId};
use crate::db::Db;
use crate::ipc::{AppError, AppResult};

/// Conversation + message storage.
pub struct ConversationRepo {
    db: Arc<Db>,
}

impl ConversationRepo {
    #[must_use]
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Create an empty conversation.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn create(&self, kind: ConversationKind) -> AppResult<Conversation> {
        let now = now_rfc3339();
        let convo = Conversation {
            id: new_conversation_id(),
            kind,
            title: None,
            created_at: now.clone(),
            updated_at: now,
        };
        let row = convo.clone();
        self.db
            .write(move |tx| {
                tx.prepare_cached(
                    "INSERT INTO conversation (id, kind, title, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )?
                .execute(rusqlite::params![
                    row.id.as_str(),
                    kind_str(row.kind),
                    row.title,
                    row.created_at,
                    row.updated_at,
                ])?;
                Ok(())
            })
            .await?;
        Ok(convo)
    }

    /// Every conversation, most-recently-active first.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list(&self) -> AppResult<Vec<Conversation>> {
        let raws: Vec<RawConvo> = self
            .db
            .read(|conn| {
                Ok(conn
                    .prepare_cached(
                        "SELECT id, kind, title, created_at, updated_at \
                         FROM conversation ORDER BY updated_at DESC, id DESC",
                    )?
                    .query_map([], RawConvo::from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await?;
        raws.into_iter().map(Conversation::try_from).collect()
    }

    /// The most-recently-active conversation, if any.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn latest(&self) -> AppResult<Option<Conversation>> {
        Ok(self.list().await?.into_iter().next())
    }

    /// One conversation header.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if `id` is unknown; a persistence error otherwise.
    pub async fn get(&self, id: &ConversationId) -> AppResult<Conversation> {
        let key = id.to_string();
        let raw: Option<RawConvo> = self
            .db
            .read(move |conn| {
                Ok(conn
                    .prepare_cached(
                        "SELECT id, kind, title, created_at, updated_at \
                         FROM conversation WHERE id = ?1",
                    )?
                    .query_map([key], RawConvo::from_row)?
                    .next()
                    .transpose()?)
            })
            .await?;
        raw.ok_or_else(|| AppError::NotFound(format!("conversation {id}")))
            .and_then(Conversation::try_from)
    }

    /// Every message in a conversation, in insertion order.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn messages(&self, id: &ConversationId) -> AppResult<Vec<Message>> {
        let key = id.to_string();
        let raws: Vec<RawMsg> = self
            .db
            .read(move |conn| {
                Ok(conn
                    .prepare_cached(
                        "SELECT id, conversation_id, role, content, created_at, \
                                gen_model, gen_stop_reason, gen_tokens, gen_duration_ms \
                         FROM message WHERE conversation_id = ?1 ORDER BY rowid",
                    )?
                    .query_map([key], RawMsg::from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await?;
        raws.into_iter().map(Message::try_from).collect()
    }

    /// Append a message and bump the conversation's `updated_at`.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if the conversation does not exist; a persistence
    /// error otherwise.
    pub async fn append(
        &self,
        conversation_id: &ConversationId,
        role: Role,
        content: MessageContent,
        generation: Option<GenerationMeta>,
    ) -> AppResult<Message> {
        let now = now_rfc3339();
        let msg = Message {
            id: new_message_id(),
            conversation_id: conversation_id.clone(),
            role,
            content,
            created_at: now.clone(),
            generation,
        };
        let content_json = serde_json::to_string(&msg.content)
            .map_err(|e| AppError::internal("serialize message content", e))?;
        let (gen_model, gen_stop, gen_tokens, gen_ms) = match &msg.generation {
            Some(g) => (
                Some(g.model.to_string()),
                Some(stop_reason_str(g.stop_reason).to_owned()),
                Some(i64::from(g.tokens)),
                Some(i64::try_from(g.duration_ms).unwrap_or(i64::MAX)),
            ),
            None => (None, None, None, None),
        };
        let row = msg.clone();
        let cid = conversation_id.to_string();
        let updated = now;
        let write = self
            .db
            .write(move |tx| {
                let affected = tx
                    .prepare_cached("UPDATE conversation SET updated_at = ?1 WHERE id = ?2")?
                    .execute(rusqlite::params![updated, cid])?;
                if affected == 0 {
                    // Signals "no such conversation"; surfaced as NotFound below.
                    return Err(crate::db::DbError::NotFound);
                }
                tx.prepare_cached(
                    "INSERT INTO message \
                       (id, conversation_id, role, content, created_at, \
                        gen_model, gen_stop_reason, gen_tokens, gen_duration_ms) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                )?
                .execute(rusqlite::params![
                    row.id.as_str(),
                    cid,
                    role_str(row.role),
                    content_json,
                    row.created_at,
                    gen_model,
                    gen_stop,
                    gen_tokens,
                    gen_ms,
                ])?;
                Ok(())
            })
            .await;
        match write {
            Ok(()) => Ok(msg),
            Err(crate::db::DbError::NotFound) => Err(AppError::NotFound(format!(
                "conversation {conversation_id}"
            ))),
            Err(e) => Err(e.into()),
        }
    }
}

// ---------------------------------------------------------------- row mapping

struct RawConvo {
    id: String,
    kind: String,
    title: Option<String>,
    created_at: String,
    updated_at: String,
}

impl RawConvo {
    fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            kind: r.get(1)?,
            title: r.get(2)?,
            created_at: r.get(3)?,
            updated_at: r.get(4)?,
        })
    }
}

impl TryFrom<RawConvo> for Conversation {
    type Error = AppError;
    fn try_from(r: RawConvo) -> Result<Self, Self::Error> {
        Ok(Self {
            id: ConversationId::from_trusted(r.id),
            kind: kind_from_str(&r.kind)?,
            title: r.title,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

struct RawMsg {
    id: String,
    conversation_id: String,
    role: String,
    content: String,
    created_at: String,
    gen_model: Option<String>,
    gen_stop_reason: Option<String>,
    gen_tokens: Option<i64>,
    gen_duration_ms: Option<i64>,
}

impl RawMsg {
    fn from_row(r: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            conversation_id: r.get(1)?,
            role: r.get(2)?,
            content: r.get(3)?,
            created_at: r.get(4)?,
            gen_model: r.get(5)?,
            gen_stop_reason: r.get(6)?,
            gen_tokens: r.get(7)?,
            gen_duration_ms: r.get(8)?,
        })
    }
}

impl TryFrom<RawMsg> for Message {
    type Error = AppError;
    fn try_from(r: RawMsg) -> Result<Self, Self::Error> {
        let generation = match (r.gen_model, r.gen_stop_reason) {
            (Some(model), Some(stop)) => Some(GenerationMeta {
                model: crate::contracts::ids::ModelId::from_trusted(model),
                stop_reason: stop_reason_from_str(&stop)?,
                tokens: u32::try_from(r.gen_tokens.unwrap_or(0)).unwrap_or(0),
                duration_ms: u64::try_from(r.gen_duration_ms.unwrap_or(0)).unwrap_or(0),
            }),
            _ => None,
        };
        Ok(Self {
            id: MessageId::from_trusted(r.id),
            conversation_id: ConversationId::from_trusted(r.conversation_id),
            role: role_from_str(&r.role)?,
            content: serde_json::from_str(&r.content)
                .map_err(|e| AppError::internal("deserialize message content", e))?,
            created_at: r.created_at,
            generation,
        })
    }
}

// ---------------------------------------------------------------- enum <-> text

fn kind_str(k: ConversationKind) -> &'static str {
    match k {
        ConversationKind::Persona => "Persona",
        ConversationKind::Character => "Character",
    }
}
fn kind_from_str(s: &str) -> AppResult<ConversationKind> {
    match s {
        "Persona" => Ok(ConversationKind::Persona),
        "Character" => Ok(ConversationKind::Character),
        other => Err(AppError::internal(
            "conversation kind",
            format!("unknown {other:?}"),
        )),
    }
}
fn role_str(r: Role) -> &'static str {
    match r {
        Role::System => "System",
        Role::User => "User",
        Role::Assistant => "Assistant",
    }
}
fn role_from_str(s: &str) -> AppResult<Role> {
    match s {
        "System" => Ok(Role::System),
        "User" => Ok(Role::User),
        "Assistant" => Ok(Role::Assistant),
        other => Err(AppError::internal(
            "message role",
            format!("unknown {other:?}"),
        )),
    }
}
fn stop_reason_str(s: StopReason) -> &'static str {
    match s {
        StopReason::EndOfText => "EndOfText",
        StopReason::MaxTokens => "MaxTokens",
        StopReason::StopSequence => "StopSequence",
        StopReason::Cancelled => "Cancelled",
        StopReason::Error => "Error",
    }
}
fn stop_reason_from_str(s: &str) -> AppResult<StopReason> {
    match s {
        "EndOfText" => Ok(StopReason::EndOfText),
        "MaxTokens" => Ok(StopReason::MaxTokens),
        "StopSequence" => Ok(StopReason::StopSequence),
        "Cancelled" => Ok(StopReason::Cancelled),
        "Error" => Ok(StopReason::Error),
        other => Err(AppError::internal(
            "stop reason",
            format!("unknown {other:?}"),
        )),
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

fn new_conversation_id() -> ConversationId {
    ConversationId::from_trusted(uuid::Uuid::new_v4().hyphenated().to_string())
}
fn new_message_id() -> MessageId {
    MessageId::from_trusted(uuid::Uuid::new_v4().hyphenated().to_string())
}
