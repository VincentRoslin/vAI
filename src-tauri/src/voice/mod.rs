//! Voice input (Phase 18): microphone capture → Silero VAD endpointing → STT
//! worker → a **user turn on the shared conversation engine** (the same path
//! typed input takes). Push-to-talk for v1 (ADR-0005).
//!
//! Ownership (Article I): Rust owns capture, device selection, VAD, the STT
//! worker lifecycle, and the transcript→turn hand-off. The worker does STT only.

pub mod capture;
pub mod resample;
pub mod segment;
pub mod vad;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod live_tests;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::{mpsc, watch, Mutex, Notify};
use tokio_util::sync::CancellationToken;

use crate::contracts::conversation::MessageContent;
use crate::contracts::ids::ConversationId;
use crate::conversation::ConversationEngine;
use crate::ipc::{AppError, AppResult};
use crate::voice::vad::{SileroVad, VadConfig, VadEvent};
use crate::worker::WorkerSupervisor;

/// The rate the VAD + STT models expect (ADR-0005).
pub const SAMPLE_RATE: u32 = 16_000;

/// What the UI shows while listening (Tauri `Channel<VoiceState>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind")]
pub enum VoiceState {
    /// Not listening.
    Idle,
    /// Worker is loading its model.
    Warming,
    /// Listening; no speech yet.
    Listening,
    /// Capturing an utterance.
    Speech,
    /// Utterance ended; transcribing.
    Transcribing,
    /// A recoverable error occurred; back to `Idle` next.
    Error,
}

/// Tunables (config `voice.*`, schema v6).
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    /// Pinned `silero_vad.onnx`.
    pub vad_model: PathBuf,
    /// Where segment WAVs are written (app cache dir).
    pub temp_dir: PathBuf,
    /// `cpal` device name; `None` = system default.
    pub input_device: Option<String>,
    /// VAD thresholds.
    pub vad: VadConfig,
    /// Pre-roll kept before speech onset.
    pub pre_roll_ms: u32,
}

/// The STT worker's `WorkerResult::Ok.data` shape (this phase owns it —
/// `docs/contracts.md`).
#[derive(Debug, Clone, Deserialize)]
pub struct SttResult {
    /// The transcript.
    pub text: String,
    /// Detected language (BCP-47).
    #[serde(default)]
    pub language: Option<String>,
    /// Audio duration in seconds (for the RTF figure).
    #[serde(default)]
    pub duration_s: Option<f32>,
    /// Mean token log-probability — low ⇒ unreliable.
    #[serde(default)]
    pub avg_logprob: Option<f32>,
    /// Model's own "this was not speech" probability.
    #[serde(default)]
    pub no_speech_prob: Option<f32>,
}

impl SttResult {
    /// The low-confidence drop policy (ADR-0005): empty, clearly non-speech, or
    /// very low average log-prob ⇒ do not create a turn.
    #[must_use]
    pub fn is_confident(&self) -> bool {
        !self.text.trim().is_empty()
            && self.no_speech_prob.unwrap_or(0.0) <= 0.6
            && self.avg_logprob.unwrap_or(0.0) >= -1.0
    }
}

/// Voice-input service. One listening session at a time (push-to-talk).
pub struct VoiceInput {
    engine: Arc<ConversationEngine>,
    stt: Arc<WorkerSupervisor>,
    cfg: VoiceConfig,
    state_tx: watch::Sender<VoiceState>,
    session: Mutex<Option<SessionHandle>>,
}

struct SessionHandle {
    cancel: CancellationToken,
    release: Arc<Notify>,
    task: tokio::task::JoinHandle<()>,
    /// Kept so the capture thread stays alive for the session.
    _capture: Option<capture::CaptureStream>,
}

impl VoiceInput {
    /// Build the service (does not open the microphone).
    #[must_use]
    pub fn new(
        engine: Arc<ConversationEngine>,
        stt: Arc<WorkerSupervisor>,
        cfg: VoiceConfig,
    ) -> Arc<Self> {
        let (state_tx, _) = watch::channel(VoiceState::Idle);
        Arc::new(Self {
            engine,
            stt,
            cfg,
            state_tx,
            session: Mutex::new(None),
        })
    }

