# Phase 19 — Voice: Chatterbox TTS · playback · barge-in

> **Status: COMPLETE** (2026-09-06) — all 7 gate items pass. Evidence
> `docs/verification/19_phase19_voice-out.md`. Both halves done: **19.A** (the
> Rust half — `chunker` / `playback` / `resample` / `tts` / the barge-in state
> machine) and **19.B** (real `workers/tts.py` Chatterbox Turbo + a live gate on
> the RTX 5080 — **barge-in trigger → silence ≈ 4 ms**, warm RTF ≈ 0.45). Owner
> cleared the torch/Chatterbox download; `torch 2.11.0+cu128` overrides the
> `chatterbox-tts` pin of `torch==2.6.0` (pre-Blackwell).

> **Architecture frozen at Phase 5.** Governing: **ADR-0005** (Chatterbox Turbo,
> clause-chunked synthesis driven by LLM token arrival, `cpal` playback with a
> ~100–200 ms buffer, barge-in state machine), **ADR-0013** (stdio JSON-lines
> worker — reuses `worker::WorkerSupervisor` from Phase 18), **ADR-0015** (worker
> network lockdown), **ADR-0018** (dev venv), `docs/spec/AI_PIPELINES.md` §4,
> `docs/spec/PERFORMANCE.md` (barge-in ≤ ~200 ms). **No new ADR** — the
> `stop_reason = Cancelled` truncation signal is the frozen Phase 17 decision;
> `worker/` already exists.

## Objective
The output half of voice: assistant text → **clause-chunked** speech synthesis →
`cpal` playback, with **barge-in** — the user speaking (VAD onset) or pressing
stop halts the LLM generation **and** TTS **and** playback fast (≤ ~200 ms) and
returns to listening; the interrupted assistant turn is persisted with a clean
spoken prefix. TTS engine: **Resemble Chatterbox Turbo** (one pinned model,
already in `models/tts/`). This is the reliability-critical half.

## Depends on
Phase 18 (capture + Silero VAD — the barge-in trigger + full-duplex),
Phase 17 (`ConversationEngine::generate` + `cancel` — cancel already persists the
partial turn with `stop_reason = Cancelled`), Phase 14 (the TTS worker is a
managed subprocess — `worker::WorkerSupervisor`), Phase 12 gate 5 (model present).

## Not in this phase
- **Multi-voice / voice cloning UI** — one default voice (`conds.pt`).
  *(Lifted post-Phase-22 as an owner-requested course insert: `voice` table
  `V0008`, `voice::voices::VoiceRepo`, `voice_import`/`list`/`delete`/`set_active`
  IPC, Settings → Voices, `voice_wav` in the TTS worker payload. One global
  active voice; per-persona binding still Phase 26. See `ROADMAP.md` §7.)*
- **Emotion / style controls** beyond Chatterbox defaults.
- **Acoustic echo cancellation** — v1 assumes headphones or push-to-talk; during
  playback the VAD onset threshold is *ducked* (raised) to cut echo
  false-triggers (ADR-0005). Full AEC is out of v1.
- **A hands-free always-on loop** — voice-out is entered from a push-to-talk
  turn (Phase 18); `start_listening(model_id: Some)` then runs
  listen → think → speak → listen until `stop`. Always-on is a later opt-in.
- **Streaming partials from the LLM into a single utterance** — the chunker
  emits complete clauses; the last partial clause is flushed at stream end.

## Architecture notes

### Ownership (Article I)
- **Rust owns** the playback `cpal` stream, the PCM queue, the clause chunker,
  the TTS worker lifecycle, and the **interruption state machine**. All of it.
- **The Chatterbox worker** does synthesis only: one clause of text in → a WAV
  file (path dictated by Rust) out. Stateless between requests, killable, no DB,
  no network (ADR-0015 env), local model path only.
- **The frontend** shows the voice state (`Speaking` / `Interrupting` added) and
  a stop control — presentation only.

### Voice state machine (extends Phase 18's `VoiceState`)
```
Idle → Listening ⇄ Speech → Transcribing
                              → (model set) Thinking → Speaking → Listening   (loop)
                              → (no model)  Idle
Speaking → (VAD onset | stop) → Interrupting → Listening
any → (cancel_listening / shutdown) → Idle
```
`Thinking` = a generation is running, no audio yet. `Speaking` = playback is
active (synthesis may still be running ahead). `Interrupting` is transient.

