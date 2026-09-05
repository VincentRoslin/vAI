# Phase 22 — Image Generation (FLUX.1 Krea)

> **Architecture frozen at Phase 5.** Governing: **ADR-0006** (diffusers sidecar,
> Krea 2 Turbo, **NF4 quant cache required**, `generate`-only / no self-managed
> VRAM), **ADR-0013** (transport), **ADR-0010** (scheduler eviction),
> `AI_PIPELINES.md` §5. The owner supplied the reference implementation's
> specifics in chat (2026-09-05) — do **not** read their other project directly.
> **Before downloading Krea 2 / any LoRA / the NF4 cache: ask the owner whether
> they already have the files locally** (`CLAUDE.md` → "Model / asset downloads").
> Benchmark torchao NVFP4 vs NF4 here (deferred item).

## Objective
Local image generation as a resource-managed workload: an isolated worker running
FLUX.1 Krea, integrated with the resource manager, model lifecycle, and storage
(filesystem blob + SQLite metadata). Cancellation releases VRAM.

## Depends on
Phase 13 (resources), Phase 14 (lifecycle), Phase 9 (persistence + blob store),
Phase 3.10 (adaptation plan for the owner's implementation).

## Not in this phase
- Character identity conditioning (Phase 27).
- Typed image actions from the LLM (Phase 28).
- Hot-swapping the LLM out to fit the image model (Phase 23) — this phase may
  require the LLM be unloaded manually; 23 automates it.

## Architecture notes
- The owner's implementation is adapted, not rewritten wholesale — reuse what
  works, rewrite only what violates the boundaries (must be an isolated
  subprocess, no direct DB, no independent GPU management, Rust-supervised).
- Generated binaries go to the content-addressed blob store; SQLite holds the
  metadata + path + hash + generation parameters.
- Generation parameters are typed data, not free-form strings.

## Performance notes
- Record VRAM footprint of FLUX.1 Krea loaded, and generation time per image at
  the target resolution.
- Determine whether it can coexist with a loaded LLM in 16 GB — if not, that is
  the Phase 23 requirement.

## Step outline (frame — refine against the owner's code)
1. Receive + review the owner's FLUX.1 Krea implementation; map it to the
   adaptation plan from Phase 3.10.
2. Wrap generation as an isolated worker (transport per ADR); `ready` handshake.
3. Typed generation-request contract (prompt, size, seed, steps, params).
4. Resource-manager integration: reserve VRAM before load, release after.
5. Model-lifecycle integration: the image model is a managed model.
6. Storage: write the image to the blob store; record metadata + path + hash +
   params in SQLite.
7. Progress events (step N of M) to the UI.
8. Cancellation mid-generation → worker abandons, VRAM released, no partial file
   in the store.
9. Failure handling: worker crash, OOM, invalid params.
10. Tests + a manual generation run recording VRAM + timing.

## Verification gate
1. A generation request produces an image via the worker.
2. VRAM is reserved before load and released after (observed via the resource
   manager).
3. The image is in the blob store; SQLite has correct metadata + path + hash +
   params.
4. Cancelling mid-generation releases VRAM and leaves no partial file.
5. Worker crash / OOM / invalid params each produce a typed error, no hang.
6. FLUX.1 Krea VRAM footprint + per-image generation time recorded; LLM
   coexistence answer recorded.

## ADRs / open questions
- What from the owner's implementation is reused vs rewritten (recorded as an
  ADR).
- LLM/image VRAM coexistence → drives Phase 23.
