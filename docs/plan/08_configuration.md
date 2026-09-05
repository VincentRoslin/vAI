# Phase 8 — Configuration

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0015). The design below is settled. Concrete implementation specifics (exact modules, crate APIs, filenames) are filled in at phase entry against the frozen ADRs — they do not change the design.

## Objective
One typed, validated, versioned configuration system: defaults → user config →
session overrides → effective config. Every subsystem reads config through it. No
subsystem invents its own settings storage. No secrets in source.

## Depends on
Phase 6 (Phase 7 for shared types where useful).

## Not in this phase
- Feature-specific settings UI (added by each feature phase).
- Secret storage mechanism beyond "not in source / not in logs".

## Architecture notes
- Rust owns config. The frontend reads/writes config only via typed IPC.
- Config has a schema version and forward migrations.
- Invalid config fails fast at boot with a named error; corrupted config falls
  back to defaults with a surfaced warning, never a silent wipe.

## Performance notes
- Config load is on the startup path — parse + validate should be sub-millisecond
  for a normal file; record it in the startup breakdown.

## Step outline
1. Define the typed config schema + defaults.
2. Loader: read file → parse → validate → produce effective config; fail fast on
   invalid, named error.
3. Persistence: atomic write (temp + rename); file location per OS conventions.
4. Schema version + migration runner (run pending migrations on load).
5. Session overrides layer (in-memory, not persisted).
6. Corrupted-file handling: back up the bad file, start from defaults, surface a
   warning.
7. Typed IPC: get effective config, set a user value, list overridable keys.
8. Wire one real consumer (e.g. the model dir + disk budget from Phase 12).
9. Tests: defaults, valid, invalid (each failure mode), persistence across
   restart, migration, session override, corrupted, missing.

## Verification gate
1. Defaults load with no file present.
2. Invalid value → boot fails with a named error naming the key.
3. Out-of-range value rejected by schema validation.
4. A setting change persists across an app restart.
5. Schema migration upgrades an old config file.
6. Session override takes effect and does not persist.
7. Corrupted file → app starts on defaults, backs up the bad file, warns.
8. `git grep` for direct env / ad-hoc settings access outside the config module
   returns nothing.

## ADRs / open questions
- Config file format (per Phase 3).
