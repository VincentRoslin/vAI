//! The context module (`docs/spec/AI_PIPELINES.md` §6, `docs/plan/20_personas.md`).
//!
//! Owns three things, all Rust-side (`CLAUDE.md` Article I):
//! - **`persona`** — the [`Persona`] domain type + [`PersonaRepo`] (all `persona`
//!   SQL). Personas are structured behaviour data for Tab 1 conversations
//!   (FR-30..35).
//! - **`sanitize`** — [`sanitize::strip_control`], applied to every untrusted
//!   string before it enters a prompt (SECURITY C2).
//! - **`builder`** — [`ContextBuilder`], **the one** prompt assembler. Every
//!   generation path calls it; it is deterministic, injection-safe, and
//!   token-budgeted with recorded [`Provenance`].

pub mod builder;
pub mod persona;
pub mod sanitize;
pub mod tokens;

pub use builder::{
    BuildInput, BuiltPrompt, CharacterContext, ContextBuilder, MemoryItem, PersonaInclusion,
    Provenance, RuntimeContext, TokenBudget, DEFAULT_SYSTEM,
};
pub use persona::{Persona, PersonaDraft, PersonaRepo};
