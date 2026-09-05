# Phase 19 — Voice: Chatterbox TTS · playback · barge-in

> **Architecture frozen at Phase 5.** Governing: **ADR-0005** (Chatterbox Turbo,
> clause-chunked, `cpal` playback with a small buffer, barge-in state machine on
> the conversation engine), `AI_PIPELINES.md` §4. Concrete steps filled in at
> phase entry; the design does not change.

## Objective
The output half of voice: assistant text → chunked speech synthesis → audio
playback, with **barge-in** (the user speaking, or a cancel, stops the LLM + TTS +
playback fast and returns to listening). TTS engine: **Resemble Chatterbox** (one
pinned model). This is the reliability-critical half of voice.

## Depends on
Phase 18 (capture + VAD provide the barge-in trigger), Phase 17 (engine),
Phase 13/14 (TTS worker), Phase 12 (TTS model acquired).

## Not in this phase
- Multi-voice / voice cloning UI (a single default voice is fine).
- Emotion/style controls beyond what falls out of Chatterbox defaults.

## Architecture notes
- Rust owns the playback stream and the interruption state machine.
- The conversation engine's `Speaking` state is explicit; `Interrupting` is a
  transient state that must (1) trip the LLM `CancellationToken`, (2) cancel the
  TTS worker, (3) stop playback, (4) finalize the assistant turn as truncated,
  (5) go to `Listening` — only when all four are acknowledged.
- TTS synthesis is **chunked by sentence/clause** so stopping loses at most one
  chunk and the spoken prefix is a clean prefix of the message.

## Performance notes
- **Barge-in latency** (trigger → silence) budget: ~150–200 ms. Playback buffer
  kept small (~100–200 ms) so stopping is near-instant.
- TTS time-to-first-audio recorded; synthesis must keep ahead of playback.

## Step outline
1. TTS worker: spawn, `ready`, synth-request / cancel / audio-chunk / done
   protocol.
2. Chunker: split assistant text into clauses aligned to sentence boundaries;
   stream chunks to the worker as tokens arrive from the LLM.
3. Playback stream (Rust): enqueue synthesized chunks, small buffer.
4. `Speaking` state in the engine; UI reflects it.
5. Barge-in: VAD speech-onset (from Phase 18) OR an explicit stop → enter
   `Interrupting` → the 5-step sequence above.
6. Truncation: the assistant message is persisted with `truncated: true` and the
   token count actually spoken; next-turn context uses the truncated text.
7. Failure handling: TTS worker crash, model missing, playback device lost.
8. Full-duplex check: capture (Phase 18) and playback run simultaneously for
   barge-in.
9. Tests + a manual barge-in run measuring trigger→silence latency.

## Verification gate
1. Assistant text is spoken via Chatterbox and plays through the selected output
   device.
2. Speaking over the assistant (or pressing stop) halts LLM + TTS + playback and
   returns to listening.
3. Trigger→silence latency measured and within the `PERFORMANCE.md` budget.
4. The interrupted assistant turn is persisted as truncated, with a clean spoken
   prefix; the next turn's context uses it.
5. TTS worker crash / missing model / lost output device each handled with a
   typed error, no hang.
6. Capture and playback run concurrently without conflict.
7. TTS time-to-first-audio recorded.

## ADRs / open questions
- Echo handling if open-mic is enabled (headphones-only vs AEC).
