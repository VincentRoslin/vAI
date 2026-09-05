# Phase 9 — SQLite Persistence

> **Architecture frozen at Phase 5.** Step detail **finalized at phase entry, 2026-09-06**. Governing: **ADR-0009** (rusqlite + `deadpool-sqlite`, dedicated writer + read pool, `refinery` forward-only migrations with verified pre-migration backup, one DB file with prefixed groups, structured `DbError`), `CLAUDE.md` Article I (Rust owns all DB access), Article IV.

## Objective
The Rust-owned persistence layer: connection topology + pragmas, a forward-only
migration runner with a verified backup before each run, a transaction helper, one
typed repository, structured `DbError`, and a startup integrity check. Minimum
schema — just enough to exercise the layer. Frontend and subprocesses never touch
the DB.

## Depends on
Phase 7 (`AppError` — `DbError` maps into it), Phase 8 (app-data root; the DB
path is *derived* from it, not a config key — nothing asks for a movable DB).
Phase 10 (observability) can land in parallel.

## Not in this phase
- Feature schemas (conversations, characters, memory) beyond a single `core_*`
  table to prove CRUD.
- The **content-addressed blob store** — lands with the first feature that stores
  a blob (ADR-0009; `asset_*` columns come with it).
- Any IPC command — no DB surface reaches the frontend this phase.
- A restore-from-backup *flow* — Phase 9 detects corruption and points at the
  backups; the guided restore is a later reliability phase (33).

## Collision noted at phase entry
The plan's original gate item 2 ("`migrate up` then `migrate down` returns to
empty") conflicts with **ADR-0009's forward-only decision** (`refinery`, no down
migrations). The ADR is frozen and wins. Gate item 2 is reframed to **idempotency**
(a second `migrate` is a no-op) + **recorded version**. Recorded here and in
`docs/verification/08_phase9_persistence.md`.

## Architecture notes
- **`rusqlite`** (`bundled` feature — SQLite compiled in, no system dependency,
  offline/packaging-clean) + **`deadpool-sqlite`** pools.
- **Topology (ADR-0009):** a **writer pool of `max_size = 1`** (serializes every
  write — no `SQLITE_BUSY` from our own code) + a **reader pool of `max_size =
  4`** (`query_only = ON`). Uniform async API; both are `deadpool_sqlite::Pool`.
- **Pragmas on every connection** (a `deadpool` post-create hook):
  `journal_mode = WAL`, `busy_timeout = 5000`, `foreign_keys = ON`,
  `synchronous = NORMAL`, `wal_autocheckpoint = 1000`. Reader connections also
  `query_only = ON`.
- **Migrations:** `refinery` with `embed_migrations!`, **grouped in one
  transaction** (all-or-nothing), forward-only. Before running, if any migration
  is pending: `VACUUM INTO '<backups>/pre-migrate-<unix>.db'`, then **verify the
  file exists and is non-zero** — if not, refuse to migrate (`DbError::Migration`,
  Phase 4 R-H7). Keep the last **5** backups.
- **Unknown-newer DB:** if the DB's last applied migration version exceeds the
  highest embedded version → refuse (`DbError::Migration`).
- **Integrity:** `PRAGMA quick_check` on open; a non-`ok` result →
  `DbError::Corruption` with a message naming the backup directory.
- **Errors:** `DbError { NotFound, Conflict, Busy, Corruption, Migration, Io,
  Other }` (`thiserror`); `impl From<DbError> for AppError` at the boundary
  (`Busy`→`ResourceExhausted`? no → `Conflict`; `Corruption`/`Migration`/`Io`/
  `Other`→`Internal` with the chain logged; `NotFound`→`NotFound`;
  `Conflict`→`Conflict`). Raw `rusqlite::Error` never escapes `db/`.
- **Transaction helper:** `Db::write(|tx| …)` — `deadpool` `interact`, `BEGIN
  IMMEDIATE`, run the closure, `COMMIT` on `Ok` / `ROLLBACK` on `Err`. **One
  level only** — no nested transactions this phase (savepoints deferred, documented).
- **Prepared-statement cache:** `Connection::prepare_cached` in the repository.
- `PRAGMA optimize` on a clean shutdown hook.

## Performance notes
- Record baselines: single insert, single indexed read (PK lookup), a 100-row
  insert in one transaction. Into `docs/verification/08_phase9_persistence.md`.
- WAL: `wal_autocheckpoint = 1000` + a `wal_checkpoint(TRUNCATE)` on the clean
  shutdown hook so the `-wal` file stays bounded.

## Steps (atomic; each independently verifiable)

**9.1 — Deps + module skeleton**
    Do:     Add `rusqlite` (`bundled`), `deadpool-sqlite`, `refinery`
            (`rusqlite`), `tokio` (`rt-multi-thread`, `macros`, `sync`) to
            `Cargo.toml`. `db/mod.rs` + `db/error.rs` + `db/migrations/`.
            Register in `lib.rs`, `src-tauri/README.md`, `ARCHITECTURE.md` §2.
    Verify: `cargo build`; `cargo tree -d` shows no rusqlite version split.

