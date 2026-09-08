//! Silero VAD (v5 ONNX) in the Rust core (ADR-0005) — it owns utterance
//! boundaries and (Phase 19) the barge-in trigger.
//!
//! The v5 model takes a 512-sample @ 16 kHz window + a `[2,1,128]` LSTM state +
//! the sample rate, and returns a speech probability + the next state. This
//! module wraps that in a small state machine that emits `SpeechStart` /
//! `SpeechEnd` with configurable thresholds and a hard max-utterance cut.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

use crate::ipc::{AppError, AppResult};
use crate::voice::SAMPLE_RATE;

/// Silero v5 window length at 16 kHz.
pub const WINDOW: usize = 512;

/// Tunable endpointing thresholds (config `voice.vad.*`, ADR-0005).
#[derive(Debug, Clone, Copy)]
pub struct VadConfig {
    /// Speech-probability threshold for "this window is speech" (0.5).
    pub onset_threshold: f32,
    /// Drop below this while already in speech before counting silence.
    /// Lower than onset so quiet mid-sentence speech is not treated as a pause
    /// (classic Silero hysteresis — the main cut-off cause).
    pub offset_threshold: f32,
    /// Silence after speech before the utterance ends.
    pub min_silence_ms: u32,
    /// Shortest run of speech that counts as an utterance (rejects blips).
    pub min_utterance_ms: u32,
    /// Absolute cap — force an endpoint even if the talker never pauses.
    pub max_utterance_ms: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            onset_threshold: 0.5,
            offset_threshold: 0.35,
            min_silence_ms: 700,
            min_utterance_ms: 250,
            max_utterance_ms: 30_000,
        }
    }
}

impl VadConfig {
    fn windows(ms: u32) -> u32 {
        // windows per ms = SAMPLE_RATE/1000 samples-per-ms ÷ WINDOW
        let per_window_ms = WINDOW as f32 * 1000.0 / SAMPLE_RATE as f32; // 32 ms
        (ms as f32 / per_window_ms).ceil() as u32
    }
}

/// What the VAD decided after the latest audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEvent {
    /// An utterance began (speech onset confirmed).
    SpeechStart,
    /// The utterance ended — endpoint by silence.
    SpeechEnd,
    /// The utterance ended — forced by `max_utterance_ms`.
    MaxDurationCut,
}

#[derive(PartialEq)]
enum State {
    Silence,
    Speech,
}

/// A running Silero VAD session.
pub struct SileroVad {
    session: Session,
    state: Vec<f32>, // [2*1*128], carried between windows
    cfg: VadConfig,
    buf: Vec<f32>, // < WINDOW leftover samples
    fsm: State,
    speech_windows: u32,
    silence_windows: u32,
    total_windows: u32,
    min_silence_w: u32,
    min_utter_w: u32,
    max_utter_w: u32,
}

impl SileroVad {
    /// Load the model at `path` (the pinned `silero_vad.onnx`).
    ///
    /// # Errors
    /// [`AppError::internal`] if the ONNX Runtime library or the model cannot be
    /// loaded (see [`ensure_ort_dylib`]).
    pub fn new(path: &Path, cfg: VadConfig) -> AppResult<Self> {
        ensure_ort_dylib();
        let session = Session::builder()
            .and_then(|mut b| b.commit_from_file(path))
            .map_err(|e| AppError::internal("load silero_vad.onnx", e))?;
        Ok(Self {
            session,
            state: vec![0.0; 2 * 128],
            cfg,
            buf: Vec::with_capacity(WINDOW),
            fsm: State::Silence,
            speech_windows: 0,
            silence_windows: 0,
            total_windows: 0,
            min_silence_w: VadConfig::windows(cfg.min_silence_ms),
            min_utter_w: VadConfig::windows(cfg.min_utterance_ms),
            max_utter_w: VadConfig::windows(cfg.max_utterance_ms),
        })
    }

