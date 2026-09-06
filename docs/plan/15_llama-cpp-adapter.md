# Phase 15 — llama.cpp Adapter

> **Status: IN PROGRESS** — step detail finalized at phase entry (2026-09-06).
> **Split, by owner decision:** the adapter, process supervision, transport, and
> the full HTTP/SSE client are built now and tested against an **in-process stub
> `llama-server`**. The gate items that need a real CUDA `llama-server` binary +
> a real GGUF (1, real-2, real-3, 5, 6, 8) are **deferred** — the machine has no
> CUDA Toolkit / `nvcc` (ADR-0004 "build from source" needs a ~3 GB install
> first). The owner has cleared downloading project assets (the Qwen 0.5B GGUF
> and, for the deferred run, a fresh `llama-server` — *not* reused from any
> other project). Tracked in `ROADMAP.md` §6 alongside the Phase 12 gate-5
> deferral.

> **Architecture frozen at Phase 5.** Governing: **ADR-0003** (`llama-server`
> supervised child over loopback HTTP), **ADR-0004** (build from source, sm_120),
> **ADR-0013** (transport — `llama-server` is upstream HTTP, so **TCP loopback +
> per-launch bearer token**, not a named pipe), `AI_PIPELINES.md` §1. Watch item:
> the WDDM-hang risk on the first sustained generation
> (`docs/verification/02_phase3_probes.md`).

## Objective
The first LLM backend. A new `llm` module implements `lifecycle::backend::
ModelBackend`; its loaded instance also implements an `LlmInstance` capability
trait (`generate` / `stream` / `cancel`). **Every** llama.cpp-specific detail
(spawn flags, `/completion` shape, SSE framing, `/health`) lives in this module
and nowhere else (`ARCHITECTURE.md` §2, gate 7).

## Depends on
Phase 14 (lifecycle — `ModelBackend`, `LoadedInstance`), Phase 13 (resources —
`-ngl` from the estimate), Phase 7 (`GenerationRequest` / `GenerationEvent` /
`StopReason` / `SamplingParams`), Phase 10 (`operation` spans).

## Not in this phase
- Personas, memory, characters, voice, images.
- Conversation state / history (Phase 16/17) — `generate` takes a
  fully-rendered `prompt` string.
- Multi-model, hot-swap, eviction (Phase 23/24).
- Wiring generation **through** the lifecycle manager end-to-end from the UI —
  Phase 16. This phase adds the `as_any` downcast hook and tests `LlmInstance`
  directly on the loaded instance.
- The CUDA build itself + the real-model gate run (deferred — see the banner).

