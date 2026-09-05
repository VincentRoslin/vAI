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

## D-10 — Identity approach (owner: **no LoRA training** — prompt-based only)

Owner decision 2026-09-05: **no per-character LoRA trainer.** Explore prompt-based
consistency. This is a real constraint — current research is unanimous that
**prompt + seed alone cannot hold a face across dramatic pose / lighting / scene
changes**; without a reference or a trained anchor, diffusion models drift. So
FR-C90's "recognisably the same across poses/clothing/environments/lighting/
scenes" is a **best-effort v1 target, not a guarantee**.

### v1 approach: reference sheet + canonical appearance block + fixed seed + batch-and-pick + gate
1. **Reference set via `QuadView_krea2_v1`.** Generate the character's reference
   images with the CharacterSheet multi-view LoRA — one generation yields a face
   close-up + 3 body views that are mutually consistent. Crop → the character's
   reference gallery.
2. **Canonical appearance block.** An **LLM captions the primary reference view in
   extreme detail** (face shape, features, hair, colouring, build, distinctive
   marks). That caption — merged with the structured appearance fields (FR-C10) —
   becomes the character's **immutable identity prompt prefix**, byte-identical on
   every future generation. Only the scene/pose/clothing part varies.
3. **Fixed per-character seed** (the owner's impl already does this) — reused for
   the character's whole lifetime.
4. **Realism LoRA always on** (`gokaygokay/Krea-2-Realism` or V2 + Skin) so every
   image is photographically believable.
5. **Batch-and-pick.** Generate N candidates (e.g. 3–4), run the identity-
   similarity gate, deliver the closest to the reference set. Cheap at 8 steps.
6. **Identity-similarity gate** (below) with bounded regeneration; on repeated
   failure, deliver the best candidate + a "couldn't closely match" note (FR-C93).
7. **Constrain the variation.** Character-sent images are mostly
   portraits/selfies/"here's me doing X" in similar framing — prompt+seed holds up
   far better there than for arbitrary full-body scene shots. See the open
   question below.

### Existing LoRAs — what they do and don't do (owner question 2026-09-05)

| LoRA | Type | Helps identity? |
| ---- | ---- | --------------- |
| `gokaygokay/Krea-2-Realism-LoRA`, Krea2-realism-V2, the Skin LoRA (already on the owner's list) | **General photo-realism / candid / skin quality** — applied to *every* generation | No — makes any character look like a believable real photo; does not lock a specific face |
| `Omnico/Krea2_turbo_diff_loras` (the 200+ archive) | **Style / aesthetic** (Artaix, Dasiwa, …), ranks r8–r128 | No — style, not identity |
| **`QuadView_krea2_v1`** (CharacterSheet collection, trained on ~300 sheets, **Krea-2 native**) | **Multi-view character sheet** — one generation produces a face close-up + 3 body views of the *same* character | **Partially — very useful for building the reference set.** All 4 views come from one generation so they're mutually consistent. Crop them → the character's reference gallery + canonical-caption source. Does **not** give consistency across *later independent* generations. |

**Bottom line:** no off-the-shelf LoRA gives cross-generation *identity* lock
without being trained on that specific person. The realism LoRAs make characters
look photographically real (keep them, always-on). **`QuadView_krea2_v1` is worth
adopting** to generate each character's reference set in one consistent shot.

### Later (no cost now)
- **IP-Adapter / PuLID / reference-image conditioning for Krea 2** — not in
  diffusers today; adopt immediately if it ships (biggest single upgrade,
  no training).
- **Flux Kontext turnaround LoRA** (5-view, "3D-rotation"-style) — Kontext is an
  *edit* model, not Krea 2; only relevant if a second image model is ever added.

### ⚠ Open question for the owner
**What range of images do characters actually send?** If it's mostly
selfies/portraits/upper-body in casual settings, the v1 prompt-based approach is
adequate. If characters need to appear consistent in **full-body shots across very
different scenes/outfits**, prompt+seed will visibly drift and there is no
in-scope fix until Krea 2 gets reference conditioning. Please confirm the
expected image range so FR-C90/C91 can be scoped honestly.

### Identity-similarity gate (FR-C93, accept vs regenerate)
- A **face-embedding model** (ArcFace / InsightFace-style, ~100 MB, fast) →
  cosine similarity between the generated image's face and the character's
  reference set. Threshold + max-retries in config.
- Fallback where no clear face (full-body/distant): **CLIP image similarity** vs
  references (weaker).
- Runs in a tiny embedder worker or folded into the image sidecar. ~tens of ms.

→ **ADR-0011** (identity): canonical appearance block (LLM-captioned reference) +
fixed seed + batch-and-pick + similarity gate. **No LoRA trainer.** IP-Adapter /
reference conditioning adopted if Krea 2 gains it. FR-C90 is a best-effort target.

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
- One is chosen as the primary; an **LLM captions it in detail** → the character's
  **canonical appearance block** (the immutable identity prompt prefix).
- These images are the character's initial gallery **and** the reference set for
  the identity-similarity gate.

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
Per pool character: ~1 LLM generation (profile) + ~1 LLM caption + ~2–4 image
generations (~17 s each, less at card resolution). Pool top-up is idle-time work;
the user rarely waits.

→ **ADR-0011** (also covers generation): schema-constrained LLM profile +
seed-locked reference images + LLM-captioned canonical appearance block +
pre-generated pool with idle top-up.

---

## Optimizations
1. **Pre-generated pool + idle top-up** — the user almost never waits for a
   character to generate.
2. **Lower-resolution discovery cards** (e.g. 768² or 896×1152) — ~2x faster than
   1024²; full-res only on "view profile" / after keeping.
3. **Batch pool generation** behind one LLM→image eviction cycle (`08` opt 1) —
   generate N characters' profiles (LLM), then evict once and generate all their
   reference images.
4. **Reuse the character's fixed seed** across its lifetime — the cheapest
   consistency lever, free.
5. **Canonical appearance block is generated once** (LLM caption of the primary
   reference) and cached — every later generation just concatenates it with the
   scene text.
6. **Batch-and-pick** shares one model load — generate N candidates in one worker
   call, gate them, keep the best.
7. **Face-embedding cache** — embed each reference image once, store the vector;
   the identity gate compares against cached vectors, not re-embedding references
   every time.
8. **Skip the identity gate for the baseline pool images** (they *define* the
   identity) — only gate *subsequent* generations.
9. **Prompt-engineering research (Phase 27)**: test which of {ultra-detailed
   caption, distinctive rare-token name, appearance-token weighting, restricting
   framing/shot-type, low step-count determinism} actually move the
   similarity-gate score on Krea 2 — measure, don't guess.

## Failure modes
- LLM produces an invalid/contradictory profile → schema constraint + quality
  gate reject; retry with a different seed.
- Reference-image generation fails → character not added to the pool; retry later.
- Identity gate rejects everything for a hard character (likely under big
  pose/scene changes) → deliver the best candidate + "couldn't closely match"
  (FR-C93). This is expected behaviour, not a bug, given the no-LoRA constraint.
- Character looks inconsistent across very different scenes → **known v1
  limitation** (see the open question); revisit if Krea 2 gains reference
  conditioning.

## Sources
- [getimg.ai — consistent characters 2026 (prompt-only limits)](https://getimg.ai/blog/how-to-create-consistent-characters-with-ai) · [stacksheriff — Flux consistent character (seed-lock limits)](https://stacksheriff.com/ai-tools/comfyui-flux-character/)
- [gokaygokay/Krea-2-Realism-LoRA](https://huggingface.co/gokaygokay/Krea-2-Realism-LoRA) · [Omnico/Krea2_turbo_diff_loras](https://huggingface.co/Omnico/Krea2_turbo_diff_loras)
- [CharacterSheet multi-view LoRAs (incl. `QuadView_krea2_v1`)](https://comfyui-wiki.com/en/news/2026-07-31-charactersheet-multiview-loras) · [HackerNoon — CharacterSheet LoRA collection](https://hackernoon.com/charactersheet-what-you-have-to-know-about-this-collection-of-lora-adapters)
