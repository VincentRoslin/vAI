# Phase 3 · Image-generation subsystem

Covers step 3.10 and D-5. Requirements: FR-90..97, FR-C80..82, FR-C90..94;
ARQ-9, ARQ-11. **One subsystem serves Tab 2 (standalone) and Tab 3 (character
images).**

Owner-provided stack (2026-09-05):
- **Base model:** Krea 2 Turbo (`unsloth/Krea-2-Turbo` — ungated mirror).
- **LoRAs:**
  - Krea 2 Realism (`gokaygokay`) — candid photography
  - Krea 2 Skin (`inlineresearch`) — skin texture
  - Lustify · Krea 2 (18+) — from `Omnico/Krea2_turbo_diff_loras` (200+ LoRA
    archive incl. NSFW; "more LoRAs" come from here)

---

## Model facts (researched)

- Krea 2 = 12B Diffusion Transformer (Krea.ai), released 2026-06-22. **Turbo =
  8-step distilled** (schnell-tier speed, more aesthetic range).
- **FP16 ≈ 36.6 GB** for 1024×1024 — **does not fit 16 GB**.
- **fp8 ≈ ~18 GB** — still does not fit with headroom.
- **NVFP4** (Blackwell-native 4-bit, 5th-gen tensor cores): ~3.5x smaller than
  FP16 → **~10–11 GB weights**, near-fp8 accuracy, ~1.7x faster than BF16.
  diffusers + TorchAO recipes exist (`sayakpaul/diffusers-blackwell-quants`).
- Diffusers path: `Krea2Pipeline` (diffusers from source as of mid-2026).

**Conclusion: Krea 2 Turbo must run in NVFP4 on this card, and even then it needs
most of the 16 GB (weights + activations + LoRAs) → the LLM must be evicted during
image generation** (see `05_resource-vram.md`, `08_scheduler.md`).

---

## D-5 — Subsystem design

### Runtime
- A **Python worker** (diffusers + torch + torchao, CUDA 12/13 build for sm_120)
  running Krea 2 Turbo. Rust-supervised, isolated, non-authoritative (Article I).
- **Not** llama.cpp-style loopback HTTP — this is our own script, so **stdio
  JSON-lines** control + **image bytes written to a controlled path** the Rust
  core dictates (blob store), path returned in the result. (Worker-protocol ADR
  D-12.)
- Model + LoRA files acquired via the shared download/verify path (Phase 12
  mechanism) into the model dir; the worker loads from local paths only.

### LoRA handling (FR-96, FR-97)
- The worker accepts a typed request: `{ prompt, negative?, width, height, steps,
  seed, guidance, loras: [{id, weight}], reference?: {image_ids, mode, strength} }`.
- LoRAs applied by id + weight; **LoRA hotswapping** (diffusers supports swapping
  LoRAs without recompiling the pipeline) so switching preset/LoRA sets between
  jobs is cheap.
- A **LoRA registry** (like the model registry): id → local file, base-model
  compatibility (`krea-2-turbo`), default weight, tags (e.g. `nsfw`), source.
  The 200+ archive is *not* bulk-imported — the user adds the ones they want
  (Tab 2 "add LoRA"), plus the three named above pinned as defaults.

### Presets (FR-95)
- A **preset** = a saved, named parameter set: model + steps + guidance + size +
  a LoRA list with weights + optional prompt scaffolding. Stored in SQLite
  (personas/characters-style CRUD). Tab 2 ships a few built-in presets; the user
  saves their own. Character-image generation (Tab 3) selects a preset internally.

### Identity conditioning (FR-C90..94, ARQ-11)
- Krea 2 / FLUX-family reference-image conditioning options to evaluate at Phase
  27: IP-Adapter-style reference, or a per-character LoRA trained from the
  character's reference images, or diffusers "reference" mode. Per-character LoRA
  gives the strongest identity but costs training time + storage per character;
  reference/IP-Adapter is instant but weaker. **Lean:** reference/IP-Adapter for
  v1 (instant, no per-character training), revisit per-character LoRA if identity
  quality is inadequate.
- **Identity-similarity check** (accept vs regenerate): a face/image embedding
  (e.g. an ArcFace-style face embedder, or CLIP image similarity as a weaker
  fallback) comparing the generated image to the character's reference set;
  threshold + max-retries in config. Runs in the worker or a tiny separate
  embedder. Cost ~tens of ms per candidate.

### Generation flow (Tab 2 and Tab 3 identical core)
```
request → resource manager: reserve VRAM (evict LLM if needed via scheduler)
        → worker: load Krea 2 Turbo (NVFP4) + requested LoRAs (if not resident)
        → generate (8 steps)
        → [Tab 3 only] identity-similarity check → accept | regenerate (bounded)
        → write image to blob store; metadata + params + hash + (score) to SQLite
        → release VRAM → scheduler restores LLM
        → deliver (Tab 2 gallery | character gallery + conversation)
```

→ **ADR-0006**: Python diffusers worker, Krea 2 Turbo NVFP4, stdio control + blob
output, LoRA + preset registries, reference-based identity for v1, embedding
similarity gate.

---

## ⚠ Need from the owner before Phase 22 (and to finalize this ADR)

Your existing implementation's specifics — please confirm:
1. **diffusers or ComfyUI?** (affects whether the worker embeds a pipeline or
   drives ComfyUI headless)
2. **Quantization you actually run** — NVFP4 via torchao? A prebuilt fp8/nvfp4
   checkpoint? GGUF via a different runtime?
3. **Typical params**: steps (8?), resolution, guidance/CFG, scheduler/sampler.
4. **How LoRAs are stacked** — weights, order, any base-prompt injection per LoRA.
5. **Reference/identity mechanism** you use now (if any) — IP-Adapter? PuLID?
   per-character LoRA? none yet?
6. Roughly **how long a generation takes** and **peak VRAM** on your hardware.
7. Is the implementation a **script**, a **server**, or a ComfyUI **workflow
   JSON**?

I'll fold your answers in and adjust the ADR; the plan doesn't need them to start
3.5–3.9, but 3.10/3.11 and Phase 22 do.

---

## Failure modes
- NVFP4 checkpoint/torchao path fails on sm_120 → fall back to fp8 with
  aggressive CPU offload (slow) or a smaller/quantized Krea variant; worst case a
  different image model. Record as a Phase 4 risk.
- OOM even after LLM eviction → reduce resolution / disable a LoRA / fewer
  concurrent activations; typed error with guidance.
- Worker crash mid-generation → VRAM released, no partial file in the blob store,
  typed error, scheduler restores the LLM.
- Identity check always fails for a hard character → after N retries, deliver the
  best-scoring candidate with a "couldn't closely match" note (FR-C93).

## Sources
- [unsloth/Krea-2-Turbo](https://huggingface.co/unsloth/Krea-2-Turbo) · [Krea 2 VRAM](https://willitrunai.com/image-models/krea-2) · [Krea 2 review (Turbo/LoRA)](https://www.buildfastwithai.com/blogs/krea-2-open-source-review-raw-turbo)
- [PyTorch: NVFP4/MXFP8 diffusion on Blackwell](https://pytorch.org/blog/faster-diffusion-on-blackwell-mxfp8-and-nvfp4-with-diffusers-and-torchao/) · [diffusers-blackwell-quants](https://github.com/sayakpaul/diffusers-blackwell-quants)
