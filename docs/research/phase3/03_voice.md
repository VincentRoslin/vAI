# Phase 3 · Voice pipeline — STT · VAD · TTS · audio I/O

Covers step 3.9 and D-4. Requirements: FR-20..26, NFR-23, NFR-24; ARQ-3.
Owner defaults: faster-whisper (STT), Silero (VAD), Chatterbox (TTS).

---

## Constraint summary (this machine)

| Component | Fits w/ LLM loaded? | Key constraint |
| --------- | ------------------- | -------------- |
| Silero VAD | yes | ~tens of MB, runs fine on CPU |
| faster-whisper | yes (~3–4 GB) | **must use `compute_type=float16`** — CTranslate2 int8 is broken on sm_120 (cuBLAS `NOT_SUPPORTED`) |
| Chatterbox Turbo | yes (~1–3 GB) | ~75 ms latency (Turbo); streaming forks reach RTF ~0.5 |

**Voice does not need a model hot-swap** — STT + VAD + TTS + a mid-size LLM
(~6–8 GB) fit in 16 GB together. This is the key difference from image generation.

---

## D-4a — STT: faster-whisper

**Confirmed viable** on the RTX 5080 with caveats:
- CTranslate2 currently supports **CUDA 12 / cuDNN 9**, not CUDA 13. CUDA 12
  runtime libs run fine under the 13.3 driver → bundle the CUDA 12 CTranslate2
  runtime with the worker.
- **`compute_type` must be `float16`** on Blackwell (int8 crashes). Slightly more
  VRAM, still ~3–4 GB for `large-v3`.
- Model size choice: `large-v3` gives best accuracy (~3 GB fp16) and is affordable
  alongside a mid LLM. `distil-large-v3` or `medium` if VRAM gets tight with a
  bigger LLM. Registry records the choice; **benchmark real-time factor in Phase
  18** on this GPU (target RTF well under 1.0; `large-v3` fp16 on a 5080 should be
  ~5–10x realtime).
- Streaming vs batch: faster-whisper is batch-oriented. For low latency, run it on
  **VAD-endpointed segments** (transcribe each utterance when Silero says it
  ended) rather than true streaming. Optional: overlapping-window partials for a
  live caption — decide in Phase 18, not required for v1.

Alternative considered: **whisper.cpp** (GGUF, C++, no Python, integrates like the
LLM backend). Pros: no CUDA-12/Python baggage, one less runtime. Cons: weaker
word-timestamp/diarization story, another native build for sm_120, and the owner
picked faster-whisper. **Keep faster-whisper**; note whisper.cpp as the fallback
if the CUDA-12-runtime bundling proves fragile.

→ **ADR-0005** (voice), STT section: faster-whisper, `float16`, CUDA-12 CTranslate2
runtime bundled, VAD-segmented transcription, model size benchmarked in Phase 18.

---

## D-4b — VAD: Silero

- Tiny ONNX model; run it **in the Rust core** via `ort` (ONNX Runtime) on the
  capture stream, or in the STT worker. Recommendation: **in the Rust core** —
  VAD decides utterance boundaries and barge-in triggers, both of which are
  orchestration concerns Rust owns; keeping it out of the worker means barge-in
  detection doesn't depend on a subprocess being alive.
- Tunables: speech-onset threshold, min-silence-to-endpoint (~500–800 ms),
  min-utterance duration (drop coughs). Config-exposed.

---

## D-4c — TTS: Chatterbox

- **Chatterbox Turbo** (~0.7–1 GB weights, low-single-digit-GB working set,
  ~75 ms latency). Fits comfortably with an LLM.
- **Streaming**: use a streaming-capable build/fork (e.g. `chatterbox-streaming`)
  or chunk at the app level: synthesize **clause-by-clause** as LLM tokens arrive,
  stream audio chunks to playback. Chunking at clause boundaries means a barge-in
  loses at most one short chunk and the spoken prefix is clean (FR-24).
- Voice-first-audio target: < ~500 ms after the LLM's first clause is ready.
- Single pinned model (owner decision); no voice picker for v1. Voice cloning /
  multi-voice is out of v1 scope.

