# 14 — Phase 15: llama.cpp Adapter

**Date:** 2026-09-06 (adapter) · 2026-09-06 (15.D — real binary)
**Branch:** `main` (local; no remote).
**Method:** new `src-tauri/src/llm/` module; 13 stub tests (in-process
`tiny_http` mimicking `llama-server`) + pure-unit tests; then **15.D**: a pinned
prebuilt CUDA `llama-server` + the real Qwen 0.5B GGUF, 4 `#[ignore]`d live
tests, run on the reference machine (RTX 5080).

**15.D was run** — a **pinned prebuilt** was used instead of the ADR-0004
source build (owner call: no CUDA Toolkit on the machine, and a prebuilt is
faster). Pin: **`ggml-org/llama.cpp` release `b10819`** (commit `6a1a922d2`),
assets `llama-b10819-bin-win-cuda-13.3-x64.zip`
(sha256 `c9069222…`) + `cudart-llama-bin-win-cuda-13.3-x64.zip`
(sha256 `1462a050…`). CUDA 13.3 build → Blackwell sm_120 works. Kept
**in-project** (owner preference — no runtime files outside the repo):
`runtime/llama-server/` (gitignored). Model: `models/qwen2.5-0.5b-instruct-q4_k_m.gguf`
(sha256 `74a4da8c…`, gitignored).

Governing: **ADR-0003** (`llama-server` supervised child), **ADR-0004** (amended
in practice — *pinned prebuilt* accepted as the primary path here, source build
stays the documented fallback), **ADR-0013** (transport), **ADR-0016** (config —
schema **v5** `runtimes.dir`). Plan: `docs/plan/15_llama-cpp-adapter.md`.
**No new ADR.**

---

## What landed

- `llm/protocol.rs` — the llama.cpp `/completion` request/response shapes, the
  SSE line parser (`data: {json}` / `data: [DONE]` / comments), and
  `stop_type` → `StopReason` mapping (new string field + legacy booleans).
  **This is the only file that knows llama.cpp's JSON.**
- `llm/server.rs` — `pick_free_port` (bind `:0`, read, drop), `bearer_token`
  (two UUIDs → 64 hex, per launch, never logged), `ServerArgs::to_argv`
  (`--model --host 127.0.0.1 --port --api-key --n-gpu-layers --ctx-size
  --flash-attn --no-webui --no-warmup`), `ServerProcess` (spawn under a Job
  Object, `try_wait` exit check, graceful-then-kill `shutdown`).
