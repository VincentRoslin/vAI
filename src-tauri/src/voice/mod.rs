//! Voice (Phase 18 in, Phase 19 out): microphone capture → Silero VAD → STT
//! worker → a **user turn on the shared conversation engine**; then (when a
//! model is set) generate → clause-chunk → Chatterbox TTS → `cpal` playback,
//! with **barge-in** (VAD onset or stop halts the LLM + TTS + playback and
//! returns to listening). Click-to-call starts it (off-hook until hang-up).
//!
//! Ownership (Article I): Rust owns capture + playback, device selection, VAD,
//! the STT/TTS worker lifecycles, the clause chunker, and the interruption
//! state machine. The workers do inference only.

pub mod capture;
pub mod chunker;
pub mod devices;
pub mod playback;
pub mod resample;
pub mod segment;
pub mod tts;
pub mod vad;
pub mod voices;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod live_tests;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::sync::{mpsc, watch, Mutex, Notify};
use tokio_util::sync::CancellationToken;

use crate::contracts::conversation::MessageContent;
use crate::contracts::generation::GenerationEvent;
use crate::contracts::ids::{ConversationId, ModelId, TaskId};
use crate::conversation::ConversationEngine;
use crate::ipc::{AppError, AppResult};
use crate::voice::chunker::ClauseChunker;
use crate::voice::tts::{SpokenSummary, TtsOutput};
use crate::voice::vad::{SileroVad, VadConfig, VadEvent};
use crate::worker::WorkerSupervisor;

/// The rate the VAD + STT models expect (ADR-0005).
pub const SAMPLE_RATE: u32 = 16_000;

/// A VAD onset while answering counts as a barge-in only after this long — the
/// tail of the user's own utterance is still in flight right after we start.
const BARGE_IN_GRACE: Duration = Duration::from_millis(500);

/// What the UI shows (Tauri `Channel<VoiceState>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(tag = "kind")]
#[ts(export, export_to = "../../src/bindings/")]
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
    /// A generation is running; no audio yet (Phase 19).
    Thinking,
    /// The assistant reply is playing (Phase 19).
    Speaking,
    /// Barge-in in progress — stopping the LLM + TTS + playback (Phase 19).
    Interrupting,
    /// A recoverable error occurred; back to `Idle` / `Listening` next.
    Error,
}

/// Tunables (config `voice.*`, schema v6/v7).
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    /// Pinned `silero_vad.onnx`.
    pub vad_model: PathBuf,
    /// Where segment / clause WAVs are written (app cache dir).
    pub temp_dir: PathBuf,
    /// `cpal` input device name; `None` = system default.
    pub input_device: Option<String>,
    /// `cpal` output device name; `None` = system default (v7).
    pub output_device: Option<String>,
    /// VAD thresholds.
    pub vad: VadConfig,
    /// Pre-roll kept before speech onset.
    pub pre_roll_ms: u32,
    /// Onset-threshold bump while TTS is playing — echo duck (v7, default 0.2).
    pub playback_duck: f32,
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