    /// Raise/lower the speech-onset threshold at runtime (echo duck while TTS
    /// plays — ADR-0005).
    pub fn set_onset_threshold(&mut self, t: f32) {
        self.cfg.onset_threshold = t.clamp(0.05, 0.98);
    }

    /// The configured onset threshold.
    #[must_use]
    pub fn onset_threshold(&self) -> f32 {
        self.cfg.onset_threshold
    }

    /// Reset the state machine + LSTM state for a fresh utterance.
    pub fn reset(&mut self) {
        self.state.iter_mut().for_each(|v| *v = 0.0);
        self.buf.clear();
        self.fsm = State::Silence;
        self.speech_windows = 0;
        self.silence_windows = 0;
        self.total_windows = 0;
    }

    /// Feed mono 16 kHz samples; returns any boundary events (in order).
    ///
    /// # Errors
    /// [`AppError::internal`] on an inference failure.
    pub fn push(&mut self, samples: &[f32]) -> AppResult<Vec<VadEvent>> {
        self.buf.extend_from_slice(samples);
        let mut events = Vec::new();
        while self.buf.len() >= WINDOW {
            let window: Vec<f32> = self.buf.drain(..WINDOW).collect();
            let prob = self.infer(&window)?;
            if let Some(ev) = self.advance(prob) {
                events.push(ev);
                if matches!(ev, VadEvent::SpeechEnd | VadEvent::MaxDurationCut) {
                    // The caller will `reset` and start a new utterance.
                    break;
                }
            }
        }
        Ok(events)
    }

    /// Force the current utterance to end now (hang-up while speaking). Returns
    /// `SpeechEnd` if an utterance was in progress and long enough.
    pub fn force_endpoint(&mut self) -> Option<VadEvent> {
        if self.fsm == State::Speech && self.speech_windows >= self.min_utter_w {
            self.fsm = State::Silence;
            Some(VadEvent::SpeechEnd)
        } else {
            None
        }
    }

    fn advance(&mut self, prob: f32) -> Option<VadEvent> {
        // Hysteresis: harder to start than to stay in speech, so a breath or a
        // quiet syllable (prob ~0.4) does not look like end-of-turn.
        let threshold = match self.fsm {
            State::Silence => self.cfg.onset_threshold,
            State::Speech => self.cfg.offset_threshold,
        };
        let is_speech = prob >= threshold;
        self.total_windows += 1;
        match self.fsm {
            State::Silence => {
                if is_speech {
                    self.speech_windows += 1;
                    self.silence_windows = 0;
                    // Onset confirmed after one speech window (Silero is smoothed).
                    self.fsm = State::Speech;
                    return Some(VadEvent::SpeechStart);
                }
                None
            }
            State::Speech => {
                if is_speech {
                    self.speech_windows += 1;
                    self.silence_windows = 0;
                } else {
                    self.silence_windows += 1;
                }
                if self.total_windows >= self.max_utter_w {
                    self.fsm = State::Silence;
                    return Some(VadEvent::MaxDurationCut);
                }
                if self.silence_windows >= self.min_silence_w {
                    self.fsm = State::Silence;
                    if self.speech_windows >= self.min_utter_w {
                        return Some(VadEvent::SpeechEnd);
                    }
                    // Too short — treat as noise, silently rearm.
                    self.speech_windows = 0;
                    self.silence_windows = 0;
                    self.total_windows = 0;
                }
                None
            }
        }
    }

    fn infer(&mut self, window: &[f32]) -> AppResult<f32> {
        let input = Tensor::from_array(([1_usize, WINDOW], window.to_vec()))
            .map_err(|e| AppError::internal("vad input tensor", e))?;
        let state = Tensor::from_array(([2_usize, 1, 128], self.state.clone()))
            .map_err(|e| AppError::internal("vad state tensor", e))?;
        let sr = Tensor::from_array(([1_usize], vec![i64::from(SAMPLE_RATE)]))
            .map_err(|e| AppError::internal("vad sr tensor", e))?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input" => input,
                "state" => state,
                "sr" => sr,
            ])
            .map_err(|e| AppError::internal("vad inference", e))?;

        let (_, prob) = outputs["output"]
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::internal("vad output extract", e))?;
        let p = prob.first().copied().unwrap_or(0.0);

        let (_, new_state) = outputs["stateN"]
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::internal("vad state extract", e))?;
        self.state.clear();
        self.state.extend_from_slice(new_state);

        Ok(p)
    }
}

