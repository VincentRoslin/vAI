# ADR-0011 — Character identity + character generation

- **Status:** PROPOSED (Phase 3 draft) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/09_character-identity.md` (D-10, D-15)
- **Note:** contains the product's biggest open technical risk (FR-C90).

## Context
FR-C90..94 want **high visual continuity** for a character across images. Krea 2
Turbo (the image model, ADR-0006) is **text-to-image only** — no img2img /
IP-Adapter / PuLID for Krea 2 today. The owner's current impl has **no identity
mechanism** (seed + prompt only). Per-character FLUX-family LoRA training is
~45–60 min on a 4080/4090-class GPU.

## Options considered
- A: seed lock + structured-appearance mega-prompt + similarity-check-and-regenerate.
- B: per-character LoRA trained from reference images.
- C: IP-Adapter / PuLID for Krea 2 (not available yet).
- D: a separate reference-capable model for character images.

## Decision
**Layered: A now → B in the background → C when available.**
1. **v1 baseline (A)**: fixed per-character seed + an appearance prompt built from
   the structured appearance fields; identity-similarity gate
   (face-embedding cosine vs the reference set; threshold + bounded retries;
   config).
2. **Background upgrade (B)**: after a character is *kept*, queue a per-character
   LoRA training job (scheduler priority 5, idle-only). Once trained, that
   character's generations use the LoRA → identity "high". Character is fully
   usable before the LoRA is ready.
3. **Later (C)**: adopt Krea 2 IP-Adapter/PuLID if it ships.

**Character generation (D-15)**: schema-constrained LLM authors the structured
profile (validated by a completeness/coherence gate — no content filtering);
~2–4 seed-locked reference images become the initial gallery + reference set.
**Discovery feed**: a pre-generated pool (~10–20 ready characters) served to the
swipe UI, topped up by the scheduler during idle windows; on-demand generation as
fallback; a character is shown only when fully formed. Feed state: seen/kept/passed.

## Consequences
- **Owner sign-off needed**: option B costs ~45–60 min background GPU per kept
  character and requires Krea-2 LoRA-training support in a local trainer
  (ai-toolkit-class) — **confirm feasible at Phase 27**. If not feasible, FR-C90
  is scoped to "option A quality" for v1.
- The pool + idle top-up means the user rarely waits for a character.
- Per-character LoRA storage: ~50–200 MB each (blob store).
- Adds a face-embedder worker (~100 MB) and a LoRA-trainer job type.

## Status detail
`UNDECIDED` sub-point: whether B is in v1 scope (pending owner + Phase 27
feasibility). A + generation + pool are decided.
