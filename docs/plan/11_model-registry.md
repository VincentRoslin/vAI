# Phase 11 — Model Registry

> **Architecture frozen at Phase 5.** Step detail **finalized at phase entry, 2026-09-06**. Governing: **ADR-0009** (persistence, `model_*` table group), **ADR-0008** (a completed download → a registry row, path confined to the model dir), Phase 7 contracts (`ModelId`, `ModelKind`, `ModelBackend`, `Quant`, `ModelCapabilities`, `ModelMetadata`), `SECURITY.md` §path-confinement, **ADR-0017** (ID generation — written this phase).

## Objective
Model metadata as **data in a table**: stable `ModelId`, display name, kind
(LLM/STT/TTS/image/embedder), backend, file path, capabilities (streaming,
context), quantization, estimated VRAM, supported devices, per-model config
blob. Queryable by id / kind / capability. A missing file is a *represented
state*, never a crash. **No loading, no downloading.**

## Depends on
Phase 7 (contracts), Phase 9 (persistence — `Db`, migrations, `DbError`),
Phase 8 (the model dir, for path confinement).

## Not in this phase
- Loading / running / resource-reserving a model (Phase 14–15).
- Downloading a model or parsing a GGUF header (Phase 12) — this phase provides
  the `ModelDraft` → row path that Phase 12 fills.
- LoRAs and generation presets — their own `model_*` tables at Phase 22.
- A settings/model-picker UI or IPC command (Phase 12+). Phase 11 is data-layer.

## Architecture notes
- New module `src-tauri/src/models/` (`mod.rs`, `tests.rs`). Migration
  `V0002__model_registry.sql` — one `STRICT` table `model_entry`.
- Domain vs contract (Phase 7 rule): a domain `Model` (the full row, incl.
  `PathBuf`, devices, config `Value`) and an explicit view to the wire type.
  New contracts in `contracts::model` (additive): `RegisteredModel` (the row as
  the frontend sees it), `RegistryAvailability { Ready, Missing }`,
  `Device { Cuda, Cpu }`.
- **`ModelId` is minted at registration** (`uuid::Uuid::new_v4()`, ADR-0017),
  stored, and never changes — not even if the file path changes.
- **Availability is computed at read time** from `path.exists()` — never stored,
  so a file deleted after registration reads back as `Missing` with no crash.
  (This is distinct from `contracts::model::ModelState`, which is *runtime*
  lifecycle — Phase 14.)
- **Path confinement (register / update):** canonicalize the model dir and the
  candidate path; the candidate must be `starts_with` the dir and contain no
  `..`; a non-existent file is refused *at registration* (a later deletion is
  fine — see availability). Outside the dir → `AppError::Validation` / `DbError`.
- **In-memory cache:** `Mutex<Option<Vec<Model>>>` — populated on first read,
  cleared on every write. Availability is overlaid on the cached rows each read,
  so it never goes stale.
- `ModelRegistry` is added to Tauri managed state in `setup()` (Phase 12 uses it);
  no commands yet.

## Performance notes
- Registry reads happen on every model selection. The whole-list cache makes a
  warm `list()` / `get()` an in-memory filter. Record a warm `get` timing.
- `model_entry.kind` is indexed (`list_by_kind`).

## Steps (atomic; each independently verifiable)

**11.1 — ADR-0017 (ID generation)**
    Do:     `docs/decisions/0017-id-generation.md` — entity ids are
            `uuid::Uuid::new_v4()` rendered lowercase-hyphenated, minted by the
            module that creates the entity, opaque forever (nothing parses
            structure out). Add `uuid = { version = "1", features = ["v4"] }`
            (already in the tree). Update `docs/decisions/README.md`.
    Verify: file + README row present.

**11.2 — Contracts (additive)**
    Do:     `contracts::model` — `RegistryAvailability { Ready, Missing }`,
            `Device { Cuda, Cpu }`, `RegisteredModel { metadata: ModelMetadata,
            path: String, availability, devices: Vec<Device> }`. Round-trip +
            unknown-variant tests in `contracts::tests`.
    Verify: `cargo test contracts` green; bindings regenerate.

**11.3 — Migration `V0002__model_registry.sql`**
    Do:     `model_entry (id TEXT PK, display_name TEXT, kind TEXT, backend TEXT,
            quant TEXT NULL, path TEXT, streaming INTEGER, context_tokens INTEGER
            NULL, estimated_vram_mb INTEGER NULL, devices TEXT, config TEXT,
            created_at TEXT, updated_at TEXT) STRICT;` + `CREATE INDEX … (kind)`.
    Verify: `cargo test` — after `migrate()`, `model_entry` + the index exist;
            `Db::migrate` reports version 2, second call a no-op.

