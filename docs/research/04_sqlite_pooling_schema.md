# 04 — SQLite: connection pooling & schema separation

Scope: the one database access layer, owned by Rust. Frontend and workers never
touch it.

---

## 1. Driver / pool choice

| Option | Notes |
| ------ | ----- |
| **`rusqlite` + `r2d2` / `deadpool-sqlite`** | Thin, synchronous; run on `tokio::task::spawn_blocking` or a dedicated thread pool. Full control. Most common for desktop. |
| **`sqlx` (sqlite)** | Async-native, compile-time-checked queries (`query!`), built-in migrations. Heavier; some friction with SQLite's threading model; macro needs a dev DB. |
| **`sea-orm` / `diesel`** | ORM overhead; likely more than this app needs. Constitution says "smallest correct". |

**Leaning:** `rusqlite` + a small pool (`deadpool-sqlite`), queries as hand-written
SQL in repository modules. `sqlx` is a reasonable alternative if the team wants
compile-checked queries + its migrator. **ADR.**

---

## 2. SQLite pragmas (set on every connection)

- `journal_mode = WAL` — concurrent readers + one writer; far fewer "database is
  locked" errors. (Note: WAL doesn't work on network shares — fine, we're local.)
- `busy_timeout = 5000` (ms) — let the driver wait out a transient lock instead of
  erroring immediately.
- `foreign_keys = ON` — off by default in SQLite, must set per connection.
- `synchronous = NORMAL` — safe with WAL, much faster than `FULL`.
- `wal_autocheckpoint` / periodic manual `wal_checkpoint(TRUNCATE)` to keep the
  `-wal` file bounded.
- Consider `mmap_size` and `cache_size` tuning later (measure first — Performance
  doc).

---

## 3. Pool topology

SQLite has one writer. A common, robust pattern:

- **1 dedicated write connection** (serialize all writes through it — an actor or a
  `Mutex<Connection>` + queue). Eliminates writer contention entirely.
- **N read connections** (pool, e.g. 4) in WAL mode for concurrent queries.
- All access via `spawn_blocking` (rusqlite is sync) or the async wrapper.

Simpler alt for v1: a small pool (size 2–4), rely on `busy_timeout` + WAL, add the
dedicated-writer split only if lock errors show up under load. Don't pre-build it.

---

## 4. Schema separation: metadata vs binary blobs

**Rule (from the guide): large images/audio live on the filesystem; SQLite stores
metadata + paths.**

- Tables hold: ids, timestamps, relationships, small text, enums, and a
  **relative path** (+ hash + byte size + mime) for any large asset.
- Binary bytes → a content-addressed store on disk:
  `data/blobs/<sha256[0:2]>/<sha256>` (dedupes identical assets, e.g. reused
  character images).
- Rust owns this dir; paths are validated + confined (no `..`, must resolve under
  the app data root — matches `CLAUDE.md` Article I filesystem ownership).
- DB row is the source of truth for "does this asset exist"; a reconcile job
  detects orphaned files and dangling paths.
- What counts as "large"? Draft threshold: > ~64 KB or any audio/image/video →
  filesystem. Tiny thumbnails could go inline as `BLOB` — decide.

### Logical schema grouping (not necessarily separate DB files)
- `core` — app/schema version, config snapshots.
- `conversations` — conversations, messages, generation metadata.
- `models` — registry entries, capabilities, resource estimates.
- `characters` — personas, character state, memory, gallery links.
- `assets` — blob metadata (path, hash, size, mime, refcount).
- `tasks` — task/job history (optional; may be log-only).

One DB file with table-name prefixes or `ATTACH`-ed files? **One file is simpler**
and lets foreign keys span groups. Multiple files only if one group needs a
different backup/retention policy. **ADR.**

---

## 5. Migrations

- Forward-only, numbered, each with an `up` (and `down` where feasible).
- Tool: `sqlx migrate`, `refinery`, or hand-rolled (`user_version` pragma +
  ordered `.sql` files). `refinery` works with `rusqlite`.
- `schema_version` recorded in `core`; app refuses to start on an unknown-newer
  version; runs pending migrations in a transaction on startup.
- Test matrix (guide Phase 9): fresh create; upgrade from each prior version;
  repeated startup (idempotent); migration failure → clean rollback, app reports
  and doesn't half-migrate; backup file written before migrating.

---

## 6. Transactions & errors

- Repository methods take `&mut Transaction` or a connection; multi-step writes
  are always in a transaction with rollback on `Err`.
- Map `rusqlite::Error` → structured `DbError` (NotFound, Conflict, Busy,
  Corruption, Migration, Other); never leak raw driver errors past the DB layer.
- `SQLITE_BUSY` / `SQLITE_LOCKED` after `busy_timeout` → retry a bounded number of
  times with jitter, then `DbError::Busy`.
- Corruption (`SQLITE_CORRUPT`): detect on startup with `PRAGMA integrity_check`
  (quick check), surface a recovery path (restore last backup).

---

## 7. Backup
- Periodic online backup via the SQLite backup API (`rusqlite::backup`) or
  `VACUUM INTO` to a timestamped file; before every migration. Keep last K.