---

## D-4d — Audio I/O

- **`cpal`** for capture + playback (cross-platform, WASAPI on Windows).
  - Capture: shared mode, 16 kHz mono (Whisper's rate) — resample from the device
    rate with `rubato` if needed.
  - Playback: small buffer (~100–200 ms) so stop is near-instant (FR-23).
- Rust owns both streams. Workers receive/return audio as **framed PCM over the
  worker protocol** or as short WAV files in a controlled temp dir — decide in
  Phase 18 based on the worker-protocol ADR (D-12); framed bytes preferred to
  avoid disk churn for short utterances.
- **Full-duplex** (capture + playback simultaneously) is required for barge-in.
  cpal supports independent input/output streams.
- **Echo**: with speakers, TTS audio leaks into the mic and can false-trigger VAD.
  v1 mitigation: **push-to-talk OR "headphones recommended" for open-mic**, plus
  duck VAD sensitivity while TTS is playing. Full acoustic echo cancellation is
  out of v1 scope (big effort). Record as a known limitation.

→ **ADR-0005** (voice): Silero VAD in Rust core, `cpal` audio I/O, clause-chunked
Chatterbox streaming, push-to-talk default with optional open-mic + headphones,
barge-in state machine owned by the conversation engine.

---

## End-to-end latency budget (NFR-23: ~1–3 s to first audio)

```
user stops speaking
  → Silero endpoint decision        ~0.1–0.3 s
  → faster-whisper transcribe       ~0.3–1.0 s  (utterance length dependent)
  → LLM first token (TTFT)          ~0.5–1.5 s
  → Chatterbox first clause synth   ~0.1–0.3 s
  → audio out                       ≈ 1.0–3.1 s total  ✓ (in budget, to validate)
```
Barge-in (FR-23, target < ~200 ms): VAD speech-onset → trip LLM + TTS cancel →
stop playback (small buffer). Measured in Phase 19.

## Optimizations
1. **VAD-gated STT** — only transcribe detected speech; never run Whisper on
   silence. Biggest single latency + compute saver.
2. **Pipeline overlap** — start Chatterbox on clause 1 while the LLM is still
   generating clause 2, and start playback of chunk 1 while chunk 2 synthesizes.
   Cuts perceived voice latency substantially.
3. **Keep STT + TTS models resident** (they're small, ~4–6 GB combined) — no swap,
   ever. Only unload if VRAM pressure from a bigger LLM demands it.
4. **`distil-large-v3` / `medium` STT** if `large-v3` RTF or VRAM is marginal with
   the chosen LLM — measure in Phase 18.
5. **Faster-whisper `batch` + `beam_size=1`** for interactive turns (greedy is
   fine for conversational STT, much faster than beam search).
6. **ONNX Runtime / TensorRT path** for Silero VAD (and evaluate a TensorRT
   Whisper as an alternative to CTranslate2 given the sm_120 int8 breakage).
7. **Speculative / short-context TTS** — Chatterbox Turbo already targets ~75 ms;
   keep the playback buffer tiny (~100 ms) so barge-in is near-instant.

## Failure modes
- STT worker crash mid-utterance → typed error, recover for next utterance.
- No input/output device, or device removed mid-session → clear error, no crash.
- VAD never endpoints (constant noise) → max-utterance timeout forces a cut.
- Empty/garbage transcript → dropped or surfaced per policy, never sent as a blank
  turn.
- TTS worker crash → conversation continues in text; error surfaced.

## Sources
- [faster-whisper #1431 (CUDA 13)](https://github.com/SYSTRAN/faster-whisper/issues/1431)
- [SubtitleEdit #10180 — RTX 50-series cuBLAS NOT_SUPPORTED, use float16](https://github.com/SubtitleEdit/subtitleedit/issues/10180)
- [Chatterbox Turbo](https://www.resemble.ai/learn/models/chatterbox-turbo) · [chatterbox-streaming](https://github.com/davidbrowne17/chatterbox-streaming) · [latency #193](https://github.com/resemble-ai/chatterbox/issues/193)
