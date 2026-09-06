# Phase 21 — Memory

> **Status: FINALIZED AT PHASE ENTRY** (2026-09-06). Step detail derived against
> the frozen architecture (ADR-0012, `AI_PIPELINES.md` §9). The design is
> settled; no split. One new Rust module, one migration, 2 IPC commands, a
> minimal Settings list. **No new venv, no downloads, no new crate** (FTS5 ships
> in the bundled `rusqlite` — verified at phase entry). **No config schema
> change** — the tunables are module constants (precedent: the context builder's
> `RESPONSE_RESERVE` / `BUDGET_MARGIN`).

> **Governing:** ADR-0012 (FTS5 keyword retrieval, per-Persona scope, async
> schema-constrained LLM extraction with importance + dedup + size-cap gate,
> budgeted context section, embeddings deferred), ADR-0009 (persistence),
> `AI_PIPELINES.md` §9, `SECURITY.md` C2 (retrieved memory is untrusted),
> `docs/product/requirements.md` FR-50..56, NFR-22 (extraction off the response
> path), NFR-31 (durable).

## Objective
1. **Persistent per-Persona memory** — a `memory` table + a standalone FTS5
   index, all Rust-owned. Extraction after a turn → validation gate → storage
   with provenance.
2. **Retrieval into the one context builder** — a BM25 keyword query from the
   current user turn, scope-filtered, top-k → the builder's memory slot
   (Phase 20), token-budgeted, lowest-ranked dropped first, recorded in
   `Provenance`.
3. **View + delete** (FR-52 / FR-53) — `memory_list(persona_id)` /
   `memory_delete(id)` IPC + a Memories list in Settings. **Edit/correct is a
   later phase** (owner decision A2).
4. **A recall-gap log** — when retrieval is asked for and returns nothing but the
   scope is non-empty, log a `target: "memory"` sample to inform the future
   embeddings decision (ADR-0012).

## Depends on
Phase 20 (the context builder has a memory slot + injection-safety), Phase 17
(the engine drives the post-turn trigger + the retrieval read), Phase 15 (a
loaded LLM does the extraction — tested against a scripted `LlmInstance`),
Phase 9 (`Db`, `refinery`).
**Used later by:** Phase 26 (character memory reuses this module with a
`character:<id>` scope).

## Not in this phase
- **Embeddings / vector search** — deferred behind the measured trigger
  (ADR-0012: a real recall gap in these tests **and** > ~500 memories/scope).
  The schema leaves room; nothing here needs changing to add `sqlite-vec` later.
- **Character-scoped memory** — `MemoryScope` has only a `Persona` variant now;
  Phase 25/26 add `Character` (additive).
- **Memory edit/correct** (FR-54) — a later phase; view + delete first.
- **A prune *scheduler*** — v1 prunes inline on insert when a scope is at its
  cap (drop the single lowest-priority row). A periodic background prune with a
  user setting is a Phase 30-ish item.
- **User-tunable memory settings** — constants for v1.
- **A rich memory browser** — a plain list + delete in Settings; polish is
  Phase 30.

## Architecture notes

### Ownership (Article I)
- **Rust owns** the `memory` table + `memory_fts` (all SQL, both kept in sync in
  one transaction — no SQL triggers), extraction orchestration, the validation
  gate, and retrieval. The frontend views + deletes over typed IPC only.
- Extraction **uses** the LLM but its output is **untrusted data**: parsed
  against a strict JSON schema, then importance / dedup / size / length gated,
  then `sanitize::strip_control`led before storage. A malformed or empty
  extraction stores nothing and is logged — never fatal.
- Retrieval feeds the Phase 20 builder; the builder already sanitises every
  memory string, so a stored memory containing prompt delimiters cannot
  restructure the prompt.

