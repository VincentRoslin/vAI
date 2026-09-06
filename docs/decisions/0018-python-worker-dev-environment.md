# ADR-0018 — Python worker dev environment

- **Status:** ACCEPTED (Phase 18, 2026-09-06) — closes the env-audit O-item
  ("Python worker environment strategy → ADR", `docs/verification/01_env_audit.md`).
- **Governing / related:** ADR-0013 (worker transport), ADR-0014 (packaging —
  the *ship* story), ADR-0015 (worker network lockdown).

## Context

Phase 18 introduces the first Python worker (STT — faster-whisper). ADR-0014
fixes the **shipped** runtime: an embedded CPython + one shared frozen venv
resolved as a sibling of the executable. It does **not** say how the worker venv
is built and used **in development**, and the env audit left "bare
`Program Files` Python vs per-worker venv vs `uv`-managed vs bundled" open.
`docs/spec/DEVELOPMENT.md` already anticipates a `uv`-managed venv.

## Options considered

- **System Python** (`C:\Program Files\Python311`) with packages installed
  globally — pollutes the machine, no pinning, no reproducibility.
- **One `uv`-managed venv at `<repo>/.venv`**, from a pinned
  `workers/requirements.txt`, shared by all workers.
- **Per-worker venv** — several GB duplicated (torch/CUDA), matches nothing about
  the ship story (ADR-0014 is explicitly *one shared* venv).
- **`uv run` with inline PEP-723 metadata per script** — neat, but the CUDA
  dependency set is large and identical across workers; a shared lock is clearer.

## Decision

**One `uv`-managed virtual environment at `<repo>/.venv`, from a pinned
`workers/requirements.txt`, shared by every worker** — the dev mirror of
ADR-0014's shared frozen venv.

- `uv` is installed to the user profile (`python -m pip install --user uv`); the
  repo does not vendor it.
- `node scripts/setup-venv.mjs` creates `.venv` and installs the pinned set;
  idempotent; re-run after a `requirements.txt` change. `.venv/` is gitignored
  (~2.2 GB — faster-whisper + the CUDA 12 / cuDNN 9 user-space libs).
- **Pins are exact** in `workers/requirements.txt` (resolved 2026-09-06,
  Python 3.11, `win_amd64`): `faster-whisper 1.2.1`, `ctranslate2 4.8.2`,
  `nvidia-cublas-cu12 12.9.2.10`, `nvidia-cudnn-cu12 9.25.1.1`, `onnxruntime
  1.29.0` (STT); `chatterbox-tts 0.1.7`, `soundfile`, and **`torch 2.11.0+cu128`
  / `torchaudio 2.11.0`** (TTS — Phase 19), + transitive.
- **PyTorch override (Phase 19).** `chatterbox-tts` hard-pins `torch==2.6.0`,
  whose CUDA wheels predate Blackwell (sm_120). `workers/overrides.txt` forces
  `torch 2.11.0+cu128` (from the `download.pytorch.org/whl/cu128` index);
  `scripts/setup-venv.mjs` passes `uv pip install … --override
  workers/overrides.txt`. Verified: Chatterbox Turbo loads + synthesises on the
  RTX 5080 (warm RTF ≈ 0.45); faster-whisper (CTranslate2, its own CUDA-12
  libs) is unaffected. The PyPI default `torch` wheel is CPU-only, so the cu128
  index is required.
- **CUDA library loading:** the box carries a CUDA **13** driver; CTranslate2
  needs the CUDA **12** + cuDNN **9** user-space libraries. Those ship as the
  `nvidia-*-cu12` wheels; `workers/stt.py` prepends each `.venv/Lib/site-packages/
  nvidia/*/bin` directory to `PATH` (and `os.add_dll_directory`) **before**
  importing `ctranslate2`. Verified: `cublas64_12.dll` / `cudnn64_9.dll` resolve,
  `large-v3` fp16 transcribes on the RTX 5080.
- **ONNX Runtime in Rust** (Silero VAD, `voice::vad`): the `ort` crate with
  `load-dynamic`; the `onnxruntime` shared library is taken from the same venv in
  dev (`ORT_DYLIB_PATH` → `.venv/.../onnxruntime/capi/onnxruntime.dll`) and
  bundled beside the app on ship. No build-time binary download.
- **The ADR-0015 env** is centralised in one place — `src-tauri/src/worker/env.rs`
  — and unit-tested against the ADR's variable list. Every worker spawn goes
  through it.
- **Config** (`workers.dir`, `workers.python`, schema v6) lets the dev build point
  at `<repo>/workers` + `<repo>/.venv/Scripts/python.exe`; the packaged build
  ignores them and uses the sibling layout.

## Consequences

- Dev needs `uv` + a one-time `setup-venv.mjs` (~1 GB download); documented in
  `docs/spec/DEVELOPMENT.md`. CI that runs the worker/voice unit tests needs
  **no** venv — those use `workers/stt_fake.py` (stdlib). Only the `#[ignore]`d
  live gate needs the real venv + GPU.
- `.venv` pins can drift from ADR-0014's eventual frozen set; Phase 37 reconciles
  (same `requirements.txt` is the input to the embedded build).
- New faster-whisper / HF-hub versions may add telemetry knobs not in the
  ADR-0015 list — re-checked here (none needed for this set); Phase 32's packet
  capture is the backstop.
- The `nvidia-cu12` + `nvidia-cu13` (llama.cpp) overlap on disk (~1 GB) is the
  cost ADR-0014 already accepted.
