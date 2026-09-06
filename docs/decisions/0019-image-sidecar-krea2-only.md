# ADR-0019 — Image sidecar: Krea 2-only adaptation of the owner's reference

- **Status:** ACCEPTED (Phase 22.C, 2026-09-06) — the live gate passed on the
  RTX 5080 (`docs/verification/22_phase22_image-generation.md`): all 6 items,
  peak VRAM 11.45 GB (≈ the 11 750 estimate), evict/restore + cancel + crash all
  clean. The measured 11.45 GB peak confirms an 8B LLM cannot coexist → Phase 23.
- **Governs:** the concrete shape of the image subsystem `docs/decisions/0006`
  froze in principle. Refines, does not change, ADR-0006 / ADR-0013 / ADR-0010.
- **Reference:** the owner's sibling project `ClaudeAI/assistant/image_gen/`
  (reviewed 2026-09-06 as a *reference to adapt, not copy*), `docs/plan/22`.

## Context

ADR-0006 froze "reuse the diffusers FastAPI sidecar pattern, adapted". Phase 22
had to turn that into code: which parts of the owner's working sidecar are
reused, which are rewritten, and how the pieces ADR-0006 named (loopback + auth,
NF4 cache, `generate`-only, no self-managed VRAM, LoRA registry) actually fit the
frozen module boundaries. The scheduler that ADR-0010 describes does not exist
yet (it lands at Phase 23/24), so Phase 22 needs an interim way to free VRAM for
the image model.

## Decision

**1. Krea 2 Turbo only; text-to-image only.** The reference's Z-Image model,
`krea2_edit` / hand-rolled `_krea2_img2img`, and `/edit` are **not** carried
over. Identity / img2img is Phase 27. The `image` module (`Krea2Backend` /
`Krea2Server`) is the sole place that names diffusers or the sidecar's HTTP
shape (`ARCHITECTURE.md` §2).

**2. Reused from the reference (ideas, re-implemented):** the fingerprinted NF4
quant-cache concept; `_convert_lora_to_diffusers` (normalizes PEFT / bare-
diffusers / Kohya layouts, drops unmappable layers instead of raising) +
`_apply_lora` (single adapter, verifies registration); `_snap_wh`; the
step-progress callback + `GET /progress` poll.

**3. Not carried over:** the sidecar's `_min_free_bytes` 503 VRAM gate and
`exclusive_vram` (Rust owns VRAM — ADR-0006); the directory-scan LoRA list (→ a
SQLite `image_lora` registry, seeded from the confined dir; `image_preset` for
named parameter sets); free-form request JSON (→ the typed `contracts::image`
vocabulary); lazy re-quantize on load (→ quantize once at acquisition — Phase
22.B; the sidecar loads NF4 from the cache and returns a typed 503 if it is
absent).

**4. Transport (ADR-0013):** a Rust-supervised child on `127.0.0.1:<free port>`
+ a per-launch bearer token on **every** endpoint (a diffusers/uvicorn server
has no named-pipe mode; a bare loopback server is callable by any local
process — Phase 4 R-C1). Free-port + token minting are shared with `llm::server`.
Windows Job Object for orphan cleanup; ADR-0015 offline env; a `ready` handshake
that also checks a protocol-version int and refuses a mismatch.

**5. Binary exchange is by path, never inline (Article I).** `/generate` takes
an `out_dir` the Rust core dictates per call; the sidecar writes one PNG per
image there and returns the paths; the core reads, blob-stores
(`app_data/blobs/<sha[0:2]>/<sha>`, ADR-0009 write order: fsync the blob before
the `asset` row commits), and deletes the temp dir. No base64, no new crate.

**6. Manual evict/restore until Phase 23.** `image::orchestrator` runs one image
generation at a time through *evict every resident non-image model → settle-wait
on the resource snapshot → load Krea 2 → generate → store → restore the evicted
models*, with the restore in a guard so it runs on success, error, and cancel
alike. It **reads** the resource snapshot but never calls the resource manager's
mutating API (the lifecycle manager owns the reservation — ADR-0010 lock
ordering). It refuses to start while an interactive chat generation is in
flight. **This module is written to be deleted:** Phase 23 (hot-swap) replaces
the manual eviction, Phase 24 formalizes the queue.

**7. The blob store lands here.** Deferred since Phase 9 as "arrives with the
first blob feature"; image generation is that feature. Voice turns become
`Audio { asset, transcript }` on top of it later, additively.

**8. Config schema v9** adds `image.{loras_dir, quant_cache_dir,
idle_shutdown_s}` (the dir overrides file-only, `null` ⇒ a subdir of
`models.dir`).

**9. NF4 for v1.** torchao NVFP4 stays the deferred benchmark (ROADMAP §7 O7);
NF4 ships (proven, the owner already has a valid cache).

## Consequences

- The Krea 2 backend registers only when `image_gen/server.py` is present
  (Phase 22.B) — until then `image_generate` returns a clean "no image model
  registered".
- `Model::availability()` now checks `path.exists()` rather than `is_file()` — a
  diffusers-layout image model is a directory.
- `lifecycle::backend` gains `as_image()` + an `ImageInstance` trait (additive,
  mirrors `as_llm`).
- The manual orchestrator is a known, contained piece of tech debt with a
  scheduled removal (Phase 23).
- The sidecar runs from its own `.venv-image` (ADR-0018 two-venv amendment,
  22.B) — `chatterbox-tts` and Krea 2 cannot share a resolve.
- NF4-from-acquisition is `acquisition::acquire_image` (22.B): it verifies the
  bf16 weights are in the HF cache (never pulls them), runs `quantize.py` once in
  `.venv-image` if the quant cache is absent, and registers the model with a
  marker `path` (the weights live in the shared HF cache, outside the model dir)
  and `config = { model_id, hf_snapshot, quant_cache_dir }`. `Krea2Backend::load`
  passes the repo id as `--model-path` for a marker dir so the sidecar resolves
  the rest of the pipeline offline.
