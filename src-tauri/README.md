# src-tauri/ — Rust application core

The authoritative nucleus (`CLAUDE.md` Article I, `ARCHITECTURE.md` §2). Owns
application state, persistence, model lifecycle, resource management, scheduling,
process supervision, and IPC. Single crate, one module per subsystem
(`docs/decisions/0001-single-rust-crate.md`).

## Current modules (Phase 19)

| Module | What | Doc |
| ------ | ---- | --- |
| `lib.rs` | The Tauri builder + `run()`; `setup()` (a free fn) assembles config + DB + registry + acquisition + resource manager + lifecycle manager (+ llama.cpp backend) + conversation service into managed state; exit hook cancels the in-flight generation, unloads models, checkpoints the WAL | — |
| `main.rs` | Thin bin entry | — |
| `logging/` | Observability — JSON stdout via a non-blocking lossy writer; boundary **secret redaction**; in-memory ring buffer (`recent_lines`); **rotating daily file sink** under `<app_data>/logs/` (Phase 18.5, `enable_file_sink` + `sweep_logs`, same redaction); hot-reloadable filter (`set_level` from config / `LOCALAI_LOG`); `operation()` span helper. No network sink. | `docs/plan/10_observability.md`, `docs/plan/18.5_deploy-diagnostics.md` |
| `ipc/` | Typed IPC boundary — `commands` (`app_*`, `frontend_log`, `config_*`), `error::{AppError, ErrorEnvelope}` | `docs/decisions/0002-ipc-design.md` |
| `contracts/` | The serializable vocabulary for **both** the IPC and worker boundaries — `ids`, `task`, `model`, `generation`, `conversation`, `resource`, `worker`. No behaviour. | `docs/contracts.md` |
| `diag.rs` | Local diagnostics snapshot (Phase 18.5) — `DiagSnapshot` (build + effective config + registry + resources + lifecycle + conversation **metadata** + recent redacted log lines + `nvidia-smi`/OS). `collect()` / `export()` (→ `<app_data>/diagnostics/diag-<ts>.json`). `diag_snapshot` / `diag_export` IPC. All local (ADR-0015). | `docs/plan/18.5_deploy-diagnostics.md` |
| `config/` | The settings authority — one JSON file (`<app_config_dir>/config.json`), layered defaults ← file ← session, schema **v7** (`models` · `logging` · `resources.vram_safety_margin_mb` · `runtimes.dir` · `workers.{dir,python}` · `voice.{input_device,output_device}`) + forward migrations, atomic write | `docs/decisions/0016-configuration.md` |
| `db/` | SQLite persistence — writer pool (1) + reader pool (4), pragma hook, `refinery` forward-only migrations (`migrations/`) with verified `VACUUM INTO` backup, `write`/`read` helpers, `error::DbError`, `AppMetaRepo` | `docs/decisions/0009-persistence.md` |
| `models/` | Model registry — `model_entry` rows (`V0002`), `ModelRegistry` CRUD + capability `query`, `ModelDraft`/`ModelFilter`, path confinement (`validate_model_path`), availability computed from `path.exists()`, `Arc`-cached list cleared on write. IDs are UUIDv4 (ADR-0017). | `docs/plan/11_model-registry.md` |
| `acquisition/` | Model acquisition — `gguf` (header parser), `hf` (HF API + range fetch), `budget` (pre-transfer guard), `download` (`reqwest` engine: `.part` + `Range` resume + SHA-256 verify + register), `AcquisitionService`, `acquire_fixed` (pinned STT/TTS bundles → short `stt` / `tts` subdirs). `model_downloads` (`V0003`). **The only runtime network egress.** | `docs/plan/12_model-acquisition.md`, ADR-0008 |
| `resources/` | Resource manager — `probe` (`HardwareProbe`: `NvmlProbe` whole-GPU + `sysinfo` RAM, `MockProbe`), `estimate` (closed-form LLM VRAM + per-model EMA `Calibration`), `ResourceManager` (in-memory reservation ledger; `request`/`commit`/`release`/`observe`/`reconcile` behind one async `Mutex`; `request` works off a cached snapshot, never the probe). Accounts only — no loading. `resources.vram_safety_margin_mb` (config v4). | `docs/plan/13_resource-manager.md`, ADR-0007 |
| `lifecycle/` | Model lifecycle manager — **the only loader/unloader of managed models**. `backend` (`ModelBackend` / `LoadedInstance` traits + `as_any` downcast hook; llama.cpp impl in `llm/`), `LifecycleManager` state machine (`Unloaded → Loading → Loaded ⇄ Busy → Unloading`, `Failed` recovery) behind one async `Mutex`; concurrent same-model loads coalesce; a Phase 13 reservation is acquired before the load and released on every exit path; `RetryPolicy` (3 attempts, 500 ms base); ~2 s liveness monitor → `Failed` + release. `lifecycle_status` IPC. | `docs/plan/14_model-lifecycle.md`, ADR-0007/0010 |
| `conversation/` | **The one conversation engine** (Phase 16 flow, Phase 17 formalized): `repo` (all SQL for `conversation` + `message`, `V0004`), `prompt` (ChatML render — text + transcribed audio), `mod` (`ConversationEngine` — `add_user_turn(typed content)` + `generate(sink) -> TaskId`; `send` = both, for text; one generation at a time — 2nd → `Conflict`; explicit `GenerationState` via `generation_state()`; `cancel` / `shutdown`). `chat_*` + `conversation_*` IPC. | `docs/plan/17_conversation-engine.md` |
| `llm/` | llama.cpp adapter (first LLM backend) — `LlamaBackend` (`ModelBackend`: spawns a supervised `llama-server` child from `runtimes.dir`), `server` (free loopback port + `--api-key` bearer + Windows Job Object kill-on-close), `client` (`/health` + `/completion` non-stream + SSE stream, per-call deadline, cancel-drops-response), `protocol` (llama.cpp JSON + SSE parser — nothing else references it), `LlamaServer` (`LoadedInstance` + `LlmInstance` generate/stream). Verified live: pinned prebuilt `b10819` + Qwen 0.5B, TTFT ≈ 23 ms. | `docs/plan/15_llama-cpp-adapter.md`, ADR-0003/0004/0013 |
| `job.rs` | Windows `JobObject` (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) — every spawned child (`llama-server`, a stdio worker) dies with the app even on a hard crash. No-op on non-Windows. | ADR-0013 |
| `worker/` | The **shared stateless-worker supervisor** (ADR-0013) — `WorkerSupervisor` spawns `python <script>` under a Job Object with the ADR-0015 lockdown env (`env`, one place, tested against the list), `WorkerHello` handshake (protocol + kind), request/response mux by `WorkerJobId`, `Progress` sink, cancel-abandons-wait, bounded-backoff restart → `Failed`. `layout` resolves interpreter + script (dev from `workers.*` config, ship from `current_exe()` siblings). Reused by TTS (19) + the embedder (27). | `docs/plan/18_voice-in.md`, ADR-0013/0015/0018 |
| `voice/` | Voice (Phase 18 in + Phase 19 out) — `capture` (`cpal` input, `!Send` stream on a parked thread), `playback` (`cpal` output + PCM queue + instant `stop`), `resample` (`rubato`: `ToMono16k`, generic `Resampler16`), `vad` (Silero v5 ONNX via `ort`; `SpeechStart`/`SpeechEnd`/`MaxDurationCut`; runtime onset duck), `segment` (endpointed WAV), `chunker` (streamed deltas → clauses), `tts` (`TtsOutput` = the Tts worker + playback, ~1 clause lookahead), `mod` (`VoiceInput` — one session; `start_listening(id, model_id: Option)` runs listen→transcribe→think→speak→listen; **barge-in** = `engine.cancel` → `tts.cancel` → `playback.stop`, timed; low-confidence drop; `watch<VoiceState>`). `voice_*` IPC. Live on the RTX 5080: faster-whisper `large-v3` fp16 (STT), Chatterbox Turbo (TTS, warm RTF ≈ 0.45, barge-in ≈ 4 ms). | `docs/plan/18_voice-in.md`, `docs/plan/19_voice-out.md`, ADR-0005/0013/0018 |

## Modules added by later phases

blob store (with the first blob feature) ·
`context` builder (P20) · `memory` (P21) · `image` (P22) · `scheduler` (P23–24) ·
`characters` (P25–29). Each phase registers its module here and in
`docs/spec/ARCHITECTURE.md` §3.

## Notes

- `crate-type` includes `cdylib`/`staticlib` for Tauri's mobile targets — leave as
  the scaffold set it.
- `[lints]` in `Cargo.toml` denies warnings + runs clippy `all` + `pedantic`.
- `cargo test` runs the `export_bindings` test which regenerates `../src/bindings/`.