### Barge-in — the 5-step sequence (ADR-0005), all Rust-side, measured
On VAD `SpeechStart` while `Speaking` **or** an explicit `stop`:
1. **Cancel the generation** — `engine.cancel(task_id)`. The engine's
   `run_generation` trips the LLM `CancellationToken` and **persists the
   accumulated assistant text with `stop_reason = Cancelled`** (Phase 16/17 — no
   change needed; that IS the truncation signal).
2. **Cancel TTS** — stop feeding the chunker; cancel any in-flight worker
   request (`WorkerSupervisor::request`'s `CancellationToken`).
3. **Stop playback** — `Playback::stop()` clears the PCM queue; the output stream
   goes silent within one buffer (~10–20 ms).
4. **Confirm** — await (1)–(3) acknowledgements (the generation task's join, the
   worker request's return, the queue drain).
5. **→ `Listening`**, and record trigger→silence.

The spoken text is a **clean clause-prefix** of the assistant message because the
chunker only ever emits complete clauses; step 1's persisted text is the full
generated-so-far text (a superset — the tail was generated but not yet spoken).
Recorded as the decision: *persisted = generated-so-far; spoken = a prefix of it*.
The next turn's context uses the persisted (truncated) text (already the case —
`repo.messages()` returns what was saved).

### New `voice/` files
| File | Responsibility |
| ---- | -------------- |
| `voice/playback.rs` | `Playback` — a `cpal` **output** stream on a parked thread (the stream is `!Send`), consuming an `Arc<Mutex<VecDeque<f32>>>` PCM queue. `open(device)`, `enqueue(&[f32])`, `stop()` (clear queue → silence), `queued_ms()`, `is_idle()`. Output at the device rate; TTS's 24 kHz is resampled up with `rubato` (generalise `resample.rs`). |
| `voice/chunker.rs` | `ClauseChunker { min_chars: 24, max_chars: 240 }` — `push(delta) -> Vec<String>` extracts complete clauses (split after `. ! ? … ; :` + space/EOL, or `\n`; force-flush at `max_chars`); `flush() -> Option<String>` for the tail. Pure, unit-tested against tricky inputs (`e.g.`, `3.14`, ellipses, code). |
| `voice/tts.rs` | `TtsOutput` — owns the TTS `WorkerSupervisor` + a `Playback`. `speak_stream(rx: mpsc::Receiver<String>, cancel) -> impl Future` — pulls clauses, calls the worker (one clause → a temp WAV → read → resample → `playback.enqueue`), keeps synthesis ~1 clause ahead, honours `cancel`. `cancel()` / `is_speaking()`. |

### `voice/mod.rs` changes
- `VoiceState` gains `Thinking`, `Speaking`, `Interrupting`.
- `VoiceInput` (kept name) `new(engine, stt, tts, cfg)` — now also holds a
  `TtsOutput`.
- `start_listening(conversation_id, model_id: Option<ModelId>)`:
  - `None` → Phase 18 behaviour (one utterance, then `Idle`).
  - `Some(model)` → the **conversational loop**: after `finish_utterance` adds
    the user turn, spawn `engine.generate(model, sink)` where `sink` forwards
    `TokenDelta` text into the chunker→TTS channel and `Done`/`Error` closes it;
    transition `Thinking` → (first audio) `Speaking`; **keep the capture stream +
    VAD running** for barge-in; on `Done` + playback drained → back to
    `Listening`; loop until `stop_listening`.
- Barge-in: the `run_session` loop already sees VAD events; a `SpeechStart` while
  `state == Speaking` runs the 5-step sequence instead of starting a new
  utterance capture.
- **Echo duck**: while `Speaking`, pass a raised `onset_threshold`
  (`cfg.vad.onset_threshold + cfg.vad.playback_duck`, default +0.2) to the VAD.

### TTS worker payload (this phase owns it — `docs/contracts.md`)
`WorkerRequest.payload` (`WorkerKind::Tts`):
```jsonc
{ "text": "one clause of assistant text.", "out_path": "<abs WAV path Rust dictates>" }
```
`WorkerResult::Ok.data`:
```jsonc
{ "sample_rate": 24000, "duration_s": 1.8 }
```
`workers/tts.py` — Chatterbox Turbo, loaded from `models/tts/` (env
`LOCALAI_TTS_MODEL_DIR`), CUDA; ADR-0015 env asserted; one clause per request;
writes a mono `s16le` WAV to `out_path`. `workers/tts_fake.py` — stdlib, writes a
short silent/sine WAV of a length proportional to the text, honours the protocol
(`ok` / `crash` / `slow` modes) — so `voice::` tests need no torch/GPU.

### Config (schema **v7**, additive — ADR-0016)
```
voice.output_device: string | null      // cpal output device; null = default
voice.vad.playback_duck: f32            // onset-threshold bump while speaking (0.2)
```
`ConfigKey::VoiceOutputDevice` (+ the duck is file-only). `WorkerSupervisor`
gets `LOCALAI_TTS_MODEL_DIR` like STT got its dir.

### Dev venv for 19.B (→ owner go-ahead)
Add to `workers/requirements.txt`: `torch` (CUDA 12.x build), `torchaudio`,
`chatterbox-tts` (or the turbo-capable release), `transformers`, `librosa`,
`s3tokenizer` — the exact set + pins finalised at 19.B.1 against the Turbo
loader. ~3–4 GB. Same `uv` venv (ADR-0018). CI stays on the fake (`torch` never
needed for `worker::` / `voice::` unit tests).

## Performance notes
- **Barge-in trigger→silence** — recorded; budget ≤ ~200 ms
  (`docs/spec/PERFORMANCE.md`). Playback buffer ~100–200 ms so `stop()` is
  near-instant; the dominant term is one VAD hop (32 ms) + the queue drain.
- **TTS time-to-first-audio** — recorded; target < ~500 ms after the first clause
  is ready (ADR-0005). Synthesis must stay ahead of playback (`queued_ms()`
  gate).
- **Full-duplex** — capture (Phase 18) and playback run on separate `cpal`
  streams / threads simultaneously; verified no device conflict.

## Steps

### 19.A — Rust half (fake TTS worker)

**19.A.1 — `chunker.rs`**
    Do:     `ClauseChunker` (pure). Split rules + `min_chars` / `max_chars`.
    Verify: `cargo test voice::chunker` — `"Hello world. How are you?"` → two
            clauses; `"e.g. this"` / `"3.14 is pi"` not split mid-token; a
            240-char run with no punctuation force-flushes; `flush()` returns the
            tail.

**19.A.2 — `playback.rs`**
    Do:     `Playback` — `cpal` output stream (parked thread), PCM queue,
            `enqueue` / `stop` / `queued_ms` / `is_idle`; 24 kHz → device-rate
            resample (`resample::to_rate`).
    Verify: `cargo test voice::playback` — enqueue N samples → `queued_ms`
            reflects it; `stop()` → queue empty + `is_idle()`; a 24 kHz sine fed
            through the resampler comes out at the device rate, right length.
            (`NoOutputDevice` cleanly in CI.)

**19.A.3 — `tts.rs` + fake worker**
    Do:     `workers/tts_fake.py` (stdlib). `TtsOutput` — supervisor + playback;
            `speak_stream(clause_rx, cancel)` pulls clauses → worker → temp WAV
            → resample → `enqueue`; ~1 clause lookahead; `cancel()` stops
            promptly.
    Verify: `cargo test voice::tts` — a scripted clause stream produces
            `queued_ms > 0`; `cancel()` mid-stream → playback stops, no further
            enqueue, the temp WAVs are cleaned; a `crash`-mode worker → typed
            error, playback stops, session recovers.

**19.A.4 — `VoiceState` + engine wiring (the loop)**
    Do:     `VoiceState::{Thinking, Speaking, Interrupting}`. `VoiceInput::new`
            takes a `TtsOutput`. `start_listening(id, model_id: Option<ModelId>)`
            — `Some` runs the loop (finish_utterance → `engine.generate` with a
            chunker-feeding sink → `Thinking` → `Speaking` → drain → `Listening`).
    Verify: `cargo test voice::` — with the fake STT + fake TTS + a scripted
            `LlmInstance`: one spoken WAV fixture → user turn → generated reply →
            `Speaking` observed → assistant turn persisted → back to `Listening`.

**19.A.5 — barge-in + echo duck**
    Do:     In `run_session`, `SpeechStart` while `Speaking` → the 5-step
            sequence (`engine.cancel` → `tts.cancel` → `playback.stop` → await →
            `Listening`), timed. Raise the VAD onset threshold by
            `playback_duck` while `Speaking`.
    Verify: `cargo test voice::` — mid-`Speaking`, inject a VAD onset →
            generation cancelled (`stop_reason = Cancelled` on the persisted
            turn), playback `is_idle()` within a tight bound, state `Listening`;
            the persisted text is a superset of what was "spoken" (clause
            prefix). An explicit `stop_listening` during `Speaking` does the
            same.

**19.A.6 — config v7 + IPC + frontend**
    Do:     config **v7** (`voice.output_device`, `voice.vad.playback_duck`) +
            `ConfigKey::VoiceOutputDevice` + step `6→7` + tests. IPC:
            `voice_output_devices()`; `voice_start` gains `modelId?`. `ChatVoice.tsx`
            — mic starts a *conversation* when a model is loaded; show
            `Thinking` / `Speaking`; Stop = barge-in. `setup.ts` mocks. Bindings.
    Verify: `node scripts/check.mjs` green; `ChatVoice.test.tsx` — speaking
            state renders, Stop calls the cancel path; `git diff --exit-code
            src/bindings`.

**19.A.7 — docs + partial gate + commit**
    Do:     `docs/verification/19_phase19_voice-out.md` (19.A rows PASS, 19.B
            rows `NOT EXECUTED — venv`); `src-tauri/README.md`,
            `docs/spec/ARCHITECTURE.md` §2/§3, `docs/contracts.md`, `ROADMAP.md`.
            Commit.
    Verify: check suite green; every 19.A gate item recorded.

### 19.B — Real Chatterbox worker + live gate  *(blocked: torch/Chatterbox venv, owner go-ahead)*

**19.B.1 — venv + `workers/tts.py`**
    Do:     add the Chatterbox stack to `workers/requirements.txt` (pins fixed
            against the Turbo loader); `scripts/setup-venv.mjs` reruns.
            `workers/tts.py` — load `models/tts/` once, synth one clause →
            `out_path` WAV, report sample rate.
    Verify: `uv run python -c "import torch, chatterbox"` ok, CUDA available;
            piping one `WorkerRequest` yields a playable WAV.

**19.B.2 — live speak + barge-in gate (RTX 5080)**
    Do:     `voice::live_tests` extended — real TTS worker + real playback:
            speak a known paragraph, assert audio plays (RMS > 0 on a capture of
            the output, or duration ≈ expected); measure **time-to-first-audio**.
            Then inject a VAD onset mid-playback, assert **trigger→silence** and
            the truncated turn. Plus a **manual** `deploy-local` run — hold mic,
            get a spoken reply, talk over it — screenshot + the log.
    Verify: first-audio + barge-in latencies recorded; truncated turn correct;
            worker-crash / missing-model / no-output-device paths typed.

**19.B.3 — close the gate**
    Do:     fill the 19.B rows; `ROADMAP.md` Phase 19 → `COMPLETE`; commit.
    Verify: all 7 gate items PASS with evidence.

## Verification gate (physically executed)
1. Assistant text is spoken via Chatterbox and plays through the selected output
   device. *(19.A.4 fake, 19.B.2 real)*
2. Speaking over the assistant (or pressing stop) halts LLM + TTS + playback and
   returns to listening. *(19.A.5, 19.B.2)*
3. Trigger→silence latency measured and within budget (~200 ms). *(19.A.5 bound,
   19.B.2 real)*
4. The interrupted assistant turn is persisted (truncated — `stop_reason =
   Cancelled`), spoken text a clean clause-prefix; next turn's context uses it.
   *(19.A.5)*
5. TTS worker crash / missing model / lost output device → typed error, no hang.
   *(19.A.3, 19.B.2)*
6. Capture and playback run concurrently without conflict. *(19.A.4/.5, 19.B.2)*
7. TTS time-to-first-audio recorded. *(19.B.2)*

## ADRs / open questions
- **No new ADR.**
- **RESOLVED — truncation:** `stop_reason = Cancelled` on the persisted turn is
  the signal (frozen Phase 17). Persisted = generated-so-far; spoken = a clause
  prefix of it.
- **Echo (open-mic):** headphones / push-to-talk assumed for v1; VAD onset ducked
  while `Speaking`. AEC deferred.
- **Download to clear with the owner (19.B):** PyTorch (CUDA) + `torchaudio` +
  the Chatterbox Turbo stack + `transformers` / `librosa` — **~3–4 GB** into the
  existing `.venv`.
- **Open:** exact Chatterbox-Turbo loader / package version — pinned at 19.B.1.
