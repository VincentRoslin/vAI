# 02 — Supervising `llama-server` from Rust

Scope: start, stream from, cancel, and recover a `llama.cpp` `llama-server`
child process, owned entirely by the Rust core.

> **Depends on an unresolved Constitution question** — see `docs/research/README.md`
> §"Constitution collision". This doc assumes the *managed model backend* category
> (localhost HTTP allowed). The stdio-only alternative is noted in §7.

---

## 1. Why `llama-server` and not linking `llama.cpp`

| Option | Pros | Cons |
| ------ | ---- | ---- |
| **`llama-server` child process** (HTTP) | Upstream-maintained; process isolation (a crash can't take down the app); trivial to kill/restart; OpenAI-compatible `/v1/...` + native `/completion` with SSE streaming; swap the binary to upgrade | Localhost socket; process management burden; startup latency; JSON-over-HTTP overhead (negligible vs token latency) |
| **`llama-cpp-2` / FFI bindings** | No socket; in-proc; lowest latency | A model crash / OOM can abort the whole app; bindings lag upstream; we own more C++ surface; harder to sandbox GPU use |
| **`llama-server` via stdio proxy** | Satisfies current Constitution text | We'd be writing+maintaining an HTTP↔stdio shim for no real benefit |

**Leaning:** `llama-server` child process. Decision belongs in an ADR +
`AI_PIPELINES.md`.

---

## 2. Startup

- Spawn with `tokio::process::Command`:
  - `kill_on_drop(true)` as a backstop.
  - `stdin(Stdio::null())`, `stdout`/`stderr` piped → drained into structured logs.
  - Bind explicitly: `--host 127.0.0.1 --port 0` is **not** supported by
    llama-server (no ephemeral port) → **we pick a free port** in Rust
    (bind a `TcpListener` to `127.0.0.1:0`, read the port, drop it, pass it) and
    accept the small race, or use a fixed port range with retry. **SPIKE.**
  - Pass model path, `-ngl` (GPU layers), context size, parallel slots, etc. from
    the model registry + resource manager — never hardcoded.
- **Readiness**: do not assume "spawned == ready". Poll `GET /health` (llama-server
  exposes it) until `200`, with a timeout (model load can take 10s–minutes for big
  GGUF). Surface load progress if possible (parse stderr load lines) or at least a
  spinner state via events.
- Windows specifics:
  - No `SIGTERM`. Graceful stop = `taskkill` / `TerminateProcess` semantics.
    Consider a **Job Object** so all descendants die with the parent
    (`windows`/`win32job` crate) — prevents orphaned `llama-server` after an app
    crash. **SPIKE.**
  - Path handling: model paths with spaces/unicode; pass as args not a shell string.

---

## 3. Streaming generation

- Endpoint: `POST /completion` with `"stream": true` → **SSE** (`data: {...}\n\n`
  lines, terminal `data: [DONE]` or a `stop` flag in the JSON).
- Client: `reqwest` with `.bytes_stream()`, or `hyper` directly. Parse SSE
  incrementally (don't wait for EOF). A small SSE parser or the `eventsource-stream`
  crate.
- Each delta → push token into the per-request Tauri **Channel** (topic 01 §4).
- Keep a per-request record: `TaskId`, start time, token count, the `reqwest`
  request future, and a `CancellationToken`.
- Backpressure: if the UI channel lags, we still must drain the HTTP body (or the
  server stalls). Bounded buffer + drop-oldest or coalesce policy — **decide**.

---

## 4. Cancellation

Two layers, both needed:
1. **HTTP-level**: drop the response future / abort the `reqwest` request →
   closes the connection. llama-server detects the client disconnect and stops
   that slot's generation. Verify this actually frees the slot promptly. **SPIKE.**
2. **App-level**: `tokio_util::sync::CancellationToken` per task; `cancel(taskId)`
   command trips it; the streaming loop `select!`s on it and stops pushing to the
   Channel, marks the task `Cancelled`, emits a lifecycle event.

Edge cases to test: cancel before first token; cancel after `[DONE]` in flight;
cancel during model load; double cancel; cancel a taskId that already finished.

---

## 5. Crash recovery

- Detect via **three signals**: `child.wait()` resolves unexpectedly; `/health`
  starts failing; a streaming request errors with connection reset.
- On crash:
  - Mark the owning model `Failed` in the lifecycle manager; fail all in-flight
    tasks for that backend with a typed `Backend { subsystem: "llm" }` error.
  - **Release the VRAM reservation** (topic 05) — the process is gone, so is its
    memory, but our bookkeeping must catch up (reconcile against `nvidia-smi`).
  - Restart policy: bounded retries with backoff (e.g. 3 attempts, exponential),
    then park in `Failed` and require user action. Never infinite-restart-loop.
  - Distinguish "crashed" from "we killed it" so an intentional unload isn't
    treated as a fault.
- Health-check cadence: lightweight `/health` every N seconds while `Loaded`;
  faster probing while `Loading`.

---

## 6. Ownership boundaries (unchanged)

- Only the **Model Lifecycle Manager** starts/stops a `llama-server`.
- All GPU-layer / context decisions come from the **Resource Manager**.
- llama.cpp-specific flags and quirks live **only** in the adapter module; the
  rest of the app sees a clean `LlmBackend` trait (`generate`, `cancel`,
  `health`, `shutdown`).

---

## 7. Alternative: stdio-only (if the Constitution stays as written)

- Run `llama-server` but wrap it in a tiny Rust-spawned proxy? No — still a socket.
- Use `llama-cli` in a loop over stdin/stdout? Loses slots, streaming control,
  health, and is not designed for long-lived serving.
- Use FFI bindings (`llama-cpp-2`) — no socket at all, but §1's cons apply
  (in-proc crash blast radius).
- **If stdio is mandatory, FFI is the only real option** and the crash-isolation
  loss must be accepted or mitigated (run inference on a dedicated thread with
  catch + process-level watchdog).

---

## 8. Crates to evaluate
`tokio` (process, sync), `tokio-util` (CancellationToken), `reqwest` (stream) or
`hyper`, `eventsource-stream`, `windows` / `win32job` (Job Objects), `nvml-wrapper`
(topic 05).