/// Voice service — one session at a time. A session is click-to-call:
/// with a model it runs `listen → transcribe → think → speak → listen` and
/// supports barge-in; without a model it transcribes one utterance and ends.
pub struct VoiceInput {
    engine: Arc<ConversationEngine>,
    stt: Arc<WorkerSupervisor>,
    tts: Arc<TtsOutput>,
    cfg: Mutex<VoiceConfig>,
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

/// The phase a with-model session is in (barge-in returns any of
/// `Thinking`/`Speaking` to `Listening`).
#[derive(PartialEq)]
enum Phase {
    /// Accumulating a user utterance.
    Listening,
    /// A generation is running / its reply is being spoken. `VoiceState` carries
    /// the finer Thinking-vs-Speaking distinction for the UI.
    Answering,
}

/// Handles for an in-flight generation + its TTS, so barge-in can stop both.
struct SpokenTurn {
    task_id: TaskId,
    tts_cancel: CancellationToken,
    tts_task: tokio::task::JoinHandle<AppResult<SpokenSummary>>,
    _chunk_task: tokio::task::JoinHandle<()>,
}

/// Everything `run_session` needs.
struct Session {
    conversation_id: ConversationId,
    model_id: Option<ModelId>,
    frames: mpsc::UnboundedReceiver<Vec<f32>>,
    in_rate: u32,
    in_channels: u16,
    cancel: CancellationToken,
    release: Arc<Notify>,
}

impl VoiceInput {
    /// Build the service (opens no devices).
    #[must_use]
    pub fn new(
        engine: Arc<ConversationEngine>,
        stt: Arc<WorkerSupervisor>,
        tts: Arc<TtsOutput>,
        cfg: VoiceConfig,
    ) -> Arc<Self> {
        let (state_tx, _) = watch::channel(VoiceState::Idle);
        Arc::new(Self {
            engine,
            stt,
            tts,
            cfg: Mutex::new(cfg),
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

    fn set_state(&self, s: VoiceState) {
        self.state_tx.send_replace(s);
    }

    /// Whether a session is active.
    pub async fn is_listening(&self) -> bool {
        self.session.lock().await.is_some()
    }

    /// Refresh device + hang tunables from the live config (Settings apply
    /// without an app restart; they take effect on the next session).
    pub async fn apply_runtime_config(
        &self,
        input_device: Option<String>,
        output_device: Option<String>,
        end_of_speech_ms: u32,
    ) {
        {
            let mut cfg = self.cfg.lock().await;
            cfg.input_device = input_device;
            cfg.output_device = output_device.clone();
            cfg.vad.min_silence_ms = end_of_speech_ms;
        }
        self.tts.set_output_device(output_device).await;
    }

    /// Start a voice session on `conversation_id`. `model_id = Some` runs the
    /// full listen→think→speak loop (with barge-in) until `stop_listening`;
    /// `None` transcribes one utterance and ends.
    ///
    /// # Errors
    /// [`AppError::Conflict`] if already active; [`AppError::BackendUnavailable`]
    /// if the microphone or VAD model is unavailable.
    pub async fn start_listening(
        self: &Arc<Self>,
        conversation_id: ConversationId,
        model_id: Option<ModelId>,
    ) -> AppResult<()> {
        let mut slot = self.session.lock().await;
        if slot.is_some() {
            return Err(AppError::Conflict("already listening".to_owned()));
        }
        let (frames_tx, frames_rx) = mpsc::unbounded_channel::<Vec<f32>>();
        let input = self.cfg.lock().await.input_device.clone();
        let capture = capture::CaptureStream::open(input.as_deref(), frames_tx)?;
        let (rate, ch) = (capture.sample_rate(), capture.channels());
        *slot = Some(self.spawn_session(
            conversation_id,
            model_id,
            frames_rx,
            rate,
            ch,
            Some(capture),
        ));
        Ok(())
    }

    fn spawn_session(
        self: &Arc<Self>,
        conversation_id: ConversationId,
        model_id: Option<ModelId>,
        frames_rx: mpsc::UnboundedReceiver<Vec<f32>>,
        in_rate: u32,
        in_channels: u16,
        capture: Option<capture::CaptureStream>,
    ) -> SessionHandle {
        let cancel = CancellationToken::new();
        let release = Arc::new(Notify::new());
        let this = Arc::clone(self);
        let (c2, r2) = (cancel.clone(), Arc::clone(&release));
        let task = tokio::spawn(async move {
            this.run_session(Session {
                conversation_id,
                model_id,
                frames: frames_rx,
                in_rate,
                in_channels,
                cancel: c2,
                release: r2,
            })
            .await;
        });
        SessionHandle {
            cancel,
            release,
            task,
            _capture: capture,
        }
    }

    /// Feed pre-recorded frames instead of a microphone (tests).
    #[cfg(test)]
    pub async fn start_with_frames(
        self: &Arc<Self>,
        conversation_id: ConversationId,
        model_id: Option<ModelId>,
        frames_rx: mpsc::UnboundedReceiver<Vec<f32>>,
        in_rate: u32,
        in_channels: u16,
    ) -> AppResult<()> {
        let mut slot = self.session.lock().await;
        if slot.is_some() {
            return Err(AppError::Conflict("already listening".to_owned()));
        }
        *slot = Some(self.spawn_session(
            conversation_id,
            model_id,
            frames_rx,
            in_rate,
            in_channels,
            None,
        ));
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

    /// Stop the session — finish an utterance in progress or barge-in on the
    /// assistant, then end. Idempotent.
    pub async fn stop_listening(&self) {
        let handle = self.session.lock().await.take();
        if let Some(h) = handle {
            h.release.notify_waiters();
            let _ = h.task.await;
        }
        self.set_state(VoiceState::Idle);
    }

    /// Abort the session immediately (shutdown). Idempotent.
    pub async fn cancel_listening(&self) {
        let handle = self.session.lock().await.take();
        if let Some(h) = handle {
            h.cancel.cancel();
            let _ = h.task.await;
        }
        self.tts.stop().await;
        self.set_state(VoiceState::Idle);
    }

    #[allow(clippy::too_many_lines)]
    async fn run_session(self: Arc<Self>, mut s: Session) {
        self.set_state(VoiceState::Warming);
        // Overlap Chatterbox / faster-whisper load with the user's first
        // utterance: listen immediately; `request` waits on the supervisor
        // lock if the hello handshake is still in flight.
        {
            let this = Arc::clone(&self);
            tokio::spawn(async move {
                let (stt_w, tts_w) = tokio::join!(this.stt.warm(), this.tts.warm());
                if let Err(e) = stt_w {
                    e.log("voice: warm STT");
                }
                if let Err(e) = tts_w {
                    e.log("voice: warm TTS");
                }
                if this.state() == VoiceState::Warming {
                    this.set_state(VoiceState::Listening);
                }
            });
        }

        let cfg = self.cfg.lock().await.clone();
        let mut vad = match SileroVad::new(&cfg.vad_model, cfg.vad) {
            Ok(v) => v,
            Err(e) => {
                e.log("voice: load VAD");
                self.set_state(VoiceState::Error);
                return;
            }
        };
        let base_onset = vad.onset_threshold();
        let mut conv = resample::ToMono16k::new(s.in_rate, s.in_channels);
        let pre_roll_cap = (cfg.pre_roll_ms as usize * SAMPLE_RATE as usize / 1000)
            .max(SAMPLE_RATE as usize / 8);
        let mut pre_roll: VecDeque<f32> = VecDeque::with_capacity(pre_roll_cap + 1);
        let mut utterance: Vec<f32> = Vec::new();
        let mut in_speech = false;
        let mut phase = Phase::Listening;
        let mut turn: Option<SpokenTurn> = None;
        // A barge-in only counts once we have been answering `BARGE_IN_GRACE` —
        // the tail of the user's own utterance is still in the pipeline right
        // after we start, and would otherwise self-interrupt.
        let mut answering_since: Option<Instant> = None;
        self.set_state(VoiceState::Listening);

        loop {
            let chunk = tokio::select! {
                () = s.cancel.cancelled() => break,
                () = s.release.notified() => {
                    match phase {
                        Phase::Listening if in_speech => {
                            if let Some(VadEvent::SpeechEnd) = vad.force_endpoint() {
                                self.transcribe_and_maybe_speak(
                                    &s, &utterance, &mut phase, &mut turn, &mut vad, base_onset,
                                ).await;
                            }
                        }
                        Phase::Answering => self.interrupt(&mut turn).await,
                        Phase::Listening => {}
                    }
                    break;
                }
                () = tokio::time::sleep(Duration::from_millis(60)), if phase != Phase::Listening => {
                    if self.speaking_finished(&mut turn).await {
                        vad.set_onset_threshold(base_onset);
                        vad.reset();
                        in_speech = false;
                        utterance.clear();
                        pre_roll.clear();
                        phase = Phase::Listening;
                        answering_since = None;
                        self.set_state(VoiceState::Listening);
                    }
                    continue;
                }
                frame = s.frames.recv() => match frame {
                    Some(f) => f,
                    None => break,
                },
            };

            let mono16 = conv.push(&chunk);
            if mono16.is_empty() {
                continue;
            }
            if phase == Phase::Listening {
                for &x in &mono16 {
                    if in_speech {
                        utterance.push(x);
                    } else {
                        if pre_roll.len() >= pre_roll_cap {
                            pre_roll.pop_front();
                        }
                        pre_roll.push_back(x);
                    }
                }
            }

            let events = match vad.push(&mono16) {
                Ok(e) => e,
                Err(err) => {
                    err.log("voice: VAD inference");
                    self.set_state(VoiceState::Error);
                    break;
                }
            };
            for ev in events {
                match (ev, &phase) {
                    (VadEvent::SpeechStart, Phase::Listening) => {
                        in_speech = true;
                        utterance.clear();
                        utterance.extend(pre_roll.iter().copied());
                        self.set_state(VoiceState::Speech);
                    }
                    (VadEvent::SpeechStart, Phase::Answering) => {
                        if answering_since.is_none_or(|t| t.elapsed() < BARGE_IN_GRACE) {
                            continue; // still the tail of our own utterance
                        }
                        self.interrupt(&mut turn).await;
                        vad.set_onset_threshold(base_onset);
                        vad.reset();
                        in_speech = true;
                        utterance.clear();
                        utterance.extend(pre_roll.iter().copied());
                        phase = Phase::Listening;
                        answering_since = None;
                        self.set_state(VoiceState::Speech);
                    }
                    (VadEvent::SpeechEnd | VadEvent::MaxDurationCut, Phase::Listening) => {
                        self.transcribe_and_maybe_speak(
                            &s, &utterance, &mut phase, &mut turn, &mut vad, base_onset,
                        )
                        .await;
                        in_speech = false;
                        utterance.clear();
                        match phase {
                            Phase::Answering => answering_since = Some(Instant::now()),
                            Phase::Listening if s.model_id.is_none() => {
                                self.teardown(&mut turn).await;
                                return;
                            }
                            Phase::Listening => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        self.teardown(&mut turn).await;
    }

    async fn teardown(&self, turn: &mut Option<SpokenTurn>) {
        if let Some(t) = turn.take() {
            let _ = self.engine.cancel(&t.task_id).await;
            t.tts_cancel.cancel();
            let _ = t.tts_task.await;
        }
        self.tts.stop().await;
    }

    /// Transcribe the utterance → a user turn. Then, if a model is set, start
    /// generating + speaking and move `phase` to `Thinking`.
    #[allow(clippy::too_many_arguments)]
    async fn transcribe_and_maybe_speak(
        self: &Arc<Self>,
        s: &Session,
        utterance: &[f32],
        phase: &mut Phase,
        turn: &mut Option<SpokenTurn>,
        vad: &mut SileroVad,
        base_onset: f32,
    ) {
        self.set_state(VoiceState::Transcribing);
        if utterance.len() < SAMPLE_RATE as usize / 10 {
            self.set_state(VoiceState::Listening);
            return;
        }
        let Some(text) = self.transcribe(utterance).await else {
            self.set_state(VoiceState::Error);
            return;
        };
        if let Err(e) = self
            .engine
            .add_user_turn(&s.conversation_id, MessageContent::Text { text })
            .await
        {
            e.log("voice: add user turn");
            self.set_state(VoiceState::Error);
            return;
        }
        let Some(model_id) = s.model_id.clone() else {
            self.set_state(VoiceState::Listening);
            return;
        };
        match self.begin_speaking(&s.conversation_id, model_id).await {
            Ok(t) => {
                *turn = Some(t);
                *phase = Phase::Answering;
                vad.set_onset_threshold(base_onset + self.cfg.lock().await.playback_duck);
                vad.reset();
                self.set_state(VoiceState::Thinking);
            }
            Err(e) => {
                e.log("voice: begin speaking");
                self.set_state(VoiceState::Listening);
            }
        }
    }

    /// STT one endpointed utterance → the confident transcript (or `None`).
    async fn transcribe(&self, utterance: &[f32]) -> Option<String> {
        let name = uuid::Uuid::new_v4().simple().to_string();
        let temp_dir = self.cfg.lock().await.temp_dir.clone();
        let wav = match segment::write_wav(&temp_dir, &name, utterance) {
            Ok(p) => p,
            Err(e) => {
                e.log("voice: write segment");
                return None;
            }
        };
        let cancel = CancellationToken::new();
        let payload = serde_json::json!({ "audio_path": wav, "language": "en" });
        let result = self.stt.request(payload, &cancel, None).await;
        segment::cleanup(&wav);

        match result.and_then(|v| {
            serde_json::from_value::<SttResult>(v)
                .map_err(|e| AppError::internal("parse STT result", e))
        }) {
            Ok(stt) if stt.is_confident() => Some(stt.text),
            Ok(_) => None,
            Err(e) => {
                e.log("voice: STT");
                None
            }
        }
    }

    /// Kick a generation whose token stream feeds the clause chunker → TTS.
    async fn begin_speaking(
        self: &Arc<Self>,
        conversation_id: &ConversationId,
        model_id: ModelId,
    ) -> AppResult<SpokenTurn> {
        let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<Option<String>>();
        let (clause_tx, clause_rx) = mpsc::channel::<String>(16);

        let chunk_task = tokio::spawn(async move {
            let mut chunker = ClauseChunker::new();
            while let Some(item) = raw_rx.recv().await {
                let Some(delta) = item else {
                    // terminal frame → flush the tail and finish
                    if let Some(tail) = chunker.flush() {
                        let _ = clause_tx.send(tail).await;
                    }
                    return;
                };
                for clause in chunker.push(&delta) {
                    if clause_tx.send(clause).await.is_err() {
                        return;
                    }
                }
            }
        });

        let tts = Arc::clone(&self.tts);
        let tts_cancel = CancellationToken::new();
        let tc = tts_cancel.clone();
        let tts_task = tokio::spawn(async move { tts.speak_stream(clause_rx, tc).await });

        let sink_tx = raw_tx.clone();
        let task_id = self
            .engine
            .generate_spoken(
                conversation_id.clone(),
                model_id,
                move |ev: GenerationEvent| match ev {
                    GenerationEvent::TokenDelta { text, .. } => {
                        let _ = sink_tx.send(Some(text));
                    }
                    GenerationEvent::Done { .. }
                    | GenerationEvent::Cancelled
                    | GenerationEvent::Error { .. } => {
                        let _ = sink_tx.send(None);
                    }
                },
            )
            .await?;
        drop(raw_tx);

        Ok(SpokenTurn {
            task_id,
            tts_cancel,
            tts_task,
            _chunk_task: chunk_task,
        })
    }

    /// Barge-in: cancel the generation (persists the partial turn), cancel TTS,
    /// stop playback — timed.
    async fn interrupt(&self, turn: &mut Option<SpokenTurn>) {
        let Some(t) = turn.take() else { return };
        self.set_state(VoiceState::Interrupting);
        let started = Instant::now();

        let _ = self.engine.cancel(&t.task_id).await;
        t.tts_cancel.cancel();
        self.tts.stop().await;
        let _ = t.tts_task.await;

        let mut waited = 0u32;
        while !self.tts.playback_idle().await && waited < 500 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            waited += 10;
        }
        tracing::info!(
            target: "voice",
            trigger_to_silence_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "barge-in complete"
        );
    }

    /// True once the generation is done and playback has drained.
    async fn speaking_finished(&self, turn: &mut Option<SpokenTurn>) -> bool {
        let Some(t) = turn.as_ref() else { return true };
        let gen_done = t.tts_task.is_finished();
        let drained = self.tts.playback_idle().await;
        if !drained && self.state() != VoiceState::Speaking {
            self.set_state(VoiceState::Speaking);
        }
        if !gen_done || !drained {
            return false;
        }
        if let Some(t) = turn.take() {
            let _ = t.tts_task.await;
        }
        true
    }
}
