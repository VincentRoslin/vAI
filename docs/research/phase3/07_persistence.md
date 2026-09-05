# Phase 3 · Persistence — driver, pool, schema, blob store

Covers step 3.6 and D-8. Requirements: FR-14, FR-15, FR-40..42, FR-31, FR-C50;
ARQ-17, NFR-31, NFR-50.

---

## D-8 — Design

### Driver + pool
- **`rusqlite`** (thin, sync, full control) + **`deadpool-sqlite`** small pool.
  Run queries on `tokio::task::spawn_blocking` / the pool's async wrapper.
- Alternative `sqlx`: async-native, compile-checked queries, built-in migrator —
  but heavier, its macro needs a dev DB, and SQLite's threading model fights its
  async model in places. **Recommendation: `rusqlite` + `deadpool-sqlite`**;
  hand-written SQL in repository modules. `sqlx` is a defensible alternative if
  the team strongly wants compile-checked queries — record as the runner-up.
- **Topology:** 1 dedicated **write connection** (all writes serialized through an
  actor / `Mutex<Connection>`) + a small **read pool** (2–4) in WAL mode. This
  eliminates writer contention entirely, which matters because memory extraction,
  conversation persistence, and character updates can all write concurrently.
  Simpler v1 alternative: one small pool + `busy_timeout`; add the dedicated
  writer only if `SQLITE_BUSY` shows up. **Lean: dedicated writer from the
  start** — it's ~30 lines and removes a whole class of Phase 4 risk.

### Pragmas (every connection)
`journal_mode=WAL` · `busy_timeout=5000` · `foreign_keys=ON` ·
`synchronous=NORMAL` · `wal_autocheckpoint` tuned + periodic
`wal_checkpoint(TRUNCATE)` · consider `mmap_size` after measuring.

### Migrations
- **`refinery`** (works with `rusqlite`, embedded numbered `.sql`/Rust
  migrations, forward-only) or a hand-rolled `user_version` runner.
  **Recommendation: `refinery`.**
- `schema_version` recorded; app refuses an unknown-newer DB; runs pending
  migrations **in a transaction** on startup; **online backup (`VACUUM INTO` to a
  timestamped file) before every migration**, keep last K.

### Schema groups (one DB file, table-name prefixes)
`core_*` (app meta, schema version, config snapshots) · `conv_*` (conversations,
messages, generation metadata) · `persona_*` · `char_*` (characters, appearance,
relationship state, links) · `mem_*` (memories + FTS5, scoped by persona/character
id) · `model_*` (registry, downloads, loras, presets) · `asset_*` (blob metadata).
One file keeps foreign keys working across groups and keeps backup simple.
Multiple files only if a group needs a different retention/backup policy (none do
now).

### Blob store (images, audio, reference sets)
- **Content-addressed on disk**: `app_data/blobs/<sha256[0:2]>/<sha256>`.
  Dedupes identical assets (reused character images, repeated generations with the
  same seed). Rust owns the dir; paths validated + confined; no `..`.
- SQLite `asset_*` row = `{ id, sha256, byte_size, mime, kind, created_at }`;
  domain tables link by asset id. The row is the source of truth for existence;
  a reconcile job finds orphaned files and dangling links.
- Threshold: anything > ~32 KB or any image/audio → blob store. Tiny thumbnails
  may be generated on the fly or cached as small blobs.

### Errors
`rusqlite::Error` → structured `DbError` (`NotFound`, `Conflict`, `Busy`,
`Corruption`, `Migration`, `Other`); raw driver errors never escape the layer.
`SQLITE_BUSY`/`LOCKED` after `busy_timeout` → bounded retry with jitter → `Busy`.
`PRAGMA quick_check` on startup; `SQLITE_CORRUPT` → surface a restore-from-backup
path.

→ **ADR-0009**: `rusqlite` + `deadpool-sqlite`, dedicated writer + read pool, WAL
pragma set, `refinery` migrations with pre-migration `VACUUM INTO` backup, one DB
file with prefixed groups, content-addressed blob store, structured `DbError`.

---

## Optimizations
1. **Content-addressed blob store** — automatic dedup of identical images/audio;
   a re-roll with the same seed+params costs zero extra disk.
2. **FTS5 for memory retrieval** (see `10_memory.md`) — no vector DB, no extra
   process, sub-ms keyword search.
3. **Dedicated writer** — removes lock contention and its retry overhead entirely.
4. **Prepared-statement cache** (`rusqlite` `Connection::prepare_cached`) on the
   hot paths (append message, retrieve memory, list conversations).
5. **Batch inserts in one transaction** for message history restore and
   memory-extraction results.
6. **WAL checkpoint on idle**, not on every commit — keeps the `-wal` file bounded
   without stalling writes.
7. **`PRAGMA optimize`** on clean shutdown to keep query plans good as data grows.
8. **Thumbnail cache** for gallery/discovery grids so the UI never decodes
   full-res images for a list view.

## Failure modes
- Migration fails mid-run → transaction rollback; the pre-migration backup exists;
  app reports and does not half-migrate.
- DB corruption → `quick_check` catches it on startup; restore-from-backup flow.
- Blob file missing but row exists → asset shows a "missing" state; reconcile job
  logs it; the owning feature degrades gracefully (character shows a placeholder).
- Disk full on a write → transaction fails cleanly; typed `DbError`; nothing
  partially written (WAL + transaction).
