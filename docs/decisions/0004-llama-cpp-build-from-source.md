# ADR-0004 — llama.cpp binary: pinned, sm_120 (prebuilt for v1, source build documented)

- **Status:** ACCEPTED (Phase 5 freeze, 2026-09-05) · **AMENDED 2026-09-06** (Phase 15.D, owner-approved) — a **pinned official prebuilt** is the primary path for v1; the source build stays the documented, supported alternative.
- **Research:** `docs/research/phase3/02_llm-runtime.md` (D-3b) · resolves part of **O5**

## Amendment (2026-09-06)
The dev machine still has no CUDA Toolkit / `nvcc`, and the official
`ggml-org/llama.cpp` releases now ship a **CUDA 13.3** Windows x64 build that
runs on Blackwell sm_120. Installing a 3 GB Toolkit + running a 10–40 min build
to get the same artifact is not worth it for a single-dev v1. **Decision:** use a
**pinned official prebuilt** (`llama-<tag>-bin-win-cuda-13.3-x64.zip` +
`cudart-…-13.3-x64.zip`), pinned by release tag + recorded asset SHA-256s in the
phase-15 verification doc. First pin: **`b10819`** (commit `6a1a922d2`). The
source-build recipe stays in `docs/spec/DEVELOPMENT.md` §5 as the fallback and
the path for reproducing/patching. Installer packaging (ADR-0014) is unchanged —
it bundles whichever binary is pinned. Supply-chain note: official GitHub
release, SHA-256 recorded; a pin bump is a deliberate, reviewed change.

## Context
Blackwell sm_120 needs CUDA 12.8+ / 13.x and an explicit
`-DCMAKE_CUDA_ARCHITECTURES=120` build. The dev machine has the CUDA **runtime**
13.3 but no Toolkit / `nvcc`. Community Windows sm_120 prebuilts exist but carry a
supply-chain + tracking cost.

## Options considered
- Build from source (install CUDA Toolkit 13.x, ~3 GB).
- Vendor a pinned community/official prebuilt `llama-server.exe` + DLLs.

## Decision
**Build `llama-server` from source**, pinned to a specific llama.cpp commit +
CUDA Toolkit 13.x, with the build **scripted and documented** in `DEVELOPMENT.md`.
Flags: `-DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=120` + flash-attention. The
built binary + required CUDA DLLs are bundled by the installer (ADR-0014).

**Fallback:** vendor a pinned community prebuilt if the source build is too
painful on this toolchain.

## Consequences
- Reproducible performance numbers (Phase 31 depends on this).
- Dev + CI need CUDA Toolkit 13.x (~3 GB) — documented prerequisite.
- We own the upgrade cadence (pin bump = deliberate).
- Installer carries ~200–500 MB of binary + CUDA DLLs.
