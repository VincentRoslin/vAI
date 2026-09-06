# Phase 18 — Voice: capture · Silero VAD · faster-whisper STT

> **Status: FINALIZED AT PHASE ENTRY** (2026-09-06). Step detail below is derived
> against the frozen architecture. The phase is **split** (precedent: Phase 15):
> **18.A** — the Rust half (capture · VAD · worker supervisor · engine wiring),
> verifiable now against a **fake STT worker**; **18.B** — the real faster-whisper
> Python worker + the live capture→transcript gate, which needs the dev Python
> environment stood up (**ADR-0018**, + owner go-ahead for the ~1 GB of
> CUDA/cuDNN wheels).

> **Architecture frozen at Phase 5.** Governing: **ADR-0005** (faster-whisper
> fp16, Silero VAD in the Rust core, `cpal` 16 kHz mono, transcribe per
> VAD-endpointed segment — not true streaming), **ADR-0013** (stdio JSON-lines
> worker + the shared Rust worker-supervisor actor), **ADR-0015** (worker network
> lockdown env), **ADR-0014** (embedded CPython + shared venv is the *ship*
> story; dev uses `uv` — ADR-0018 ratifies), `docs/spec/AI_PIPELINES.md` §2–3,
> `docs/contracts.md` (`contracts::worker`).

## Objective
The input half of voice: microphone capture → voice-activity detection
(endpointing) → speech-to-text → a **user turn on the shared conversation
engine** (`ConversationEngine::add_user_turn`), the same path typed input takes.
STT engine: **faster-whisper** `large-v3`, `compute_type=float16`, one pinned
model (already acquired — `models/stt/`, Phase 12 gate 5). VAD: **Silero** ONNX in
the Rust core. Interaction model: **push-to-talk** for v1 (open-mic deferred).

## Depends on
- Phase 17 (conversation engine — `add_user_turn(MessageContent)` is the seam;
  `render_chatml` already renders `Audio { transcript: Some }`).
- Phase 14 (lifecycle manager — the STT worker is a Rust-supervised subprocess;
  the worker supervisor is a *sibling* of the model-backend supervisor, not the
  same actor — see Architecture notes).
- Phase 13 (resource manager — STT holds ~3–4 GB VRAM; a reservation is taken
  around a loaded worker).
- Phase 12 gate 5 (`models/stt/` present).

## Not in this phase
- **TTS / playback / barge-in** — Phase 19 (barge-in reuses this phase's VAD
  onset signal).
- **Wake-word / always-on / open-mic** — push-to-talk only for v1.
- **Streaming partial transcripts** — ADR-0005 makes them optional and not
  v1-required; we transcribe once per endpointed segment. The contract leaves
  room (a `partial` progress frame) but the worker does not emit one in v1.
- **Diarization, multi-language auto-detect UI** — `large-v3` auto-detects
  language internally; no UI for it.
- **The embedded/bundled Python runtime** — Phase 37. Dev venv only here.
- **A general blob store** — a final transcript is `MessageContent::Text` in v1
  (see "Message content" below); raw-audio retention is a Phase 22 concern.

## Architecture notes

### Ownership (Article I)
- **Rust owns** the capture stream, device enumeration + selection, the audio
  ring buffer, resampling, VAD (onset/endpoint decisions), the worker process
  lifecycle, and the transcript→turn hand-off. All of it.
- **The Python worker** does STT inference only: reads a request line, loads the
  local model once, transcribes a PCM buffer handed to it via a path Rust
  dictates, writes a response line. No state between requests, killable at any
  moment, no DB, no network (ADR-0015 env), no repo-id resolution (local path
  only).
- **The frontend** shows device pick, a push-to-talk control, VAD/recording
  state, and the (final) transcript landing as a user bubble — presentation only,
  over typed IPC + one `Channel` for capture/VAD state.