**9.2 — `DbError` + `AppError` mapping**
    Do:     `db/error.rs` — the enum, `From<rusqlite::Error>`,
            `From<deadpool_sqlite::PoolError>`, `From<DbError> for AppError`.
    Verify: `cargo test db::error` — each variant maps to the documented
            `AppError` kind; a `rusqlite` `QueryReturnedNoRows` → `DbError::NotFound`.

**9.3 — Connection factory + pools + pragmas**
    Do:     `Db::open(db_path: &Path)` — build the writer pool (1) + reader pool
            (4) with a post-create pragma hook; run `quick_check` on one
            connection.
    Verify: `cargo test db::opens_and_sets_pragmas` — `journal_mode` is `wal`,
            `foreign_keys` is `1`, a reader connection rejects a write
            (`query_only`).

**9.4 — Migration runner + backup**
    Do:     `Db::migrate()` — compute pending; if any, `VACUUM INTO` a timestamped
            backup + verify non-zero (else `DbError::Migration`), then
            `refinery` grouped run; refuse an unknown-newer DB; prune to 5
            backups.
    Verify: `cargo test db::migration_*` — fresh DB: all migrations apply,
            `applied_version()` == embedded max, a backup exists; second call is a
            no-op; a hand-bumped history version → refused.

**9.5 — Migration 0001 (minimal schema)**
    Do:     `V0001__init.sql` — `core_app_meta(key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL, updated_at TEXT NOT NULL) STRICT;`
    Verify: after `migrate()`, `core_app_meta` exists (`sqlite_master` query in a
            test).

**9.6 — Transaction helper**
    Do:     `Db::write<T>(f: impl FnOnce(&Transaction) -> Result<T, DbError>)` —
            `BEGIN IMMEDIATE` … commit/rollback. Reject re-entry (debug assert +
            doc).
    Verify: `cargo test db::tx_*` — commit persists; a closure returning `Err`
            leaves zero rows; a panic in the closure rolls back (no poison leak
            into later calls).

**9.7 — `AppMetaRepo`**
    Do:     `get(key) -> Option<String>`, `set(key, value)` (upsert, stamps
            `updated_at`), `all() -> Vec<(String, String)>`. `prepare_cached`.
    Verify: `cargo test db::app_meta_*` — set→get round-trips; overwrite updates
            `updated_at`; `get` of a missing key is `None` (not an error).

**9.8 — Concurrency + restart**
    Do:     (covered by the topology; tests only.)
    Verify: `cargo test db::concurrent_writers` — N tasks each `set` a distinct
            key concurrently → all succeed, all present, no `Busy`.
            `cargo test db::survives_reopen` — write, drop `Db`, `open` the same
            path, value is still there.

**9.9 — Startup wiring + shutdown hook**
    Do:     `lib.rs setup()` — `Db::open(<app_data>/localai.db)` → `migrate()` →
            log schema version + open time → `app.manage(db)`. On exit / window
            close: `wal_checkpoint(TRUNCATE)` + `PRAGMA optimize`.
    Verify: `npm run tauri dev` — log shows the DB opened + migrated + version, no
            errors; `localai.db` + `-wal` created under the app data dir.

**9.10 — Baselines + corruption check**
    Do:     A `#[test]` timing single insert / PK read / 100-row tx (print
            ns/op). A test that writes garbage over a DB file → `open` returns
            `DbError::Corruption`.
    Verify: numbers recorded; corruption test passes.

**9.11 — Gate run + docs + commit**
    Do:     `node scripts/check.mjs`; `docs/verification/08_phase9_persistence.md`;
            update `DEVELOPMENT.md` (DB path + reset already there — refine),
            `ROADMAP.md` §1/§4/§5. Commit.
    Verify: check suite green; every gate check recorded with evidence.

## Verification gate
1. DB creates from empty on first run (file + schema). *(9.4, 9.9)*
2. Migration is **forward-only + idempotent**: a fresh DB applies all migrations
   and records the version; a second `migrate()` is a no-op. *(ADR-0009 —
   replaces the original "migrate down"; 9.4)*
3. Transaction commit persists; a forced mid-transaction error (and a panic)
   leaves zero partial writes. *(9.6)*
4. Data survives an app restart (drop + reopen). *(9.8)*
5. A forced migration failure rolls back cleanly (grouped transaction) and a
   verified pre-migration backup file exists. *(9.4)*
6. Concurrent writers are handled — N parallel writes, all succeed, no unhandled
   `SQLITE_BUSY`. *(9.8)*
7. Baseline timings recorded (insert / indexed read / 100-row tx). *(9.10)*
8. `PRAGMA quick_check` catches a corrupted file on open → `DbError::Corruption`. *(9.10)*
9. Full check suite green; no `rusqlite` version split in `cargo tree -d`. *(9.11)*

## ADRs / open questions
- Confirms ADR-0009. Raises a follow-up only if `deadpool` + `refinery` force a
  `rusqlite` version pin worth recording.
- `synchronous = FULL` on the writer (durability vs speed) — ADR-0009 deferred it
  to "Phase 9 with measurements". Decision: **keep `NORMAL`** (WAL + `NORMAL` is
  crash-safe against app crashes; `FULL` only adds protection against OS/power
  loss at a real write cost). Record the reasoning in the verification doc; revisit
  only if corruption is ever observed.
