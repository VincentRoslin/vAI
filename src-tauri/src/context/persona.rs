//! Personas — structured behaviour data for Tab 1 conversations (FR-30..35,
//! `V0005`). Rust owns the `persona` table (`CLAUDE.md` Article I, ADR-0009).
//!
//! Persona field values are **untrusted** — [`crate::context::builder`] runs
//! them through [`crate::context::sanitize`] before they enter a prompt.

use std::sync::Arc;

use rusqlite::Row;
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use ts_rs::TS;

use crate::contracts::ids::PersonaId;
use crate::db::Db;
use crate::ipc::{AppError, AppResult};

/// A stored persona.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Persona {
    /// Stable id.
    pub id: PersonaId,
    /// Display name; also used as "You are {name}." in the prompt.
    pub name: String,
    /// One line — who they are.
    pub summary: String,
    /// Personality traits.
    pub personality: String,
    /// Tone of voice.
    pub tone: String,
    /// Communication style.
    pub style: String,
    /// Do / don't lines rendered as "Always: …".
    pub guidance: Vec<String>,
    /// RFC-3339.
    pub created_at: String,
    /// RFC-3339.
    pub updated_at: String,
}

/// The editable fields (create / update).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct PersonaDraft {
    /// Display name (required, non-blank).
    pub name: String,
    /// One line — who they are.
    #[serde(default)]
    pub summary: String,
    /// Personality traits.
    #[serde(default)]
    pub personality: String,
    /// Tone of voice.
    #[serde(default)]
    pub tone: String,
    /// Communication style.
    #[serde(default)]
    pub style: String,
    /// Do / don't lines.
    #[serde(default)]
    pub guidance: Vec<String>,
}

impl PersonaDraft {
    fn validate(&self) -> AppResult<()> {
        if self.name.trim().is_empty() {
            return Err(AppError::Validation(
                "persona name must not be empty".to_owned(),
            ));
        }
        Ok(())
    }
}

/// All `persona` SQL.
pub struct PersonaRepo {
    db: Arc<Db>,
}

