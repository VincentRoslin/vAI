# Phase 22 — Image Generation (Krea 2 Turbo)

**Status: COMPLETE** (2026-09-06). Governing: ADR-0006 (image sidecar in
principle), ADR-0013 (loopback + bearer transport), ADR-0010 (lock ordering),
ADR-0015 (worker network lockdown), **ADR-0018** (dev venv — amended here to two
venvs), **ADR-0019** (Krea 2-only adaptation — `PROPOSED` at 22.A, **ACCEPTED**
here). Plan: `docs/plan/22_image-generation.md`. Split 22.A (blob store +
contracts + Rust `image` module vs a stdlib fake sidecar — verified with no GPU)
/ 22.B (real `server.py` + `quantize.py` + `.venv-image` + `acquire_image`) /
22.C (the RTX 5080 live gate).

## What shipped

- **`blob/`** — the content-addressed blob store (`app_data/blobs/<sha[0:2]>/
  <sha>`; `put` = sha256 → tmp → fsync → rename, dedupes, no SQLite;
  `get`/`read`/`contains` confine to 64-hex; `reconcile`). Deferred since Phase 9;
  image generation is its first user.
- **`image/`** — `protocol` / `server` / `client` (Rust-supervised sidecar child
  on a free loopback port + per-launch bearer token on every endpoint + Windows
  Job Object + ADR-0015 env + `ready` + protocol-version handshake; **all
  diffusers / Krea JSON confined here**, `ARCHITECTURE.md` §2). `Krea2Backend`
  (`ModelBackend`) + `Krea2Server` (`LoadedInstance` + the new **`ImageInstance`**
  capability trait via `as_image()`; `generate` writes PNGs to a Rust-dictated
  per-call `out_dir` — **binary exchange by path, no base64** — polls `/progress`,
  reads, blob-stores, cleans up). `repo` (`ImageRepo` — `image_lora` +
  `image_preset` registries seeded from the confined loras dir, `generated_image`
  + image-side `asset` rows, `V0007`). `orchestrator` (`ImageOrchestrator` — the
  manual **evict every resident non-image model → settle-wait on the resource
  snapshot → load Krea 2 → generate → blob + rows → restore** sequence, one at a
  time, guard-restored on success / error / cancel; reads the resource snapshot
  but never the RM mutating API; refuses to start during a chat generation;
  **written to be deleted at Phase 23**).
