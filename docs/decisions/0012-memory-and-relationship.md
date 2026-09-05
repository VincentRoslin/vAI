# ADR-0012 — Memory (FTS5) + discrete relationship stages

- **Status:** ACCEPTED (Phase 5 freeze, 2026-09-05; superseding attacks folded in via Phase 4) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/10_memory.md` (D-11, D-16)

## Context
FR-50..56, FR-C30..33 (memory); FR-C40..43 (relationship). Owner: SQLite-first
memory, per-Persona / per-Character scoping, discrete relationship stages.

## Options considered
- Retrieval: SQLite FTS5 keyword vs a local embedding + vector index.
- Relationship: discrete stages vs continuous/attribute vs hybrid transitions.

## Decision
**Memory:**
- **FTS5 (BM25) keyword retrieval** — no extra process, no model, sub-ms.
  Embeddings **deferred** behind a measured trigger (recall gap in Phase 21 tests
  + corpus > ~500/scope); if added, a small local embedder + `sqlite-vec`
  in-process.
- **Scope**: `persona:<id>` for Tab 1, `character:<id>` for Tab 3; retrieval
  always filters by active scope; no cross-scope leakage.
- **Extraction**: async (off the response path), schema-constrained LLM pass →
  candidates → importance-threshold + FTS5-dedup + size-cap gate → store with
  provenance.
- **Injection**: dedicated context-builder section, config token budget,
  lowest-rank dropped first; retrieved text is untrusted (Phase 20 injection
  safety).
- **View + delete: v1. Edit/correct: later phase** (owner A2).

**Relationship:**
- **Discrete stages**: `Stranger → Acquaintance → Friend → Close Friend →
  Romantic Interest → Partner` (+ possible regression); tuned in Phase 26.
- Stored `char_relationship {stage, stage_since, points, history:[{stage,at,reason}]}`.
- **Hybrid transitions**: rule-based signed points per turn for the moment-to-
  moment number; an occasional LLM assessment (~every 20 turns / on a big event)
  to catch what rules miss and to write `history` reasons. Hysteresis to prevent
  oscillation.
- **Effects**: stage + guidance injected into conversation context (verifiable,
  Phase 20); stage **gates the character-image action allow-list** (`image_type`
  / `scene` sets are a function of stage — a character-behaviour rule the owner
  controls, not a content filter; NFR-15 unaffected).

## Consequences
- No vector DB, no extra process for v1.
- Relationship logic is cheap per turn (rules); the LLM check is occasional.
- Stage → image-action gating gives the owner a behavioural dial without any
  censorship mechanism.
- Recall-gap samples are logged to inform the future embeddings decision.
