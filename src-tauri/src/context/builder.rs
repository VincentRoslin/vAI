//! The one context builder (`docs/spec/AI_PIPELINES.md` §6). Every generation
//! path assembles its prompt here.
//!
//! Deterministic (same inputs → byte-identical output), injection-safe (every
//! untrusted section is [`sanitize::strip_control`]led), token-budgeted with
//! [`Provenance`] recorded + logged.

use serde::Serialize;
use ts_rs::TS;

use crate::context::persona::Persona;
use crate::context::sanitize::strip_control;
use crate::context::tokens::estimate_tokens;
use crate::contracts::conversation::{Message, MessageContent, Role};

/// The base system instruction for a bare Persona chat (no persona set).
pub const DEFAULT_SYSTEM: &str = "You are a helpful assistant.";

/// Tokens held back from the context window for the model's own response.
pub const RESPONSE_RESERVE: u32 = 1024;
/// Extra headroom for estimate error vs the real tokenizer.
pub const BUDGET_MARGIN: u32 = 256;

/// A retrieved memory item (Phase 21 populates these; empty until then).
#[derive(Debug, Clone)]
pub struct MemoryItem {
    /// The memory text.
    pub text: String,
}

/// Character / relationship context (Phase 26 populates this; `None` until
/// then). A plain struct — no trait objects.
#[derive(Debug, Clone, Default)]
pub struct CharacterContext {
    /// The character's identity + appearance + personality block.
    pub identity: String,
    /// Current relationship-stage guidance.
    pub relationship: String,
}

/// Runtime facts appended to the system block.
#[derive(Debug, Clone)]
pub struct RuntimeContext {
    /// RFC-3339 date, rendered as "Current date: YYYY-MM-DD.".
    pub now: String,
}

impl RuntimeContext {
    /// Now, UTC.
    #[must_use]
    pub fn now() -> Self {
        Self {
            now: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        }
    }
}

/// The fraction of the context window the retrieved-memory section may use
/// (ADR-0012 "config token budget" — a module constant for v1, like
/// [`RESPONSE_RESERVE`]).
pub const MEMORY_CONTEXT_FRACTION: f32 = 0.15;

/// The maximum prompt size for this generation.
#[derive(Debug, Clone, Copy)]
pub struct TokenBudget {
    /// `context_tokens - RESPONSE_RESERVE - BUDGET_MARGIN`.
    pub max_prompt_tokens: u32,
    /// The slice of `max_prompt_tokens` the retrieved-memory section may use
    /// (`MEMORY_CONTEXT_FRACTION` of the context window, never more than half
    /// the prompt budget).
    pub max_memory_tokens: u32,
}

impl TokenBudget {
    /// From a model's context window.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn from_context_window(context_tokens: Option<u32>) -> Self {
        let ctx = context_tokens.unwrap_or(4096);
        let max_prompt_tokens = ctx
            .saturating_sub(RESPONSE_RESERVE)
            .saturating_sub(BUDGET_MARGIN);
        let max_memory_tokens = ((f64::from(ctx) * f64::from(MEMORY_CONTEXT_FRACTION)) as u32)
            .min(max_prompt_tokens / 2);
        Self {
            max_prompt_tokens,
            max_memory_tokens,
        }
    }
}

/// Everything the builder needs. Borrowed — the caller batches the reads.
pub struct BuildInput<'a> {
    /// Base instruction (usually [`DEFAULT_SYSTEM`]).
    pub system: &'a str,
    /// Active persona, if any.
    pub persona: Option<&'a Persona>,
    /// Character context (Phase 26).
    pub character: Option<&'a CharacterContext>,
    /// Retrieved memory (Phase 21).
    pub memory: &'a [MemoryItem],
    /// Full conversation, oldest first.
    pub history: &'a [Message],
    /// Runtime facts.
    pub runtime: RuntimeContext,
    /// Size cap.
    pub budget: TokenBudget,
}

/// Whether the persona made it into the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum PersonaInclusion {
    /// No persona was set.
    Absent,
    /// All persona fields included.
    Full,
    /// Persona shortened to fit the budget.
    Truncated,
}

/// What went into the assembled prompt.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Provenance {
    /// Estimated tokens in the whole prompt.
    pub total_tokens: u32,
    /// The budget it was built against.
    pub budget_tokens: u32,
    /// Estimated tokens in the system turn (system + persona + character +
    /// memory + runtime).
    pub system_tokens: u32,
    /// Persona inclusion.
    pub persona: PersonaInclusion,
    /// Memory items included.
    pub memory_items: u32,
    /// Estimated tokens of memory.
    pub memory_tokens: u32,
    /// History turns kept.
    pub history_turns_included: u32,
    /// History turns dropped to fit.
    pub history_turns_dropped: u32,
}

