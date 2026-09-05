# Phase 3 · Packaging (Windows installer)

Covers step 3.14 and D-13. Requirements: NFR-1, NFR-52, NFR-60; ARQ-19.

---

## What has to ship

| Component | Size (approx) | Notes |
| --------- | ------------- | ----- |
| Tauri app (Rust core + WebView2 frontend) | ~15–30 MB | WebView2 runtime present on Win 11; bundle the evergreen bootstrapper as fallback |
| `llama-server.exe` + CUDA DLLs | ~200–500 MB | built from source, pinned (see `02`); CUDA runtime DLLs (cublas, cudart) |
| Python image server (diffusers + torch + bitsandbytes) | **~4–8 GB** | torch + CUDA wheels dominate; this is the heavy part |
| Python STT worker (faster-whisper + CTranslate2 CUDA-12 runtime) | ~1–2 GB | CUDA 12 CTranslate2 libs |
| Python TTS worker (Chatterbox) | ~0.5–1 GB | |
| face-embedder (InsightFace-class) | ~0.1 GB | for the identity gate |
| **Models** | **NOT bundled** | acquired on first run (`06`); Krea 2 ~34 GB, LLM user-chosen, STT/TTS pinned |

Installer without models: **~7–12 GB**. This is large but unavoidable for a local
GPU AI app; models on top are user-managed.

---

## D-13 — Design

### Bundler
- **MSI (WiX)**, not NSIS. Reason: the known Tauri v2 bug where **NSIS doesn't
  replace `externalBin` sidecars on upgrade** → a stale Python server after an
  update. MSI handles component replacement correctly. (Re-check if the NSIS bug
  is fixed before Phase 37.)

### Python workers — how they ship
Options:
| Option | Pros | Cons |
| ------ | ---- | ---- |
| **Embedded CPython + a frozen venv per worker** (or one shared venv) | No system Python needed; deterministic; Rust locates it by a known relative path | Large; must vendor CUDA wheels |
| **PyInstaller one-file/one-dir per worker** | Single artifact; Tauri `externalBin` friendly | PyInstaller + torch + CUDA is fragile; big; slow cold start |
| **`uv`-managed venv, resolved on install** | Smaller download (wheels fetched at install) | Needs network at install → violates "fully offline install" for some users |

**Recommendation: embedded CPython + one shared frozen venv** containing torch,
diffusers, bitsandbytes, faster-whisper + CTranslate2 (CUDA 12), Chatterbox,
InsightFace. One venv (not per-worker) keeps size down; the workers are just
scripts run with `<app>/python/python.exe <app>/workers/<name>.py`. Rust resolves
these by relative path, never PATH.

### Native layout (deterministic)
```
<install>/
  LocalAI.exe
  bin/  llama-server.exe, cuda dlls
  python/  python.exe, Lib/, site-packages/  (the shared venv)
  workers/  stt.py, tts.py, image_server.py, embedder.py, trainer.py
  webview2/  (bootstrapper fallback)
```
Rust reads `std::env::current_exe()` → resolves siblings. No dev paths, no PATH
lookups (NFR-52).

### First run (FR-2, FR-3, NFR-1)
1. App launches, checks the layout, verifies workers can start.
2. Model check: none present → onboarding: "download an LLM" (picker) +
   "download voice models" (~few GB, one click) + "download the image model"
   (~34 GB, explicit).
3. Everything skippable — the app is usable for what's downloaded (FR-3).
4. **After a full offline install + models on a USB drive**, the app must run with
   the network permanently off. Provide a "models folder" setting so a user can
   point at pre-downloaded models.

### Code signing
- **Deferred** (needs a certificate). The build pipeline includes a signing step
  that is a no-op without a cert. Unsigned installer shows a SmartScreen warning —
  document it. Flag to the owner.

→ **ADR-0014**: MSI/WiX bundler, embedded CPython + one shared frozen venv,
deterministic sibling layout, first-run model onboarding, offline-install path,
signing step stubbed.

---

## Optimizations
1. **One shared venv**, not per-worker — torch/CUDA vendored once (~saves several
   GB vs per-worker PyInstaller).
2. **Split installer**: a small base installer + optional "AI runtime" component
   downloaded on first run for users who'd rather not carry 8 GB — but keep a
   full offline installer available (NFR-1).
3. **`torch` CUDA wheel matched to the bundled CUDA** — don't ship two CUDA
   toolkits (one for CTranslate2 CUDA-12, one for torch); pick a torch build and
   a CTranslate2 build that can share DLLs where possible, or accept the ~1 GB
   overlap.
4. **Lazy worker spawn** (see `11`) so install size, not startup, carries the
   cost.
5. **Delta updates** for the app binary (small) without re-downloading the Python
   runtime (large, changes rarely) — MSI patch (.msp) or Tauri updater with
   component granularity.
6. **NF4-quant cache** (`04`, `06`) is produced on the user's machine post-install
   — not shipped, but a big first-run time saver.
7. **Pre-warm** the CTranslate2 / torch DLL load on a background thread at app
   start so the first voice/image call isn't the first DLL load.

## Failure modes
- Stale sidecar after upgrade → MSI (not NSIS); a version check on the worker's
  `ready` handshake refuses a mismatched worker and prompts a repair.
- Missing VC++ redistributable / CUDA DLL → the installer bundles them; a
  startup self-check reports precisely what's missing.
- WebView2 absent (older Windows) → bundled bootstrapper runs.
- Antivirus quarantines a PyInstaller/embedded exe → prefer the embedded-CPython
  approach (less AV-suspicious than PyInstaller one-file); document.
- User moves the install folder → `current_exe()`-relative resolution still works;
  absolute cached paths (models) are validated and re-prompted if broken.

## Sources
- [Tauri v2 sidecar / externalBin](https://v2.tauri.app/develop/sidecar/) · [NSIS stale-sidecar bug #15134](https://github.com/tauri-apps/tauri/issues/15134)
