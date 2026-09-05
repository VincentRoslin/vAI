# ADR-0004 — Build llama.cpp from source, pinned, sm_120

- **Status:** PROPOSED (Phase 3 draft) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/02_llm-runtime.md` (D-3b) · resolves part of **O5**

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
