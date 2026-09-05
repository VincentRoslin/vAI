# 03 — Python worker lifecycle: non-blocking stdin/stdout JSON-lines

Scope: STT, TTS, image-generation workers. Python, stateless, Rust-supervised,
**JSON-lines over stdin/stdout only** (this fits the Constitution as written).

---

## 1. Protocol

- **Framing**: one JSON object per line (`\n`-terminated), UTF-8. No embedded
  newlines (compact `json.dumps`). This is the entire wire format.
- **Direction**: Rust → worker on stdin = requests; worker → Rust on stdout =
  responses/events. **stderr = logs only**, never parsed for control flow.
- **Message shape** (sketch, mirror of the IPC contracts):
  ```
  → {"type":"request","id":"<taskId>","op":"transcribe","params":{...}}
  → {"type":"cancel","id":"<taskId>"}
  ← {"type":"progress","id":"<taskId>","pct":0.4}
  ← {"type":"chunk","id":"<taskId>","data":{...}}     # streaming partial output
  ← {"type":"result","id":"<taskId>","data":{...}}
  ← {"type":"error","id":"<taskId>","error":{"kind":"...","message":"..."}}
  ← {"type":"ready","protocol":1,"capabilities":[...]}  # sent once on startup
  ```
- **Correlation**: every message carries `id`; a worker may process one request at
  a time (simplest, recommended) or N concurrent — decide per worker type. STT/TTS
  = one at a time; image-gen = one at a time (GPU-bound anyway).
- **Handshake**: worker emits `ready` with a protocol version; Rust refuses a
  mismatch. Guards against a stale venv.

---

## 2. Rust side (non-blocking)

- `tokio::process::Command`, all three stdio piped, `kill_on_drop(true)`.
- stdout: `BufReader::new(stdout).lines()` in a dedicated task → parse each line →
  route by `id` to the waiting task's channel. A malformed line is logged and
  skipped, not fatal (but count them; many = kill + restart).
- stdin: an `mpsc` channel feeds a writer task; `write_all(line).await` +
  `flush().await`. Never block the supervisor on a slow worker read.
- stderr: drained line-by-line into structured logs, tagged with worker id.
- Per-worker actor: owns the child, a `HashMap<TaskId, oneshot/mpsc sender>`, and
  restart state. Public API is a trait (`SttWorker::transcribe(...) -> Stream`).

## 3. Python side (non-blocking)

- The naive `for line in sys.stdin` **blocks** the whole worker — can't read a
  `cancel` while generating. Options:
  | Approach | Notes |
  | -------- | ----- |
  | **Reader thread + `queue.Queue`** | Simplest. One thread does blocking `stdin.readline()`, puts messages on a queue; main loop polls the queue between work chunks and checks a `threading.Event` for cancel. Works for CPU/GPU loops that can yield periodically. |
  | **`asyncio` + `loop.connect_read_pipe`** | Clean for IO-bound; awkward for long synchronous model calls (need `run_in_executor`). |
  | **`selectors` on `sys.stdin`** | POSIX-friendly; **on Windows `selectors` can't poll pipes reliably** → the thread approach is the portable choice. |
- **Flush every write** (`print(json, flush=True)` or `sys.stdout.write` + flush) —
  Python buffers stdout when not a TTY; without flush Rust sees nothing until exit.
- Cooperative cancellation: model inference loops need a callback/hook to check the
  cancel event (whisper.cpp/faster-whisper expose limited hooks; diffusers has a
  `callback_on_step_end`). Where no hook exists, cancellation = kill the process.

---

## 4. Lifecycle

- **Spawn**: lazily on first use, or eagerly at app start — decide per worker
  (image-gen is heavy → lazy; STT maybe eager for voice latency).
- **Idle shutdown**: unload after N minutes idle to free VRAM/RAM; re-spawn on
  demand. Coordinated with the resource manager.
- **Health**: periodic `{"type":"ping"}` → `{"type":"pong"}` with a timeout; no
  pong → kill + restart.
- **Crash**: `child.wait()` resolves → fail in-flight tasks (typed error), release
  resource reservations, bounded-backoff restart, park in `Failed` after N tries.
- **Shutdown**: send `{"type":"shutdown"}`, wait briefly for clean exit, then kill.
  On Windows use a Job Object (topic 02 §2) so workers die with the app.

---

## 5. Environment (from the env audit — open decision)

- Bare `C:\Program Files\Python311` vs per-worker **venv** vs **`uv`** vs a
  **bundled/embedded** Python for distribution.
- For dev: a `uv`-managed venv per worker (`workers/stt/`, `workers/tts/`, …) with
  pinned `requirements.txt` / `uv.lock`.
- For the shipped app: embedded Python or PyInstaller-frozen workers — big open
  question, affects installer size and GPU library loading (CUDA DLLs). **ADR.**
- Rust must locate the interpreter deterministically (config, not PATH-guessing).

---

## 6. Things to test (guide Phase 18 intent)
recording cancellation; STT failure; TTS failure; worker crash mid-request;
malformed line from worker; worker never sends `ready`; slow worker (stdin
backpressure); kill during model load; two requests racing; unicode in payloads;
very large payloads (push binary to files, pass paths — topic 04).
