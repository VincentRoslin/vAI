//! TTS output (Phase 19) — drives the Chatterbox worker one clause at a time and
//! streams the audio into [`Playback`]. Owns the interruption of its own half of
//! barge-in (stop feeding + stop playback); the engine cancel is `mod.rs`'s job.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::ipc::{AppError, AppResult};
use crate::voice::playback::{Playback, TTS_RATE};
use crate::voice::resample::Resampler16;
use crate::voice::voices::VoiceRepo;
use crate::worker::WorkerSupervisor;

/// Bound on one TTS job. `WorkerSupervisor::request` applies no timeout of its
/// own — without this, a hung Chatterbox call (CUDA stall, a wedged
/// `prepare_conditionals`) would silently stop playback forever. Generous
/// enough to cover a cold Chatterbox load (~5-6s) plus one clause's synthesis.
const TTS_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// The TTS worker's `Ok.data` shape (this phase owns it — `docs/contracts.md`).
#[derive(Debug, Clone, Deserialize)]
pub struct TtsResult {
    /// Sample rate of the written WAV.
    pub sample_rate: u32,
    /// Audio duration in seconds.
    #[serde(default)]
    pub duration_s: Option<f32>,
}

/// What a completed `speak_stream` produced.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpokenSummary {
    /// Clauses successfully synthesised + queued.
    pub clauses: u32,
    /// Characters of text spoken (a clean prefix of the message).
    pub chars: u32,
}

/// Owns the TTS worker + the playback stream.
pub struct TtsOutput {
    worker: Arc<WorkerSupervisor>,
    playback: Mutex<Option<Playback>>,
    temp_dir: PathBuf,
    /// Resolves the active cloned voice at each session start. `None` in tests
    /// that don't exercise voice selection.
    voices: Option<Arc<VoiceRepo>>,
    cancel: Mutex<Option<CancellationToken>>,
    output_device: Mutex<Option<String>>,
}

impl TtsOutput {
    /// Build from a `WorkerKind::Tts` supervisor + where to write clause WAVs.
    /// `voices` resolves the active cloned voice at each session start.
    #[must_use]
    pub fn new(
        worker: Arc<WorkerSupervisor>,
        temp_dir: PathBuf,
        output_device: Option<String>,
        voices: Option<Arc<VoiceRepo>>,
    ) -> Self {
        Self {
            worker,
            playback: Mutex::new(None),
            temp_dir,
            voices,
            cancel: Mutex::new(None),
            output_device: Mutex::new(output_device),
        }
    }

    /// Swap the playback device; takes effect on the next `speak_stream`.
    pub async fn set_output_device(&self, name: Option<String>) {
        *self.output_device.lock().await = name;
        *self.playback.lock().await = None;
    }

    /// Spawn the TTS worker (loads Chatterbox) without synthesising. Also
    /// `prepare_conditionals` for the active cloned voice so the first clause
    /// is not clone-prep-bound.
    pub async fn warm(&self) -> AppResult<()> {
        self.worker.warm().await?;
        let voice_wav = match &self.voices {
            Some(v) => v.active_path().await.unwrap_or(None),
            None => None,
        };
        let Some(path) = voice_wav else {
            return Ok(());
        };
        let cancel = CancellationToken::new();
        let payload = serde_json::json!({
            "warm": true,
            "voice_wav": path,
        });
        match tokio::time::timeout(
            TTS_CALL_TIMEOUT,
            self.worker.request(payload, &cancel, None),
        )
        .await
        {
            Ok(Err(e)) => e.log("tts: warm clone"),
            Err(_) => {
                cancel.cancel();
                tracing::warn!(target: "voice", "tts: warm clone timed out — restarting worker");
                self.worker.shutdown().await;
            }
            Ok(Ok(_)) => {}
        }
        Ok(())
    }

    /// Synthesise + play every clause `rx` yields, in order, until the channel
    /// closes or `cancel` fires. Keeps ~1 clause of lookahead via the playback
    /// queue. On any error the playback is stopped and the error returned.
    ///
    /// # Errors
    /// [`AppError::BackendUnavailable`] (worker / output device),
    /// [`AppError::Cancelled`] (barge-in), or a synth error from the worker.
    pub async fn speak_stream(
        &self,
        mut rx: mpsc::Receiver<String>,
        cancel: CancellationToken,
    ) -> AppResult<SpokenSummary> {
        *self.cancel.lock().await = Some(cancel.clone());
        self.ensure_playback().await?;

        // Resolve the active cloned voice once per session — a Settings change
        // takes effect on the next voice session (matches `config` v8 voice keys).
        let voice_wav = match &self.voices {
            Some(v) => v.active_path().await.unwrap_or(None),
            None => None,
        };
        if let Some(p) = &voice_wav {
            tracing::info!(voice = %p.display(), "tts: using cloned voice");
        }

        let mut summary = SpokenSummary::default();
        let result = loop {
            let clause = tokio::select! {
                () = cancel.cancelled() => break Err(AppError::Cancelled),
                next = rx.recv() => match next {
                    Some(c) => c,
                    None => break Ok(()),
                },
            };
            match self.speak_one(&clause, voice_wav.as_deref(), &cancel).await {
                Ok(()) => {
                    summary.clauses += 1;
                    summary.chars += u32::try_from(clause.chars().count()).unwrap_or(0);
                }
                Err(AppError::Cancelled) => break Err(AppError::Cancelled),
                Err(e) => break Err(e),
            }
        };

        *self.cancel.lock().await = None;
        match result {
            Ok(()) => Ok(summary),
            Err(e) => {
                self.stop().await;
                Err(e)
            }
        }
    }