## Transport decision (confirms ADR-0013)
`llama-server` is an upstream HTTP server (cpp-httplib); it has no Windows
named-pipe mode. Per ADR-0013 that means: **`127.0.0.1:<free port>` + a
per-launch bearer token** (`--api-key <token>`, random 32 bytes hex, generated
by Rust, never logged). The server rejects unauthenticated requests. Free port =
bind `TcpListener` to `127.0.0.1:0`, read the port, drop, pass to the child
(small, accepted spawn race). A Windows **Job Object** with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` owns the child so it cannot outlive us.

## Architecture notes
- **New module `src-tauri/src/llm/`**:
  - `mod.rs` — `LlamaBackend` (`ModelBackend` impl) + the `LlmInstance` trait +
    `LlmGenerate` request/params mapping + `as_llm(&Arc<dyn LoadedInstance>)`
    downcast helper.
  - `server.rs` — `ServerProcess`: free-port pick, bearer token, arg builder
    (`--model --port --api-key --n-gpu-layers --ctx-size --flash-attn
    --no-webui …`), spawn under a Job Object, `/health` poll with a load
    timeout, `child.wait()` exit watch, `shutdown` (graceful then kill).
  - `job.rs` — thin Windows Job Object wrapper (`windows` crate,
    `Win32_System_JobObjects` + `Win32_System_Threading` + `Win32_Foundation`).
    A no-op stub on non-Windows so the crate still builds there.
  - `protocol.rs` — `/completion` request/response types, SSE line parser
    (`data: {json}\n\n`, `[DONE]`), map llama.cpp `stop_type` → `StopReason`.
  - `client.rs` — `reqwest` calls: `health()`, `complete()` (non-stream),
    `stream()` (returns a `futures` stream of `GenerationEvent`), all with the
    bearer header and a per-call timeout; `cancel` = drop the response future
    (llama-server frees the slot on disconnect) + trip the `CancellationToken`.
  - `tests.rs` — an **in-process `tiny_http` stub** (already a dev-dep) that
    mimics `/health`, `/completion` (stream + non-stream), the `[DONE]`
    sentinel, and **401 without the right bearer**; plus pure-unit tests for the
    SSE parser, the arg builder, the free-port picker, and `stop_type` mapping.
- **`LoadedInstance` gains `fn as_any(&self) -> &(dyn Any + Send + Sync)`**
  (additive; every impl is `{ self }`, incl. `FakeBackend`'s instance) — the
  downcast hook Phase 16 uses to get an `&dyn LlmInstance` back from the
  lifecycle manager. No wire contract touched.
- **`-ngl`**: the adapter asks for it via a param derived from the resource
  reservation / estimate; never hardcoded. For Phase 15 the lifecycle manager
  passes the estimate through `LoadRequest`; the adapter computes a layer count
  or passes `-1` (all) when the whole model fits the reservation.
- Backend key registered with the lifecycle manager: `"llama.cpp"` (matches the
  `ModelBackend` string written by `acquisition` — `LLAMA_BACKEND`).
- `SECURITY.md`: the token is generated with `getrandom`/OS RNG, passed as a
  process arg (acceptable on a single-user machine; a arg-list read needs local
  admin), redacted by the logging layer's `Bearer` rule, never in an event.

## Performance notes
- Record **TTFT** and **tokens/sec** for the Qwen 0.5B reference prompt — the
  Phase 16 gate and Phase 31 baseline need it. (Deferred with the real run.)
- The HTTP/SSE overhead per token must stay far below token latency — the
  `TokenDelta` round-trip probe (Phase 7) already bounds the serialization side.
- Health poll during load: 250 ms interval, a generous load timeout (120 s) —
  a cold GGUF load off disk is slow.

## Steps

**15.1 — Deps + module skeleton**
    Do:     Add `windows` (JobObjects/Threading/Foundation features) +
            `getrandom` as direct deps (both already transitive — confirm 0/low
            net crates). Create `llm/` with the files above, empty impls,
            register `pub mod llm;`.
    Verify: `cargo build`; `cargo tree` delta reviewed.

**15.2 — `protocol.rs`: types + SSE parser**
    Do:     `CompletionRequest` / `CompletionResponse` / streaming chunk types;
            `parse_sse_line`; `stop_type_to_reason`.
    Verify: `cargo test llm::protocol` — parses a real llama.cpp SSE capture
            fixture (chunk, final, `[DONE]`); rejects malformed; maps every
            `stop_type`.

**15.3 — `server.rs`: free port + arg builder + token**
    Do:     `pick_free_port`, `ServerArgs::build`, `bearer_token()`.
    Verify: `cargo test llm::server` — port is in range + bindable; args contain
            `--api-key`, `--port`, `--model`, `--flash-attn`, `--no-webui`, the
            ngl value; token is 64 hex chars, distinct per call.

**15.4 — `job.rs`: Windows Job Object**
    Do:     `JobObject::new()` + `assign(child)` with kill-on-close; `Drop`
            closes the handle (→ kills the child). No-op impl for non-Windows.
    Verify: `cargo test llm::job` (Windows) — create + assign a short-lived
            `cmd /c` child; dropping the job kills it (process gone from a
            `tasklist` check). *(This is a real subprocess but not llama-server —
            runs now.)*

**15.5 — `client.rs`: health + non-stream + stream against the stub**
    Do:     `LlamaClient::{health, complete, stream}` with the bearer header +
            timeout. Stub `tiny_http` server in `tests.rs`.
    Verify: `cargo test llm::client` — health ok/refused; non-stream returns the
            text + token count + stop reason; stream yields ordered `TokenDelta`s
            then `Done`; a wrong/absent token → `AppError` (401 mapped).

**15.6 — Cancellation + timeout**
    Do:     `stream`/`complete` select on the `CancellationToken`; a per-call
            deadline → `AppError::Timeout`. On cancel, drop the response so the
            server sees the disconnect.
    Verify: `cargo test llm::cancel` — a mid-stream cancel stops the event
            stream promptly and emits `GenerationEvent::Cancelled`; a stub that
            stalls past the deadline → `AppError::Timeout`.

**15.7 — `LlamaBackend` + `LlmInstance`**
    Do:     `ModelBackend::load` = spawn (Job Object) → `/health` poll →
            `Ok(LlamaServer)`; `LlamaServer: LoadedInstance + LlmInstance`.
            `health` = `/health`; `measured_vram_mb` from the load estimate
            (llama-server doesn't report it — `None` is also fine); `shutdown` =
            graceful then kill. `as_any`.
    Verify: `cargo test llm::backend` — against a stub binary path that is a
            script echoing a health-ready server: `load` reaches `Ok`, health
            passes, generate works, `shutdown` leaves nothing. *(Uses the stub;
            the real llama-server is the deferred gate.)*

**15.8 — Lifecycle integration**
    Do:     `lib.rs` — `lifecycle.register_backend("llama.cpp",
            Arc::new(LlamaBackend::new(...)))`. A `#[cfg(test)]` path in the
            lifecycle tests that swaps in a stub-backed `LlamaBackend`.
    Verify: `cargo test lifecycle::` still green; a new test loads/unloads a
            model through the manager with the stub llama backend.

