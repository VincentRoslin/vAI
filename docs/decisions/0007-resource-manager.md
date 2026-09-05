# ADR-0007 — Resource manager: nvml + reservation ledger

- **Status:** ACCEPTED (Phase 5 freeze, 2026-09-05; superseding attacks folded in via Phase 4) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/05_resource-vram.md` (D-6)

## Context
16 GB shared with the desktop (~13–14 GB effective). LLM ⟂ active image
generation. Model file size ≠ runtime VRAM (NFR-35). Per-process VRAM on
Blackwell/WDDM is unconfirmed (env audit showed N/A in `nvidia-smi`).

## Options considered
- Measurement: `nvml-wrapper` per-process vs whole-GPU + own ledger.
- Estimate: closed-form vs pure measurement-driven.

## Decision
- **`nvml-wrapper`** for `{total, free, used}` — **confirmed working** (probe
  2026-09-05, `docs/verification/02_phase3_probes.md`).
- **Whole-GPU accounting + our own reservation ledger** — this is **the** model.
  Per-process VRAM attribution is **confirmed unavailable** on this driver
  (610.88) + WDDM: every process returns `used_gpu_memory = Unavailable`. The
  resource manager reads `free`/`used` for ground truth, tracks what *we* loaded
  in the ledger, and on an unexplained shortfall trusts the measurement +
  reconciles (it cannot identify an external holder).
- **Lifecycle**: request(estimate) → reserve (ledger, pre-spawn) → commit (first
  real Δ measurement) → observe (poll ~1–2 s) → release → reconcile (periodic:
  ledger vs measured; recover stale).
- **Estimate** = closed-form per model type (weights + KV cache + CUDA context +
  buffers + margin), with a **learned per-model correction factor** from stored
  `(estimated, measured)` pairs.
- **One async mutex/actor** serializes every reserve/commit/release/swap. Lock
  ordering is explicit (ADR-0010, Phase 4 R-C4): the mutex holder performs the
  whole transition; no worker or nested call re-enters it.
- `vram_safety_margin_mb` is config (default ~1500).
- **System RAM is a second tracked constraint** (Phase 4 R-C3): CPU-offloaded
  models (Krea 2) live in RAM; a 32 GB machine can exhaust it. The resource
  manager checks free system RAM before an image load and won't keep the sidecar
  resident under RAM pressure.
- **GPU-reset / driver-TDR path** (Phase 4 R-H1): if *all* GPU consumers' health
  checks fail within a short window, treat it as a device reset → kill every GPU
  subprocess, `reconcile` from zero, reload the last-known desired state, surface
  one "graphics driver reset — recovering" notice.
- The resource manager **accounts**; the scheduler (ADR-0010) **orders + evicts**.

## Consequences
- Works without per-process VRAM (**confirmed the case here**).
- External apps grabbing VRAM are handled by trusting measured `free` +
  reconcile; we cannot name the offender, only see the shortfall.
- Estimates start conservative (waste some margin) and tighten over time.
- KV-cache estimation inputs (n_layers, n_kv_heads, head_dim, context) come from
  the GGUF header (probe confirmed parseable — ADR-0008).
- A WDDM free-memory settle wait is needed in the swap path (ADR-0010).
- **WDDM hang risk** (Hyper-V enabled on host) is inconclusive from inspection →
  Phase 15 watch item + Phase 4 risk (`docs/verification/02_phase3_probes.md`).
