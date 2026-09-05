# Phase 15 — llama.cpp Adapter

> **Architecture frozen at Phase 5.** Governing: **ADR-0003** (`llama-server`
> supervised child), **ADR-0004** (build from source, sm_120), **ADR-0013**
> (named-pipe / token'd loopback transport), `AI_PIPELINES.md` §1. Concrete steps
> filled in at phase entry — the design does not change. Note the WDDM-hang watch
> item (`docs/verification/02_phase3_probes.md`).

## Objective
The first LLM backend, implementing the `ModelBackend` / `LlmBackend` interface.
All llama.cpp-specific detail (flags, protocol, quirks) confined to this adapter
module. Lifecycle owned by Phase 14, resources by Phase 13.

## Depends on
Phase 14 (lifecycle), Phase 13 (resources), Phase 3.4 ADR (transport + CUDA build).

## Not in this phase
- Personas, memory, characters, voice, images.
- Conversation state (Phase 16/17).
- Multi-model or hot-swap (Phase 23).

## Architecture notes
- The rest of the app sees only `LlmBackend` (generate, stream, cancel, health,
  shutdown). No component imports llama.cpp types.
- Transport per ADR: if loopback HTTP — Rust picks a free `127.0.0.1` port,
  supervises the child, uses a Windows Job Object for orphan cleanup; if FFI —
  inference on a dedicated thread with a crash guard.
- CUDA build per ADR (sm_120 / CUDA 13): documented, reproducible.

## Performance notes
- Record **time-to-first-token (TTFT)** and **tokens/sec** for a reference prompt
  + model — the Phase 31 baseline and the Phase 16 gate depend on it.
- GPU layer count (`-ngl`) comes from the resource manager's estimate, not
  hardcoded.

## Step outline
1. Build/obtain the CUDA-enabled llama.cpp per the ADR; document the exact steps.
2. Adapter skeleton implementing `LlmBackend`; llama.cpp types never leak.
3. Startup + readiness (health poll with timeout; surface load progress).
4. Non-streaming generation.
5. Streaming generation → push token deltas into the Phase 7 streaming contract.
6. Cancellation: both transport-level (disconnect / abort) and app-level
   (`CancellationToken`); verify the backend actually stops.
7. Timeout handling.
8. Crash detection (process exit / health failure / stream error) → report to
   Phase 14.
9. Clean shutdown; no orphan process (Job Object test on Windows).
10. Wire through Phase 14 so `load`/`unload`/`Busy` all work.
11. Tests + a manual reference-prompt run capturing TTFT + tokens/sec.

## Verification gate
1. Startup + readiness check succeed for a real GGUF.
2. Non-streaming and streaming generation both produce correct output.
3. Cancellation stops generation and frees the slot (verified, not assumed).
4. A timeout is handled with a typed error.
5. Killing the backend is detected; Phase 14 moves to `Failed`; recovery works.
6. Clean shutdown leaves no orphan process (checked with a process list).
7. No file outside the adapter references llama.cpp.
8. TTFT + tokens/sec recorded.

## ADRs / open questions
- Confirms the transport ADR in practice; raise a new ADR if the chosen transport
  proves unworkable (fall back to the alternative).