**15.9 — `as_any` hook (contract-free)**
    Do:     Add `as_any` to `LoadedInstance`; implement on `FakeBackend`'s
            instance + `LlamaServer`; `llm::as_llm` helper.
    Verify: `cargo test` — `as_llm` on a `LlamaServer` returns `Some`; on the
            fake returns `None`.

**15.10 — Docs + gate run + commit**
    Do:     `node scripts/check.mjs`; write `docs/verification/14_phase15_llama.md`
            (mock gate items PASS, real items NOT EXECUTED with the reason +
            exact deferred plan); `DEVELOPMENT.md` (the CUDA build recipe —
            written, not run), `src-tauri/README.md`, `ARCHITECTURE.md`,
            `docs/contracts.md` (the `as_any` hook), `PERFORMANCE.md`,
            `ROADMAP.md` §1/§4/§5/§6. Commit.
    Verify: check suite green; every gate item recorded (PASS or NOT EXECUTED
            with reason).

**15.D — Deferred: the real binary + model** *(not this turn)*
    Do:     Obtain a fresh CUDA `llama-server` (build from source after a CUDA
            Toolkit 13.x install per ADR-0004, or a pinned prebuilt — surface
            the choice). Download `Qwen/Qwen2.5-0.5B-Instruct-GGUF`
            (`qwen2.5-0.5b-instruct-q4_k_m.gguf`, ~400 MB) via the Phase 12
            acquisition path. Run gate items 1, 2 (real), 3 (real), 5, 6, 8.
    Verify: real startup + streamed output + cancel-frees-slot + kill→`Failed`
            + no orphan (`tasklist`) + TTFT / tokens-sec recorded in
            `PERFORMANCE.md` and the verification doc.

## Verification gate
1. Startup + readiness check succeed for a real GGUF. **[deferred → 15.D]**
2. Non-streaming and streaming generation produce correct output.
   *(stub now; real → 15.D)*
3. Cancellation stops generation and frees the slot (verified, not assumed).
   *(stub now; real slot-free → 15.D)*
4. A timeout is handled with a typed error. *(15.6)*
5. Killing the backend is detected; Phase 14 → `Failed`; recovery works.
   **[deferred → 15.D]** *(the lifecycle side is already proven in Phase 14 with
   `FakeBackend`; this is the llama-server-specific exit detection)*
6. Clean shutdown leaves no orphan process (process-list check).
   *(Job Object proven with a non-llama child in 15.4; real llama-server → 15.D)*
7. No file outside `llm/` references llama.cpp. *(15.10 — `git grep`)*
8. TTFT + tokens/sec recorded. **[deferred → 15.D]**
9. Full check suite green; bindings regenerated. *(15.10)*

## ADRs / open questions
- Confirms ADR-0013 transport in practice (TCP + token, no pipe for
  `llama-server`). Raise a new ADR only if the real run shows TCP+token
  unworkable.
- `-ngl` derivation (layer count vs `-1`) — a tuning detail, settled in 15.7,
  refined at Phase 31.
- No new ADR.
