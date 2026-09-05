# 10 — Phase 11: Model Registry

**Date:** 2026-09-06
**Branch:** `main` (local; no remote).
**Method:** new `src-tauri/src/models/` module + `V0002__model_registry.sql`;
additive contracts; 14 async unit tests over the gate; a real `tauri dev` launch
(v1 → v2 migration on the existing dev DB).

Governing: ADR-0009, ADR-0008, **ADR-0017** (new — UUIDv4 ids). Plan:
`docs/plan/11_model-registry.md`.

---

## What landed

- `models/mod.rs` — `Model` (domain row), `ModelDraft`, `ModelFilter`,
  `validate_model_path`, `ModelRegistry` (`register` / `get` / `list` /
  `list_by_kind` / `query` / `update` / `delete`), `Arc<Vec<Model>>` cache
  cleared on write, availability from `path.is_file()`.
- `db/migrations/V0002__model_registry.sql` — `model_entry` `STRICT` table +
  `model_entry_kind_idx`.
- `contracts::model` (additive) — `RegistryAvailability { Ready, Missing }`,
  `Device { Cuda, Cpu }`, `RegisteredModel`.
- `lib.rs` — `Db` → `Arc<Db>` in managed state; `app.manage(ModelRegistry::new(db))`.
- ADR-0017 + `docs/decisions/README.md`. `uuid` (v4) direct dep (already in tree).

---

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | Register a test model; find it; read its metadata back correctly | **PASS** — `register_then_get_round_trips_metadata`: id, `display_name`, `kind`, `capabilities.context_tokens`, `quant`, `availability`, `devices` all match. `list_and_list_by_kind`, `update_changes_a_field_and_bumps_updated_at` (id + `created_at` unchanged), `delete_removes_the_row`. |
| 2 | A registered model whose file is removed shows `availability: Missing`, no crash | **PASS** — `a_removed_file_reads_back_as_missing`: register a real file, delete it, `get()` → `RegisteredModel` with `availability == Missing`, row still present, `list().len() == 1`. |
| 3 | Capability query returns correct results | **PASS** — `capability_query_filters_correctly`: 3 seeded models; `{kind: Llm, min_context_tokens: 8192}` → 1; `{streaming: true}` → 2. |
| 4 | Invalid metadata (each failure mode) is rejected with the field named | **PASS** — `rejects_invalid_metadata`: empty `display_name`, `context_tokens == 0`, `estimated_vram_mb == 0`, empty `devices`, non-object `config` — each `Err(Validation(msg))` with the field in `msg`. `a_row_with_an_unknown_kind_is_an_error_not_a_panic` → `AppError::Internal`, no panic. |
| 5 | A path outside the model dir (or with `..`) is refused | **PASS** — `path_validation_confines_to_the_model_dir` (outside → `Validation`, `..` → `Validation`, missing → `NotFound`, inside → `Ok`); `register_refuses_a_path_outside_the_model_dir`. |
| 6 | `ModelId` is stable across a restart | **PASS** — `id_is_stable_across_a_registry_rebuild`: register, drop the `ModelRegistry`, reopen the same `Db` file, build a fresh registry, `get(&id)` returns the same id. `cache_reflects_writes` confirms invalidation. |
| 7 | Full check suite green; bindings regenerated + committed | **PASS** — `node scripts/check.mjs` all green. New bindings: `RegisteredModel.ts`, `RegistryAvailability.ts`, `Device.ts`. |

`cargo test`: **130 Rust tests** (116 prior + 14 registry). `vitest`: 5.

Real-world bonus: the `tauri dev` launch migrated the existing dev DB **v1 → v2**,
producing a second `pre-migrate-*.db` backup — Phase 9's backup-before-migrate
mechanism exercised on a genuine N>1 migration.

## Baseline

| Operation | Time | Notes |
| --------- | ---- | ----- |
| Warm registry `get` (cache hit) | **~18 µs** | `warm_get_baseline`; no SQLite touch. An async fn call + a mutex lock + one `to_registered()` clone. Well under the 1 ms ceiling; model selection is a user action, not a hot loop. |

## Decisions taken at phase entry

- **ADR-0017: UUIDv4** entity ids, minted by the creating module, opaque forever.
- **Availability is computed, never stored** — `path.is_file()` at read time, so a
  file deleted after registration reads back `Missing` with no stale state.
- **`update` is replace-style** (`update(&id, ModelDraft, models_dir)`) rather
  than a `ModelPatch` struct — simpler and sufficient for the few real update
  cases (path fix, vram correction); id and `created_at` are preserved.
- **Capability schema** (the plan's open question): a small typed set on
  `ModelCapabilities` extended additively, `Device` a closed enum, per-backend
  knobs in the opaque `config` blob — *not* free-form flags.
- **LoRAs / presets** stay Phase 22 (`model_*` gets more tables then).

**Phase 11 complete.** Pointer → Phase 12 (Model Acquisition & Picker).
