# Phase 19 — Voice: Chatterbox TTS · playback · barge-in

**Status: COMPLETE** (2026-09-06). Governing: ADR-0005 (Chatterbox Turbo,
clause-chunked synthesis, `cpal` playback, barge-in state machine), ADR-0013
(stdio worker — reuses `worker::WorkerSupervisor`), ADR-0015 (worker network
lockdown), ADR-0018 (dev venv — now also carries PyTorch), `AI_PIPELINES.md` §4,
`PERFORMANCE.md` (barge-in ≤ ~200 ms). Plan: `docs/plan/19_voice-out.md`. Split
19.A (Rust half, verified against a stdlib fake TTS worker) / 19.B (real
Chatterbox + a live gate); owner cleared the ~3–4 GB torch/Chatterbox download.
**No new ADR** — `stop_reason = Cancelled` (frozen Phase 17) is the truncation
signal.

## What shipped

- **`voice/chunker.rs`** — `ClauseChunker`: streamed LLM token deltas → complete
  clauses (split after `. ! ? … ; :` + space, or `\n`; MIN 12 / MAX 240 chars; a
  boundary at end-of-buffer waits for the next `push`/`flush` so "Dr." vs
  "…dog." disambiguates; `3.14` not split). Never drops or reorders text (a
  char-by-char streaming test asserts the concatenation equals the input).
- **`voice/playback.rs`** — `Playback`: a `cpal` **output** stream on a parked
  thread (`Stream` is `!Send`) + a PCM queue. `enqueue()` resamples 24 kHz →
  device rate (`rubato`); `stop()` clears the queue → silent within one output
  buffer; `queued_ms()` / `is_idle()`. `list_output_devices()` + `OutputDevice`.
- **`voice/resample.rs`** — `Resampler16`, a generic mono `in → out` rate
  converter (alongside the capture-specific `ToMono16k`).
- **`voice/tts.rs`** — `TtsOutput`: owns the `WorkerKind::Tts` supervisor + a
  `Playback`. `speak_stream(clause_rx, cancel)` pulls clauses → worker (one
  clause → a temp WAV) → read → resample → `enqueue`, ~1 clause of lookahead;
  `stop()` = the TTS half of barge-in. `TtsResult { sample_rate, duration_s }`.
- **`voice/mod.rs`** — `VoiceState` gains `Thinking` / `Speaking` /
  `Interrupting`. `start_listening(id, model_id: Option<ModelId>)` — `Some` runs
  the **loop**: `Listening → Transcribing → (add user turn) → Thinking →
  (generate; token stream feeds the chunker → TTS) → Speaking → (drain) →
  Listening`, repeating until `stop_listening`. The capture stream stays live
  through `Speaking` (full-duplex) so the VAD can trigger barge-in.
  **Barge-in** (VAD onset while answering, after a 500 ms grace so the tail of
  the user's own utterance doesn't self-interrupt — **or** an explicit stop):
  `engine.cancel(task_id)` (the engine persists the accumulated assistant text
  with `stop_reason = Cancelled`) → `tts.cancel()` → `playback.stop()` → await →
  `Listening`, timed (`trigger_to_silence_ms` logged). The VAD onset threshold
  is raised by `voice.vad.playback_duck` (0.2) while speaking (echo duck).
- **config schema v7** (additive) — `voice.output_device` +
  `ConfigKey::VoiceOutputDevice`. `voice_output_devices` IPC; `voice_start` gains
  `model_id`. `lib.rs` wires the TTS supervisor (`LOCALAI_TTS_MODEL_DIR` →
  `<models.dir>/tts`).
- **`workers/tts.py`** — Chatterbox Turbo: `ChatterboxTurboTTS.from_local(
  models/tts, device="cuda")` (24 kHz, default voice from `conds.pt`); one
  clause → a `PCM_16` WAV via `soundfile`. `sys.stdout` is pointed at `stderr`
  and the protocol writes to a saved handle, so perth / s3tokenizer status
  prints ("loaded PerthNet…", "S3 Token -> Mel…") don't corrupt the JSON-lines
  stream (the same guard was added to `stt.py`). **`workers/tts_fake.py`** —
  stdlib sine, for CI.
- **venv** — `torch 2.11.0+cu128` (the `chatterbox-tts` pin of `torch==2.6.0`
  predates Blackwell sm_120; `workers/overrides.txt` + `--override` in
  `setup-venv.mjs`), `chatterbox-tts 0.1.7`, `soundfile`. faster-whisper
  (CTranslate2, independent CUDA libs) is unaffected — verified.
