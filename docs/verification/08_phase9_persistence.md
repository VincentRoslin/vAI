# 08 — Phase 9: SQLite Persistence

**Date:** 2026-09-06
**Branch:** `main` (local; no remote).
**Method:** new `src-tauri/src/db/` module; 11 async unit tests over the gate; a
real `tauri dev` launch with the DB open + migrate observed in the structured log.

Governing: **ADR-0009**. Plan: `docs/plan/09_sqlite-persistence.md`.

---

## What landed

- `db/mod.rs` — `Db` (writer pool `max_size = 1` + reader pool `max_size = 4`,
  `deadpool-sqlite`), per-connection pragma hook (WAL, `busy_timeout = 5000`,
  `foreign_keys = ON`, `synchronous = NORMAL`, `wal_autocheckpoint = 1000`,
  `query_only` on readers), `open` (with a `quick_check` pre-flight on an existing
  file), `migrate` (forward-only, grouped, verified `VACUUM INTO` backup, keep 5,
  refuse an unknown-newer DB), `write` (`IMMEDIATE` tx, commit/rollback),
  `read`, `integrity_check`, `checkpoint_and_optimize`, `AppMetaRepo`.
- `db/error.rs` — `DbError { NotFound, Conflict, Busy, Corruption, Migration, Io,
  Other }` + `From<rusqlite::Error / PoolError / InteractError / io::Error /
  refinery::Error>` + `From<DbError> for AppError`.
- `db/migrations/V0001__init.sql` — `core_app_meta(key, value, updated_at) STRICT`.
- `lib.rs` — `setup()` opens + migrates the DB (`<app_data>/localai.db`) into
  managed state and logs the schema version + open time; `RunEvent::ExitRequested`
  runs `wal_checkpoint(TRUNCATE)` + `PRAGMA optimize`.
- Deps: `rusqlite 0.37` (`bundled`), `deadpool-sqlite 0.12`, `refinery 0.9`
  (no-default + `rusqlite`), `tokio 1` (`rt-multi-thread`, `macros`, `sync`).

---

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | DB creates from empty on first run | **PASS** — `tauri dev`: `localai.db` + `-wal` + `-shm` + `db-backups/` created under `%APPDATA%\com.localai.app\`; `"database ready" schema_version=1`. Test `migration_creates_the_schema_and_is_idempotent` confirms `core_app_meta` exists. |
| 2 | Migration is forward-only + idempotent (replaces "migrate down", per ADR-0009) | **PASS** — `migration_creates_the_schema_and_is_idempotent`: fresh DB → version 1; a second `migrate()` returns the same version, no error, no work. |
| 3 | Transaction commit persists; a forced mid-tx error / panic leaves zero partial writes | **PASS** — `transaction_commit_persists_and_error_rolls_back` (closure returns `Err` after an insert → row absent); `transaction_panic_rolls_back_and_pool_recovers` (panic inside the closure → row absent, pool still serves the next call). |
| 4 | Data survives an app restart | **PASS** — `data_survives_a_reopen`: write → drop `Db` → `open` + `migrate` same path → value present. |
| 5 | Forced migration failure rolls back cleanly + a verified pre-migration backup exists | **PASS** — grouped (`set_grouped(true)`) = one transaction, all-or-nothing. `migration_takes_a_verified_backup`: exactly one `pre-migrate-<unix>.db`, non-zero. `migration_refuses_a_newer_database`: a hand-inserted `version 999` history row → `DbError::Migration`. `back_up` returns `DbError::Migration` if the backup is empty (R-H7). |
| 6 | Concurrent writers handled — no unhandled `SQLITE_BUSY` | **PASS** — `concurrent_writers_all_succeed`: 24 parallel `tokio` tasks each `set` a distinct key → all `Ok`, all 24 present. The writer pool of 1 serializes them. |
| 7 | Baseline timings recorded | **PASS** — see below. |
| 8 | `quick_check` catches a corrupted file on open → `DbError::Corruption` | **PASS** — `corrupt_file_is_rejected_on_open`: garbage bytes at the DB path → `Db::open` → `DbError::Corruption`. |
| 9 | Full check suite green; no `rusqlite` version split | **PASS** — `node scripts/check.mjs` all green; `cargo tree -d` shows no `rusqlite` / `libsqlite3-sys` duplication. |

`cargo test`: **99 Rust tests** (88 prior + 11 db). Frontend unchanged (5 Vitest).

---

## Baselines (warm pools; for Phase 31)

| Operation | Time | Notes |
| --------- | ---- | ----- |
| Single insert (upsert, prepared) | **~0.12 ms** | `AppMetaRepo::set` |
| Single indexed read (PK lookup, prepared) | **~0.05 ms** | `AppMetaRepo::get` |
| 100-row insert in one transaction | **~0.16 ms** | prepared statement reused in the tx |
| Cold DB open + first migration + backup | **~23 ms** | one-time, first run only (`tauri dev` log) |
| Warm DB open + `migrate()` no-op | **~2 ms** | subsequent runs |

First read on a cold pool is ~8 ms — that is connection creation + the pragma
hook, not query cost; the numbers above are after a warm-up call.

## Decisions taken at phase entry

- **`synchronous = NORMAL`** kept (ADR-0009 deferred this to "Phase 9 with
  measurements"). WAL + `NORMAL` is safe against application and OS crashes;
  `FULL` only adds protection against power loss mid-write, at a real per-commit
  cost. Revisit only if corruption is ever observed.
- **Gate item 2 reframed** from "migrate down" to "forward-only + idempotent" —
  `refinery` and ADR-0009 are forward-only. Down-migrations are not a thing here.
- **Backup on the very first migration** (empty DB → 4 KB backup) is kept for a
  uniform code path; the "verify non-zero" check still runs. Real value is on
  migration N>1.
- Version alignment: `refinery 0.9` caps `rusqlite` at `<= 0.37`, so the stack is
  pinned at `rusqlite 0.37` + `deadpool-sqlite 0.12`. Recorded here; revisit if a
  newer `rusqlite` feature is needed.
- **Blob store deferred** to the first feature that stores a blob (ADR-0009).

**Phase 9 complete.** Pointer → Phase 10 (Observability).
