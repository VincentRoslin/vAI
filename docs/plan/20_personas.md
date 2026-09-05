# Phase 20 — Personas & Context Builder

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Structured persona data plus one predictable context builder that assembles the
model prompt from: system instructions + persona + character info + conversation +
relevant memory + runtime context. It must be **verifiable that the persona
content actually reaches the model** — the UI showing it is not proof.

## Depends on
Phase 17 (engine). Used later by memory (21) and characters (26).

## Not in this phase
- Character-specific fields (Phase 25).
- Memory retrieval (Phase 21 — the builder just has a slot for it).

## Architecture notes
- One context builder. Every generation path (chat, voice, character) goes
  through it.
- Persona is structured data (fields), not a hidden text blob.
- The builder is deterministic and testable: given the same inputs it produces the
  same prompt.
- Token budgeting lives here (truncate oldest conversation turns, cap memory
  section) with provenance for what was included.

## Performance notes
- Context assembly is on every generation — keep it O(context size), no repeated
  DB round-trips (batch the reads).
- Record assembled-prompt token count vs the model's context window.

## Step outline
1. Persona schema (name, instructions, style, constraints) + storage + CRUD.
2. Context builder API: `build(conversation, persona, character?, memory?,
   runtime) → Prompt` (typed).
3. Section ordering + token budget + truncation policy with provenance.
4. Injection-safety: persona/character/memory text cannot break prompt structure
   (delimiters escaped/normalized).
5. A test harness that captures the exact string sent to the LLM adapter.
6. Wire the builder into the Phase 17 engine (replace the minimal prompt from
   Phase 16).
7. Tests: determinism; persona present in the assembled prompt; budget respected;
   injection attempt neutralized; switching persona changes the prompt.

## Verification gate
1. Persona is stored and edited as structured data.
2. Context-builder unit tests pass (determinism, ordering, budget, provenance).
3. A test asserts the exact prompt sent to the LLM contains the active persona's
   instructions.
4. A persona/memory string containing prompt delimiters cannot alter prompt
   structure.
5. Switching persona changes model behaviour in a scripted before/after check.
6. Assembled-prompt token count is logged and never exceeds the model context.

## ADRs / open questions
- Token-budget split between conversation history and memory.
