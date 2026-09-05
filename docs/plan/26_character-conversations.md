# Phase 26 — Character Conversations

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Conversations with a character run on the **shared conversation engine** (Phase
17), with the character's identity, personality, relationship, and memory
assembled into model context by the **shared context builder** (Phase 20). No
separate character chat system.

## Depends on
Phase 25 (character data), Phase 20 (context builder), Phase 21 (memory),
Phase 17 (engine).

## Not in this phase
- Image sending (Phase 28).
- Discovery UX (Phase 29).
- Voice with characters (falls out of Phase 18/19 + this — verify, don't rebuild).

## Architecture notes
- A character conversation is a normal engine conversation with a `character_id`.
- The context builder gains a character section (fed from Phase 25 structured
  fields + Phase 21 character-scoped memory).
- Relationship state updates are derived by explicit rules / a typed extraction
  step — the model does not directly rewrite relationship state.
- Character memory is scoped: retrieval for a character conversation pulls that
  character's memories.

## Performance notes
- No regression to chat TTFT/throughput from the added context sections (measure).
- Character load + context assembly batched (one DB round trip).

## Step outline
1. Extend the context builder with a character section (identity, personality,
   relationship, interests).
2. Scope memory retrieval by `character_id`.
3. Start/resume a character conversation via the engine.
4. Relationship-state update step (typed, rule- or extraction-based, validated).
5. A test that captures the exact prompt and asserts character identity +
   relationship context is present.
6. Persistence: character conversation history + relationship state survive
   restart.
7. Tests: conversation runs on the shared engine; identity reaches the model;
   memory is character-scoped; relationship updates are bounded/valid; restart
   restores everything.

## Verification gate
1. A character conversation runs entirely on the shared engine (no second
   engine — `git grep`).
2. A test asserts the assembled prompt contains the character's identity and
   current relationship context.
3. Memory retrieved in a character conversation is scoped to that character.
4. Relationship state changes only through the validated update step, within
   defined bounds.
5. Character conversation history + relationship state persist across restart.
6. No TTFT/throughput regression vs the Phase 16 baseline.

## ADRs / open questions
- How relationship state is derived (rules vs LLM extraction vs hybrid) — ADR.
