# 14 — Phase 15: llama.cpp Adapter (split)

**Date:** 2026-09-06
**Branch:** `main` (local; no remote).
**Method:** new `src-tauri/src/llm/` module; 13 tests against an in-process
`tiny_http` stub `llama-server` + pure-unit tests; a `npm run tauri dev` launch.
**The real CUDA `llama-server` binary + a real GGUF are not available on this
machine** (no CUDA Toolkit / `nvcc`; ADR-0004 build needs a ~3 GB install), so
the gate items that need them are **NOT EXECUTED — deferred to plan step 15.D**,
tracked in `ROADMAP.md` §6. This split was decided with the owner.

Governing: **ADR-0003** (`llama-server` supervised child), **ADR-0004** (build
from source, sm_120 — recipe now in `DEVELOPMENT.md` §5), **ADR-0013** (transport).
Plan: `docs/plan/15_llama-cpp-adapter.md`. **No new ADR** (confirms ADR-0013).

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
  `%APPDATA%\com.localai.app\runtimes\llama-server.exe` exists; otherwise logs
  an info line and LLM loads stay unavailable (until 15.D).
- Deps: `windows` (`Win32_System_JobObjects` + `_Threading` + `_Foundation`) and
  the `tokio` `process` feature. `windows` was already transitive (via
  `sysinfo`); `process` adds `signal-hook-registry` (1 crate).

---

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | Startup + readiness check succeed for a real GGUF | **NOT EXECUTED — deferred (15.D).** No CUDA `llama-server` binary. The `load` path (spawn → `/health` poll → `Ok`, with child-exit and timeout branches) is written; the health-poll client half is proven against the stub (gate 2). |
| 2 | Non-streaming and streaming generation produce correct output | **PASS (stub) / real deferred (15.D).** `non_streaming_completion_returns_text_and_stop_reason`: `complete()` → text `"hello world"`, 2 tokens, `EndOfText`. `streaming_completion_yields_ordered_deltas_then_done`: SSE → `TokenDelta{0,"hel"}`, `TokenDelta{1,"lo"}`, `Done{EndOfText, 2}` — indices contiguous, order preserved. `sse_lines_parse` + `chunk_stop_reason_maps_every_variant` cover the parser. |
| 3 | Cancellation stops generation and frees the slot | **PASS (app side) / real slot-free deferred (15.D).** `a_mid_stream_cancel_emits_cancelled`: a stalled stream + `CancellationToken::cancel()` → the stream ends promptly with `GenerationEvent::Cancelled` and the client drops the HTTP response (so a real server sees the disconnect). |
| 4 | A timeout is handled with a typed error | **PASS** — `a_stalled_server_hits_the_deadline`: a server that accepts then never responds → `complete()` returns `AppError::Timeout` at the 200 ms deadline. The stream path has the same `tokio::time::timeout` guard on connect + per-chunk idle. |
| 5 | Killing the backend is detected; Phase 14 → `Failed`; recovery works | **NOT EXECUTED — deferred (15.D)** for the llama-server-specific exit detection (`ServerProcess::exit_status` + `health()` checking the child). **The lifecycle side is already proven** in Phase 14 (`docs/verification/13`, `a_dead_backend_is_detected_and_reconciled`) with `FakeBackend`. |
| 6 | Clean shutdown leaves no orphan process (process-list check) | **PARTIAL.** `job_object_can_be_created` proves the Job Object path builds + runs. A real orphan-check (spawn a child, drop the job, confirm it is gone from `tasklist`) with `llama-server` is **deferred (15.D)**; `ServerProcess` also sets `kill_on_drop(true)` and an explicit `shutdown`. |
| 7 | No file outside the adapter references llama.cpp | **PASS** — `git grep -l llama` outside `src-tauri/src/llm/` returns only: the `"llama.cpp"` **backend-key string** in `acquisition` (the opaque registry identifier, ADR-0017) + `lib.rs` (`llm::BACKEND_KEY` / `llm::LlamaBackend` wiring) + doc comments + one test string. No llama.cpp **types, protocol, or flags** leak — `CompletionRequest`, the SSE parser, `--api-key`, `/completion` all live only in `llm/`. |
| 8 | TTFT + tokens/sec recorded | **NOT EXECUTED — deferred (15.D).** Needs the real binary + Qwen 0.5B GGUF. `PERFORMANCE.md` rows marked "not yet measured". |
| 9 | Full check suite green; bindings regenerated | **PASS** — `node scripts/check.mjs` all green: `cargo fmt` / `clippy -D warnings` / **209 rust tests** (+13 llm; 1 `#[ignore]`d) / `tsc` / eslint / prettier / **7 vitest** / `vite build`. No new bindings (the `as_any` hook is not a wire contract; `ModelState`/`LifecycleStatus` unchanged this phase). |

### Wire-in confirmation (15.8)

`npm run tauri dev` on the reference machine:

```
{"message":"resource manager ready","gpu":"Some(GpuMemory { total_mb: 16303, ... })", ...}
{"message":"no llama-server binary — LLM loads unavailable until it is installed (plan 15.D)",
 "expected":"C:\\Users\\Vincent\\AppData\\Roaming\\com.localai.app\\runtimes\\llama-server.exe"}
```

Clean startup; the backend registration is correctly skipped.

---

## Deferred — plan step 15.D (before / with Phase 16)

Phase 16 (the first vertical slice) needs real streamed tokens, so 15.D runs
before or alongside it. Steps:

1. **Get a fresh CUDA `llama-server`** — the owner wants it built for this
   project, *not* reused from another. Either: install CUDA Toolkit 13.x and run
   the `DEVELOPMENT.md` §5 build (pin the llama.cpp commit here when done), or a
   pinned official prebuilt (ADR-0004 fallback). Place at
   `%APPDATA%\com.localai.app\runtimes\llama-server.exe` (+ CUDA DLLs).
2. **Download** `Qwen/Qwen2.5-0.5B-Instruct-GGUF`
   (`qwen2.5-0.5b-instruct-q4_k_m.gguf`, ~400 MB) via the `/models` picker
   (owner-cleared).
3. Run gate items **1, 2 (real), 3 (real slot-free), 5, 6 (real orphan check),
   8 (TTFT + tokens/sec)** and record the numbers here + in `PERFORMANCE.md`.
4. Watch the WDDM-hang risk on the first sustained generation
   (`docs/verification/02_phase3_probes.md`) — 3-step mitigation ladder ready.

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
- **Binary discovery by convention** (`<app_data>/runtimes/llama-server.exe`),
  not a config key — added when the binary lands at 15.D.

**Phase 15 complete for the split scope** (gate items 4, 7, 9 + the stub sides
of 2, 3, 6 pass; items 1, 5, 8 and the real sides of 2/3/6 deferred to 15.D with
the reason recorded). Pointer → Phase 16 (15.D runs first).
