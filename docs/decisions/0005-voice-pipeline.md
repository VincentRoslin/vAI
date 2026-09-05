# ADR-0005 — Voice pipeline

- **Status:** PROPOSED (Phase 3 draft) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/03_voice.md` (D-4)

## Context
Voice is a modality of the one conversation engine (FR-20..26). Owner defaults:
faster-whisper (STT), Silero (VAD), Chatterbox (TTS). Blackwell has a known
CTranslate2 int8 breakage.

## Options considered
- STT: faster-whisper vs whisper.cpp.
- VAD location: Rust core vs STT worker.
- TTS: Chatterbox base vs Turbo; app-level chunking vs a streaming fork.
- Audio: `cpal` vs higher-level crates.

## Decision
- **STT: faster-whisper**, `compute_type=float16` (int8 crashes on sm_120), CUDA
  12 CTranslate2 runtime bundled. Model size (`large-v3` vs `distil`/`medium`)
  benchmarked in Phase 18 for RTF + VRAM alongside the chosen LLM. Transcription
  runs on **VAD-endpointed segments**, not true streaming (streaming partials
  optional, not v1-required).
- **VAD: Silero, in the Rust core** (via ONNX Runtime) — it owns utterance
  boundaries and barge-in triggers, which are orchestration.
- **TTS: Chatterbox Turbo**, one pinned model, **clause-chunked** synthesis
  driven by LLM token arrival; small playback buffer (~100–200 ms).
- **Audio I/O: `cpal`** (WASAPI), 16 kHz mono capture, independent input/output
  streams for full-duplex barge-in; `rubato` for resampling.
- **Barge-in**: VAD speech-onset (or explicit stop) → conversation engine
  `Interrupting` state → cancel LLM + TTS, stop playback, persist truncated turn,
  → `Listening`. Target < ~200 ms.
- **Echo**: v1 = push-to-talk default; open-mic optional with "headphones
  recommended" + VAD ducking during TTS. Full AEC is out of v1.

## Consequences
- STT loses int8 efficiency (fp16) — still ~5–10x realtime on the 5080, acceptable.
- A CUDA-12 runtime is bundled alongside the CUDA-13 llama.cpp build → ~1 GB
  overlap in the installer (ADR-0014 opt 3).
- Voice coexists with a mid-size LLM — **no model swap for voice** (unlike image).
- whisper.cpp kept as the documented fallback if CUDA-12 bundling is fragile.
