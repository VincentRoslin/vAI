# ADR-0009 — Persistence: rusqlite + dedicated writer + refinery + blob store

- **Status:** ACCEPTED (Phase 5 freeze, 2026-09-05; superseding attacks folded in via Phase 4) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/07_persistence.md` (D-8)

## Context
Rust-owned local persistence (NFR-50/51). Concurrent writers (conversation
persistence, memory extraction, character updates). Large binaries (images, audio)
must not bloat SQLite.

## Options considered
- Driver: `rusqlite` + `deadpool-sqlite` vs `sqlx`.
- Pool: plain pool + `busy_timeout` vs dedicated writer + read pool.
- Migrations: `refinery` vs hand-rolled `user_version`.
- Schema: one DB file (prefixed groups) vs multiple attached files.

## Decision
- **`rusqlite` + `deadpool-sqlite`**, hand-written SQL in repository modules.
  (`sqlx` recorded as runner-up if compile-checked queries are wanted.)
- **One dedicated write connection** (writes serialized through an actor) + a
  **2–4 read pool** in WAL mode. Removes writer contention entirely.
- Pragmas every connection: `WAL`, `busy_timeout=5000`, `foreign_keys=ON`,
  `synchronous=NORMAL`, tuned checkpointing.
- **`refinery`** migrations, forward-only, transactional, with a **`VACUUM INTO`
  backup before every migration** (keep last K). The runner **verifies the backup
  file exists and is non-zero before applying any migration** — if the backup
  fails (e.g. disk full), it refuses to migrate and surfaces the problem
  (Phase 4 R-H7). App refuses an unknown-newer DB.
- **One DB file**, table-name-prefixed groups: `core_ conv_ persona_ char_ mem_
  model_ asset_`.
- **Content-addressed blob store** `app_data/blobs/<sha256[0:2]>/<sha256>` for
  images/audio/reference sets; SQLite `asset_*` holds metadata + links.
  **Write order: write blob → fsync → commit the row** (Phase 4 R-M8), so a crash
  leaves at worst an orphan file (harmless, reclaimed) never a dangling link. A
  reconcile job finds orphans/dangling links. Threshold ~32 KB.
- `rusqlite::Error` → structured `DbError`; `PRAGMA quick_check` on startup with a
  restore-from-backup path.

## Consequences
- No `SQLITE_BUSY` class of Phase 4 risk (dedicated writer).
- Blob dedup is automatic (same seed+params = same bytes = same file).
- FTS5 (ADR-0012) lives in the same file, one backup covers everything.
- Prepared-statement caching + batch-insert transactions are the hot-path
  optimizations (research §Optimizations).
