# ADR-0001 — Single Rust crate with module boundaries

- **Status:** PROPOSED (Phase 3 draft) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/01_desktop-ipc.md` (D-1) · relates to O4

## Context
The Rust core has many subsystems (config, db, models, resources, scheduler, llm,
voice, image, characters, memory, ipc). `CLAUDE.md` Art. IV: smallest correct
structure, no premature boundaries.

## Options considered
- **Single crate**, one module per subsystem.
- **Cargo workspace**, subsystems as separate crates.

## Decision
**Single Tauri crate**, strict module boundaries, `pub(crate)` discipline so
modules interact through defined interfaces. Split a module into its own crate
only when a concrete trigger appears:
1. it needs its own integration-test binary that would otherwise compile the app;
2. the worker protocol gets a standalone conformance harness;
3. compile time exceeds an agreed budget and a leaf crate parallelizes it.

## Consequences
- One `Cargo.toml`, fast navigation, no cross-crate version juggling.
- Boundaries are enforced by convention + review, not the compiler — the Phase 36
  maintainability audit checks them.
- If a split is later needed it is mechanical (module → crate).
