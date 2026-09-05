# ADR-0007 — Resource manager: nvml + reservation ledger

- **Status:** PROPOSED (Phase 3 draft) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/05_resource-vram.md` (D-6)

## Context
16 GB shared with the desktop (~13–14 GB effective). LLM ⟂ active image
generation. Model file size ≠ runtime VRAM (NFR-35). Per-process VRAM on
Blackwell/WDDM is unconfirmed (env audit showed N/A in `nvidia-smi`).

## Options considered
- Measurement: `nvml-wrapper` per-process vs whole-GPU + own ledger.
- Estimate: closed-form vs pure measurement-driven.

## Decision
- **`nvml-wrapper`** for `{total, free, used}`. **Whole-GPU accounting +
  our own reservation ledger** as the primary model; use per-process data as a
  bonus **only if a runtime probe confirms it works** on driver 610.88 (Phase 3
  exit item).
- **Lifecycle**: request(estimate) → reserve (ledger, pre-spawn) → commit (first
  real Δ measurement) → observe (poll ~1–2 s) → release → reconcile (periodic:
  ledger vs measured; recover stale).
- **Estimate** = closed-form per model type (weights + KV cache + CUDA context +
  buffers + margin), with a **learned per-model correction factor** from stored
  `(estimated, measured)` pairs.
- **One async mutex/actor** serializes every reserve/commit/release/swap.
- `vram_safety_margin_mb` is config (default ~1500).
- The resource manager **accounts**; the scheduler (ADR-0010) **orders + evicts**.

## Consequences
- Works without reliable per-process VRAM (the likely case here).
- External apps grabbing VRAM are handled by trusting measured `free` +
  reconcile.
- Estimates start conservative (waste some margin) and tighten over time.
- A WDDM free-memory settle wait is needed in the swap path (ADR-0010).
- **Phase 3 exit items**: nvml per-process probe; confirm no WDDM hang on the 5080
  bare-metal compute path; first-pass calibration constants.