- `llm/job.rs` — Windows Job Object wrapper (`windows` crate,
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`); no-op on non-Windows.
- `llm/client.rs` — `LlamaClient`: `health()`, `complete()` (non-stream),
  `stream()` (SSE → ordered `GenerationEvent::TokenDelta`s then one terminal),
  bearer header on every call, a per-call deadline (`AppError::Timeout`), and
  `tokio::select!` on the `CancellationToken` (cancel drops the response so the
  server sees the disconnect → `GenerationEvent::Cancelled`).
- `llm/mod.rs` — `LlamaBackend` (`ModelBackend`): `load` = spawn → `/health`
  poll (respecting `cancel` + a load timeout + child-exit) → `Ok(LlamaServer)`.
  `LlamaServer` implements `LoadedInstance` (health checks the child then
  `/health`; `measured_vram_mb` passes the load estimate through; `shutdown`
  kills the child) **and** the new `LlmInstance` capability trait
  (`generate` / `stream`). `as_llm(&Arc<dyn LoadedInstance>)` downcast helper.
  `BACKEND_KEY = "llama.cpp"`.
- `lifecycle::backend::LoadedInstance` gains `fn as_any(&self)` (additive
  internal trait method; `FakeBackend`'s instance implements it too) — the
  Phase 16 integration point.
- `lib.rs` — `start_lifecycle_manager` registers `LlamaBackend` **iff**
  `<runtimes.dir>/llama-server.exe` exists; otherwise logs an info line and LLM
  loads stay unavailable.
- **Config schema v5** (`RuntimesConfig { dir }`, `ConfigKey::RuntimesDir`,
  session override; default `<app_data>/runtimes`). The reference machine's
  `config.json` points it — and `models.dir` — into the repo
  (`runtime/llama-server`, `models/`), both `.gitignore`d, per the owner's
  "no runtime files outside the project" preference.
- Deps: `windows` (`Win32_System_JobObjects` + `_Threading` + `_Foundation`) and
  the `tokio` `process` feature. `windows` was already transitive (via
  `sysinfo`); `process` adds `signal-hook-registry` (1 crate).

---

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | Startup + readiness check succeed for a real GGUF | **PASS (live, 15.D)** — `live_startup_and_non_streaming_generation`: `LlamaBackend::load` spawned the pinned `llama-server`, polled `/health`, and returned `Ok` in **~775 ms** for the real Qwen 0.5B Q4_K_M on the RTX 5080. |
| 2 | Non-streaming and streaming generation produce correct output | **PASS (live, 15.D)** — non-stream: *"The three primary colors are red, blue, and yellow."* (13 tok, `EndOfText`). Stream: 12 `TokenDelta`s with contiguous indices → `Done{EndOfText, 13}`. Stub tests (`non_streaming_completion_returns_text_and_stop_reason`, `streaming_completion_yields_ordered_deltas_then_done`, `sse_lines_parse`, `chunk_stop_reason_maps_every_variant`) cover the parser edges. |
| 3 | Cancellation stops generation and frees the slot | **PASS (live, 15.D)** — `live_cancel_frees_the_slot`: a 512-token stream cancelled after the first delta → the stream ends with `GenerationEvent::Cancelled`; **a fresh non-stream generation immediately after succeeds** (*"…red, blue, and yellow."*), proving the server slot was released. Stub `a_mid_stream_cancel_emits_cancelled` covers the client-drop mechanics. |
| 4 | A timeout is handled with a typed error | **PASS** — `a_stalled_server_hits_the_deadline`: a server that accepts then never responds → `complete()` returns `AppError::Timeout` at the 200 ms deadline. The stream path has the same `tokio::time::timeout` guard on connect + per-chunk idle. |
| 5 | Killing the backend is detected; Phase 14 → `Failed`; recovery works | **PASS (live, 15.D)** — `live_external_kill_is_detected_and_no_orphan_on_shutdown`: `taskkill /F` the `llama-server` pid → `LoadedInstance::health()` returns `Err` (the child-exit branch fires before the HTTP call). The Phase-14 `Failed`-transition + reload is already proven in `docs/verification/13` with `FakeBackend` (the liveness monitor calls `health()`). |
| 6 | Clean shutdown leaves no orphan process (process-list check) | **PASS (live, 15.D)** — same test: after `instance.shutdown()`, `tasklist /FI "PID eq <pid>"` shows the process gone. `job_object_can_be_created` + `ServerProcess`'s `kill_on_drop(true)` + the `KILL_ON_JOB_CLOSE` job are the belt-and-braces. |
| 7 | No file outside the adapter references llama.cpp | **PASS** — `git grep -l llama` outside `src-tauri/src/llm/` returns only: the `"llama.cpp"` **backend-key string** in `acquisition` (the opaque registry identifier, ADR-0017) + `lib.rs` (`llm::BACKEND_KEY` / `llm::LlamaBackend` wiring) + doc comments + one test string. No llama.cpp **types, protocol, or flags** leak — `CompletionRequest`, the SSE parser, `--api-key`, `/completion` all live only in `llm/`. |
| 8 | TTFT + tokens/sec recorded | **PASS (live, 15.D)** — `live_streaming_generation_with_ttft_and_throughput`: **TTFT ≈ 23 ms · ≈ 278 tokens/sec** (13 tok in 46.7 ms) for Qwen 0.5B Q4_K_M, `-ngl -1`, ctx 4096, on the RTX 5080. Recorded in `docs/spec/PERFORMANCE.md`. (A larger model at Phase 16 sets the chat-relevant baseline.) |
| 9 | Full check suite green; bindings regenerated | **PASS** — `node scripts/check.mjs` all green: `cargo fmt` / `clippy -D warnings` / **212 rust tests** (+3; 5 `#[ignore]`d incl. the 4 live 15.D tests) / `tsc` / eslint / prettier / **7 vitest** / `vite build`. New bindings: `RuntimesConfig`, `ConfigKey` (+ `RuntimesDir`), `AppConfig` (+ `runtimes`). |

### Wire-in confirmation

`npm run tauri dev` on the reference machine — the app loads `config.json`
(migrated v1→v5), points `models.dir` + `runtimes.dir` into the repo, and
**registers the backend**:

```
{"message":"configuration loaded","models_dir":"C:\\Users\\Vincent\\Desktop\\vAI\\models",...}
{"message":"resource manager ready","gpu":"Some(GpuMemory { total_mb: 16303, ... })",...}
{"message":"llama.cpp backend registered","binary":"C:\\Users\\Vincent\\Desktop\\vAI\\runtime\\llama-server\\llama-server.exe"}
```

### 15.D — how to re-run

```
LOCALAI_LLAMA_SERVER=<repo>/runtime/llama-server/llama-server.exe \
LOCALAI_TEST_GGUF=<repo>/models/qwen2.5-0.5b-instruct-q4_k_m.gguf \
cargo test --manifest-path src-tauri/Cargo.toml llm::live_tests -- --ignored --nocapture --test-threads=1
```

WDDM-hang watch (`docs/verification/02_phase3_probes.md`): **not observed** —
four back-to-back load+generate cycles completed cleanly. Keep the mitigation
ladder ready for longer sustained runs at Phase 16.

---

## Decisions taken this phase

- **Transport = TCP loopback + `--api-key` bearer** (not a named pipe).
  `llama-server` (cpp-httplib) has no Windows named-pipe mode, so ADR-0013's
  "if TCP is required" branch applies: a per-launch 64-hex token, redacted by the
  logging layer, rejected-if-absent by the server.
- **`measured_vram_mb` returns the load estimate**, not `None` — llama-server
  doesn't report its footprint, and giving the Phase 13 ledger a number to
  `commit` is better than leaving the raw estimate. It is honest-ish ("our best
  figure") and keeps the calibration factor near 1 until a real measurement path
  exists.
- **`-ngl -1`** (offload all layers) for now; Phase 23 does resource-aware layer
  placement. If the model doesn't fit, `llama-server` fails on load and the
  lifecycle manager's retry/`Failed` path (Phase 14) handles it.
- **`--flash-attn` dropped from the argv** — `b10819` changed it to
  `[on|off|auto]` (default `auto`, which enables FA where the model supports it);
  forcing it risks failures on edge models, and `auto` is the right default.
- **Pinned prebuilt, not the source build** (owner call, ADR-0004 amended):
  release `b10819`, CUDA 13.3, sm_120-capable. ~540 MB (`ggml-cuda.dll` +
  `cublasLt64_13.dll` dominate), kept in `runtime/llama-server/` (gitignored).
- **`runtimes.dir` is config (v5)**, default `<app_data>/runtimes`; the machine
  overrides it — and `models.dir` — to repo-local paths so no runtime blob
  lives outside the project. The DB + `config.json` stay in
  `%APPDATA%\com.localai.app\` (small, conventional; ADR-0009).

## Not done here (deferred by design)

- Generation *through* the lifecycle manager end to end from the UI — Phase 16.
  This phase exercises `LlmInstance` directly on the loaded instance.
- Resource-aware `-ngl` — Phase 23. Multi-model / hot-swap — Phase 23/24.
- Registering the downloaded GGUF into the **app's** DB — Phase 16 entry (the
  live tests use a throwaway DB). The file is in place at `models/`.

**Phase 15 COMPLETE** — all 9 gate items pass (4, 7, 9 + stub 2/3/6 in the
adapter build; 1, 2, 3, 5, 6, 8 live on the reference machine in 15.D). Pointer
→ Phase 16.