    /// Stop feeding + clear playback (the TTS half of barge-in). Idempotent.
    pub async fn stop(&self) {
        if let Some(c) = self.cancel.lock().await.take() {
            c.cancel();
        }
        if let Some(pb) = self.playback.lock().await.as_ref() {
            pb.stop();
        }
    }

    /// Playback has drained (all queued audio has played).
    pub async fn playback_idle(&self) -> bool {
        self.playback
            .lock()
            .await
            .as_ref()
            .is_none_or(Playback::is_idle)
    }

    /// Milliseconds of audio still queued.
    pub async fn queued_ms(&self) -> u32 {
        self.playback
            .lock()
            .await
            .as_ref()
            .map_or(0, Playback::queued_ms)
    }

    async fn ensure_playback(&self) -> AppResult<()> {
        let mut slot = self.playback.lock().await;
        if slot.is_none() {
            let name = self.output_device.lock().await.clone();
            *slot = Some(Playback::open(name.as_deref())?);
        }
        Ok(())
    }

    async fn speak_one(
        &self,
        clause: &str,
        voice_wav: Option<&std::path::Path>,
        cancel: &CancellationToken,
    ) -> AppResult<()> {
        let text = crate::voice::chunker::ClauseChunker::for_tts(clause);
        if text.is_empty() {
            return Ok(());
        }
        let _ = std::fs::create_dir_all(&self.temp_dir);
        let name = uuid::Uuid::new_v4().simple().to_string();
        let wav = self.temp_dir.join(format!("tts-{name}.wav"));
        let payload = serde_json::json!({
            "text": text,
            "out_path": wav,
            "voice_wav": voice_wav,
            "exaggeration": 0.7,
            "cfg_weight": 0.3,
        });

        let data = match tokio::time::timeout(
            TTS_CALL_TIMEOUT,
            self.worker.request(payload, cancel, None),
        )
        .await
        {
            Ok(r) => r?,
            Err(_) => {
                tracing::warn!(target: "voice", "tts: request timed out — restarting worker");
                self.worker.shutdown().await;
                return Err(AppError::Timeout("tts worker call".to_owned()));
            }
        };
        let meta: TtsResult =
            serde_json::from_value(data).map_err(|e| AppError::internal("parse TTS result", e))?;

        let samples = read_wav_mono(&wav)?;
        let _ = std::fs::remove_file(&wav);

        let samples = if meta.sample_rate == TTS_RATE {
            samples
        } else {
            let mut r = Resampler16::new(meta.sample_rate, TTS_RATE);
            let mut out = r.push(&samples);
            out.extend(r.flush());
            out
        };

        if let Some(pb) = self.playback.lock().await.as_ref() {
            pb.enqueue(&samples);
        }
        Ok(())
    }
}

/// Read a WAV (any int/float, any channel count) as mono f32 in `-1.0..=1.0`.
fn read_wav_mono(path: &std::path::Path) -> AppResult<Vec<f32>> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| AppError::internal("open tts wav", e))?;
    let spec = reader.spec();
    let ch = spec.channels.max(1) as usize;
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max = f32::from(i16::MAX) * f32::powi(2.0, i32::from(spec.bits_per_sample) - 16);
            reader
                .samples::<i32>()
                .map(|s| s.map_or(0.0, |v| v as f32 / max))
                .collect()
        }
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
    };
    Ok(if ch == 1 {
        raw
    } else {
        raw.chunks(ch)
            .map(|c| c.iter().sum::<f32>() / ch as f32)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tts_result_parses() {
        let v = serde_json::json!({ "sample_rate": 24000, "duration_s": 1.5 });
        let r: TtsResult = serde_json::from_value(v).unwrap();
        assert_eq!(r.sample_rate, 24000);
        assert_eq!(r.duration_s, Some(1.5));
    }
}
