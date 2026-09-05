# Phase 28 — Typed Character Image Actions

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
The LLM requests images only through a **typed, validated action** (e.g.
`send_image { character_id, image_type, scene, mood, clothing, context }`). Rust
validates the action against a schema and allow-list, decides whether to run it,
and executes via the image subsystem. No raw generation command ever comes from
model output. (`CLAUDE.md` Article III.)

## Depends on
Phase 27 (identity-consistent generation), Phase 7 (contracts), Phase 26
(characters converse and can emit actions), Phase 24 (scheduler runs the job).

## Not in this phase
- New image capabilities beyond wiring the action → the Phase 27 pipeline.
- Non-image actions (this establishes the pattern; other typed actions reuse it).

## Architecture notes
- The model's output is parsed for a structured action; anything that isn't a
  well-formed, allow-listed action is ignored (and logged), never executed.
- The action's `character_id` must match the conversation's character — a
  cross-character request is refused.
- The action is translated into a Phase 27 generation request by Rust; the model
  never touches the generation parameters directly beyond the typed fields.
- Every action attempt (accepted or rejected) is audit-logged.

## Performance notes
- Action parsing/validation is negligible; the cost is the generation (scheduled,
  Phase 24).

## Step outline
1. Define the `send_image` action contract + the allow-list of image types.
2. Action extraction from model output (structured output / tool-call per the
   Phase 3 IPC+LLM ADRs).
3. Validation: schema, allow-list, `character_id` matches the conversation,
   field ranges.
4. Rejection path: malformed / unknown / cross-character → logged audit entry,
   nothing executed, the conversation continues gracefully.
5. Execution path: valid action → Phase 27 generation request → scheduled job →
   result linked to the character and surfaced in the conversation.
6. Audit log entries for every attempt.
7. Security tests (feed adversarial model output).

## Verification gate
1. A well-formed `send_image` action → validated → executed → an
   identity-consistent image appears in the conversation, linked to the character.
2. A malformed or unknown action is rejected + audit-logged; nothing runs.
3. An action with a `character_id` for a different character is refused.
4. No code path converts model text into a shell command, filesystem mutation,
   process launch, or DB command (reviewed + tested with adversarial output).
5. Every action attempt has an audit-log entry.

## ADRs / open questions
- Action-extraction mechanism (native tool-calling vs structured-output parsing)
  — from the Phase 3 ADRs.