    /// Subscribe to state changes.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<VoiceState> {
        self.state_tx.subscribe()
    }

    /// Current state.
    #[must_use]
    pub fn state(&self) -> VoiceState {
        *self.state_tx.borrow()
    }

    /// Update the state (always applied, even with no subscribers).
    fn set_state(&self, s: VoiceState) {
        self.state_tx.send_replace(s);
    }

    /// Whether a listening session is active.
    pub async fn is_listening(&self) -> bool {
        self.session.lock().await.is_some()
    }

    /// Start listening for one utterance on `conversation_id`.
    ///
    /// # Errors
    /// [`AppError::Conflict`] if already listening; [`AppError::BackendUnavailable`]
    /// if the microphone or VAD model is unavailable.
    pub async fn start_listening(
        self: &Arc<Self>,
        conversation_id: ConversationId,
    ) -> AppResult<()> {
        let mut slot = self.session.lock().await;
        if slot.is_some() {
            return Err(AppError::Conflict("already listening".to_owned()));
        }

        let (frames_tx, frames_rx) = mpsc::unbounded_channel::<Vec<f32>>();
        let capture = capture::CaptureStream::open(self.cfg.input_device.as_deref(), frames_tx)?;
        let in_rate = capture.sample_rate();
        let in_channels = capture.channels();
        *slot = Some(self.spawn_session(
            conversation_id,
            frames_rx,
            in_rate,
            in_channels,
            Some(capture),
        ));
        Ok(())
    }

    fn spawn_session(
        self: &Arc<Self>,
        conversation_id: ConversationId,
        frames_rx: mpsc::UnboundedReceiver<Vec<f32>>,
        in_rate: u32,
        in_channels: u16,
        capture: Option<capture::CaptureStream>,
    ) -> SessionHandle {
        let cancel = CancellationToken::new();
        let release = Arc::new(Notify::new());
        let this = Arc::clone(self);
        let cancel2 = cancel.clone();
        let release2 = Arc::clone(&release);
        let task = tokio::spawn(async move {
            this.run_session(
                conversation_id,
                frames_rx,
                in_rate,
                in_channels,
                cancel2,
                release2,
            )
            .await;
        });
        SessionHandle {
            cancel,
            release,
            task,
            _capture: capture,
        }
    }

    /// Feed pre-recorded frames instead of opening a microphone (tests).
    #[cfg(test)]
    pub async fn start_with_frames(
        self: &Arc<Self>,
        conversation_id: ConversationId,
        frames_rx: mpsc::UnboundedReceiver<Vec<f32>>,
        in_rate: u32,
        in_channels: u16,
    ) -> AppResult<()> {
        let mut slot = self.session.lock().await;
        if slot.is_some() {
            return Err(AppError::Conflict("already listening".to_owned()));
        }
        *slot = Some(self.spawn_session(conversation_id, frames_rx, in_rate, in_channels, None));
        Ok(())
    }

    /// Wait for the current session's task to finish (tests).
    #[cfg(test)]
    pub async fn wait_idle(&self) {
        let handle = self.session.lock().await.take();
        if let Some(h) = handle {
            let _ = h.task.await;
        }
    }

    /// Stop listening — finish (transcribe) an utterance in progress, then end
    /// the session. Idempotent.
    pub async fn stop_listening(&self) {
        let handle = self.session.lock().await.take();
        if let Some(h) = handle {
            h.release.notify_waiters();
            let _ = h.task.await;
        }
        self.set_state(VoiceState::Idle);
    }

    /// Abort listening without transcribing (shutdown / barge-in later).
    pub async fn cancel_listening(&self) {
        let handle = self.session.lock().await.take();
        if let Some(h) = handle {
            h.cancel.cancel();
            let _ = h.task.await;
        }
        self.set_state(VoiceState::Idle);
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_session(
        self: Arc<Self>,
        conversation_id: ConversationId,
        mut frames: mpsc::UnboundedReceiver<Vec<f32>>,
        in_rate: u32,
        in_channels: u16,
        cancel: CancellationToken,
        release: Arc<Notify>,
    ) {
        let mut vad = match SileroVad::new(&self.cfg.vad_model, self.cfg.vad) {
            Ok(v) => v,
            Err(e) => {
                e.log("voice: load VAD");
                self.set_state(VoiceState::Error);
                return;
            }
        };
        let mut conv = resample::ToMono16k::new(in_rate, in_channels);
        let pre_roll_cap = (self.cfg.pre_roll_ms as usize * SAMPLE_RATE as usize / 1000)
            .max(SAMPLE_RATE as usize / 8);
        let mut pre_roll: VecDeque<f32> = VecDeque::with_capacity(pre_roll_cap + 1);
        let mut utterance: Vec<f32> = Vec::new();
        let mut in_speech = false;
        self.set_state(VoiceState::Listening);

        loop {
            let chunk = tokio::select! {
                () = cancel.cancelled() => return,
                () = release.notified() => {
                    if in_speech {
                        if let Some(VadEvent::SpeechEnd) = vad.force_endpoint() {
                            self.finish_utterance(&conversation_id, &utterance).await;
                        }
                    }
                    return;
                }
                frame = frames.recv() => match frame {
                    Some(f) => f,
                    None => return,
                },
            };

            let mono16 = conv.push(&chunk);
            if mono16.is_empty() {
                continue;
            }

            // Route samples: into the utterance once speaking, else the pre-roll.
            for &s in &mono16 {
                if in_speech {
                    utterance.push(s);
                } else {
                    if pre_roll.len() >= pre_roll_cap {
                        pre_roll.pop_front();
                    }
                    pre_roll.push_back(s);
                }
            }

            let events = match vad.push(&mono16) {
                Ok(e) => e,
                Err(err) => {
                    err.log("voice: VAD inference");
                    self.set_state(VoiceState::Error);
                    return;
                }
            };
            for ev in events {
                match ev {
                    VadEvent::SpeechStart => {
                        in_speech = true;
                        utterance.clear();
                        utterance.extend(pre_roll.iter().copied());
                        self.set_state(VoiceState::Speech);
                    }
                    VadEvent::SpeechEnd | VadEvent::MaxDurationCut => {
                        self.finish_utterance(&conversation_id, &utterance).await;
                        return;
                    }
                }
            }
        }
    }

    async fn finish_utterance(&self, conversation_id: &ConversationId, utterance: &[f32]) {
        self.set_state(VoiceState::Transcribing);
        if utterance.len() < SAMPLE_RATE as usize / 10 {
            self.set_state(VoiceState::Idle);
            return;
        }

        let name = uuid::Uuid::new_v4().simple().to_string();
        let wav = match segment::write_wav(&self.cfg.temp_dir, &name, utterance) {
            Ok(p) => p,
            Err(e) => {
                e.log("voice: write segment");
                self.set_state(VoiceState::Error);
                return;
            }
        };

        let cancel = CancellationToken::new();
        let payload = serde_json::json!({ "audio_path": wav, "language": null });
        let result = self.stt.request(payload, &cancel, None).await;
        segment::cleanup(&wav);

        let parsed = result.and_then(|v| {
            serde_json::from_value::<SttResult>(v)
                .map_err(|e| AppError::internal("parse STT result", e))
        });
        match parsed {
            Ok(stt) if stt.is_confident() => {
                if let Err(e) = self
                    .engine
                    .add_user_turn(conversation_id, MessageContent::Text { text: stt.text })
                    .await
                {
                    e.log("voice: add user turn");
                    self.set_state(VoiceState::Error);
                    return;
                }
                self.set_state(VoiceState::Idle);
            }
            Ok(_) => {
                // Low-confidence: no turn, transient notice.
                self.set_state(VoiceState::Error);
            }
            Err(e) => {
                e.log("voice: STT");
                self.set_state(VoiceState::Error);
            }
        }
    }
}
