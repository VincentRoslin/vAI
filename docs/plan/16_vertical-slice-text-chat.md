# Phase 16 — First Vertical Slice / Text Chat

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0015). The design below is settled. Concrete implementation specifics (exact modules, crate APIs, filenames) are filled in at phase entry against the frozen ADRs — they do not change the design.

## Objective
The entire stack does one useful thing reliably: React → typed IPC → Rust →
conversation service → LLM service → llama.cpp → streamed tokens → UI, with
working cancellation, persistence, and restart recovery. Basic text chat **only**.
This gate is the first real product milestone.

## Depends on
Phase 15 (LLM adapter), Phase 12 (a model can be acquired), Phase 9
(persistence), Phase 6 (UI shell + IPC).

## Not in this phase
- Voice, images, personas, memory, characters, discovery, embeddings, RAG.
- Multiple conversations UI polish (a single working conversation is enough).
- Model hot-swap.

## Architecture notes
- A thin conversation *service* here; it is formalized into the shared engine in
  Phase 17. Do not build two engines — build the minimum, then generalize.
- All state authoritative in Rust; the UI renders it.
- Cancellation path: UI → `cancel(taskId)` command → `CancellationToken` → LLM
  adapter → llama.cpp.

## Performance notes
- **TTFT budget**: first token visible in the UI within the budget set in
  `PERFORMANCE.md` (derived from the Phase 15 measurement).
- Incremental rendering must not thrash layout (append, don't re-render the whole
  transcript).

## Step outline
1. Conversation service: create conversation, append message, persist (Phase 9).
2. Generation flow: user message → build a minimal prompt → start LLM generation
   → stream deltas → persist the assistant message on completion.
3. Cancellation wired end to end.
4. IPC: send-message command, token-stream channel, cancel command, load-model
   command.
5. UI: composer, streaming transcript, stop button, model-not-loaded state.
6. Restart recovery: on launch, restore the last conversation from the DB.
7. Failure handling: llama.cpp crash mid-generation → typed error to the UI, the
   partial assistant message persisted as truncated, model recovers for the next
   turn.
8. Clean shutdown during generation (drain or cancel, then exit 0).
9. Concurrency policy: a second generation request while one is running →
   rejected or queued per the decision, not a crash.
10. Manual test script covering the gate scenarios.

## Verification gate (physically executed)
1. Send a message → tokens stream into the UI; assistant message persists.
2. Cancel immediately after sending → clean stop, model still usable.
3. Cancel mid-generation → stops within budget, partial text persisted as
   truncated, model still usable.
4. Restart the app → the previous conversation is restored exactly.
5. Kill the llama.cpp backend mid-generation → detected, typed error shown, next
   generation works.
6. Close the app during generation → exits cleanly (0), no orphan process.
7. A concurrent generation attempt is handled per policy (no crash, no state
   corruption).
8. TTFT measured and within the `PERFORMANCE.md` budget.

## ADRs / open questions
- Concurrency policy for simultaneous generations (reject vs queue).
