# Phase 21 — Memory

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0015). The design below is settled. Concrete implementation specifics (exact modules, crate APIs, filenames) are filled in at phase entry against the frozen ADRs — they do not change the design.

## Objective
Persistent memory: conversation → extraction → importance/validation → storage →
relevant retrieval → context construction. **SQLite + FTS5 keyword retrieval
first.** A vector store is added only if measurements show a real recall gap
(criteria set in Phase 3.12).

## Depends on
Phase 20 (context builder has a memory slot), Phase 17 (engine), Phase 9
(persistence), Phase 15 (an LLM to do extraction).

## Not in this phase
- Embeddings / vector search (deferred; the schema leaves room).
- Character-specific memory semantics (Phase 26 builds on this).

## Architecture notes
- Memory is Rust-owned data. Extraction uses the LLM but the result is validated
  before storage (importance threshold, dedup).
- Retrieval feeds the Phase 20 builder's memory section with provenance.
- Prompt-injection risk: retrieved memory text is untrusted — the builder's
  injection-safety (Phase 20) applies.

## Performance notes
- Retrieval on every turn — FTS5 query must be fast (indexed); cap result count
  and token budget.
- Extraction runs async (post-turn), not on the response path.

## Step outline
1. Memory schema (content, kind, importance, source conversation/message,
   timestamps) + FTS5 index.
2. Extraction: post-turn LLM pass → candidate memories.
3. Validation: importance threshold, dedup against existing, size cap.
4. Storage repository.
5. Retrieval: FTS5 keyword query from the current turn → ranked memories →
   builder slot with provenance.
6. Budgeting: cap the memory section; drop lowest-ranked first.
7. Determinism: fixed store + query → identical retrieval.
8. Tests: extract+store, retrieve-relevant, budget respected, deterministic,
   survives restart, injection text neutralized.
9. Record the recall gap (if any) to inform the future embeddings decision.

## Verification gate
1. A memory is extracted from a conversation and stored (after validation).
2. Retrieval returns relevant memories for a new turn (scripted check).
3. The memory section stays within its token budget.
4. Retrieval is deterministic for a fixed store + query.
5. Memory survives an app restart.
6. A memory containing prompt delimiters cannot alter prompt structure.

## ADRs / open questions
- The "introduce embeddings when X" trigger (measured recall gap, corpus size) —
  set in Phase 3.12, re-checked here with real data.
