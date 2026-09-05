# ADR-0014 — Packaging: MSI, embedded CPython + shared venv

- **Status:** PROPOSED (Phase 3 draft) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/12_packaging.md` (D-13)

## Context
Windows installer bundling the Tauri app, `llama-server` + CUDA DLLs, and Python
AI workers (diffusers/torch, faster-whisper, Chatterbox, face-embedder). Models
are NOT bundled (acquired on first run). Must support a fully-offline install.

## Options considered
- Bundler: MSI (WiX) vs NSIS.
- Python: embedded CPython + frozen venv vs PyInstaller per worker vs `uv`-on-install.

## Decision
- **MSI (WiX)** — NSIS has a known Tauri v2 bug where `externalBin` sidecars
  aren't replaced on upgrade (stale Python server). Re-check the bug before
  Phase 37.
- **Embedded CPython + one shared frozen venv** (torch, diffusers, bitsandbytes,
  faster-whisper + CTranslate2 CUDA-12, Chatterbox, InsightFace). Workers are
  scripts: `<app>/python/python.exe <app>/workers/<name>.py`.
- **Deterministic sibling layout** resolved from `current_exe()`, never PATH:
  `bin/` (llama-server + CUDA DLLs), `python/`, `workers/`, `webview2/`.
- **First run**: layout + worker self-check → model onboarding (LLM picker; voice
  models one-click; Krea 2 explicit ~34 GB) → all skippable → app usable for
  what's present. A "models folder" setting supports pre-downloaded/USB models.
- **Code signing**: pipeline step **stubbed** (no-op without a cert); unsigned →
  SmartScreen warning, documented. Owner to decide on a certificate.
- Installer size without models ≈ **7–12 GB**.

## Consequences
- Large installer — intrinsic to a local GPU AI app.
- One shared venv (not per-worker) saves several GB vs PyInstaller.
- ~1 GB CUDA-11/12 (CTranslate2) vs CUDA-13 (llama.cpp) overlap accepted, or
  reconciled later.
- Delta updates: app binary via MSI patch without re-shipping the Python runtime.
- A Windows firewall prompt on first loopback bind → prefer named pipes
  (ADR-0013) or pre-authorize in the installer.