**11.4 — Domain types + row mapping**
    Do:     `models/mod.rs` — `Model` (row), `ModelDraft` (pre-insert: everything
            but `id` / timestamps), `ModelPatch` (optional fields for `update`).
            `Model::availability()` (`path.exists()`), `Model::to_registered()`
            → `RegisteredModel`. `Kind`/`Device` ↔ TEXT helpers.
    Verify: `cargo test models::mapping` — a `Model` round-trips domain → row
            columns → domain; `to_registered()` carries the computed availability.

**11.5 — `ModelRegistry` CRUD**
    Do:     `register(draft, models_dir) -> DbResult<ModelId>` (validate path,
            mint id, insert, invalidate cache); `get(&id)`, `list()`,
            `list_by_kind(kind)`, `update(&id, patch)`, `delete(&id)`.
            Raw `rusqlite` errors never escape — `DbError`.
    Verify: `cargo test models::crud` — register → `get` returns matching
            metadata; `list` / `list_by_kind` correct; `update` changes a field +
            bumps `updated_at`; `delete` then `get` → `DbError::NotFound`.

**11.6 — Path validation**
    Do:     `validate_model_path(candidate, models_dir) -> AppResult<PathBuf>` —
            reject `..`, reject outside `models_dir` (canonicalized), require the
            file exists at registration.
    Verify: `cargo test models::path_validation` — a path outside the dir →
            `Err`; a `..` path → `Err`; a real file inside → `Ok`; registering a
            non-existent path → `Err`.

**11.7 — Availability (missing file)**
    Do:     Covered by 11.4/11.5; a dedicated test.
    Verify: `cargo test models::missing_file` — register a real file, delete it,
            `get(&id)` returns `RegisteredModel` with `availability == Missing`,
            no error/panic; the row is still there.

**11.8 — Capability query**
    Do:     `ModelFilter { kind: Option<ModelKind>, streaming: Option<bool>,
            min_context_tokens: Option<u32> }`; `query(filter) -> Vec<RegisteredModel>`.
    Verify: `cargo test models::capability_query` — seed 3 models; a filter for
            `kind = Llm, min_context_tokens = 8192` returns exactly the matching
            one; `streaming = Some(true)` filters correctly.

**11.9 — Invalid-metadata rejection**
    Do:     `ModelDraft::validate()` — empty `display_name`, unknown `kind` string
            on read, `context_tokens == 0`, `estimated_vram_mb == 0`, bad device
            token → `AppError::Validation` naming the field.
    Verify: `cargo test models::rejects_invalid` — each failure mode is an `Err`
            with the field named; a deserialize of a row with a bad `kind` →
            `DbError`, not a panic.

**11.10 — ID stability across restart + cache invalidation**
    Do:     (behaviour; test only.)
    Verify: `cargo test models::id_stable_across_reload` — register, drop the
            `ModelRegistry` (keep the `Db` file), build a fresh `ModelRegistry`,
            `get(&id)` still returns it with the same id. `models::cache_invalidation`
            — `list()`, `register` another, `list()` again reflects it.

**11.11 — Startup wiring + baseline**
    Do:     `lib.rs setup()` — `app.manage(ModelRegistry::new(db.clone()))`
            (`Db` becomes `Arc<Db>` in managed state; adjust the exit hook).
            A `#[test]` timing a warm `get`.
    Verify: `npm run tauri dev` launches clean; warm-`get` number recorded.

**11.12 — Gate run + docs + commit**
    Do:     `node scripts/check.mjs`; `docs/verification/10_phase11_registry.md`;
            update `src-tauri/README.md`, `ARCHITECTURE.md` §2, `docs/contracts.md`,
            `docs/decisions/README.md`, `ROADMAP.md` §1/§4/§5. Commit.
    Verify: check suite green; every gate check recorded.

## Verification gate
1. Register a test model; find it; read its metadata back correctly. *(11.5)*
2. A registered model whose file is removed shows `availability: Missing`, no
   crash. *(11.7)*
3. Capability query returns correct results. *(11.8)*
4. Invalid metadata (each failure mode) is rejected with the field named. *(11.9)*
5. A path outside the model dir (or containing `..`) is refused. *(11.6)*
6. `ModelId` is stable across a restart (drop + rebuild the registry, same
   `Db`). *(11.10)*
7. Full check suite green; `src/bindings` regenerated + committed. *(11.12)*

## ADRs / open questions
- **ADR-0017** (this phase): entity ID generation = UUIDv4.
- **Capability schema** (the plan's open question): resolved as a **small typed
  set** on `ModelCapabilities` (`streaming: bool`, `context_tokens: Option<u32>`)
  extended additively — *not* free-form flags. Devices are a closed `Device`
  enum. Per-model backend knobs go in the opaque `config` blob, not capabilities.