### New Rust module: `src-tauri/src/voice/`
| File | Responsibility |
| ---- | -------------- |
| `mod.rs` | `VoiceInput` service — owns the capture→VAD→STT→turn orchestration; `start_listening` / `stop_listening` (push-to-talk), `state()`; one `tokio::sync::Mutex<Inner>`. Rejects a second concurrent utterance. |
| `capture.rs` | `cpal` input-stream management: `list_input_devices()`, `CaptureStream` (open by device id, 16 kHz mono f32; `rubato` resample when the device rate differs), a lock-free producer into an `AudioRing`. |
| `ring.rs` | `AudioRing` — fixed-capacity sample ring (a few seconds); the VAD loop drains it; a segment is copied out on endpoint. |
| `vad.rs` | `SileroVad` — `ort` (ONNX Runtime) session over the pinned Silero model; `push(&[f32]) -> VadEvent` (`SpeechStart` / `SpeechEnd` / `Continue`); tunables from config; a `max_utterance` hard cut. |
| `segment.rs` | endpointed-segment assembly (pre-roll padding, WAV/PCM write to a temp path under the app cache dir). |

### New Rust module: `src-tauri/src/worker/`
The **shared worker-supervisor actor** ADR-0013 mandates — generic, reused by TTS
(19) and the embedder (27).
| File | Responsibility |
| ---- | -------------- |
| `mod.rs` | `WorkerSupervisor` — spawn `python.exe <script>` with the ADR-0015 env + a Job Object; read the `WorkerHello` line, check `protocol_version` against `WORKER_PROTOCOL_VERSION`; a request/response map keyed by `WorkerJobId`; `stderr` → `tracing` at `debug`; bounded-backoff restart → `Failed` after N; `request(payload, cancel) -> AppResult<Value>`; `Progress` frames forwarded to an optional sink. |
| `proto.rs` | thin serde helpers over `contracts::worker` (`WorkerRequest` / `WorkerResponse` / `WorkerResult`); newline framing; `flush` discipline documented for the Python side. |
| `env.rs` | `hostile_network_env()` — the exact ADR-0015 var set, one place, unit-tested against the ADR list. |
| `layout.rs` | resolve `python/` + `workers/` siblings from `current_exe()` in a packaged build; in dev, from the repo (`.venv/Scripts/python.exe`, `workers/`). Config override: `voice.python_path` is **not** added — `runtimes.dir` pattern is for native servers; the worker layout is `workers.dir`-adjacent. **Add `config` key `workers.python` + `workers.dir`** (schema v6) so dev can point at the venv. |

### Worker payload schema (this phase owns it; `docs/contracts.md` gets a row)
`WorkerRequest.payload` for STT:
```jsonc
{ "audio_path": "<abs path to a 16kHz mono s16le WAV>", "language": null }
```
`WorkerResult::Ok.data` for STT:
```jsonc
{ "text": "…", "language": "en", "duration_s": 2.4, "avg_logprob": -0.31,
  "no_speech_prob": 0.02 }
```
Low-confidence policy (Rust): drop the turn when `text.trim()` is empty **or**
`no_speech_prob > 0.6` **or** `avg_logprob < -1.0` (tunable consts; recorded as
the decision). A dropped utterance surfaces a transient "didn't catch that"
state, never a blank turn.

### Message content
The final transcript becomes `add_user_turn(MessageContent::Text { text })` in
v1. `MessageContent::Audio { asset, transcript }` is the eventual shape, but it
needs the blob store (not in this phase — `AI_PIPELINES.md` defers it to the
first blob feature). **Decision:** v1 voice-in persists `Text`; when the blob
store lands, a migration is not required (additive — audio turns start carrying
`Audio`). Recorded in the verification doc.

### Resource interaction
The STT worker, once it has loaded the model, holds ~3–4 GB VRAM. `VoiceInput`
takes a `ResourceManager` reservation (`ResourceRequest { kind: Stt, … }`) when
it starts the worker and releases it on worker stop/crash. STT **coexists** with
the LLM (no eviction — ADR-0005). The worker is started lazily on the first
`start_listening` and kept warm; an idle timeout unloads it (config
`voice.stt_idle_timeout_s`, default 300).

