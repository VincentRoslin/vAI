# Phase 9 — SQLite Persistence

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0015). The design below is settled. Concrete implementation specifics (exact modules, crate APIs, filenames) are filled in at phase entry against the frozen ADRs — they do not change the design.

## Objective
The Rust-owned persistence layer: migrations, schema versioning, connection
management, transactions, repositories, structured errors. Minimum schema for now
(not every future table). Frontend and subprocesses never touch the DB.

## Depends on
Phase 7 (contracts), Phase 8 (config: DB path), Phase 10 can land in parallel.

## Not in this phase
- Feature schemas (conversations, characters, memory) beyond the minimum needed
  to prove the layer.
- Blob storage internals (metadata columns are fine; the blob store lands with
  the first feature that needs it).

## Architecture notes
- Per the Phase 3 persistence ADR: driver, pool topology (plain pool vs dedicated
  writer), migration tool.
- Pragmas on every connection: WAL, `busy_timeout`, `foreign_keys=ON`,
  `synchronous=NORMAL`.
- Repositories expose typed methods; raw driver errors never escape the layer.
- Backup before every migration.

## Performance notes
- Record baseline timings for a single insert, a single indexed read, and a
  100-row transaction — Phase 31 references these.
- WAL checkpoint strategy so the `-wal` file stays bounded.

## Step outline
1. Connection factory with the pragma set; pool per the ADR.
2. Migration runner (numbered, forward-only, transactional, `user_version` or the
   ADR's tool); backup-before-migrate.
3. Migration 0001: minimal schema (schema-version table + one real table to
   exercise CRUD, e.g. `app_meta`).
4. Transaction helper (commit on Ok, rollback on Err; nesting rule).
5. One repository with typed CRUD + structured `DbError` mapping.
6. Integrity check on startup (`PRAGMA quick_check`) with a recovery path.
7. Backup mechanism (online backup API or `VACUUM INTO`), retain last K.
8. Tests: create-from-empty, migrate up/down, migrate idempotent, commit,
   rollback, persistence across restart, migration failure → clean rollback +
   backup exists, lock contention (concurrent writers) handled.

## Verification gate
1. DB creates from empty on first run.
2. `migrate up` then `migrate down` returns to empty; second `migrate up` is a
   no-op.
3. Transaction commit persists; forced mid-transaction error leaves zero partial
   writes.
4. Data survives an app restart.
5. A forced migration failure rolls back cleanly and a pre-migration backup file
   exists.
6. Concurrent-writer contention is handled (no unhandled `SQLITE_BUSY`).
7. Baseline timings recorded.

## ADRs / open questions
- Confirms the persistence ADR; may raise one on dedicated-writer if lock errors
  appear under load.
