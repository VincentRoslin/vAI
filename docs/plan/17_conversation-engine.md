# Phase 17 — Conversation Engine

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Formalize the single conversation engine that voice, personas, memory, and
characters will all share: conversation lifecycle, messages, roles, content types,
timestamps, streaming state, cancellation, generation metadata, persistence.
**There is exactly one conversation engine.**

## Depends on
Phase 16 (the vertical slice proved the flow).

## Not in this phase
- Voice, personas, memory, character specifics (they *use* the engine later).
- New UI beyond what re-hosting text chat needs.

## Architecture notes
- The engine is backend-agnostic (LLM now; the same engine drives voice turns).
- Message content is typed (text now; extensible to audio-ref, image-ref later
  without a schema break).
- Streaming state + cancellation are first-class engine concepts, not per-caller.
- The Phase 16 service is refactored *into* this engine — no parallel code path
  survives.

## Performance notes
- Re-hosting text chat on the engine must not regress the Phase 16 TTFT /
  throughput / cancellation numbers.

## Step outline
1. Define the engine's public API (start conversation, add turn, stream, cancel,
   get history, persist).
2. Define the message model (role, typed content, timestamps, generation metadata,
   truncated flag).
3. Streaming state machine (idle → generating → done/cancelled/error) owned by the
   engine.
4. Persistence integration (Phase 9): conversations + messages tables.
5. Migrate the Phase 16 text-chat service onto the engine; delete the old path.
6. Engine unit tests: lifecycle, streaming, cancellation, persistence, restart.
7. Re-run the Phase 16 gate against the re-hosted chat.

## Verification gate
1. Engine unit tests pass (lifecycle + streaming + cancel + persistence +
   restart).
2. Text chat runs entirely on the engine; the old Phase 16 service code is gone
   (`git grep`).
3. The full Phase 16 verification gate still passes on the re-hosted chat.
4. No performance regression vs the Phase 16 baseline.

## ADRs / open questions
- Message content type taxonomy (how audio/image references attach).