### New module: `src-tauri/src/memory/`
| File | Responsibility |
| ---- | -------------- |
| `mod.rs` | `MemoryService` — the orchestrator. `new(db, lifecycle)`. `retrieve(scope, query_turn) -> Vec<RetrievedMemory>` (pure read, hot path). `spawn_extraction(model_id, scope, exchange)` (fire-and-forget, `Semaphore(1)` — a 2nd request while one runs is **dropped** + logged, the next turn covers it). `list(scope)` / `delete(id)`. `shutdown()` cancels an in-flight extraction. `MemoryScope` enum (`Persona(PersonaId)` — `as_key()` → `"persona:<id>"`). |
| `repo.rs` | `MemoryRepo` — all `memory` + `memory_fts` SQL (`V0006`). `insert(row)` (writes both tables in one tx; prunes the lowest-priority row first if the scope is at `PER_SCOPE_CAP`), `search(scope, fts_query, k) -> Vec<(Memory, f64 rank)>` (BM25, `ORDER BY rank`), `near_match(scope, content) -> Option<f64>` (dedup probe — best BM25 for the candidate text), `list(scope)`, `get(id)`, `delete(id)`, `count(scope)`. `RawMemory` + `TryFrom`. |
| `extract.rs` | `run_extraction(llm, exchange) -> Vec<MemoryCandidate>` — the fixed schema-constrained prompt, `llm.generate` (temp 0.0, `max_tokens` `EXTRACT_MAX_TOKENS`, stop `"\n\n"`), strict `serde_json` parse of a `{ "memories": [ { content, kind, importance } ] }` object (anything else → `vec![]` + a warn). `validate(candidate, repo, scope)` — `importance >= MIN_IMPORTANCE`, `content` 3..=`MAX_CONTENT_CHARS` after `strip_control`, `near_match` rank not better than `DEDUP_BM25` (too-similar → drop). |
| `retrieve.rs` | `fts_query_for(turn: &str) -> Option<String>` — lowercase, split on non-alphanumerics, drop a small stopword set + tokens < 3 chars, unique, **sorted** (determinism), cap `MAX_QUERY_TERMS`, join with `" OR "`, each term `"<term>"`-quoted for FTS5. `None` when nothing usable. `rank(rows) -> Vec<RetrievedMemory>` — already BM25-ordered; carries `importance` for the tie-break and the builder's drop order. |
| `#[cfg(test)] mod tests` | see the gate. |

### Constants (`memory/mod.rs`, documented as tunables)
```
MIN_IMPORTANCE        = 3      // 1..=5 scale; below this is not stored
RETRIEVE_K            = 8      // max memories pulled per turn
PER_SCOPE_CAP         = 500    // matches the ADR-0012 embeddings-trigger corpus size
MAX_CONTENT_CHARS     = 500
MAX_QUERY_TERMS       = 24
DEDUP_BM25            = -6.0   // a candidate whose best existing match ranks
                              // better (more negative) than this is a duplicate
EXTRACT_MAX_TOKENS    = 256
```
`builder.rs` gains `MEMORY_CONTEXT_FRACTION = 0.15` — the memory sub-budget is
`round(context_tokens * 0.15)`, computed in `TokenBudget::from_context_window`.

### Schema — `V0006__memory.sql`
```sql
CREATE TABLE memory (
    id          TEXT PRIMARY KEY NOT NULL,   -- MemoryId (UUIDv4, ADR-0017)
    scope       TEXT NOT NULL,               -- "persona:<id>" (later "character:<id>")
    kind        TEXT NOT NULL,               -- Fact | Preference | Event | Trait
    content     TEXT NOT NULL,               -- the memory text (sanitised at store)
    importance  INTEGER NOT NULL,            -- 1..5
    source_conversation_id TEXT
                    REFERENCES conversation(id) ON DELETE SET NULL,
    source_message_id      TEXT,             -- provenance; no FK (messages may be pruned)
    created_at  TEXT NOT NULL                -- RFC-3339
) STRICT;

CREATE INDEX memory_scope_idx ON memory (scope);

-- Standalone FTS5 (not external-content): the repo writes both tables in the
-- same transaction, so no sync triggers. `mem_id` / `scope` are unindexed
-- payload columns; `content` is the searchable one.
CREATE VIRTUAL TABLE memory_fts USING fts5 (
    mem_id UNINDEXED,
    scope  UNINDEXED,
    content,
    tokenize = 'unicode61 remove_diacritics 2'
);
```
`refinery` forward-only, additive. Prune on insert-at-cap: delete the row with
`ORDER BY importance ASC, created_at ASC, id ASC LIMIT 1` (and its `memory_fts`
row) before the new insert.