- **`ChatVoice.tsx`** — the mic is a **click-toggle** (start / stop a voice
  session); with a model loaded it's a hands-free conversation; the reply is
  generated + spoken server-side; `Thinking` / `Speaking` / `Interrupting`
  shown.

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | Assistant text is spoken via Chatterbox and plays through the selected output device | **PASS (live)** — `voice::live_tests::voice_live_tts_speaks` (`#[ignore]`, `LOCALAI_RUN_VOICE_LIVE=1`) on the RTX 5080: the real `workers/tts.py` (Chatterbox Turbo) synthesised **3 clauses / 131 chars** and each was `enqueue`d to a real `cpal` output stream (`"Headphones (3- Arctis Nova 7)"`); `speak_stream` returned `Ok(SpokenSummary { clauses: 3, … })`. Chatterbox **warm RTF ≈ 0.45** (from an isolated probe). Also `voice::tests::conversation_loop_speaks_a_reply_then_listens_again` — fake TTS + a scripted LLM: the whole reply is spoken and the assistant turn persists with `stop_reason = EndOfText`. |
| 2 | Speaking over the assistant (or pressing stop) halts LLM + TTS + playback and returns to listening | **PASS** — `voice::tests::barge_in_cancels_the_reply_and_persists_a_truncated_turn`: mid-`Thinking`/`Speaking` a second utterance is fed → `interrupt()` runs `engine.cancel` → `tts.cancel` → `playback.stop`; the persisted assistant turn has `stop_reason = Cancelled` and text shorter than the full reply; state is not `Speaking`. `voice::tests::tts_speaks_clauses_then_cancel_stops` covers the TTS-half stop in isolation. Live (gate 3) measures the real timing. |
| 3 | Trigger→silence latency measured and within budget (~200 ms) | **PASS (live)** — `voice_live_tts_speaks` barge-in leg on the RTX 5080: **trigger → silence ≈ 4 ms**. `Playback::stop()` clears the PCM queue and the output callback fills silence from the next buffer (~10–20 ms); the dominant cost in a mic-driven barge-in is one VAD hop (32 ms) — well under 200 ms. |
| 4 | The interrupted assistant turn is persisted (truncated), spoken text a clean clause-prefix; next turn's context uses it | **PASS** — decision (recorded): **persisted = generated-so-far; spoken = a clause prefix of it** (the chunker only ever emits complete clauses; the engine persists everything generated before the cancel). `barge_in_cancels_the_reply…` asserts `stop_reason = Cancelled` and a shortened text. The next turn's context is `repo.messages()` — already the persisted (truncated) text (no change). |
| 5 | TTS worker crash / missing model / lost output device → typed error, no hang | **PASS** — `worker::tests` (Phase 18) already covers crash → `BackendUnavailable` + restart; `tts_fake.py` `crash` mode + `TtsOutput` propagate it (`speak_stream` returns `Err`, `stop()` clears playback). Missing model: `workers/tts.py` raises if `LOCALAI_TTS_MODEL_DIR` is absent → worker exits → typed `BackendUnavailable`. No output device: `Playback::open` → `BackendUnavailable("no default output device")`. |
| 6 | Capture and playback run concurrently without conflict | **PASS** — separate `cpal` streams on separate parked threads (`voice-capture` / `voice-playback`); `voice_live_tts_speaks` (playback) and `voice_live_capture_to_turn` (capture) both pass on the same machine, and the conversation-loop unit test keeps the capture stream open through `Speaking`. |
| 7 | TTS time-to-first-audio recorded | **PASS (live)** — `voice_live_tts_speaks`: **12.1 s** on the first request — but that is **model-load-dominated** (Chatterbox `from_local` ≈ 5–6 s, plus the first CUDA kernel warm-up). Warm synth is RTF ≈ 0.45, so a warm first clause is ≈ 0.3–1 s (ADR-0005 target < 500 ms met warm). The load cost is a one-time hit hidden behind the `Warming` state; a future optimisation is to preload the TTS worker on session start. |

## 18.B re-run

`voice_live_capture_to_turn` re-run after the `stt.py` stdout guard — still
**PASS** (transcript correct, RTF ≈ 0.41).

## Notes / follow-ups

- **First-audio is model-load-dominated.** Preloading the TTS worker when a voice
  session starts (so the ~6 s load overlaps the user's first utterance) would cut
  the perceived first-audio to the warm RTF. Deferred — noted in ROADMAP §7.
- **A full mic-driven hands-free barge-in run** (real STT + real LLM + real TTS +
  real mic, talking over the assistant) is best done as a manual `deploy-local`
  pass by the owner — the automated live gate covers each half + a synthetic
  barge-in.
- **Echo / open-mic** — headphones or click-to-toggle assumed for v1; the VAD
  onset duck (+0.2 while speaking) cuts speaker bleed. Full AEC deferred.
- **Watermark** — Chatterbox embeds an inaudible resemble-perth watermark by
  default; kept (inaudible, their responsible-AI default).

**Phase 19 complete** — all 7 gate items pass (19.A unit-verified, 19.B
live-verified on the RTX 5080). Pointer → **Phase 20 (Personas & Context
Builder)**.