/// Point `ort` at the ONNX Runtime shared library (ADR-0018 — `load-dynamic`).
/// Dev: the repo venv's `onnxruntime.dll`. Ship: a sibling of the exe. A caller
/// may override with `ORT_DYLIB_PATH` before first use.
pub fn ensure_ort_dylib() {
    if std::env::var_os("ORT_DYLIB_PATH").is_some() {
        return;
    }
    for cand in dylib_candidates() {
        if cand.is_file() {
            std::env::set_var("ORT_DYLIB_PATH", cand);
            return;
        }
    }
}

fn dylib_candidates() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let name = if cfg!(windows) {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    };
    // Next to the executable (shipped layout).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join(name));
        }
    }
    // The dev venv (ADR-0018).
    let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(root) = repo.parent() {
        let sub = if cfg!(windows) {
            root.join(".venv/Lib/site-packages/onnxruntime/capi")
        } else {
            root.join(".venv/lib")
        };
        out.push(sub.join(name));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("models/vad/silero_vad.onnx")
    }

    /// Load a fixture WAV as mono 16 kHz f32.
    fn load_wav_16k(name: &str) -> Vec<f32> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let mut reader = hound::WavReader::open(path).unwrap();
        let spec = reader.spec();
        let raw: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => reader
                .samples::<i32>()
                .map(|s| s.unwrap() as f32 / f32::from(i16::MAX))
                .collect(),
            hound::SampleFormat::Float => reader.samples::<f32>().map(Result::unwrap).collect(),
        };
        let mono: Vec<f32> = if spec.channels == 1 {
            raw
        } else {
            raw.chunks(spec.channels as usize)
                .map(|c| c.iter().sum::<f32>() / f32::from(spec.channels))
                .collect()
        };
        let mut conv = crate::voice::resample::ToMono16k::new(spec.sample_rate, 1);
        conv.push(&mono)
    }

    #[test]
    fn skips_without_the_model() {
        if !model_path().is_file() {
            eprintln!("no silero_vad.onnx — skipping");
        }
    }

    #[test]
    fn endpoints_a_spoken_utterance() {
        if !model_path().is_file() {
            return;
        }
        let mut vad = SileroVad::new(&model_path(), VadConfig::default()).unwrap();
        let speech = load_wav_16k("hello_localai.wav");
        // 0.5 s of near-silence appended so the silence endpoint fires.
        let mut audio = speech.clone();
        audio.extend(std::iter::repeat_n(0.0, SAMPLE_RATE as usize / 2));

        let mut events = Vec::new();
        for chunk in audio.chunks(WINDOW * 4) {
            events.extend(vad.push(chunk).unwrap());
            if events.contains(&VadEvent::SpeechEnd) {
                break;
            }
        }
        assert_eq!(events.first(), Some(&VadEvent::SpeechStart), "onset");
        assert!(
            events.contains(&VadEvent::SpeechEnd),
            "endpoint: {events:?}"
        );
    }

    #[test]
    fn silence_only_produces_no_event() {
        if !model_path().is_file() {
            return;
        }
        let mut vad = SileroVad::new(&model_path(), VadConfig::default()).unwrap();
        let silence = vec![0.0_f32; SAMPLE_RATE as usize * 2];
        let mut events = Vec::new();
        for chunk in silence.chunks(WINDOW * 8) {
            events.extend(vad.push(chunk).unwrap());
        }
        assert!(events.is_empty(), "silence: {events:?}");
    }
}
