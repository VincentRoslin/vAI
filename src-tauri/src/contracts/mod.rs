//! Application contracts — the strongly-typed, serializable vocabulary shared
//! across **both** boundaries the Rust core talks over:
//!
//! - the typed Tauri IPC boundary to the React frontend (ADR-0002), and
//! - the JSON-lines stdio boundary to stateless Python workers (ADR-0013).
//!
//! These are **contract types**, deliberately separate from domain types. The
//! phase that introduces a domain type also adds the explicit `From` / `TryFrom`
//! conversion to its contract form — contracts never depend on domain modules.
//!
//! ## Rules
//! - IDs are opaque newtypes ([`ids`]), never bare `String`.
//! - Enums that cross the wire as data are **adjacently tagged**
//!   (`#[serde(tag = "type", content = "data")]`) so the generated TypeScript is
//!   a clean discriminated union.
//! - Every contract type derives `Serialize + Deserialize + TS` and has a
//!   round-trip test; every `validate()` has a rejection test
//!   (`contracts::tests`).
//! - Evolution is **additive-only**. See `docs/contracts.md`.
//!
//! No behaviour lives here: no `invoke` handlers, no persistence, no I/O.

pub mod acquisition;
pub mod conversation;
pub mod generation;
pub mod ids;
pub mod memory;
pub mod model;
pub mod resource;
pub mod task;
pub mod worker;

#[cfg(test)]
mod tests;

/// Current version of the worker JSON-lines protocol ([`worker::WorkerHello`]).
/// Bump only on a breaking envelope change; policy in `docs/contracts.md`.
pub const WORKER_PROTOCOL_VERSION: u32 = 1;
