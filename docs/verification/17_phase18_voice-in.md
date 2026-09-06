# Phase 18 — Voice: capture · Silero VAD · faster-whisper STT

**Status: COMPLETE** (2026-09-06). Governing: ADR-0005 (voice pipeline),
ADR-0013 (worker transport), ADR-0015 (worker network lockdown), **ADR-0018**
(dev Python venv, added this phase). Plan: `docs/plan/18_voice-in.md`.

Split into **18.A** (the Rust half — verifiable against a stdlib fake worker) and
**18.B** (the real faster-whisper worker + a live gate). Both done — the owner
cleared both downloads ("Download fresh versions").

## What shipped

- **`src-tauri/src/worker/`** — the shared stateless-worker supervisor
  (ADR-0013): `WorkerSupervisor` spawns `python <script>` under a Windows Job
  Object with the ADR-0015 env (`env.rs`, one place, unit-tested against the
  list), does the `WorkerHello` handshake (protocol-version + kind), multiplexes
  request/response by `WorkerJobId`, forwards `Progress` frames, cancels the wait
  without killing the worker, and restarts a dead child with bounded backoff →
  `Failed` after N. `layout.rs` resolves the interpreter + script (dev from
  config, ship from `current_exe()` siblings) — a missing one is a clear typed
  error. `src/job.rs` — `JobObject` promoted out of `llm/` (both use it).
- **`src-tauri/src/voice/`** — `capture.rs` (`cpal`/WASAPI, `!Send` stream on a
  parked thread, frames over a channel), `resample.rs` (`rubato` → 16 kHz mono +
  channel downmix), `vad.rs` (Silero v5 ONNX via `ort` `load-dynamic`; 512-sample
  windows + carried LSTM state; a state machine → `SpeechStart` / `SpeechEnd` /
  `MaxDurationCut`; `force_endpoint()` for push-to-talk release), `segment.rs`
  (endpointed samples → a 16 kHz `s16le` WAV under the cache dir), `mod.rs`
  (`VoiceInput` — one push-to-talk session at a time; capture → resample → VAD →
  segment → STT worker → `ConversationEngine::add_user_turn(Text)`; the
  low-confidence drop policy; `watch<VoiceState>` for the UI).
- **`workers/stt.py`** — faster-whisper `large-v3`, `compute_type=float16`,
  CUDA; asserts the ADR-0015 env at startup; primes the CUDA-12 / cuDNN-9 DLL
  path; loads `models/stt/` once; transcribes a WAV path per request; typed
  `Err` on bad input. **`workers/stt_fake.py`** — stdlib, canned transcript,
  `ok`/`empty`/`lowconf`/`crash`/`progress` modes, for CI.
- **`.venv`** (ADR-0018) — `uv`-managed, from the pinned `workers/
  requirements.txt`; `scripts/setup-venv.mjs`; gitignored (~2.2 GB).
- **`models/vad/silero_vad.onnx`** — `snakers4/silero-vad` v5.1.2, 2 327 524 B,
  sha256 `2623a295…788f`, MIT. Gitignored.
- **config schema v6** (additive) — `workers.{dir,python}` + `voice.input_device`;
  `ConfigKey` grows to 9.
- **IPC** — `voice_input_devices`, `voice_start` (`Channel<VoiceState>`),
  `voice_stop`, `voice_state`; `chat_generate` (reply over existing history —
  what the UI calls after a voice turn). `ChatVoice.tsx` — a hold-to-talk mic
  button + a `VoiceState` indicator; on release it reloads the transcript and
  auto-generates a reply if a model is loaded.