### Config additions (schema **v6**, forward step `5 → 6`, additive — ADR-0016)
```
voice: {
  input_device: string | null,        // cpal device name; null = system default
  vad: {
    onset_threshold: f32,             // 0.5
    min_silence_ms: u32,              // 700
    min_utterance_ms: u32,            // 250
    max_utterance_ms: u32,            // 30000
    pre_roll_ms: u32,                 // 300
  },
  stt_idle_timeout_s: u32,            // 300
}
workers: { dir: PathBuf, python: PathBuf }   // dev: <repo>/workers, <repo>/.venv/Scripts/python.exe
```
New `ConfigKey`s: `VoiceInputDevice`, `WorkersDir`, `WorkersPython` (+ the VAD
scalars are file/session only, not surfaced as flat keys in v1 — nested set is a
later config feature; defaults + file editing suffice).

### Dev Python environment (→ **ADR-0018**)
- **`uv`-managed venv at `<repo>/.venv`** (DEVELOPMENT.md already commits to this;
  ADR-0014 covers ship). `uv` installed to the user profile.
- `workers/requirements.txt` (pinned): `faster-whisper==<pin>` → pulls
  `ctranslate2`, `tokenizers`, `onnxruntime`, `av`, `huggingface-hub`. CUDA-12
  execution needs `nvidia-cudnn-cu12` + `nvidia-cublas-cu12` wheels (~0.8–1 GB).
- The **owner go-ahead** is required for that download (CLAUDE.md — large asset;
  ~1 GB of wheels). Until then 18.B is `BLOCKED`, 18.A proceeds against a fake
  worker (`workers/stt_fake.py`, stdlib only — echoes a canned transcript, honours
  the protocol).
- `scripts/setup-venv.mjs` (or a documented `uv` command) creates `.venv` +
  installs; `.venv/` gitignored.

### Silero VAD model
Pinned ONNX (`snakers4/silero-vad`, `silero_vad.onnx`, ~2 MB, MIT). **Small — but
still a download; ask.** Fetched via the same acquisition engine (a new
`FixedModel::Vad`? — no: it is a *code asset*, not a user model; put it under
`models/vad/silero_vad.onnx` via a tiny one-shot fetch in the setup script, or
add `acquire_fixed(Vad)` if that is cleaner). **Decision at 18.A.3.**

### `ort` crate
`ort` (ONNX Runtime bindings) with `download-binaries` **off** — link the
onnxruntime shared lib the venv's `onnxruntime` wheel ships, or a pinned
standalone. In dev, `load-dynamic`. Recorded in ADR-0018.

## Performance notes
- **STT real-time factor** (audio seconds ÷ wall seconds) recorded for `large-v3`
  fp16 on the RTX 5080 alongside the loaded LLM. Target ≪ 1.0 (ADR-0005 expects
  ~5–10×).
- **Endpoint→transcript latency** (VAD `SpeechEnd` → transcript in hand) recorded.
- **Capture→VAD-decision latency** (frame in → `VadEvent`) recorded; the VAD loop
  runs on 30 ms hops, must keep up in real time on one core.
- First-utterance cost includes the worker's model load (~2–5 s) — measured
  separately and hidden behind a "warming up" state.

## Steps

### 18.A — Rust half (fake worker)

**18.A.1 — `worker/` supervisor + `env.rs` + protocol**
    Do:     new `src-tauri/src/worker/` (`mod`, `proto`, `env`, `layout`).
            `WorkerSupervisor::spawn(script, kind)` with Job Object + ADR-0015
            env; `WorkerHello` handshake + version check; `request()` with a
            `WorkerJobId` map, per-call `CancellationToken`, `Progress` sink;
            bounded restart → `Failed`. `stderr` → `tracing`.
    Verify: `cargo test worker::` — against `workers/stt_fake.py` (stdlib):
            handshake ok; a request round-trips; a protocol-version mismatch is
            refused; killing the child mid-request → typed error + one restart;
            `env.rs` matches the ADR-0015 list exactly.

**18.A.2 — `cpal` capture + ring + resample**
    Do:     `voice/capture.rs` + `voice/ring.rs`. `list_input_devices()`;
            `CaptureStream::open(device, 16_000)`; `rubato` when the native rate
            differs; producer into `AudioRing`.
    Verify: `cargo test voice::capture` — enumeration returns ≥1 device on the
            dev box (or `NoInputDevice` cleanly in CI); a synthetic 48 kHz sine
            fed through the resampler comes out 16 kHz at the right length; ring
            wrap-around keeps the last N samples.

