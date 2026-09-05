# Phase 3 · Image-generation subsystem

Covers step 3.10 and D-5. Requirements: FR-90..97, FR-C80..82, FR-C90..94;
ARQ-9, ARQ-11. **One subsystem serves Tab 2 (standalone) and Tab 3 (character
images).**

---

## Owner's existing implementation (from their other project, 2026-09-05)

**This is a working reference to adapt, not a spec — the owner asked for
optimization.** Must stay offline / local / no third-party UI.

| Aspect | Current implementation |
| ------ | ---------------------- |
| Runtime | **diffusers**, no ComfyUI. A **FastAPI sidecar** `image_gen/server.py` (`POST /generate {model, prompt, width, height, seed, lora, lora_weight}`), driven by Rust `image_gen.rs` / `commands/image.rs` + the Image tab. |
| Model | Un-quantized bf16 `unsloth/Krea-2-Turbo` (ungated mirror of `krea/Krea-2-Turbo`). ~34 GB download; ~24 GB transformer. |
| Quantization | **bitsandbytes NF4 at load**: `PipelineQuantizationConfig(quant_backend="bitsandbytes_4bit", bnb_4bit_quant_type="nf4", compute_dtype=bf16)` on transformer + text encoder, then `enable_model_cpu_offload()`. Re-quantized **on every load**. |
| Params | steps 8, `guidance_scale 0.0`, no negative prompt (inert at guidance 0), FlowMatch Euler (pipeline default, not exposed). Resolution: aspect buttons 1024²–1664×928, snapped to /16. |
| LoRA | **Single, not stacked.** `load_lora_weights(adapter_name=…)` → `set_adapters([name], [weight])`, weight 0–1 (default 0.9). A Kohya→diffusers converter in `server.py` loads attn/MLP layers, skips norm layers. |
| Identity | **None.** No IP-Adapter, PuLID, reference image, per-character LoRA. Krea 2 in diffusers is **text-to-image only** (no img2img/edit pipeline yet). Consistency = seed lock + prompt. |
| Timing | ~**17–18 s** per 1024² once loaded (8 steps). Cold first-ever 306 s (incl. download + first quant). Subsequent cold loads ~**90 s** (re-quant). |
| VRAM | Peak ~**11.4 GB** during a generation (LLM + TTS unloaded first via `exclusive_vram`). **~1.6 GB at rest** between generations (CPU offload). |

**LocalAI uses Krea 2 Turbo as the only image model** (the other project's
"Z-Image" is **out of scope** per owner, 2026-09-05). The sidecar keeps a
`model` field for forward-compatibility, but no second image model ships.

---

## D-5 — Subsystem design (adaptation)

### What to reuse
- **The FastAPI sidecar pattern.** Rust supervises a Python image server over a
  **loopback HTTP socket** (`127.0.0.1:<free port>`). This matches the owner's
  working setup *and* the `llama-server` decision (`02_llm-runtime.md`) — same
  supervision + transport pattern for both "big model servers". (Revises the
  earlier lean toward stdio for the image worker — see `11_worker-protocol.md`.)
- The `POST /generate {model, …}` shape — `model` kept for forward-compat, but
  Krea 2 Turbo is the only image model LocalAI ships.
- `enable_model_cpu_offload()` — the reason idle VRAM is only ~1.6 GB. Keep it.
- The Kohya→diffusers LoRA converter.
- `exclusive_vram` (unload LLM + TTS before an image job) → becomes the
  resource-manager / scheduler contract (`05`, `08`).

### Optimizations to evaluate (owner invited these)

1. **Persist the NF4-quantized weights** (highest value). bitsandbytes can
   serialize a quantized model (`save_pretrained` on the quantized pipeline /
   `bnb` state). Re-quantizing ~24 GB on every load is the ~90 s cost; loading
   pre-quantized NF4 from disk should be ~15–25 s. One-time quantize on first run
   (part of model acquisition), then cache. **Recommend for Phase 22.**
2. **Blackwell-native NVFP4 via torchao** instead of bitsandbytes NF4. Pros:
   5th-gen tensor-core support, ~1.7x faster inference than BF16, comparable
   memory; diffusers+torchao recipes exist for FLUX-family. Cons: a different
   quant path to validate on Krea 2 specifically; bitsandbytes NF4 already works.
   **Benchmark both in Phase 22**; if NVFP4 is stable on Krea 2 it likely wins on
   speed (17 s → ~10–12 s) and load (if a pre-quantized checkpoint is cached).