/// The assembled prompt + its provenance.
#[derive(Debug, Clone)]
pub struct BuiltPrompt {
    /// The exact string handed to the LLM adapter.
    pub text: String,
    /// What went in.
    pub provenance: Provenance,
}

/// The context builder. Stateless; cheap to construct.
#[derive(Debug, Default, Clone, Copy)]
pub struct ContextBuilder;

impl ContextBuilder {
    /// Assemble the prompt. Deterministic; logs the provenance at
    /// `target: "context"`.
    #[must_use]
    pub fn build(self, input: &BuildInput<'_>) -> BuiltPrompt {
        let budget = input.budget.max_prompt_tokens;

        // --- system block, trying full persona then a truncated one ---
        let (system_block, persona_incl, mem_items, mem_tokens) =
            Self::system_block(input, budget, input.budget.max_memory_tokens);
        let system_tokens = estimate_tokens(&system_block);

        // --- history, newest-first while it fits (hard ceiling: never exceed
        // `budget`, so `provenance.total_tokens <= budget_tokens` by
        // construction — the margin absorbs estimate error vs the real
        // tokenizer). ---
        let mut rendered: Vec<String> = Vec::new();
        let mut used = system_tokens + PROMPT_SCAFFOLD_TOKENS;
        let mut dropped = 0u32;
        let mut budget_hit = false;
        for msg in input.history.iter().rev() {
            let Some(turn) = render_turn(msg) else {
                continue; // image / untranscribed audio — contributes no text
            };
            let cost = estimate_tokens(&turn);
            if budget_hit || used + cost > budget {
                // Keep the kept turns contiguous: once one turn overflows, every
                // older turn is dropped too.
                budget_hit = true;
                dropped += 1;
                continue;
            }
            used += cost;
            rendered.push(turn);
        }
        rendered.reverse();

        let mut text = String::new();
        text.push_str(&system_block);
        for turn in &rendered {
            text.push_str(turn);
        }
        text.push_str("<|im_start|>assistant\n");

        let provenance = Provenance {
            total_tokens: estimate_tokens(&text),
            budget_tokens: budget,
            system_tokens,
            persona: persona_incl,
            memory_items: mem_items,
            memory_tokens: mem_tokens,
            history_turns_included: u32::try_from(rendered.len()).unwrap_or(u32::MAX),
            history_turns_dropped: dropped,
        };
        tracing::info!(
            target: "context",
            total_tokens = provenance.total_tokens,
            budget_tokens = provenance.budget_tokens,
            persona = ?provenance.persona,
            memory_items = provenance.memory_items,
            history_included = provenance.history_turns_included,
            history_dropped = provenance.history_turns_dropped,
            "context assembled"
        );

        BuiltPrompt { text, provenance }
    }

    /// Build the ChatML `system` turn. Sheds memory, then truncates the persona,
    /// to fit `budget` — never the base `system` line.
    fn system_block(
        input: &BuildInput<'_>,
        budget: u32,
        mem_budget: u32,
    ) -> (String, PersonaInclusion, u32, u32) {
        let base = strip_control(input.system);
        let runtime = format!(
            "Current date: {}.",
            &input.runtime.now[..input.runtime.now.len().min(10)]
        );

        let character = input
            .character
            .map(|c| {
                let mut s = strip_control(&c.identity);
                let rel = strip_control(&c.relationship);
                if !rel.is_empty() {
                    s.push_str("\n\n");
                    s.push_str(&rel);
                }
                s
            })
            .filter(|s| !s.is_empty());

        // Memory arrives ranked best-first. Take from the front while the
        // running token sum stays within the memory sub-budget; the rest are
        // dropped (lowest-ranked first).
        let mut mem_budgeted: Vec<String> = Vec::new();
        let mut mem_used = 0u32;
        for m in input.memory {
            let clean = strip_control(&m.text);
            if clean.is_empty() {
                continue;
            }
            let cost = estimate_tokens(&clean);
            if mem_used + cost > mem_budget {
                break;
            }
            mem_used += cost;
            mem_budgeted.push(clean);
        }

        // Shed order (plan §"Token budget + truncation"): full persona + memory,
        // then drop memory, then truncate the persona, then drop the persona —
        // never the base `system` line. The last attempt is returned regardless
        // so the base system + runtime always survive.
        let last = 3;
        for attempt in 0..=last {
            let (persona_text, incl) = match (input.persona, attempt) {
                (None, _) => (None, PersonaInclusion::Absent),
                (Some(_), a) if a == last => (None, PersonaInclusion::Truncated),
                (Some(p), 0 | 1) => (Some(render_persona(p, false)), PersonaInclusion::Full),
                (Some(p), _) => (Some(render_persona(p, true)), PersonaInclusion::Truncated),
            };
            let mem: &[String] = if attempt == 0 { &mem_budgeted } else { &[] };

            let block = assemble_system(
                &base,
                persona_text.as_deref(),
                character.as_deref(),
                mem,
                &runtime,
            );
            if estimate_tokens(&block) <= budget || attempt == last {
                let mem_tokens = mem.iter().map(|m| estimate_tokens(m)).sum();
                return (
                    block,
                    incl,
                    u32::try_from(mem.len()).unwrap_or(u32::MAX),
                    mem_tokens,
                );
            }
        }
        unreachable!("the loop returns on the last attempt")
    }
}

