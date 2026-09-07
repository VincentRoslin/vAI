//! Cloned TTS voices (Phase 19 follow-up, `V0008`). Chatterbox Turbo clones a
//! speaker from a short reference WAV; this module owns the `voice` table and
//! the reference-WAV files under `<models.dir>/tts/voices/` (Rust owns the
//! filesystem and SQLite — `CLAUDE.md` Article I).
//!
//! `name` is untrusted user text — it is only ever shown in the UI, never fed
//! to a model. The reference WAV is handed to the TTS worker by absolute path
//! (the worker never chooses a path).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::Row;
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use ts_rs::TS;

use crate::contracts::ids::VoiceId;
use crate::db::Db;
use crate::ipc::{AppError, AppResult};

/// Largest reference WAV we accept (bytes). A clean 10–30 s clip is well under.
const MAX_WAV_BYTES: usize = 15 * 1024 * 1024;
/// Chatterbox asserts the (resampled) reference is longer than 5 s
/// (`chatterbox/tts_turbo.py`). Require a little more from the source clip.
const MIN_WAV_SECONDS: f64 = 6.0;

/// A stored cloned voice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Voice {
    /// Stable id.
    pub id: VoiceId,
    /// User label (shown in the UI only).
    pub name: String,
    /// `true` for the one voice the assistant currently speaks in.
    pub active: bool,
    /// RFC-3339.
    pub created_at: String,
}

/// All `voice` SQL + the reference-WAV directory.
pub struct VoiceRepo {
    db: Arc<Db>,
    dir: PathBuf,
}

impl VoiceRepo {
    /// `dir` = `<models.dir>/tts/voices`.
    #[must_use]
    pub fn new(db: Arc<Db>, dir: PathBuf) -> Self {
        Self { db, dir }
    }

    /// Every voice, newest first.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn list(&self) -> AppResult<Vec<Voice>> {
        let raws: Vec<RawVoice> = self
            .db
            .read(|conn| {
                Ok(conn
                    .prepare_cached(
                        "SELECT id, name, active, created_at FROM voice ORDER BY created_at DESC",
                    )?
                    .query_map([], RawVoice::from_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .await?;
        Ok(raws.into_iter().map(Into::into).collect())
    }

    /// Import a reference WAV: validate it, copy it into the voices directory,
    /// and record the row. Does **not** activate it.
    ///
    /// # Errors
    /// [`AppError::Validation`] for a blank name, non-WAV bytes, a clip shorter
    /// than [`MIN_WAV_SECONDS`], or bytes over [`MAX_WAV_BYTES`]; a filesystem
    /// or persistence error otherwise.
    pub async fn import(&self, name: &str, bytes: &[u8]) -> AppResult<Voice> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::Validation(
                "voice name must not be empty".to_owned(),
            ));
        }
        if bytes.len() > MAX_WAV_BYTES {
            return Err(AppError::Validation(format!(
                "reference WAV is too large ({} MB); the limit is {} MB",
                bytes.len() / (1024 * 1024),
                MAX_WAV_BYTES / (1024 * 1024),
            )));
        }
        validate_wav(bytes)?;

        let id = VoiceId::from_trusted(uuid::Uuid::new_v4().hyphenated().to_string());
        let file = format!("{id}.wav");
        write_atomic(&self.dir, &file, bytes)?;

        let now = now_rfc3339();
        let (key, file_col, name_col, now_col) =
            (id.to_string(), file.clone(), name.to_owned(), now.clone());
        let write = self
            .db
            .write(move |tx| {
                tx.prepare_cached(
                    "INSERT INTO voice (id, name, file, active, created_at) \
                     VALUES (?1, ?2, ?3, 0, ?4)",
                )?
                .execute(rusqlite::params![key, name_col, file_col, now_col])?;
                Ok(())
            })
            .await;
        if let Err(e) = write {
            // The row is the source of truth — don't leave an orphan file.
            let _ = std::fs::remove_file(self.dir.join(&file));
            return Err(e.into());
        }

        Ok(Voice {
            id,
            name: name.to_owned(),
            active: false,
            created_at: now,
        })
    }

    /// Delete a voice — its row and its reference WAV.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn delete(&self, id: &VoiceId) -> AppResult<()> {
        let key = id.to_string();
        let file: Option<String> = self
            .db
            .write(move |tx| {
                let file: Option<String> = tx
                    .prepare_cached("SELECT file FROM voice WHERE id = ?1")?
                    .query_map([&key], |r| r.get::<_, String>(0))?
                    .next()
                    .transpose()?;
                tx.prepare_cached("DELETE FROM voice WHERE id = ?1")?
                    .execute([&key])?;
                Ok(file)
            })
            .await?;
        if let Some(file) = file {
            let _ = std::fs::remove_file(self.dir.join(file));
        }
        Ok(())
    }

    /// Make `id` the active voice (`None` = the model's built-in voice).
    ///
    /// # Errors
    /// [`AppError::NotFound`] if `id` is unknown; a persistence error.
    pub async fn set_active(&self, id: Option<VoiceId>) -> AppResult<()> {
        let key = id.map(|i| i.to_string());
        let write = self
            .db
            .write(move |tx| {
                tx.execute("UPDATE voice SET active = 0 WHERE active = 1", [])?;
                if let Some(key) = key {
                    let n = tx
                        .prepare_cached("UPDATE voice SET active = 1 WHERE id = ?1")?
                        .execute([key])?;
                    if n == 0 {
                        return Err(crate::db::DbError::NotFound);
                    }
                }
                Ok(())
            })
            .await;
        match write {
            Ok(()) => Ok(()),
            Err(crate::db::DbError::NotFound) => Err(AppError::NotFound("voice".to_owned())),
            Err(e) => Err(e.into()),
        }
    }

    /// Absolute path of the active voice's reference WAV, if one is set and the
    /// file is present.
    ///
    /// # Errors
    /// A persistence error.
    pub async fn active_path(&self) -> AppResult<Option<PathBuf>> {
        let file: Option<String> = self
            .db
            .read(|conn| {
                Ok(conn
                    .prepare_cached("SELECT file FROM voice WHERE active = 1")?
                    .query_map([], |r| r.get::<_, String>(0))?
                    .next()
                    .transpose()?)
            })
            .await?;
        Ok(file.map(|f| self.dir.join(f)).filter(|p| p.is_file()))
    }
}