**18.A.3 — Silero VAD (`ort`) + model fetch**
    Do:     acquire the pinned `silero_vad.onnx` (**ask owner** — ~2 MB; via a
            setup-script fetch or `acquire_fixed`). `voice/vad.rs`:
            `SileroVad::new(path, VadConfig)`, `push(&[f32]) -> VadEvent`, state
            reset, `max_utterance` cut.
    Verify: `cargo test voice::vad` — a WAV fixture with a known
            speech/silence layout produces `SpeechStart` then `SpeechEnd` within
            tolerance of the annotated boundaries; silence-only → no event;
            a too-short blip is rejected by `min_utterance_ms`.

**18.A.4 — segment assembly + temp file**
    Do:     `voice/segment.rs` — on `SpeechEnd`, copy `pre_roll + speech` out of
            the ring, write `s16le` mono 16 kHz WAV to a temp path under the app
            cache dir; hand the path to the supervisor; delete after the response.
    Verify: `cargo test voice::segment` — a written WAV re-reads to the expected
            duration/format; the temp file is gone after a (fake) transcription.

**18.A.5 — `VoiceInput` orchestration + engine wiring**
    Do:     `voice/mod.rs` — `VoiceInput::new(engine, supervisor, resources,
            config)`; `start_listening(conversation_id)` /
            `stop_listening()` (push-to-talk: `stop` also forces an endpoint);
            capture→VAD→segment→worker→`add_user_turn(Text)` on success; the
            low-confidence drop policy; a `Channel<VoiceState>` for the UI
            (`Idle` / `Warming` / `Listening` / `Speech` / `Transcribing` /
            `Error`). Reject a second concurrent utterance. Idle-timeout unload +
            resource release.
    Verify: `cargo test voice::` — with the fake worker + a canned WAV pushed
            through: a full utterance yields exactly one `Text` user turn on a
            real `ConversationEngine` (in-memory DB); an empty/low-conf fake
            result yields **no** turn + an `Error`→`Idle` blip; `stop_listening`
            mid-speech still transcribes; worker crash mid-transcription → typed
            error, next utterance works.

**18.A.6 — IPC + config v6 + frontend**
    Do:     `config` v6 (`voice`, `workers`) + step `5→6` + `ConfigKey`s +
            `tests`. IPC: `voice_input_devices()`, `voice_start(conversation_id,
            Channel<VoiceState>)`, `voice_stop()`, `voice_state()`. `src/lib/ipc.ts`
            wrappers + `contracts.ts` re-exports + bindings. `ChatVoice.tsx`: a
            device picker (Settings or inline), a push-to-talk button, the
            `VoiceState` indicator, the transcript landing as a normal user
            bubble then the usual generation. `setup.ts` mocks.
    Verify: `node scripts/check.mjs` green; `ChatVoice.test.tsx` — push-to-talk
            toggles state via mocked IPC, a delivered transcript renders as a
            user bubble; `git diff --exit-code src/bindings`.

**18.A.7 — docs + partial gate + commit**
    Do:     `docs/verification/17_phase18_voice-in.md` (18.A rows PASS, 18.B rows
            `NOT EXECUTED — blocked on ADR-0018 / venv`); `src-tauri/README.md`
            (`voice/`, `worker/` module rows); `docs/spec/ARCHITECTURE.md` §2/§3;
            `docs/contracts.md` (STT worker payload row); `ROADMAP.md`. Draft
            **ADR-0018**. Commit.
    Verify: check suite green; every 18.A gate item recorded; ADR-0018 drafted.

### 18.B — Real faster-whisper worker + live gate  *(blocked: ADR-0018 + owner go-ahead for ~1 GB wheels)*

