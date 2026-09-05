# Phase 18 — Voice: capture · Silero VAD · faster-whisper STT

> **Architecture frozen at Phase 5.** Governing: **ADR-0005** (faster-whisper
> fp16, Silero VAD in the Rust core, `cpal`, VAD-segmented transcription),
> **ADR-0013** (stdio worker), `AI_PIPELINES.md` §2–3. Concrete steps filled in at
> phase entry; the design does not change.

## Objective
The input half of voice: microphone capture → voice-activity detection
(endpointing) → speech-to-text → a user message on the shared conversation engine.
STT engine default: **faster-whisper** (one pinned model). VAD: **Silero**.

## Depends on
Phase 17 (conversation engine), Phase 13 (resources — STT uses GPU), Phase 14
(the STT worker is a managed subprocess), Phase 12 (STT model acquired).

## Not in this phase
- TTS / playback / barge-in (Phase 19).
- Wake-word / always-on.
- Open-mic if the ADR chose push-to-talk for v1.

## Architecture notes
- Rust owns the audio capture stream and device selection (`cpal` per ADR).
- Silero VAD runs where the ADR says (Rust-side ONNX vs in the worker); it decides
  utterance start/end.
- The faster-whisper worker is an isolated subprocess (transport per the Phase 3
  ADR); it does AI only, holds no state, is killable.
- A finished transcript becomes a user turn via the Phase 17 engine — same path as
  typed input.

## Performance notes
- **STT real-time factor** (audio seconds ÷ processing seconds) recorded; target
  well under 1.0 for the chosen model on this GPU.
- Capture-to-VAD-decision latency and endpoint-to-transcript latency recorded.
- Partial transcripts (if the ADR chose streaming) update without UI thrash.

## Step outline
1. Audio device enumeration + selection (persisted in config).
2. Capture stream (sample rate/format the models expect; resample if needed).
3. Silero VAD integration: speech onset + endpoint detection with tunable
   thresholds.
4. STT worker: spawn, `ready` handshake, request/cancel/result protocol.
5. Feed endpointed audio to the worker; receive partial (optional) + final
   transcript.
6. Turn creation: final transcript → user message on the conversation engine.
7. Failure handling: STT worker crash, model missing, no input device, garbage
   transcript (empty / below confidence).
8. Cancellation: abandon an in-progress transcription.
9. Tests + a manual capture→transcript run recording the latency figures.

## Verification gate
1. Device enumeration lists real inputs; selection persists across restart.
2. Speaking a phrase produces a correct final transcript that becomes a user turn.
3. VAD endpoints an utterance without a manual stop (or per push-to-talk if that's
   the ADR).
4. STT worker crash mid-transcription → typed error, recovers for the next
   utterance.
5. Missing input device / missing model → clear error, no crash.
6. An empty / below-confidence transcript is handled per policy (dropped or
   surfaced), not sent as a blank turn.
7. STT real-time factor and end-to-end latency recorded.

## ADRs / open questions
- Streaming partials vs batch-on-endpoint (from Phase 3.9).
- Push-to-talk vs open-mic for v1.