/// Rough token cost of the ChatML scaffolding (`<|im_start|>system\n…<|im_end|>`
/// wrappers + the assistant tail) that `estimate_tokens` on the section text
/// alone misses.
const PROMPT_SCAFFOLD_TOKENS: u32 = 16;

fn assemble_system(
    base: &str,
    persona: Option<&str>,
    character: Option<&str>,
    memory: &[String],
    runtime: &str,
) -> String {
    let mut inner = String::new();
    inner.push_str(base);
    if let Some(p) = persona {
        if !p.is_empty() {
            inner.push_str("\n\n");
            inner.push_str(p);
        }
    }
    if let Some(c) = character {
        inner.push_str("\n\n");
        inner.push_str(c);
    }
    if !memory.is_empty() {
        inner.push_str("\n\nRelevant memories:\n");
        for m in memory {
            inner.push_str("- ");
            inner.push_str(m);
            inner.push('\n');
        }
        inner.pop();
    }
    inner.push_str("\n\n");
    inner.push_str(runtime);

    format!("<|im_start|>system\n{inner}<|im_end|>\n")
}

/// Render a persona's fields as prose. `truncate` keeps only name + summary + a
/// clamped personality (budget fallback).
fn render_persona(p: &Persona, truncate: bool) -> String {
    let name = strip_control(&p.name);
    let mut s = if name.is_empty() {
        String::new()
    } else {
        format!("You are {name}.")
    };
    let summary = strip_control(&p.summary);
    if !summary.is_empty() {
        push_sentence(&mut s, &summary);
    }
    if truncate {
        let pers = strip_control(&p.personality);
        if !pers.is_empty() {
            push_line(&mut s, "Personality", &clamp(&pers, 240));
        }
        return s;
    }
    for (label, value) in [
        ("Personality", &p.personality),
        ("Tone", &p.tone),
        ("Style", &p.style),
    ] {
        let v = strip_control(value);
        if !v.is_empty() {
            push_line(&mut s, label, &v);
        }
    }
    let guidance: Vec<String> = p
        .guidance
        .iter()
        .map(|g| strip_control(g))
        .filter(|g| !g.is_empty())
        .collect();
    if !guidance.is_empty() {
        push_line(&mut s, "Always", &guidance.join("; "));
    }
    s
}

fn push_sentence(s: &mut String, text: &str) {
    if !s.is_empty() {
        s.push(' ');
    }
    s.push_str(text);
}

fn push_line(s: &mut String, label: &str, value: &str) {
    if !s.is_empty() {
        s.push('\n');
    }
    s.push_str(label);
    s.push_str(": ");
    s.push_str(value);
}

fn clamp(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_owned();
    }
    let cut: String = s.chars().take(max_chars).collect();
    format!("{}…", cut.trim_end())
}

/// One history turn as ChatML, content sanitised. `None` for content that
/// contributes no text (image / untranscribed audio).
fn render_turn(msg: &Message) -> Option<String> {
    let text = match &msg.content {
        MessageContent::Text { text } => text.as_str(),
        MessageContent::Audio {
            transcript: Some(t),
            ..
        } => t.as_str(),
        MessageContent::Audio {
            transcript: None, ..
        }
        | MessageContent::Image { .. } => return None,
    };
    let clean = strip_control(text);
    if clean.is_empty() {
        return None;
    }
    Some(format!(
        "<|im_start|>{}\n{clean}<|im_end|>\n",
        role_tag(msg.role)
    ))
}

fn role_tag(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

#[cfg(test)]
mod tests;