// ---------------------------------------------------------------- helpers

fn validate_wav(bytes: &[u8]) -> AppResult<()> {
    let reader = hound::WavReader::new(std::io::Cursor::new(bytes))
        .map_err(|e| AppError::Validation(format!("not a valid WAV file: {e}")))?;
    let spec = reader.spec();
    let seconds = f64::from(reader.duration()) / f64::from(spec.sample_rate.max(1));
    if seconds < MIN_WAV_SECONDS {
        return Err(AppError::Validation(format!(
            "reference clip is {seconds:.1}s; use at least {MIN_WAV_SECONDS:.0}s of clean speech"
        )));
    }
    Ok(())
}

fn write_atomic(dir: &Path, file: &str, bytes: &[u8]) -> AppResult<()> {
    std::fs::create_dir_all(dir).map_err(|e| AppError::internal("create voices dir", e))?;
    let tmp = dir.join(format!("{file}.tmp"));
    let final_path = dir.join(file);
    std::fs::write(&tmp, bytes).map_err(|e| AppError::internal("write voice wav", e))?;
    std::fs::rename(&tmp, &final_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::internal("finalise voice wav", e)
    })?;
    Ok(())
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

struct RawVoice {
    id: String,
    name: String,
    active: bool,
    created_at: String,
}

impl RawVoice {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            active: row.get::<_, i64>(2)? != 0,
            created_at: row.get(3)?,
        })
    }
}

impl From<RawVoice> for Voice {
    fn from(r: RawVoice) -> Self {
        Self {
            id: VoiceId::from_trusted(r.id),
            name: r.name,
            active: r.active,
            created_at: r.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal valid PCM16 mono WAV of `seconds` of silence at 16 kHz.
    fn wav(seconds: f64) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut buf, spec).unwrap();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let samples = (16_000.0 * seconds) as usize;
            for _ in 0..samples {
                w.write_sample(0i16).unwrap();
            }
            w.finalize().unwrap();
        }
        buf.into_inner()
    }

    async fn repo() -> (VoiceRepo, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(&tmp.path().join("v.db")).await.unwrap();
        db.migrate().await.unwrap();
        let dir = tmp.path().join("voices");
        (VoiceRepo::new(Arc::new(db), dir), tmp)
    }

    #[tokio::test]
    async fn import_writes_row_and_file() {
        let (repo, _tmp) = repo().await;
        let v = repo.import("  Ada  ", &wav(8.0)).await.unwrap();
        assert_eq!(v.name, "Ada");
        assert!(!v.active);
        assert!(repo.dir.join(format!("{}.wav", v.id)).is_file());
        assert_eq!(repo.list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn import_rejects_blank_name_short_clip_and_non_wav() {
        let (repo, _tmp) = repo().await;
        assert!(matches!(
            repo.import("   ", &wav(8.0)).await,
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            repo.import("x", &wav(2.0)).await,
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            repo.import("x", b"not a wav").await,
            Err(AppError::Validation(_))
        ));
        assert!(repo.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn set_active_keeps_a_single_active_and_resolves_path() {
        let (repo, _tmp) = repo().await;
        let a = repo.import("A", &wav(7.0)).await.unwrap();
        let b = repo.import("B", &wav(7.0)).await.unwrap();

        assert!(repo.active_path().await.unwrap().is_none());

        repo.set_active(Some(a.id.clone())).await.unwrap();
        repo.set_active(Some(b.id.clone())).await.unwrap();
        let active: Vec<_> = repo
            .list()
            .await
            .unwrap()
            .into_iter()
            .filter(|v| v.active)
            .map(|v| v.id)
            .collect();
        assert_eq!(active, vec![b.id.clone()]);
        assert_eq!(
            repo.active_path().await.unwrap(),
            Some(repo.dir.join(format!("{}.wav", b.id)))
        );

        repo.set_active(None).await.unwrap();
        assert!(repo.active_path().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn set_active_unknown_is_not_found() {
        let (repo, _tmp) = repo().await;
        assert!(matches!(
            repo.set_active(Some(VoiceId::from_trusted("ghost"))).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn delete_removes_row_and_file() {
        let (repo, _tmp) = repo().await;
        let v = repo.import("A", &wav(7.0)).await.unwrap();
        let path = repo.dir.join(format!("{}.wav", v.id));
        repo.delete(&v.id).await.unwrap();
        assert!(!path.exists());
        assert!(repo.list().await.unwrap().is_empty());
    }
}
