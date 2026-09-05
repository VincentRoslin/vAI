# Phase 3 · LLM runtime — integration mode & CUDA build

Covers step 3.4 and D-3. Requirements: FR-10..13, FR-16, FR-62..64, NFR-21,
NFR-40, NFR-52; O3, O5. Machine: RTX 5080 16 GB, Blackwell sm_120, CUDA runtime
13.3.

---

## D-3a — Integration mode (resolves O3)

| Option | How | Pros | Cons |
| ------ | --- | ---- | ---- |
| **`llama-server` child process** (loopback HTTP) | Rust spawns `llama-server` bound to `127.0.0.1:<free port>`, supervises it, talks OpenAI-compatible `/v1/...` + native `/completion` (SSE) | Upstream-maintained; **crash isolation** (a model crash / CUDA OOM kills only the child); trivial kill/restart; slots, health, streaming all built-in; swap binary to upgrade | localhost socket; port management; process lifecycle burden; ~a few ms JSON/HTTP overhead (negligible vs token latency) |
| **`llama-cpp-2` FFI** | Link llama.cpp into the Rust process | No socket; lowest latency; in-proc control | **A CUDA OOM or llama.cpp assert can abort the whole app** — unacceptable given the 16 GB budget and frequent hot-swaps; bindings lag upstream; we own more C++ surface; harder to bound |
| stdio proxy around `llama-server` | — | satisfies a stdio-only rule | pure overhead, nothing gained |

**Recommendation: `llama-server` as a Rust-supervised child process over a
loopback-only HTTP socket.** The crash-isolation argument is decisive here: this
app deliberately pushes VRAM to the limit and swaps models constantly (Krea 2
eviction, see `04_image-generation.md`), so an LLM OOM must not take down the
conversation UI, the scheduler, or an in-flight image job. `CLAUDE.md` Article I
already carves out "managed model backend, loopback socket" as a valid category.

Implementation notes for Phase 15:
- Rust binds a `TcpListener` to `127.0.0.1:0`, reads the port, drops it, passes it
  to `llama-server` (llama-server has no ephemeral-port mode); accept the tiny
  race or use a fixed range + retry.
- `tokio::process::Command`, `kill_on_drop(true)`, stdout/stderr → structured logs.
- **Windows Job Object** (`windows`/`win32job` crate) so `llama-server` dies with
  the app even on a hard crash — no orphaned process holding 10 GB of VRAM.
- Readiness: poll `/health` until 200 with a generous timeout (large GGUF load can
  take tens of seconds).
- Streaming: `reqwest` `bytes_stream()` + an SSE parser → push deltas into the
  per-request Tauri Channel.
- Cancellation: drop the `reqwest` request (closes the connection; llama-server
  frees the slot — **verify promptly in Phase 15**) **and** trip a
  `CancellationToken`.
- Crash detection: `child.wait()` resolves unexpectedly, or `/health` fails, or a
  stream errors → report to the lifecycle manager (Phase 14), release the VRAM
  reservation, fail in-flight tasks with a typed `AppError`.

**Fallback:** if `llama-server` proves unworkable on Windows/Blackwell, fall back
to `llama-cpp-2` FFI with inference on a dedicated thread + a process-level
watchdog. Documented, not preferred.

→ **ADR-0003**.

---

## D-3b — CUDA build strategy (resolves part of O5)