### New id — `contracts::ids::MemoryId`
Added to the `id_newtype!` set.

### Contracts (`contracts::memory`, `ts-rs`-exported)
```rust
pub enum MemoryKind { Fact, Preference, Event, Trait }          // serde unit variants

pub struct Memory {                                             // the view row (FR-52)
    pub id: MemoryId,
    pub kind: MemoryKind,
    pub content: String,
    pub importance: u32,
    pub source_conversation_id: Option<ConversationId>,
    pub created_at: String,
}
```
`MemoryScope` stays **Rust-internal** (the wire uses `PersonaId` directly on the
two IPC commands). `MemoryCandidate` / `RetrievedMemory` are internal.

### Context-builder change (Phase 20 module, additive)
- `TokenBudget { max_prompt_tokens, max_memory_tokens }` — `from_context_window`
  fills both.
- `system_block`: the memory list handed in is **already ranked best-first**;
  include from the front while the running memory-token sum + the next item
  `<= max_memory_tokens` **and** the whole system block still fits the prompt
  budget. Record the real `memory_items` / `memory_tokens` in `Provenance`. The
  over-budget shed path is unchanged (memory dropped entirely before the persona
  is truncated).
- `MemoryItem` stays `{ text: String }` — slice order **is** the rank.
- Determinism preserved: same inputs → same subset → byte-identical prompt.

### Engine wiring (`conversation/mod.rs`)
- `ConversationEngine::new` constructs a `MemoryService` internally from the
  `db` + `lifecycle` it already holds (same pattern as `PersonaRepo` in
  Phase 20) — **`new`'s signature does not change**. `memory() -> &MemoryService`
  accessor for IPC (like `personas()`).
- `build_prompt`: if the conversation has a `persona_id`, call
  `memory.retrieve(MemoryScope::Persona(pid), last_user_turn_text)` and pass the
  result into `BuildInput.memory`. No persona ⇒ no scope ⇒ no memory (FR-56).
- `run_generation`: after the assistant turn is persisted **and**
  `stop_reason` is a normal completion (not `Error` / `Cancelled`) **and** the
  conversation has a `persona_id`, call
  `memory.spawn_extraction(model_id, scope, exchange)` where `exchange` = the
  last user + assistant message text. Fire-and-forget; never blocks
  `run_generation` from returning.
- `shutdown()` also calls `memory.shutdown()`.

### IPC (`ipc::commands`)
- `memory_list(persona_id: PersonaId) -> Vec<Memory>` — newest first.
- `memory_delete(id: MemoryId) -> ()`.

### Frontend
- `src/pages/Settings.tsx` — a **Memories** section under Personas: pick a
  persona → its memory list (kind · importance · content · date) with a Delete
  per row. Mirrors the persona section; presentation only, typed IPC.
- `ipc.ts` wrappers + `contracts.ts` re-exports + `setup.ts` mocks + bindings.
- `ChatVoice.tsx` — the "Show prompt" `<details>` already surfaces
  `provenance.memory_items` / `memory_tokens` (Phase 20 wiring); add the two
  numbers to the summary line so retrieval is visible.

## Performance notes
- **Retrieval is a pure read** on the response path — one FTS5 BM25 query
  (indexed, sub-ms) + one `SELECT … WHERE id IN (…)`. No writes, no counters.