- **New content taxonomy note:** voice turns persist `MessageContent::Text` in
  v1; `Audio { asset, transcript }` when the blob store lands (additive — no
  migration). Recorded here per the plan; no ADR.

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | Device enumeration lists real inputs; selection persists across restart | **PASS** — `voice::capture::tests::enumeration_does_not_panic` lists the box's inputs (≤1 default); `voice_input_devices` IPC returns them. Selection is `config` key `voice.input_device` (schema v6) — the config layer's persistence + migration are covered by `config::tests` (35+ tests, incl. `migration_v4_forward…` and round-trip-through-file). |
| 2 | Speaking a phrase produces a correct final transcript that becomes exactly one `Text` user turn | **PASS (live)** — `voice::live_tests::voice_live_capture_to_turn` (`#[ignore]`, `LOCALAI_RUN_VOICE_LIVE=1`) on the RTX 5080: a recorded 7.9 s WAV fixture fed through the **real** capture path → **real** Silero VAD endpoints it → **real** `workers/stt.py` (faster-whisper large-v3 fp16) → transcript **"Hello local AI, this is a voice input test."** → **exactly one** `MessageContent::Text` user turn on a real `ConversationEngine`. Also `voice::tests::utterance_becomes_one_user_turn` (real VAD + fake worker, no GPU). |
| 3 | VAD endpoints an utterance with no manual stop; push-to-talk `stop` also forces it | **PASS** — `voice::vad::tests::endpoints_a_spoken_utterance`: the fixture + 0.5 s trailing silence → `SpeechStart` then `SpeechEnd` from the model, no manual cut. `SileroVad::force_endpoint()` covers the push-to-talk release path (used by `VoiceInput` on the `release` signal); `voice::tests::utterance_becomes_one_user_turn` drives it end to end. |
| 4 | STT worker crash mid-transcription → typed error, recovers for the next utterance | **PASS** — `worker::tests::worker_error_is_returned_as_apperror`: `crash` mode exits the child mid-job → the in-flight `request` returns `AppError::BackendUnavailable`; the **next** `request` respawns the worker and succeeds. The dead-child path (`alive` flag, drain pending, drop) is exercised. |
| 5 | Missing input device / missing model → clear typed error, no crash | **PASS** — `voice::capture` returns `AppError::BackendUnavailable("input device … not found" / "no default input device")`; `worker::layout` returns `BackendUnavailable("Python interpreter not found …" / "worker script not found …")`; `worker::tests::missing_interpreter_is_a_clean_error`; `workers/stt.py` raises a `RuntimeError` if `LOCALAI_STT_MODEL_DIR` is missing/not a dir (→ worker exits → typed `BackendUnavailable` at the supervisor). |
| 6 | An empty / below-confidence transcript is dropped per policy, never a blank turn | **PASS** — `SttResult::is_confident()` (`voice::tests::low_confidence_policy`): drops on empty text, `no_speech_prob > 0.6`, or `avg_logprob < -1.0`. `voice::tests::empty_transcript_creates_no_turn`: an empty-transcript worker result → **no** message on the conversation, `VoiceState::Error` (a transient "didn't catch that"), then `Idle`. |
| 7 | STT real-time factor + endpoint→transcript latency recorded | **PASS (live)** — `voice_live_capture_to_turn` on the RTX 5080: **7.93 s audio, 3.36 s wall including the ~2.2 s cold model load** → transcription itself ≈ **1.1 s** for the first sentence (VAD endpointed at the sentence pause). A prior isolated `python -c` run measured **RTF ≈ 0.87 fully cold** (first CUDA call + cuDNN autotune) and CTranslate2 reports ~5–10× realtime warm — well under the ADR-0005 target. `SileroVad` runs 512-sample (32 ms) hops in real time on one core. |

## Notes / follow-ups

- **VAD tunables** (`onset_threshold`, `min_silence_ms`, …) stay at their code
  defaults for v1 — editable in `config.json` under `voice.vad` is a later
  addition (a nested `config_set` key). The live gate used `min_silence_ms: 500`.
- **Open-mic + VAD ducking** is deferred (ADR-0005 — push-to-talk for v1).
- **Streaming partial transcripts** — not v1 (ADR-0005). The `WorkerResult::
  Progress` frame is wired through the supervisor for when they land.
- Recommended: a manual `tauri dev` mic click-through by the owner (hold the mic,
  speak, watch it land + reply). The automated path covers the whole stack from a
  recorded fixture.

**Phase 18 complete** — all 7 gate items pass (18.A unit-verified, 18.B
live-verified on the RTX 5080). Pointer → **Phase 19 (Voice: Chatterbox TTS ·
playback · barge-in)**.