**18.B.1 — `uv` venv + `workers/requirements.txt`**
    Do:     install `uv`; `workers/requirements.txt` pinned; create `<repo>/.venv`;
            `scripts/setup-venv.mjs` + a `docs/spec/DEVELOPMENT.md` §recipe.
            `.venv/` gitignored. Finalize **ADR-0018** (ACCEPTED).
    Verify: `uv run python -c "import faster_whisper, ctranslate2"` succeeds;
            `ctranslate2` reports a CUDA device.

**18.B.2 — `workers/stt.py`**
    Do:     the real worker: `WorkerHello`; load `models/stt/` once
            (`WhisperModel(model_dir, device="cuda", compute_type="float16")`);
            per request transcribe `audio_path`; emit the `Ok.data` schema;
            ADR-0015 env asserted at startup (fail loud if a telemetry var is
            unset); stdout flush per line; reader-thread pattern.
    Verify: piping one `WorkerRequest` line + a WAV path on stdin yields a
            correct `WorkerResponse` line; a garbage path → `WorkerResult::Err`.

**18.B.3 — live capture→transcript gate (RTX 5080)**
    Do:     `voice/live_tests.rs` (`#[ignore]`, env-gated
            `LOCALAI_RUN_VOICE_LIVE`): real venv + real worker + a **recorded WAV
            fixture** (deterministic; no mic in CI) pushed through the real
            capture/VAD path; assert the transcript, the RTF, the latencies.
            Plus a **manual mic run** in `tauri dev` — speak a phrase, watch it
            land as a turn — screenshotted.
    Verify: transcript correct; **RTF** and **endpoint→transcript** recorded;
            worker-crash recovery live; missing-model path → typed error;
            manual mic run captured.

**18.B.4 — close the gate**
    Do:     fill the 18.B rows in `docs/verification/17_phase18_voice-in.md`;
            flip `ROADMAP.md` Phase 18 → `COMPLETE`; commit.
    Verify: all 7 gate items PASS with evidence.

## Verification gate (physically executed)
1. Device enumeration lists real inputs; selection persists across restart.
   *(18.A.2, 18.A.6)*
2. Speaking a phrase (or the recorded WAV fixture through the real path) produces
   a correct final transcript that becomes exactly one `Text` user turn on the
   conversation engine. *(18.A.5 fake, 18.B.3 real)*
3. VAD endpoints an utterance with no manual stop; push-to-talk `stop` also
   forces the endpoint. *(18.A.3, 18.A.5)*
4. STT worker crash mid-transcription → typed error, recovers for the next
   utterance. *(18.A.1, 18.A.5, 18.B.3)*
5. Missing input device / missing model → clear typed error, no crash.
   *(18.A.2, 18.B.2/3)*
6. An empty / below-confidence transcript is dropped per policy (transient
   notice, never a blank turn). *(18.A.5)*
7. STT real-time factor and endpoint→transcript latency recorded for `large-v3`
   fp16 on the RTX 5080. *(18.B.3)*

## ADRs / open questions
- **RESOLVED — streaming partials vs batch:** batch, one transcription per
  VAD-endpointed segment (ADR-0005; partials optional, not v1). The contract
  keeps a `Progress` frame for later.
- **RESOLVED — push-to-talk vs open-mic:** push-to-talk for v1 (ADR-0005);
  open-mic + VAD ducking is a later opt-in.
- **NEW — ADR-0018 — dev Python worker environment:** `uv` venv at `<repo>/.venv`,
  pinned `workers/requirements.txt`, `ort` via `load-dynamic`, the ADR-0015 env
  centralised in `worker/env.rs`. Ratifies the env-audit O-item ("Python worker
  environment strategy → ADR"). Drafted at 18.A.7, ACCEPTED at 18.B.1.
- **NEW — message content for voice turns:** persist `Text` in v1; move to
  `Audio { asset, transcript }` when the blob store lands (additive, no
  migration). Recorded in the verification doc, not an ADR.
- **Downloads to clear with the owner:** (a) `silero_vad.onnx` ~2 MB; (b) the
  faster-whisper wheel set incl. `nvidia-cudnn-cu12` / `nvidia-cublas-cu12`
  ~0.8–1 GB. (a) gates 18.A.3, (b) gates all of 18.B.