- **Extraction is fully off the response path** — spawned after the turn is
  persisted, serialized by a `Semaphore(1)`, a 2nd request dropped. It shares
  the loaded model via `lifecycle.begin_use` (the adapter serialises; latency is
  irrelevant here).
- The builder ranks + budgets the memory list **once** per turn; the result is
  reused for both the prompt and `Provenance`.
- `PER_SCOPE_CAP` + dedup keep each scope small, so retrieval stays fast and the
  context section stays cheap.

## Steps

**21.1 — schema + `MemoryId` + `MemoryRepo`**
    Do:     `V0006__memory.sql`; `contracts::ids::MemoryId`; `memory/repo.rs`
            (`MemoryRepo` — `insert` [both tables one tx, prune-at-cap],
            `search`, `near_match`, `list`, `get`, `delete`, `count`;
            `RawMemory` + `TryFrom`). `memory/mod.rs` skeleton + `MemoryScope`.
            `lib.rs` `pub mod memory;`.
    Verify: `cargo test memory::repo` — insert → `list` (scope-filtered) → `get`
            → `delete`; `search` returns BM25-ordered rows for a scope and
            **nothing** for another scope (no cross-scope leakage); insert at
            `PER_SCOPE_CAP` drops exactly the lowest-priority row; `memory_fts`
            stays in sync across insert + delete; a fresh `Db` over the same file
            sees the rows (restart). FTS5 available in the bundled build
            (asserted).

**21.2 — contracts + `contracts::memory`**
    Do:     `contracts/memory.rs` — `MemoryKind`, `Memory`; `contracts/mod.rs`
            registration; round-trip + rejection tests in `contracts::tests`.
    Verify: `cargo test contracts::` green; `Memory` / `MemoryKind` bindings
            generated; `git diff --exit-code src/bindings` after commit.

**21.3 — extraction + validation**
    Do:     `memory/extract.rs` — `run_extraction` (fixed prompt, `llm.generate`,
            strict JSON parse), `validate` (importance / length / dedup gate).
            `MemoryCandidate`. `MemoryService::spawn_extraction` (Semaphore(1),
            `begin_use` + `instance` + `as_llm`, cancel token, per-candidate
            validate → `repo.insert`).
    Verify: `cargo test memory::extract` against a scripted `LlmInstance`:
            a well-formed JSON block → N candidates stored with provenance;
            malformed / non-JSON / empty → **zero** stored, a warn logged;
            `importance` below `MIN_IMPORTANCE` dropped; a near-duplicate of an
            existing memory dropped (`near_match`); over-long content dropped;
            a 2nd `spawn_extraction` while one runs is dropped (Semaphore).

**21.4 — retrieval + builder budget**
    Do:     `memory/retrieve.rs` (`fts_query_for` — deterministic sorted unique
            terms; `RetrievedMemory`). `MemoryService::retrieve`. `builder.rs`:
            `TokenBudget.max_memory_tokens` + `MEMORY_CONTEXT_FRACTION`; the
            `system_block` memory loop includes from the front within the
            sub-budget; `Provenance.memory_*` truthful. Recall-gap log
            (`target: "memory"`) when a non-empty scope yields nothing.
    Verify: `cargo test memory::retrieve context::builder` —
            store a handful, query from a turn that shares vocabulary → the
            relevant memories come back, BM25-ordered; a turn from a different
            topic → fewer / none; **scope isolation** (persona A's query never
            returns persona B's memory); **determinism** (`retrieve` twice on a
            fixed store + query → identical ids in identical order);
            `fts_query_for` is order-independent of the input word order;
            builder: 20 memories + a tiny `max_memory_tokens` → a front prefix
            included, `memory_items` / `memory_tokens` recorded, `total_tokens
            <= budget_tokens`; a memory containing `<|im_end|>` does not add a
            turn (extends the Phase 20 injection test).

