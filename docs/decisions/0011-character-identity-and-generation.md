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
- A: prompt-based — canonical appearance block + fixed seed + batch-and-pick + gate.
- B: per-character LoRA trained from reference images. **Rejected by owner
  2026-09-05** (no LoRA trainer).
- C: IP-Adapter / PuLID / reference conditioning for Krea 2 — not available yet.
- Existing LoRAs: realism/skin LoRAs (aesthetic, not identity); `QuadView_krea2_v1`
  CharacterSheet LoRA (multi-view sheet in one generation — useful for the
  reference set).

## Decision
**Prompt-based, no training.** FR-C90 ("high visual continuity across
poses/clothing/environments/lighting/scenes") is a **best-effort v1 target** —
current research is clear that prompt + seed cannot fully hold a face across
dramatic scene changes without a reference or trained anchor.
1. **Reference set** generated with `QuadView_krea2_v1` (one consistent multi-view
   sheet), cropped into the character's gallery.
2. **Canonical appearance block**: an LLM captions the primary reference view in
   extreme detail → merged with structured appearance → the character's immutable
   identity prompt prefix, byte-identical every generation.
3. **Fixed per-character seed** for the character's whole lifetime.
4. **Realism LoRA always on** (`gokaygokay/Krea-2-Realism`/V2 + Skin).
5. **Batch-and-pick**: generate N, run the identity-similarity gate
   (face-embedding cosine vs the reference set; threshold + bounded retries;
   config), deliver the best; on repeated failure deliver the best candidate +
   "couldn't closely match" (FR-C93).
6. **Adopt IP-Adapter / reference conditioning for Krea 2 immediately if it ships**
   — the biggest possible upgrade, no training.
7. **Phase 27 prompt-engineering research**: measure which prompt techniques
   actually move the similarity score on Krea 2.

**Character generation (D-15)**: schema-constrained LLM authors the structured
profile (completeness/coherence gate — no content filtering); reference sheet via
QuadView; LLM caption → canonical block. **Discovery feed**: pre-generated pool
(~10–20), scheduler idle top-up, on-demand fallback, only fully-formed characters
shown; feed state seen/kept/passed.

## Consequences
- **FR-C90/C91 scoped to prompt-based quality for v1.** Owner to confirm the
  expected *range* of character images (mostly portraits/selfies → adequate;
  full-body across varied scenes → visible drift, no in-scope fix).
- No LoRA trainer, no `lora_train` job kind (ADR-0010), no trainer in the venv
  (ADR-0014).
- Adds a face-embedder worker (~100 MB) and the QuadView + realism LoRAs to the
  pinned LoRA set.
- Pool + idle top-up means the user rarely waits for a character.
