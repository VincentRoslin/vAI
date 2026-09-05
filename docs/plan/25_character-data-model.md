# Phase 25 — Character Data Model & CRUD

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0015). The design below is settled. Concrete implementation specifics (exact modules, crate APIs, filenames) are filled in at phase entry against the frozen ADRs — they do not change the design.

## Objective
Characters as **persistent structured entities**, not prompts: identity, name,
appearance, personality, interests, relationship state, memory links, reference
images, generated images. Create / read / update / delete with validation and
migrations.

## Depends on
Phase 9 (persistence), Phase 20 (persona/context builder — characters extend it),
Phase 7 (contracts). The exact fields come from `PROJECT.md` (Phase 5) which comes
from Product Definition.

## Not in this phase
- Character conversations (Phase 26).
- Image identity (Phase 27), typed image actions (Phase 28), discovery (Phase 29).
- Generating characters — this phase stores and edits them.

## Architecture notes
- One character schema; the "one authority" for character state is the Rust core.
- Structured fields, not a serialized blob — so the context builder and identity
  system can read individual attributes.
- Reference/generated images are blob-store entries linked by id, not embedded.
- Relationship state is a typed field with defined transitions (not free text the
  model rewrites).

## Performance notes
- Character list (for discovery/galleries) is a frequent read — indexed, paginated.
- Loading a character for a conversation batches its related reads (images,
  memory links) in one round trip.

## Step outline
1. Character schema (migration) from `PROJECT.md`; relationship-state enum;
   image link tables.
2. Repository: create, get (with related data), list (paginated), update, delete
   (cascade images/memory links with confirmation).
3. Validation: required fields, enum ranges, image links must resolve, no
   traversal in any path.
4. Character ↔ conversation link (a conversation belongs to a character).
5. Character ↔ memory link (character-scoped memory, building on Phase 21).
6. Tests: CRUD round-trip, validation failures, cascade delete leaves no orphans,
   persists across restart, stable id.

## Verification gate
1. Create / read / update / delete a character as structured data.
2. Validation rejects each malformed-character case.
3. Deleting a character removes its images + memory links with no orphans
   (query-verified).
4. Character state persists across an app restart.
5. Character id is stable.
6. Character list read is paginated and indexed.

## ADRs / open questions
- Relationship-state model (levels vs attributes) — ADR, informed by `PROJECT.md`.
