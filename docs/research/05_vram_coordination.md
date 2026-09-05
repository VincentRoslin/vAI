# 05 — VRAM coordination for model hot-swapping

Scope: the resource manager's job — decide whether an operation can safely use the
GPU, and prevent CUDA OOM when swapping models. Target hardware (env audit): single
**RTX 5080, 16 GB VRAM**, WDDM driver, CUDA 13.3 runtime.

---

## 1. Why this is hard / what NOT to assume

- **Model file size ≠ runtime VRAM.** GGUF on disk + KV cache (grows with context
  length × batch) + CUDA context (~300–600 MB) + compute buffers + fragmentation.
  A 9 GB file can need 12–14 GB resident at 8k context.
- **WDDM shares VRAM with the desktop.** The compositor, browser, other apps take
  1–3 GB and it fluctuates. `nvidia-smi` "free" is a moving target.
- **Allocation is not atomic across processes.** Between "we checked free VRAM" and
  "llama-server actually allocates", another process (or our own image worker) can
  grab memory → OOM.
- CUDA OOM in llama.cpp / PyTorch is often **fatal to that process** (not a
  recoverable `Err`). So the strategy is *prevention*, plus *clean detection* when
  it happens anyway.

---

## 2. Lifecycle model (from the guide)

```
request → estimate → reserve → commit → observe → release
                                   ↘ reconcile ↗
```

- **estimate**: predicted VRAM for this model+config (see §3).
- **reserve**: a logical booking in the resource manager (a number, held under a
  lock), *before* spawning the backend. Includes a **safety margin**.
- **commit**: backend loaded, `/health` OK; replace the estimate with the first
  real `nvidia-smi` delta measurement.
- **observe**: periodic real measurement while loaded; update the accounting.
- **release**: on unload/crash/cancel, free the reservation.
- **reconcile**: periodically compare (sum of reservations) vs (measured used);
  large drift → log, and trust the measurement; catch leaked/stale reservations.

---

## 3. Estimation

Inputs from the model registry + request:
- quantization + parameter count → base weights bytes (roughly file size for GGUF).
- `n_gpu_layers` (may be partial offload → only some layers count).
- context length × `n_parallel` → KV cache bytes (formula per architecture:
  `2 × n_layers × n_kv_heads × head_dim × ctx × parallel × dtype_bytes`).
- fixed overhead constant (CUDA context + buffers), calibrated empirically.

Approach: a conservative closed-form estimate, then **learn a correction factor**
per model from observed commits (store measured-vs-estimated in the DB, refine).
Never treat the estimate as exact — that's what the margin is for.

---

## 4. Measurement

- **`nvml-wrapper`** crate (bindings to NVIDIA NVML, same source as `nvidia-smi`):
  `Device::memory_info()` → `{ total, free, used }`; per-process usage via
  `running_compute_processes()`.
- Poll cadence: ~1–2 s while any model loaded or loading; back off when idle.
- Track: total, free, used, our-attributed-used (sum of committed backends),
  headroom = free − margin.
- Blackwell/5080 + NVML on driver 610.88: **SPIKE** — confirm `nvml-wrapper`
  version supports the installed driver and returns per-process memory on WDDM
  (per-process can be N/A on consumer WDDM — the process list above showed `N/A`
  GPU-memory for compute procs). If per-process is unavailable, fall back to
  whole-GPU accounting + our own reservation ledger.

---

## 5. The hot-swap decision

To load model B while model A is loaded:

1. `need_B = estimate(B) + margin`.
2. `available = measured_free`  (not `total − our_reservations` — use the real
   number, it accounts for the desktop).
3. If `need_B ≤ available` → reserve, spawn, commit. Done (both resident).
4. Else → **must evict**. Policy options (ADR):
   - LRU: unload least-recently-used model(s) until `need_B` fits.
   - Explicit: only ever one LLM resident; loading B always unloads A.
   - User-pinned models exempt from eviction.
5. Eviction = graceful unload of A (finish or cancel A's in-flight tasks first —
   don't yank memory from under a running generation), wait for the process to
   exit, **wait for `nvidia-smi` free to actually recover** (WDDM can lag by
   100s of ms), re-measure, then load B.
6. Timeout / can't-fit-even-after-eviction → typed `ResourceExhausted` error to
   the caller, A stays loaded.

**Serialize load/unload operations** through a single async mutex / actor — no two
swaps in flight at once, no image-gen worker spawning mid-swap. All GPU consumers
(LLM backend, image worker, future embedders) go through the same manager.

---

## 6. When OOM happens anyway

- Detect: process exits during load, or stderr shows `CUDA error: out of memory` /
  `cudaMalloc failed`.
- Treat as a failed load: release reservation, mark model `Failed`, **bump that
  model's correction factor up** (it needed more than we thought), surface a clear
  "not enough VRAM — free up X GB or use a smaller quant" message.
- Never auto-retry the same config immediately (will OOM again). Retry only after
  eviction frees measurable space.

---

## 7. Concurrency test matrix (guide Phase 12)
insufficient VRAM; duplicate reservation for same model; two concurrent load
requests; failed load releases reservation; cancel during load releases; stale
reservation after process crash; reconcile after external app grabs 4 GB; swap
while a generation is streaming; image-gen + LLM both requesting GPU.

---

## 8. Crates
`nvml-wrapper`, `tokio` (Mutex, actor), `sysinfo` (system RAM, as a secondary
constraint for CPU-offloaded layers).
