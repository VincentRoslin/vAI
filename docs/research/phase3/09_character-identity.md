# Phase 3 · Character visual identity + character generation

Covers steps 3.11, 3.10 (character generation) and D-10, D-15. Requirements:
FR-C1..4, FR-C10..11, FR-C60..65, FR-C90..94; ARQ-11, ARQ-13, ARQ-14.

**This area contains the product's biggest open technical risk** (FR-C90 "high
visual continuity").

---

## The constraint

- Image model = Krea 2 Turbo, **text-to-image only** in diffusers (no img2img /
  edit / IP-Adapter / PuLID for Krea 2 today).
- Owner's current impl: identity = **seed lock + prompt, nothing else**.
- Product wants a character recognisably the *same person* across poses, clothing,
  scenes, lighting.

Seed-lock + prompt alone **will not** meet FR-C90..91. Something more is required.

---

## D-10 — Identity approach

| Option | Identity strength | Cost | Local/offline |
| ------ | ----------------- | ---- | ------------- |
| **A. Seed lock + structured-appearance mega-prompt + similarity-check-and-regenerate** | Low–medium | Free, immediate | ✅ |
| **B. Per-character LoRA** trained from the character's reference images | **High** | ~45–60 min training on the 5080 (background); ~50–200 MB storage/character; needs a trainer (ai-toolkit) + Krea-2 LoRA-training support | ✅ |
| **C. IP-Adapter / PuLID / InstantID for Krea 2** | Medium–high, instant | Not available for Krea 2 yet; may land | ✅ if it ships |
| **D. Separate reference-capable model** (a FLUX/SDXL with PuLID/InstantID) for character images | Medium–high, instant | Two image stacks; VRAM juggling; the Krea-2 LoRA ecosystem doesn't apply | ✅ |

### Recommended: **A now, B in the background, C when available**
1. **v1 baseline (A):** when a character is created, generate its reference images
   with a **fixed seed** + a detailed prompt built from the **structured
   appearance** fields (FR-C10). Every later generation reuses the seed +
   appearance prompt + the requested scene/pose. Run the **identity-similarity
   gate** (below) and regenerate up to N.
2. **Background upgrade (B):** after a character is *kept* (FR-C62), queue a
   **per-character LoRA training** job (scheduler priority: lowest, idle-only —
   see `08`). Once trained, that character's generations use the LoRA →
   identity jumps to "high". The character is fully usable before the LoRA is
   ready; it just gets better.
3. **Later (C):** if Krea 2 gains IP-Adapter/PuLID, adopt it — instant identity
   without per-character training.

**Flag to owner:** option B is the realistic path to FR-C90's bar, and it costs
~45–60 min of background GPU per character and needs Krea-2 LoRA-training support
in a local trainer. Confirm this is acceptable, or we scope FR-C90 down to
"option A quality" for v1.

### Identity-similarity gate (FR-C93, accept vs regenerate)
- A **face-embedding model** (ArcFace / InsightFace-style, ~100 MB, fast) →
  cosine similarity between the generated image's face and the character's
  reference set. Threshold + max-retries in config.
- Fallback where no clear face (full-body/distant): **CLIP image similarity** vs
  references (weaker).
- Runs in a tiny embedder worker or folded into the image sidecar. ~tens of ms.

→ **ADR-0011** (identity): A + background-B + gate. Trainer = ai-toolkit-style,
Krea-2 support to confirm at Phase 27.

---

## D-15 — Character generation subsystem (ARQ-13/14)

### Profile generation
- The **LLM** authors a character: given a light seed (or nothing), produce
  structured JSON — identity, name, structured appearance (FR-C10 schema),
  personality, interests, backstory, a starting relationship stage. A
  **schema-constrained generation** (grammar / JSON mode) so the output is valid
  by construction. Validate + store (Phase 25).
- A **quality gate**: reject profiles with empty required fields, contradictory
  appearance, or safety-irrelevant garbage (no content filtering — just
  completeness/coherence, FR-C64).

### Reference images
- From the structured appearance → build the appearance prompt → generate ~2–4
  reference images (portrait + a couple of variations) via the shared image
  subsystem with a fixed per-character seed.
- These become the character's initial gallery **and** the reference set for the
  identity gate / LoRA training.

### Discovery feed (ARQ-14)
- **Pre-generated pool**: keep ~10–20 ready characters in a `char_pool` table
  (profile + reference images done). The swipe feed serves from the pool.
- **Background top-up**: when the pool drops below a threshold (user swiped
  through several), the scheduler queues profile+image generation for new ones
  during idle windows (`08` priority 4).
- **On-demand fallback**: if the pool is empty (heavy swiping), generate the next
  card on demand with a visible "finding someone…" state.
- A character is only shown when **fully formed** (profile complete + reference
  images present) — FR-C64.
- Feed state: `seen` / `kept` / `passed` per character id; passed characters are
  not re-shown (or re-shown rarely).

### Cost
Per pool character: ~1 LLM generation (profile, seconds) + ~2–4 image generations
(~17 s each, or less at lower res for cards) + optional LoRA later. Pool top-up is
idle-time work; the user rarely waits.

→ **ADR-0011** (also covers generation): schema-constrained LLM profile +
seed-locked reference images + pre-generated pool with idle top-up.

---

## Optimizations
1. **Pre-generated pool + idle top-up** — the user almost never waits for a
   character to generate.
2. **Lower-resolution discovery cards** (e.g. 768² or 896×1152) — ~2x faster than
   1024²; full-res only on "view profile" / after keeping.
3. **Batch pool generation** behind one LLM→image eviction cycle (`08` opt 1) —
   generate N characters' profiles (LLM), then evict once and generate all their
   reference images.
4. **Reuse the character's fixed seed** across its lifetime so even the
   option-A baseline has maximal consistency for free.
5. **Train the per-character LoRA from the pool reference images** already on
   disk — no extra generation to build a training set.
6. **Shared base-model + hot-swappable per-character LoRA** (diffusers LoRA
   hotswap) — switching which character you're generating for is a LoRA swap, not
   a model reload.
7. **Face-embedding cache** — embed each reference image once, store the vector;
   the identity gate compares against cached vectors, not re-embedding references
   every time.
8. **Skip the identity gate for the baseline pool images** (they *define* the
   identity) — only gate *subsequent* generations.

## Failure modes
- LLM produces an invalid/contradictory profile → schema constraint + quality
  gate reject; retry with a different seed.
- Reference-image generation fails → character not added to the pool; retry later.
- Per-character LoRA training OOM / fails → character stays on option-A quality;
  log; optionally retry once.
- Identity gate rejects everything for a hard character → deliver best candidate +
  "couldn't closely match" (FR-C93); the LoRA (once trained) usually fixes this.
- Krea 2 LoRA-training unsupported by available trainers → fall back to option A
  quality for v1 + revisit (C) — **confirm feasibility at Phase 27, not now**.

## Sources
- [FLUX character LoRA training ~45–60 min on RTX 4080/4090 (2026)](https://apatero.com/blog/flux-2-pro-lora-training-character-consistency-2026) · [ai-toolkit (ostris)](https://diffusiondoodles.substack.com/p/how-to-train-a-lora-ostris-ai-toolkit) · [FLUX.2 klein LoRA under 60 min](https://huggingface.co/blog/black-forest-labs/flux-2-klein-lora)
