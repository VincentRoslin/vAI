# ADR-0006 — Image subsystem: diffusers sidecar, Krea 2 Turbo NF4

- **Status:** PROPOSED (Phase 3 draft) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/04_image-generation.md` (D-5)

## Context
One shared image subsystem serves Tab 2 (standalone) and Tab 3 (character
images). Owner has a working diffusers FastAPI sidecar (Krea 2 Turbo, bitsandbytes
NF4 + CPU offload, ~11.4 GB peak / ~1.6 GB idle, ~17–18 s/image, single LoRA).
Krea 2 is text-to-image only; no identity mechanism. Z-Image is out of scope.

## Options considered
- Runtime: reuse the diffusers sidecar vs ComfyUI headless vs a fresh worker.
- Quant: keep bitsandbytes NF4 vs torchao NVFP4 vs prebuilt checkpoint.
- Load cost: re-quantize every load (~90 s) vs cache the quantized weights.
- LoRA: single-select (current) vs multi-LoRA stacking.

## Decision
- **Reuse the diffusers FastAPI sidecar pattern**, adapted: Rust-supervised child,
  **loopback HTTP** (ADR-0013), `enable_model_cpu_offload()` kept.
- **Krea 2 Turbo, 8 steps, guidance 0.0**, FlowMatch Euler; only image model.
- **Quantization: bitsandbytes NF4 for v1** (proven). **The NF4 quant cache is
  REQUIRED, not an optimization** (Phase 4 R-C3): quantize the ~24 GB bf16 model
  **once during acquisition**, persist the NF4 weights, load NF4 directly (~6 GB)
  thereafter. The bf16 model is resident only during that one-time step, behind a
  system-RAM pre-check. **Benchmark torchao NVFP4 in Phase 22**; adopt if stable
  on Krea 2.
- **LoRA registry** (SQLite: id → file, base compat, format, default weight,
  tags) + **preset registry** (named parameter sets). **Single-select LoRA in v1
  UI**; the request contract carries a list from day one (multi-LoRA is a
  non-breaking later add). Kohya→diffusers converter reused.
- **Sidecar stays alive** (~1.6 GB) while the Image or Discovery tab is active;
  the scheduler evicts the LLM (+ TTS) only for the ~11.4 GB generation spike.
- **The image server exposes `generate` only.** All load/unload/eviction
  decisions belong to the Rust scheduler + resource manager (Phase 4 R-H8) — the
  owner's `exclusive_vram` behaviour is reimplemented as a scheduler contract,
  not kept in the Python server. No self-managed VRAM in the sidecar.
- Output → content-addressed blob store; params + seed + hash + (identity score)
  → SQLite.

## Consequences
- Image generation **cannot coexist with the LLM** → the scheduler's evict/restore
  is on the critical path (ADR-0010).
- Character visual identity is **not solved by this ADR** — see ADR-0011; it is
  the biggest open risk.
- The installer carries diffusers + torch + bitsandbytes (~4–8 GB, ADR-0014).
- `model` field kept in the API for forward-compat though only Krea 2 ships.