Findings: Blackwell **sm_120** needs CUDA **12.8+** or 13.x; llama.cpp builds with
`-DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=120`. Community Windows prebuilts for
sm_120 exist (e.g. Andgihat's build; llama-cpp-python Blackwell wheels). Our env
has the CUDA **runtime** 13.3 (driver) but **no CUDA Toolkit / `nvcc`**.

| Option | Pros | Cons |
| ------ | ---- | ---- |
| **Build llama.cpp from source** (install CUDA Toolkit 13.x, ~3 GB) | Exact version pin; reproducible; we control flags (FP4 tensor cores, flash-attn); no trust in a third-party binary | Adds a ~3 GB toolchain dependency for dev; CI needs it; build time |
| **Vendor a pinned community/official prebuilt** `llama-server.exe` + CUDA DLLs | No toolkit needed; fast | Trust + supply-chain surface; must track upstream; sm_120 prebuilts are newish |
| **Official llama.cpp release binaries** (if they now ship sm_120 CUDA Windows builds) | Maintained, signed-ish | Verify they cover sm_120; still a vendored binary |

**Recommendation:** **build from source, pinned to a specific llama.cpp commit +
CUDA Toolkit 13.x**, with the build scripted and documented (`DEVELOPMENT.md`).
Reproducibility matters for a project that will run for months and needs the
Phase 31 performance numbers to be stable. Ship the built `llama-server.exe` +
required CUDA DLLs in the installer (Phase 37). Keep "vendor a prebuilt" as the
fallback if the source build is too painful on this toolchain.

→ **ADR-0004**: build llama.cpp from source, pinned commit + CUDA Toolkit 13.x,
scripted; installer bundles the binary + DLLs.

---

## Model format & selection (FR-16, FR-60)
- **GGUF** only (owner decision). Quant target for a ~8–14B model on 16 GB with
  headroom for KV cache: **Q4_K_M** (~4.5–8 GB weights) or Q5_K_M for smaller
  models. The registry stores quant + estimated VRAM (Phase 11/12).
- Mid-conversation model switch (FR-16): the conversation engine re-truncates
  history to the new model's context window and continues; chat-template
  differences are handled by the adapter (each backend/model advertises its
  template). Record as a small risk for Phase 4 (template mismatch → garbled
  output).
- `-ngl` (GPU layers) and context size come from the resource manager's estimate
  (see `05_resource-vram.md`), never hardcoded.

## Optimizations
1. **Flash attention** (`-DGGML_CUDA_FA_ALL_QUANTS` / `--flash-attn`) — faster
   prefill + less KV memory. On by default for supported models.
2. **KV-cache quantization** (`--cache-type-k q8_0 --cache-type-v q8_0`) — roughly
   halves KV memory → longer context in the same budget, or headroom for voice.
   Measure quality impact in Phase 15.
3. **Blackwell FP4 tensor-core build** — the sm_120 community builds add TurboQuant
   KV compression + MTP (multi-token prediction). Evaluate MTP for throughput once
   the base build is stable.
4. **Keep `llama-server` warm** — only ever unload it for an image generation.
   Persona/model switches reuse the running server where the model is unchanged.
5. **`--parallel` slots + context reuse** — one server serving multiple
   conversations without reload; prompt-cache the shared persona prefix.
6. **`-ngl` from measured free VRAM**, not a fixed number — offload the maximum
   layers that fit, leaving KV + margin.
7. **Speculative decoding** with a tiny draft model — only if VRAM allows
   (unlikely alongside everything else on 16 GB; note for a future bigger card).

## Failure modes
- CUDA OOM on load → child exits; caught; reservation released; typed "not enough
  VRAM, free X GB or pick a smaller quant" message; **no auto-retry same config**.
- `llama-server` hangs (no crash, no tokens) → health-check + generation timeout →
  kill + restart, bounded.
- Orphan `llama-server` after app crash → Job Object prevents it.
- Slot not freed on client disconnect → verified in Phase 15; if real, send an
  explicit stop to the slot endpoint.

## Sources
- [llama.cpp #22696 — Blackwell sm_120 Windows CUDA workarounds](https://github.com/ggml-org/llama.cpp/issues/22696)
- [sm_120 Windows prebuilt (Andgihat)](https://github.com/Andgihat/llama-cpp-mtp-turboquant-sm120-blackwell-windows)
- [Build llama.cpp from source with CUDA (2026)](https://bestllmfor.com/how-to/install-llama-cpp-from-source/)
