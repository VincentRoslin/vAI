# Phase 3 · Memory + relationship progression

Covers steps 3.12, 3.8 (relationship) and D-11, D-16. Requirements: FR-50..56,
FR-C30..33, FR-C40..43; ARQ-5, ARQ-15.

---

## D-11 — Memory

### Retrieval mechanism: SQLite FTS5 first (owner decision O1)
- **FTS5** virtual table over memory content. BM25 ranking. Sub-millisecond
  keyword search, zero extra process, no model, fully offline.
- Query = keywords/entities from the current turn (+ recent context). Top-k by
  BM25, filtered by scope.
- **Embeddings deferred.** Trigger to reconsider (record in the ADR): a measured
  recall gap in Phase 21 testing (relevant memory exists but FTS5 misses it
  because of vocabulary mismatch) *and* a per-scope corpus large enough
  (> ~500 memories) that it matters. If added later: a small local embedding
  model (e.g. a ~100 MB `bge-small`-class) + `sqlite-vec` extension — still no
  separate process.

### Scoping (resolved)
- **Tab 1 (Persona chat): per-Persona.** `mem_*` rows carry `scope = persona:<id>`.
- **Tab 3 (Character): per-Character.** `scope = character:<id>`.
- Retrieval always filters by the active scope. No cross-scope leakage
  (a Persona never recalls a Character's memories or vice versa).

### Extraction pipeline (FR-50, FR-51)
```
turn completes
  → (async, off the response path) LLM extraction pass:
      "from this exchange, list durable facts worth remembering, or none"
      schema-constrained → candidate memories
  → validation: importance score >= threshold; dedup vs existing (FTS5
      near-match); size cap; drop transient/absurd
  → store (mem_* + FTS5 index), with provenance (source conversation + message)
```
- Extraction runs **after** the turn, not blocking it (NFR-22).
- Importance threshold + dedup keep the store lean (FR-51).

### Injection into context (via the Phase 20 context builder)
- Retrieved memories go in a dedicated context section with a **token budget**
  (config; e.g. 15% of the model's context). Lowest-ranked dropped first.
- Provenance kept internally so "view memories" (FR-52) can show where each came
  from.
- Retrieved memory text is **untrusted** — the context builder's injection-safety
  (Phase 20) applies (a memory can't contain prompt delimiters that restructure
  the prompt).

### View / delete / correct (FR-52..54)
- View + delete: v1. A memory list per scope, searchable, with provenance.
- **Correct/edit: later phase** (owner decision A2) — edit re-indexes FTS5.

→ **ADR-0012** (memory): FTS5 keyword retrieval, per-Persona / per-Character
scope, async schema-constrained LLM extraction with importance + dedup gate,
budgeted context section, embeddings deferred behind a measured trigger.

---

## D-16 — Relationship progression (ARQ-15)

### Model: discrete stages (owner decision A6)
Starting set (ADR — tune during Phase 26):
`Stranger → Acquaintance → Friend → Close Friend → Romantic Interest → Partner`
(+ possibly regressions, e.g. after long absence or conflict).

- Stored as `char_relationship { character_id, stage, stage_since, points,
  history: [{stage, at, reason}] }`.
- `history` (FR-C42 "remembers how the relationship developed") is a small append
  log, also usable as memory-like context.

### How the stage changes
Two candidate mechanisms (decide in Phase 26 with real conversations):
- **Rule-based points**: each turn contributes a small signed delta from cheap
  signals (message length, sentiment, shared personal info, time spent,
  consistency). Cross a threshold → advance a stage. Predictable, testable, no
  extra LLM cost.
- **LLM-assessed**: periodically (every N turns) an LLM classifies the current
  relationship state from recent history. Richer, costs a generation, less
  predictable.
- **Hybrid (recommended)**: rule-based points for the moment-to-moment number;
  an occasional LLM check (every ~20 turns or on a big event) to catch what the
  rules miss and to write the `history` reason text.

### What a stage affects (FR-C41, FR-C43)
- **Conversation**: the context builder injects the stage + its behavioural
  guidance ("you are close friends; warm, familiar, teasing"). Verifiable it
  reaches the model (Phase 20 harness).
- **Character-sent images (FR-C43)**: the stage gates what the character will
  send — the typed image action's allowed `image_type` / `scene` set is a
  function of the current stage (enforced in Rust, `28`). Early stages: none or
  casual selfies; later stages: more. **No content filtering** (NFR-15) — this is
  a *character-behaviour* rule the owner controls, not a censor.

### Persistence
Relationship state + history survive restart (NFR-31); loaded with the character
(`07` batched read).

→ **ADR-0012** (also covers relationship): discrete stages, hybrid
points+occasional-LLM progression, stage drives conversation context and the
character-image action allow-list.

---

## Optimizations
1. **FTS5, not a vector DB** — no process, no model, no embedding compute per
   turn. Revisit only on a measured recall gap.
2. **Async extraction** — never on the response path.
3. **Rule-based relationship points** as the default — near-zero cost per turn;
   the LLM check is occasional.
4. **Dedup at extraction** (FTS5 near-match) — the store stays small, so
   retrieval stays fast and the context section stays cheap.
5. **Cache the current relationship stage + its guidance string** on the loaded
   character — no lookup per turn.
6. **Budget + rank memories once per turn**, reuse for both the prompt and the
   "what was included" provenance.
7. If embeddings are ever added: `sqlite-vec` in-process, not a separate vector
   service.

## Failure modes
- Extraction LLM returns garbage/invalid → schema constraint rejects; no memory
  stored; logged.
- Memory store grows huge despite dedup → a periodic prune (lowest-importance,
  oldest, never-retrieved) with a user-visible setting.
- Relationship points oscillate around a threshold → hysteresis (need to exceed
  threshold + margin, and stay for M turns, to advance/regress).
- FTS5 misses an obviously relevant memory → logged as a recall-gap sample for the
  embeddings decision.
- A memory or relationship-history entry contains injection text → neutralised by
  the Phase 20 context builder.

## Sources
- SQLite FTS5 + `sqlite-vec` are standard; no external citation needed beyond the
  SQLite docs.