3. **Keep the image sidecar alive** (idle ~1.6 GB) vs **on-demand start** (pay the
   load cost, shut down after idle). Given the load cost even optimized is
   10–25 s and idle cost is only ~1.6 GB, **keep it alive** while the Discovery or
   Image tab is active; shut it down after N minutes idle or when VRAM pressure
   from voice needs the 1.6 GB. Scheduler decides (`08`).
4. **Torch compile / fusion** for the 8-step loop — measure; may shave a few
   seconds; risk of long first-call compile.
5. **Resolution / step tuning** — 8 steps is already turbo-minimal; below 1024²
   for character thumbnails/discovery cards to cut time.

### LoRA + preset registries
- **LoRA registry** (SQLite): id → local file, base compat (`krea-2-turbo`),
  format (kohya/diffusers), default weight, tags (`nsfw`, …), source. User adds
  LoRAs in Tab 2 ("add LoRA"); the three named defaults
  (`gokaygokay` Realism, `inlineresearch` Skin, Lustify Krea 2) are pinned.
  Not a bulk import of the 200+ archive.
- **Single-select LoRA for v1** (matches the current impl). Multi-LoRA stacking is
  a possible later enhancement — the request contract allows a list from day one
  so it's non-breaking, but v1 UI is single-select.
- **Preset** (SQLite CRUD): named `{ model, steps, guidance, size, lora?, weight,
  prompt_scaffold? }`. Tab 2 ships a few; user saves their own; character-image
  generation picks a preset internally.

### Generation flow (Tab 2 and Tab 3 identical core)
```
request → scheduler: acquire GPU (evict LLM + TTS; wait for VRAM to settle)
        → image server: ensure model + LoRA loaded (cached NF4)
        → generate (8 steps, ~10–18 s)
        → [Tab 3] identity check → accept | regenerate (bounded)   ← see below
        → write image to blob store; params + hash + (seed) to SQLite
        → scheduler: release GPU → reload LLM (+ TTS if voice active)
        → deliver (Tab 2 gallery | character gallery + conversation)
```

→ **ADR-0006**: diffusers image sidecar over loopback HTTP (reuse the owner's
pattern), Krea 2 Turbo, **cached NF4 quant** (benchmark NVFP4/torchao),
`enable_model_cpu_offload`, single-select LoRA + preset registries, kept alive
while relevant tabs active.

---

## ⚠ Identity consistency — prompt-based only (owner: no LoRA training)

Krea 2 in diffusers is text-to-image only (no img2img / IP-Adapter / edit), and
the owner ruled out per-character LoRA training. So character visual identity is
**prompt-based**: `QuadView_krea2_v1` for the reference sheet + an LLM-captioned
**canonical appearance block** + fixed per-character seed + realism LoRA +
batch-and-pick against a **face-embedding similarity gate**. Full detail and the
honest scoping of FR-C90 (best-effort; drifts under big pose/scene changes) is in
**`09_character-identity.md`** — this is the product's biggest open risk.

The identity-similarity check (accept/regenerate): a face-embedding model
(ArcFace-style) or CLIP image similarity vs the character's reference set;
threshold + max-retries in config.

---

## Failure modes
- NF4 cache invalid / bitsandbytes breaks on a torch upgrade → re-quantize (slow
  path) or fall back to bf16 + full CPU offload (very slow) → typed error with
  guidance.
- OOM even after LLM+TTS eviction (a big LoRA + large resolution) → reduce
  resolution / drop the LoRA / fewer steps; typed error.
- Image sidecar crash mid-generation → GPU released, no partial file, typed error,
  scheduler reloads the LLM.
- Identity check never passes (likely for full-body/varied-scene shots) → deliver
  best candidate + "couldn't closely match" (expected, not a bug — see `09`).

## Sources
- [unsloth/Krea-2-Turbo](https://huggingface.co/unsloth/Krea-2-Turbo) · [Krea 2 review (Turbo/LoRA)](https://www.buildfastwithai.com/blogs/krea-2-open-source-review-raw-turbo)
- [PyTorch: NVFP4/MXFP8 diffusion on Blackwell + torchao](https://pytorch.org/blog/faster-diffusion-on-blackwell-mxfp8-and-nvfp4-with-diffusers-and-torchao/) · [diffusers-blackwell-quants](https://github.com/sayakpaul/diffusers-blackwell-quants)
- [HF blog: LoRA fine-tuning FLUX on consumer hardware](https://huggingface.co/blog/flux-qlora)