- **`image_gen/server.py`** — the real Krea 2-only sidecar (FastAPI, `127.0.0.1`,
  bearer-auth): loads the NF4 transformer + text-encoder from the fingerprinted
  quant cache, the rest of the pipeline from the HF repo id resolved offline,
  `enable_model_cpu_offload()`. `_convert_lora_to_diffusers` (PEFT / bare-
  diffusers / Kohya, drop-don't-raise) + single-adapter apply + registration
  check. **Never quantizes** — a stale / absent cache → typed 503.
  `image_gen/quantize.py` — the one-time NF4 build (26 GB RAM gate).
  `image_gen/server_fake.py` — the stdlib fake for 22.A + CI.
- **`.venv-image`** (ADR-0018 amendment) — the image sidecar runs from its own
  venv; `chatterbox-tts` (workers venv) hard-pins torch / transformers /
  diffusers against Krea 2's needs. `scripts/setup-venv.mjs` builds both.
- **`acquisition::acquire_image`** (`acquisition/krea2.rs`) + `acquire_image_model`
  IPC — locate the `unsloth/Krea-2-Turbo` snapshot in the HF cache (**never
  pulls**), build the NF4 cache once via `quantize.py` in `.venv-image` if its
  layout is absent, register `ModelKind::Image` / `backend "krea2-diffusers"` /
  `estimated_vram_mb = 11_750` / marker `path` / `config = { model_id,
  hf_snapshot, quant_cache_dir }`. Idempotent.
- **Config schema v9** — `image.{loras_dir, quant_cache_dir, idle_shutdown_s}`.
- **`contracts::image`** — `ImageRequest` + `validate`, `LoraSelection`,
  `ImageEvent` (adjacently tagged, mirrors `GenerationEvent`), `ImageProgress` +
  `ImagePhase`, `GeneratedImageRow`, `ImageLora`, `ImagePreset` + ids. IPC:
  `image_generate`(Channel) / `image_cancel` / `image_loras` / `image_presets` /
  `image_history` / `image_bytes`.
- **`src/pages/ImageGenerator.tsx`** — prompt / negative, size segmented control,
  count 1–8, seed, LoRA select + weight slider, Generate / Cancel, phase-labelled
  progress, result grid via `image_bytes` blob URLs, recent strip. No existing
  page touched.
- **`lifecycle::backend`** gains `as_image()` + `ImageInstance` (additive).
  **`models::Model::availability()`** now `path.exists()` (a diffusers model is a
  directory).

## 22.A / 22.B — offline verification

- **373 → 379 rust tests, 23 vitest, check suite green** across 22.A + 22.B.
- 22.A: 16 `image` tests — blob `put`/`get`/confinement/fsync-before-row;
  `contracts::image` round-trip + rejection + `ts-rs` regen; `ImageRepo` seed
  idempotency + present-file-only LoRA rows + generation round-trip; sidecar
  `load` → `ready` handshake, protocol-mismatch refused, `shutdown` no orphan;
  4 orchestrator tests against a `FakeBackend` LLM + the real `Krea2Backend` /
  fake sidecar (evict + generate + restore with phases in order + LoRA name;
  cancel restores the LLM + writes nothing; 2nd concurrent gen = `Conflict`;
  invalid req = `Validation`).
- 22.B: 3 `acquisition::krea2` tests — quant-cache layout state machine;
  HF-cache snapshot detection (needs `model_index.json`); weights-absent →
  `AppError::NotFound` (no download). `cargo check` + `cargo clippy` green;
  `.venv-image` install deferred to the box.

## 22.C — live gate on the RTX 5080 (16 303 MB VRAM, 32 GB RAM)

`src-tauri/src/image/live_tests.rs`, `#[ignore]`d, gated on
`LOCALAI_RUN_IMAGE_LIVE=1` (+ `LOCALAI_LLAMA_SERVER` / `LOCALAI_TEST_GGUF`).
Real Krea 2 sidecar (`.venv-image`, the staged NF4 quant cache + 3 LoRAs under
`<repo>/models/image/`) driven through the real `ImageOrchestrator` with a real
`llama-server` (Qwen 0.5B) resident. `.venv-image` built with
`node scripts/setup-venv.mjs image` — `Krea2Pipeline` imports, `diffusers
0.41.0.dev0` / `transformers 5.16.1` / `torch 2.11.0+cu128`, CUDA available. The
quant-cache `fingerprint.json` matches the installed libs exactly.

Run twice end to end (`--test-threads=1`); numbers stable across runs.

| # | Check | Result |
| - | ----- | ------ |
| 1 | `image_generate` produces an image via the sidecar | **PASS** — `live_generate_evicts_the_llm_and_restores_it`: base `unsloth/Krea-2-Turbo` NF4, 1024², 8 steps, guidance 0 → one PNG in the blob store (`blob.contains` true), one `generated_image` row. Then the **Realism LoRA** at weight 0.9 with `batch_count = 2` → 2 more PNGs, `generated_image.lora` = `"Realism"`, 3 rows total. |
| 2 | VRAM reserved before the load, released after; LLM evicted then restored | **PASS** — whole-GPU NVML via `ResourceManager::observe()` (a 250 ms poller for the peak; `snapshot()` alone returns the *last* observation — Phase 13): LLM resident **2097 MB** → **peak 11 446 MB** during the base generate → **2105 MB** after restore, LLM back to `ModelState::Loaded`. Phases seen in order: `Evicting → Loading → Generating → Restoring`. The lifecycle manager holds the reservation across the load and releases it on every exit (Phase 14) — the orchestrator never calls the RM mutating API (ADR-0010). |
| 3 | Binary in the blob store, metadata / path / hash in SQLite | **PASS** — `app_data/blobs/<sha[0:2]>/<sha>` (fsync before the `asset` row, ADR-0009); `generated_image` carries prompt / dims / steps / guidance / seed / `lora_id` + weight / `model_id`; the wire never carries a path (`image_bytes` = `blob.read`). |
| 4 | Cancel mid-generate releases VRAM, writes nothing | **PASS** — `live_cancel_mid_generate_restores_llm_and_writes_nothing`: `batch_count = 4`, cancelled ~6 s in → terminal `ImageEvent::Cancelled`; `generated_image` empty, **no blob written** (`blob.list_ids` empty), LLM restored to `Loaded` (2100 MB). |
| 5 | Sidecar crash → typed error, GPU released, LLM restored | **PASS** — `live_sidecar_crash_is_a_typed_error_and_llm_restores`: the sidecar `python.exe` (matched by a PowerShell CIM query on `image_gen*server.py*--port`) is force-killed mid-generate → terminal `ImageEvent::Error`, LLM restored to `Loaded` (2101 MB), the image model **not** left `Loaded`. |
| 6 | `acquire_image` end to end, no download | **PASS** — `live_acquire_image_registers_without_downloading`: `AcquisitionService::acquire_image` with `models.dir` → `<repo>/models` → registry row (`kind = Image`, `backend = "krea2-diffusers"`) in **9 ms** (staged quant cache reused, no `quantize.py`); a second call returns the same id. |

## Performance notes (RTX 5080, warm CUDA)

| Figure | Measured | Note |
| ------ | -------- | ---- |
| Krea 2 NF4 load + first 1024²/8-step image | ~31 s total | load-dominated; the sidecar loads NF4 transformer + text-encoder from the cache, builds the pipeline, `enable_model_cpu_offload()`, then runs 8 steps |
| Realism LoRA, `batch_count = 2`, from a cold sidecar | ~53 s total | includes the same one-time load + LoRA state-dict convert/apply; ≈ 11–13 s per image warm |
| Idle sidecar VRAM (resident, CPU offload, between generations) | not separately measured | expected ~1.6 GB (plan); `idle_shutdown_s` (config v9, default 300) is the timer |
| **Peak VRAM, 1024² generation** | **~11.45 GB** (11 446 / 11 445 MB across runs) | registry `estimated_vram_mb = 11_750` is accurate — **no change** |
| LLM ⇄ image eviction round-trip | folded into the ~31 s above | evict Qwen → settle → load Krea 2; restore is ~1 s (Qwen reload) |
| torchao NVFP4 vs NF4 | not measured | stays the deferred benchmark (ROADMAP §7 O7); NF4 ships |

## LLM / image VRAM coexistence — the Phase 23 trigger

**They do not coexist for the real workload.** Krea 2's measured peak is
**~11.45 GB** of the 16 303 MB card. With the shipped-target LLM (an 8B-class
model, ~6–9 GB resident) also loaded, total demand is **~17–20 GB — over the
card.** The gate only passes with a resident LLM because Qwen 0.5B (~1.4 GB) is a
stand-in; 11.45 + 1.4 happens to fit, but that is not representative.

⇒ **Phase 23 (model hot-swapping) is required** — the manual
`ImageOrchestrator` eviction is the interim, exactly as ADR-0019 §6 states, and
is written to be deleted. Recorded in `ROADMAP.md` §7 (O-item for Phase 23).

## Notes / follow-ups

- **Idle-sidecar VRAM** was not isolated in the automated gate (the poller only
  spans a generate). A cheap follow-up: snapshot once after `Done` before the
  `idle_shutdown_s` timer fires. Low value — the orchestrator unloads promptly.
- **A real 8B LLM + image coexistence run** is not worth automating pre-Phase-23;
  the arithmetic above is decisive.
- **`quantize.py`** was not executed live (the owner's staged cache matches the
  fingerprint). It is compile-checked + `--help`-verified; it runs on any machine
  whose libs don't match. First real exercise is whenever the pinned `diffusers`
  commit moves.

**Phase 22 complete** — 22.A + 22.B offline-verified, all 6 live gate items pass
on the RTX 5080. ADR-0019 → **ACCEPTED**. Pointer → **Phase 23 (Model
Hot-Swapping)**.