**21.5 — engine wiring**
    Do:     `ConversationEngine` constructs `MemoryService` internally;
            `memory()` accessor; `build_prompt` retrieves when a persona is set;
            `run_generation` spawns extraction on a clean completion with a
            persona; `shutdown` cancels it. Update the 5 `ConversationEngine`
            construction sites only if `new`'s signature changes (it should not).
    Verify: `cargo test conversation::` all green (no regression);
            a new test: scripted LLM whose `generate` returns an extraction JSON
            block → send a turn → after it settles, `memory.list(scope)` has the
            memory; a following turn whose text matches it → `preview_prompt`
            contains the memory content in the system block; a conversation with
            **no** persona → no extraction, no retrieval.

**21.6 — IPC + frontend**
    Do:     `memory_list` / `memory_delete` commands + handler registration +
            `export_bindings`. `ipc.ts` (`memoryList` / `memoryDelete`),
            `contracts.ts`, `setup.ts` mocks. A Memories section in
            `Settings.tsx` (persona picker → list → delete). `ChatVoice.tsx`
            "Show prompt" summary shows `memory_items` / `memory_tokens`.
    Verify: `node scripts/check.mjs` green; `Settings.test.tsx` — list +
            delete a memory via mocked IPC; `git diff --exit-code src/bindings`.

**21.7 — docs + gate + commit**
    Do:     `docs/verification/21_phase21_memory.md`; `src-tauri/README.md`
            (`memory/` row + `context/` builder-budget note), `docs/spec/
            ARCHITECTURE.md` §2/§3 (the `memory` module as real + single
            authority), `docs/contracts.md` (`Memory` / `MemoryKind` /
            `MemoryId` rows), `docs/decisions/0012` (note: FTS5 standalone +
            repo-synced, constants not config, recall-gap log live),
            `ROADMAP.md`. Record any recall-gap samples seen in testing.
            Commit.
    Verify: check suite green; every gate item recorded with evidence.

## Verification gate (physically executed)
1. A memory is extracted from a conversation exchange and stored **after the
   validation gate** (importance + dedup + length). *(21.3, 21.5)*
2. Retrieval returns relevant memories for a new turn — a scripted check where
   the turn shares vocabulary with a stored memory. *(21.4)*
3. The memory section stays within its token budget — `provenance.memory_tokens
   <= max_memory_tokens` and `total_tokens <= budget_tokens` with many memories
   against a small budget. *(21.4)*
4. Retrieval is deterministic for a fixed store + query — `retrieve` twice
   returns identical ids in identical order; `fts_query_for` is independent of
   input word order. *(21.4)*
5. Memory survives an app restart — a fresh `Db` over the same file retrieves
   the same rows. *(21.1)*
6. A memory containing prompt delimiters cannot alter prompt structure — a
   builder test with `<|im_end|>` in a memory string: exactly one system turn,
   the text inert. *(21.4)*
7. Scope isolation — persona A's retrieval never returns persona B's memories
   (no cross-scope leakage, FR-56). *(21.1, 21.4)*

## ADRs / open questions
- **No new ADR** — ADR-0012 fixes the design. Phase-entry finalisations recorded
  here + in the verification doc:
  - **FTS5 is a standalone table kept in sync by the repo in one transaction**
    (not external-content + triggers) — Rust owns all SQL, so no trigger layer.
  - **Tunables are module constants**, not config (precedent: the context
    builder's budget constants). Revisit if a real need appears.
  - **Retrieval is a pure read** for v1 — no `retrieved_count` / `last_retrieved`
    columns. The inline prune uses `importance` + `created_at` only.
  - **Extraction concurrency**: `Semaphore(1)`, a 2nd request dropped (the next
    turn re-covers recent context).
- **Open (revisited here with data):** the embeddings trigger — ADR-0012 says a
  measured recall gap **and** > ~500 memories/scope. The `target: "memory"`
  recall-gap log is the instrument; samples from these tests + early real use go
  in the verification doc. No change to the deferral this phase.
- **Open (later):** memory edit/correct (FR-54), a background prune scheduler
  with a user setting, character-scoped memory (Phase 26).
