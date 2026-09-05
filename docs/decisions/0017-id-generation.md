# ADR-0017 — Entity ID generation: UUIDv4

- **Status:** ACCEPTED (decided at Phase 11 entry, 2026-09-06) · **Date:** 2026-09-06
- **Closes:** the ID-generation policy Phase 7 deferred to "Phase 9, where IDs are
  minted" (Phase 9 minted none — Phase 11 is the first).

## Context
`contracts::ids` newtypes (`ModelId`, `ConversationId`, `MessageId`, …) are opaque
non-empty strings. Something has to mint them. The model registry (Phase 11) is
the first entity store that creates rows with an id.

## Options considered
- **UUIDv4** — random, no coordination, `uuid` crate already in the tree.
- **UUIDv7** — time-ordered (better index locality) but needs a newer `uuid`
  feature and the ordering benefit is marginal at this data scale.
- **Content-addressed** (hash of the entity's identity) — idempotent
  re-registration, but "identity" is fuzzy for a model (same GGUF from two repos)
  and meaningless for a conversation.
- **DB rowid-backed** — ties the public id to storage internals; breaks if a row
  is ever re-inserted (restore, sync).

## Decision
- **UUIDv4**, rendered lowercase-hyphenated (`uuid::Uuid::new_v4()` →
  `Hyphenated`), wrapped in the relevant `contracts::ids` newtype.
- **Minted by the module that creates the entity** (`ModelRegistry::register`,
  later the conversation engine, …), never by the frontend.
- **Opaque forever** — no code parses structure out of an id (ADR-0016 §6 already
  says this for config; it is the rule for all ids).
- `uuid = { version = "1", features = ["v4"] }` as a direct dependency (already in
  the tree via `tauri-utils` — no new build cost).

## Consequences
- IDs are not sortable by creation time. Where creation order matters, order by a
  `created_at` column, not the id.
- Re-registering the same model file produces a new id. Idempotency, if ever
  needed, is a separate lookup (by path / by hash), not an id property.
- Revisit UUIDv7 only if index locality becomes a measured problem (Phase 31).