impl PersonaRepo {
    #[must_use]
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    /// Every persona, newest first.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list(&self) -> AppResult<Vec<Persona>> {
        let raws: Vec<RawPersona> = self
            .db
            .read(|conn| {
                Ok(conn
                    .prepare_cached(
                        "SELECT id, name, summary, personality, tone, style, guidance, \
                                created_at, updated_at FROM persona ORDER BY created_at DESC",
                    )?
                    .query_map([], RawPersona::from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await?;
        raws.into_iter().map(Persona::try_from).collect()
    }

    /// One persona.
    ///
    /// # Errors
    /// [`AppError::NotFound`] if `id` is unknown; a persistence error otherwise.
    pub async fn get(&self, id: &PersonaId) -> AppResult<Persona> {
        let key = id.to_string();
        let raw: Option<RawPersona> = self
            .db
            .read(move |conn| {
                Ok(conn
                    .prepare_cached(
                        "SELECT id, name, summary, personality, tone, style, guidance, \
                                created_at, updated_at FROM persona WHERE id = ?1",
                    )?
                    .query_map([key], RawPersona::from_row)?
                    .next()
                    .transpose()?)
            })
            .await?;
        raw.ok_or_else(|| AppError::NotFound(format!("persona {id}")))
            .and_then(Persona::try_from)
    }

    /// Create a persona.
    ///
    /// # Errors
    /// [`AppError::Validation`] for a blank name; a persistence error.
    pub async fn create(&self, draft: PersonaDraft) -> AppResult<PersonaId> {
        draft.validate()?;
        let id = PersonaId::from_trusted(uuid::Uuid::new_v4().hyphenated().to_string());
        let now = now_rfc3339();
        let guidance = serde_json::to_string(&draft.guidance)
            .map_err(|e| AppError::internal("serialize persona guidance", e))?;
        let key = id.to_string();
        self.db
            .write(move |tx| {
                tx.prepare_cached(
                    "INSERT INTO persona \
                     (id, name, summary, personality, tone, style, guidance, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                )?
                .execute(rusqlite::params![
                    key,
                    draft.name.trim(),
                    draft.summary,
                    draft.personality,
                    draft.tone,
                    draft.style,
                    guidance,
                    now,
                ])?;
                Ok(())
            })
            .await?;
        Ok(id)
    }

    /// Update a persona in place.
    ///
    /// # Errors
    /// [`AppError::Validation`] for a blank name; [`AppError::NotFound`] if `id`
    /// is unknown; a persistence error.
    pub async fn update(&self, id: &PersonaId, draft: PersonaDraft) -> AppResult<()> {
        draft.validate()?;
        let now = now_rfc3339();
        let guidance = serde_json::to_string(&draft.guidance)
            .map_err(|e| AppError::internal("serialize persona guidance", e))?;
        let key = id.to_string();
        let write = self
            .db
            .write(move |tx| {
                let n = tx
                    .prepare_cached(
                        "UPDATE persona SET name = ?2, summary = ?3, personality = ?4, \
                                tone = ?5, style = ?6, guidance = ?7, updated_at = ?8 \
                         WHERE id = ?1",
                    )?
                    .execute(rusqlite::params![
                        key,
                        draft.name.trim(),
                        draft.summary,
                        draft.personality,
                        draft.tone,
                        draft.style,
                        guidance,
                        now,
                    ])?;
                if n == 0 {
                    return Err(crate::db::DbError::NotFound);
                }
                Ok(())
            })
            .await;
        match write {
            Ok(()) => Ok(()),
            Err(crate::db::DbError::NotFound) => Err(AppError::NotFound(format!("persona {id}"))),
            Err(e) => Err(e.into()),
        }
    }

    /// Delete a persona. Conversations bound to it fall back to the default
    /// assistant (`ON DELETE SET NULL`).
    ///
    /// # Errors
    /// A persistence error.
    pub async fn delete(&self, id: &PersonaId) -> AppResult<()> {
        let key = id.to_string();
        self.db
            .write(move |tx| {
                tx.prepare_cached("DELETE FROM persona WHERE id = ?1")?
                    .execute([key])?;
                Ok(())
            })
            .await
            .map_err(Into::into)
    }
}

// ---------------------------------------------------------------- row mapping

struct RawPersona {
    id: String,
    name: String,
    summary: String,
    personality: String,
    tone: String,
    style: String,
    guidance: String,
    created_at: String,
    updated_at: String,
}

impl RawPersona {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            summary: row.get(2)?,
            personality: row.get(3)?,
            tone: row.get(4)?,
            style: row.get(5)?,
            guidance: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}

impl TryFrom<RawPersona> for Persona {
    type Error = AppError;

    fn try_from(r: RawPersona) -> Result<Self, Self::Error> {
        Ok(Self {
            id: PersonaId::from_trusted(r.id),
            name: r.name,
            summary: r.summary,
            personality: r.personality,
            tone: r.tone,
            style: r.style,
            guidance: serde_json::from_str(&r.guidance).unwrap_or_default(),
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn repo() -> (PersonaRepo, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(&tmp.path().join("p.db")).await.unwrap();
        db.migrate().await.unwrap();
        (PersonaRepo::new(Arc::new(db)), tmp)
    }

    fn draft(name: &str) -> PersonaDraft {
        PersonaDraft {
            name: name.to_owned(),
            summary: "a summary".to_owned(),
            personality: "curious".to_owned(),
            tone: "warm".to_owned(),
            style: "concise".to_owned(),
            guidance: vec!["be kind".to_owned(), "cite sources".to_owned()],
        }
    }

    #[tokio::test]
    async fn crud_round_trip() {
        let (repo, _tmp) = repo().await;
        let id = repo.create(draft("Ada")).await.unwrap();

        let got = repo.get(&id).await.unwrap();
        assert_eq!(got.name, "Ada");
        assert_eq!(got.guidance, ["be kind", "cite sources"]);
        assert_eq!(got.created_at, got.updated_at);

        let mut d = draft("Ada Lovelace");
        d.tone = "dry".to_owned();
        repo.update(&id, d).await.unwrap();
        let got = repo.get(&id).await.unwrap();
        assert_eq!(got.name, "Ada Lovelace");
        assert_eq!(got.tone, "dry");

        assert_eq!(repo.list().await.unwrap().len(), 1);

        repo.delete(&id).await.unwrap();
        assert!(matches!(repo.get(&id).await, Err(AppError::NotFound(_))));
        assert!(repo.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn blank_name_is_rejected() {
        let (repo, _tmp) = repo().await;
        assert!(matches!(
            repo.create(draft("   ")).await,
            Err(AppError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn update_unknown_is_not_found() {
        let (repo, _tmp) = repo().await;
        let err = repo
            .update(&PersonaId::from_trusted("nope"), draft("X"))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn name_is_trimmed_on_write() {
        let (repo, _tmp) = repo().await;
        let id = repo.create(draft("  Bo  ")).await.unwrap();
        assert_eq!(repo.get(&id).await.unwrap().name, "Bo");
    }
}
