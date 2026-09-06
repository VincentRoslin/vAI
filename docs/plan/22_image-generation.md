# Phase 22 — Image Generation (FLUX.1 Krea)

> **Architecture frozen at Phase 5.** Governing: **ADR-0006** (diffusers sidecar,
> Krea 2 Turbo, **NF4 quant cache required**, `generate`-only / no self-managed
> VRAM), **ADR-0013** (transport — model server = loopback HTTP + per-launch
> bearer token), **ADR-0010** (evict/restore sequence — *manual* here, the
> scheduler that automates it is Phase 23/24), **ADR-0009** (blob write-order),
> **ADR-0016** (config), **ADR-0018** (dev venv). `AI_PIPELINES.md` §5,
> `ARCHITECTURE.md` §3 (single-authority map).
>
> **Reference reviewed 2026-09-06.** The owner exposed a working sibling project
> (`ClaudeAI/assistant/image_gen/`) as a **reference to adapt, not copy** — its
> structure does not dictate ours. Reused *ideas*: the fingerprinted NF4 quant
> cache, `_convert_lora_to_diffusers` (3 in-the-wild LoRA formats, drop-don't-raise
> on unmappable layers), `_apply_lora` (single adapter + registration check),
> `_snap_wh`, the step-progress callback + `/progress` poll. **Not carried over:**
> Z-Image, `krea2_edit` / `_krea2_img2img` / `/edit` (Krea 2 is text-to-image
> only for v1 — identity/img2img is Phase 27), the sidecar's `_min_free_bytes`
> 503 VRAM gate and `exclusive_vram` (Rust owns VRAM — ADR-0006), the
> directory-scan LoRA list (→ SQLite registry), free-form request JSON (→ typed
> contract), lazy re-quantize on load (→ quantize once at acquisition).
>
> **Assets — all confirmed present on the owner's machine 2026-09-06** (do not
> re-download without asking): `unsloth/Krea-2-Turbo` bf16 (~34 GB, HF cache),
> the NF4 quant cache for Krea 2 (~9.6 GB, in the reference project), and the 3
> realism LoRAs (`krea2-realism` 469 MB, `krea2-skin` 192 MB,
> `krea2-lustify-nsfw` 116 MB). See **Phase-entry questions** below.

## Objective

Local image generation as a resource-managed workload: a Rust-supervised,
isolated **image sidecar** running Krea 2 Turbo (NF4 + CPU offload), integrated
with the resource manager (Phase 13) and model lifecycle (Phase 14), writing
results to a new **content-addressed blob store** with metadata / params / seed /
hash in SQLite. Cancellation and crash both release VRAM and restore the LLM.

This phase also **lands the blob store** — deferred since Phase 9 as "arrives
with the first feature that stores a blob" (Phase 18/19 voice turns persist
`Text` and become `Audio { asset, transcript }` additively once it exists).

## Depends on

Phase 9 (persistence — the `Db` pool, migration runner), Phase 13 (resources —
`ResourceManager` reserve/commit/observe/release), Phase 14 (lifecycle —
`ModelBackend` / `LoadedInstance`, `ModelRegistry`), Phase 12 (acquisition — the
download/verify/register path this extends for Krea 2 + the one-time quantize),
Phase 11 (registry — Krea 2 is a registered model with `estimated_vram_mb`).

## Not in this phase

- **Character identity conditioning / img2img / reference sheets** — Phase 27.
- **Typed image actions proposed by the LLM** ("send her a photo of…") — Phase 28.
  This phase's trigger is the user pressing Generate on Tab 2.
- **The scheduler.** Evict-LLM → load-image → restore-LLM is done **manually**
  here by the `image` orchestrator calling the lifecycle manager directly. It is
  a single serialized operation guarded so restore runs on every exit. Phase 23
  replaces it with hot-swap; Phase 24 formalizes the queue. The manual code is
  written to be deleted — keep it in one place (`ipc::commands::image` + a small
  `image::orchestrator`), do not spread evict/restore logic around.
- **Multi-LoRA stacking** — the contract carries `loras: Vec<LoraSelection>` from
  day one; v1 applies `loras[0]` only and the UI is single-select (ADR-0006).
- **torchao NVFP4** — benchmark noted as a deferred item (ROADMAP §7 O7); v1
  ships bitsandbytes NF4 (proven, cache exists). Record a short comparison note
  in the gate if cheap; do not block the phase on it.
- **Discovery-feed / pool generation** (idle-priority character images) — Phase 25+.
- **A LoRA/preset *editor* UI** — v1 seeds the registry and lists it; CRUD is later.

## Architecture notes

### Ownership (Article I)

- **Rust owns**: the sidecar process lifecycle (spawn under a Job Object,
  `/health`, bounded-restart → `Failed`), the loopback port + bearer token, the
  VRAM reservation, the evict/restore of the LLM, the blob store (all filesystem
  writes), every SQLite row (LoRA registry, preset registry, generated-image
  metadata), and cancellation.
- **The image sidecar** does inference only: a typed generate request in → PNG
  bytes + seeds out (over loopback HTTP, bearer-authenticated). No DB, no
  network (ADR-0015 env, set by the Rust supervisor), no VRAM management, no
  decision about when to load/unload — it loads on first `/generate` and unloads
  on Rust's `/unload`. Model weights + quant cache + LoRA files are read-only
  paths Rust dictates on the command line.
- **The frontend** shows the prompt form, LoRA picker, progress, and result grid,
  and a "waiting for image generation…" state while the LLM is evicted —
  presentation only. LoRA list + presets come over IPC; results are `AssetId`s
  resolved to bytes via an IPC read, never a direct file path.
- **Untrusted** (Article III): the prompt and LoRA-selection ids are user/model
  data. The prompt is passed to the sidecar as an opaque string (diffusers
  tokenizes it — it is never shell/eval'd). LoRA ids resolve through the registry
  to a **basename-validated** file inside the confined loras dir; a name that
  escapes the dir or is absent is a typed 400 before any load.

### New module: `src-tauri/src/image/`

Mirrors `llm/` — **nothing outside this module names diffusers, Krea, or the
sidecar's HTTP shape** (`ARCHITECTURE.md` §2).

| File | Role |
| ---- | ---- |
| `image/mod.rs` | `Krea2Backend` (impl `lifecycle::backend::ModelBackend`) — `load` spawns + supervises the sidecar, waits for `ready` + `/health`, returns `Krea2Server`. `Krea2Server` impls `LoadedInstance` (`health` / `measured_vram_mb` from `/health` / `shutdown` → `/unload` + kill) **and** the new `ImageInstance` capability trait. `BACKEND_KEY = "krea2-diffusers"`. |
| `image/server.rs` | Spawn + supervise `image_gen/server.py`: `pick_free_port()` + `bearer_token()` (reused from `llm::server`, or lifted to `crate::job` / a shared `net` helper — pick the smallest move), `--port` / `--token` / `--model-path` / `--quant-cache` / `--loras-dir` args, ADR-0015 env, Windows `JobObject`, `/health` poll with a load deadline, `ready` handshake (a `GET /health` returning `{status:"ok", protocol: N}` — refuse a protocol mismatch). |
| `image/client.rs` | HTTP client (`reqwest`, already a dep): `POST /generate`, `GET /progress` polled ~every 250 ms during a generate and forwarded to the `ProgressSink`, `POST /unload`. Per-call deadline + `CancellationToken` (abort the request; the sidecar's own generation is abandoned on the next `/unload`). **All Krea/diffusers JSON lives here + `protocol.rs`.** |
| `image/protocol.rs` | The wire structs to/from the sidecar (`GenerateBody`, `GenerateResponse`, `ProgressResponse`). Confined. |
| `image/orchestrator.rs` | The **manual, Phase-23-replaceable** evict→load→generate→restore sequence (see below). Small. |
| `image/repo.rs` | `ImageRepo` over the `Db` pool: LoRA registry CRUD-read + seed, preset read + seed, `insert_generated_image`, `list_generated_images`. |
| `image/tests.rs` | Unit tests against the fake sidecar (22.A). |
| `image/live_tests.rs` | `#[ignore]`, env-gated (`LOCALAI_RUN_IMAGE_LIVE`) — the RTX 5080 gate (22.C). |

### New module: `src-tauri/src/blob/`

The content-addressed blob store (`ARCHITECTURE.md` §3 already lists it;
`app_data/blobs/<sha256[0:2]>/<sha256>`).

- `blob/mod.rs` — `BlobStore { root: PathBuf }`.
  - `put(bytes: &[u8], media_type: &str) -> AppResult<AssetId>` — SHA-256 the
    bytes → `AssetId` → if the file exists, return it (dedupe); else write to
    `…/<sha>.tmp`, `fsync`, atomic `rename`, `fsync` the dir. **Does not touch
    SQLite** — the caller commits the `asset` row *after* `put` returns
    (ADR-0009 write-order: blob durable before the row that references it).
  - `get(id: &AssetId) -> AppResult<PathBuf>` — confined path, existence-checked.
  - `read(id) -> AppResult<Vec<u8>>` — for the IPC image-bytes read.
  - `contains(id) -> bool`.
- Path confinement + traversal rejection (an `AssetId` is 64 lowercase hex — the
  newtype already rejects empty; add a hex-shape check here).
- A `reconcile()` (orphan blob with no `asset` row → log; dangling `asset` row
  with no blob → log) — wired but only invoked from `diag` for now.

### Migration `V0007__blob_and_image.sql` (STRICT tables, UUIDv4 ids — ADR-0017)

```sql
-- Content-addressed blobs. id = lowercase hex SHA-256 of the bytes.
CREATE TABLE asset (
    id          TEXT PRIMARY KEY NOT NULL,   -- AssetId = sha256
    media_type  TEXT NOT NULL,               -- "image/png"
    byte_len    INTEGER NOT NULL,
    created_at  TEXT NOT NULL                -- RFC-3339
) STRICT;

-- Realism LoRAs available to Krea 2 (ADR-0006 LoRA registry).
CREATE TABLE image_lora (
    id             TEXT PRIMARY KEY NOT NULL,  -- ImageLoraId (UUIDv4)
    file           TEXT NOT NULL UNIQUE,       -- basename inside the confined loras dir
    display_name   TEXT NOT NULL,
    base_compat    TEXT NOT NULL,              -- "krea2"
    format         TEXT NOT NULL,              -- "peft" | "kohya" | "diffusers" | "unknown"
    default_weight REAL NOT NULL,              -- 0..1
    tags           TEXT NOT NULL,              -- JSON array of strings
    created_at     TEXT NOT NULL
) STRICT;

-- Named parameter sets (ADR-0006 preset registry).
CREATE TABLE image_preset (
    id          TEXT PRIMARY KEY NOT NULL,     -- ImagePresetId (UUIDv4)
    name        TEXT NOT NULL UNIQUE,
    params      TEXT NOT NULL,                 -- JSON: {steps,guidance,width,height,...}
    created_at  TEXT NOT NULL
) STRICT;

-- One generated image: what it is, how it was made.
CREATE TABLE generated_image (
    id           TEXT PRIMARY KEY NOT NULL,    -- GeneratedImageId (UUIDv4)
    asset_id     TEXT NOT NULL REFERENCES asset(id),
    prompt       TEXT NOT NULL,
    negative     TEXT,
    width        INTEGER NOT NULL,
    height       INTEGER NOT NULL,
    steps        INTEGER NOT NULL,
    guidance     REAL NOT NULL,
    seed         INTEGER NOT NULL,
    lora_id      TEXT REFERENCES image_lora(id), -- null = base model
    lora_weight  REAL,
    model_id     TEXT NOT NULL,                -- registry ModelId of Krea 2
    created_at   TEXT NOT NULL
) STRICT;

CREATE INDEX generated_image_created_idx ON generated_image (created_at);
```

Seed `image_lora` (idempotent `INSERT … ON CONFLICT(file) DO NOTHING`, run after
the migration by `ImageRepo::seed`): the 3 LoRAs, only the rows whose file is
actually present in the loras dir (a row for an absent file would fail the
picker). `format` best-effort from the file's key layout; `tags`
`["realism"]` / `["skin"]` / `["nsfw","realism"]`; `default_weight` 0.9.

### Contract: `contracts::image` (ts-rs exported from `image::`)

New ids in `contracts::ids`: `ImageLoraId`, `ImagePresetId`, `GeneratedImageId`
(reuse the `id_newtype!` macro).

| Type | Shape | Notes |
| ---- | ----- | ----- |
| `ImageRequest` | `prompt: String`, `negative: Option<String>`, `width: u32`, `height: u32`, `steps: Option<u32>`, `guidance: Option<f32>`, `seed: Option<i64>`, `batch_count: u32`, `loras: Vec<LoraSelection>` | `validate()`: prompt non-empty & ≤ 2000 chars; `width`/`height` multiples of 16 within 512..=1664 and total pixels ≤ 1664×1664; `steps` 1..=50; `batch_count` 1..=8; `loras` ≤ 1 in v1 (reject >1 with a clear message); `guidance` 0.0..=10.0. |
| `LoraSelection` | `id: ImageLoraId`, `weight: f32` (0..=1) | |
| `ImageEvent` | adjacently tagged: `Progress(ImageProgress)` \| `Done { images: Vec<GeneratedImageRow> }` \| `Error(AppError)` \| `Cancelled` | the `image_generate` Channel payload; mirrors `GenerationEvent` |
| `ImageProgress` | `step: u32`, `total_steps: u32`, `image_index: u32`, `batch_count: u32`, `phase: ImagePhase` | `ImagePhase` = `Evicting` \| `Loading` \| `Generating` \| `Restoring` — the UI's "waiting…" copy |
| `GeneratedImageRow` | `id`, `asset: AssetId`, `prompt`, `width`, `height`, `seed`, `lora: Option<String>`, `created_at` | list + Done payload |
| `ImageLora` | `id`, `display_name`, `base_compat`, `default_weight`, `tags: Vec<String>` | picker row (no `file` on the wire) |
| `ImagePreset` | `id`, `name`, `params: ImagePresetParams` | |

Domain→contract `From` lives in `image::` (contracts stay dependency-free).

### IPC (`ipc::commands`, registered in `lib.rs`)

| Command | Signature | Notes |
| ------- | --------- | ----- |
| `image_generate` | `(req: ImageRequest, on_event: Channel<ImageEvent>) -> TaskId` | rejects a concurrent image generate (one at a time, like `chat_send`); runs the orchestrator |
| `image_cancel` | `(task_id: TaskId)` | fires the token; orchestrator still restores the LLM |
| `image_loras` | `() -> Vec<ImageLora>` | |
| `image_presets` | `() -> Vec<ImagePreset>` | |
| `image_history` | `(limit: u32) -> Vec<GeneratedImageRow>` | recent generations |
| `image_bytes` | `(asset: AssetId) -> Vec<u8>` | frontend renders via a blob URL; path never crosses the wire |

### The manual evict/restore sequence (`image::orchestrator`, ADR-0010 shape)

One serialized operation behind a `tokio::Mutex` (no scheduler yet):

1. Reject if an interactive LLM generation is in flight (`ConversationEngine::
   generation_state()` == `Generating`) → typed `Busy` error, no eviction.
2. Emit `Progress{phase: Evicting}`. If an LLM model is `Loaded`, record its
   `ModelId`, `lifecycle.unload(llm_id)`.
3. `resources.observe()` poll until `free` recovers past the Krea 2 estimate or a
   timeout (WDDM settle-wait, ADR-0010).
4. Emit `Loading`. `lifecycle.load(krea2_id)` — the lifecycle manager acquires
   the VRAM reservation (estimate from the registry row, ~11 750 MB) before the
   backend `load`, and the backend spawns the sidecar.
5. Emit `Generating`. `krea2_server.as_image().generate(req, progress_tx, cancel)`
   — `progress_tx` maps the sidecar's `/progress` to `ImageProgress`.
6. For each returned `(png_bytes, seed)`: `blob.put(png, "image/png")` →
   `asset` row → `generated_image` row (one tx per image).
7. **`finally` guard** (runs on Ok, Err, cancel, panic): emit `Restoring`;
   `lifecycle.unload(krea2_id)`; if step 2 recorded an LLM, `lifecycle.load(llm_id)`.
8. Emit `Done { images }` or `Error` / `Cancelled`.

The reservation is the lifecycle manager's to hold and release (Phase 14
already releases on every exit path) — the orchestrator never calls the resource
manager's mutating methods directly (ADR-0010 lock ordering).

### Python sidecar `image_gen/server.py` (new file in *this* repo)

FastAPI, `127.0.0.1` only, adapted down from the reference:

- **Endpoints**: `GET /health` → `{status, protocol, loaded, vram_mb}`;
  `POST /generate`; `GET /progress`; `POST /unload`. All except `/health`
  require `Authorization: Bearer <token>` (the `--token` value) — a bare loopback
  server is callable by any local process (ADR-0013 R-C1).
- **Model**: `Krea2Pipeline` only. Load path: read the NF4 transformer +
  text-encoder from `--quant-cache <dir>` (fingerprint-checked, from the
  reference's `_load_quantized_components` idea); the rest of the pipeline
  (`vae`/`scheduler`/`tokenizer`) from `--model-path`. `enable_model_cpu_offload()`.
  **If the quant cache is absent or fingerprint-stale → `503` with a typed
  message** ("run image-model setup") — the sidecar never quantizes (acquisition
  owns that, ADR-0006 R-C3).
- **`/generate` body**: `{prompt, negative, width, height, steps, guidance,
  seed, batch_count, lora: {file, weight} | null}`. Rust resolves the LoRA id →
  basename before calling; the sidecar re-validates the basename is inside
  `--loras-dir`. Reuse `_convert_lora_to_diffusers` + `_apply_lora` +
  registration check + `_snap_wh` + the step-progress callback.
- **Removed**: everything Z-Image, `krea2_edit`, `_krea2_img2img`, `/edit`,
  `/load` as a public endpoint (load is implicit on first `/generate`),
  `_min_free_bytes` / VRAM 503 gate, `SAFETY_NEGATIVE`/`QUALITY_NEGATIVE` auto-
  append (pass `negative` through verbatim; it is inert at guidance 0 anyway —
  keep that as a one-line comment, not 20 lines).
- **`image_gen/server_fake.py`** — stdlib `http.server`, no torch: honours the
  bearer token, returns a solid-colour PNG per `batch_count` with deterministic
  seeds, ticks `/progress` from 0→total over ~1 s. For 22.A + CI.
- **`image_gen/requirements.txt`** — torch `2.11.0+cu128` (already in the
  ADR-0018 `.venv` from Phase 19), `diffusers @ git` (Krea2Pipeline not in a
  tagged release), `transformers`, `accelerate`, `bitsandbytes>=0.50` (Blackwell),
  `peft`, `safetensors`, `sentencepiece`, `fastapi`, `uvicorn`, `pydantic`,
  `pillow`. **ADR-0018 amended** to add these to the one dev venv (see 22.B.1).

### Config (schema **v9**, additive — ADR-0016)

`image` section:
- `image.loras_dir: Option<PathBuf>` — default `<models.dir>/image/loras`.
- `image.quant_cache_dir: Option<PathBuf>` — default `<models.dir>/image/quant_cache/krea2`.
- `image.idle_shutdown_s: u64` — default `300`. The sidecar stays resident
  (~1.6 GB) this long after a generation, then Rust `/unload`s + kills it. (v1:
  a simple timer in the orchestrator; no tab-visibility signal yet.)

`ConfigKey` gains `ImageLorasDir` / `ImageQuantCacheDir` (session-overridable,
same pattern as `RuntimesDir`).

### Acquisition (extends `acquisition`, ADR-0006 / ADR-0008)

- `FixedModel::Image` variant → `acquire_fixed(Image)`:
  1. Ensure `unsloth/Krea-2-Turbo` present in the HF cache (skip the ~34 GB pull
     if `HF_HOME` already has it — **ask the owner first**, per CLAUDE.md).
  2. **One-time NF4 quantize** → `image.quant_cache_dir`. A small Python
     `image_gen/quantize.py` (run via the venv by the Rust acquisition step,
     stdout progress): load bf16 transformer + text-encoder,
     `BitsAndBytesConfig(nf4, bf16)`, `save_pretrained`, write a fingerprint
     (`recipe_version`, model id, lib versions). Behind a **system-RAM
     pre-check** — the bf16 transformer is ~24 GB resident during this step; the
     reference machine has 32 GB, so gate on `free_ram > 26 GB` and say so.
  3. Register Krea 2 in the model registry: `ModelKind::Image`,
     `backend = "krea2-diffusers"`, `estimated_vram_mb = 11_750`, path = the
     model dir, `config` blob = `{ quant_cache_dir }`.
- LoRA files: `acquisition` gets `import_image_lora(src_path)` /
  `download_image_lora(url, filename, civitai_token?)` (streaming `.part` →
  rename, from the reference's `download_lora`) writing into the confined
  `loras_dir`; then `ImageRepo::seed` registers what landed. **Which of the 3
  LoRAs come from where is a phase-entry question** (below).

### `lifecycle::backend` — additive change

Add `fn as_image(&self) -> Option<&dyn ImageInstance> { None }` alongside
`as_llm`, and an `ImageInstance` trait:

```rust
#[async_trait]
pub trait ImageInstance: Send + Sync {
    async fn generate(
        &self,
        req: ImageGenerateArgs,          // resolved: lora is a basename or None
        progress: ProgressSink,          // (f32, Option<String>) — reuse worker::ProgressSink
        cancel: CancellationToken,
    ) -> AppResult<Vec<GeneratedPng>>;   // { bytes, seed }
}
```

`ModelKind::Image` already exists in the contract (`contracts::model`); confirm,
add if not (additive).

## Performance notes

Record on the RTX 5080 (16 GB, 32 GB RAM):
- Krea 2 NF4 **load** time from a warm quant cache (target ≈ 15–25 s).
- **VRAM**: idle (sidecar resident, CPU offload — expect ~1.6 GB) and **peak
  during a 1024² generation** (expect ~11.4 GB). Feed the peak back into the
  registry `estimated_vram_mb` if it differs materially.
- **Per-image** time at 1024², 8 steps, base model (expect ~17 s) and with the
  realism LoRA.
- **LLM coexistence answer**: with the 8B LLM (`estimated_vram_mb`) resident,
  does Krea 2's peak fit 16 GB? Expected **no** → that is the Phase 23
  requirement; record it explicitly in `ROADMAP.md` §7 and the gate.
- Eviction round-trip: LLM unload → settle → Krea 2 load, and the restore.
- (Cheap-only) a torchao NVFP4 load+generate data point vs NF4.

## Steps

### 22.A — Blob store + contracts + Rust `image` module against the fake sidecar

*All verifiable now — no GPU, no models, no venv.*

1. **`blob/` module + `V0007` migration (asset table only first).**
   Verify: `cargo test blob::` — `put` dedupes, `get` confines, a traversal id
   is rejected, blob is fsynced before the row (test observes ordering via a
   `BlobStore` + repo seam).
2. **`contracts::image` + new ids + `validate()` + round-trip/rejection tests.**
   Verify: `cargo test contracts::` green; `node scripts/check.mjs` regenerates
   `src/bindings/` with the new types, `tsc` clean.
3. **`V0007` full (image_lora / image_preset / generated_image) + `ImageRepo`
   (reads + `seed`).**
   Verify: `cargo test image::repo` — seed is idempotent, only present-file LoRA
   rows are inserted, `insert_generated_image` + `list` round-trip.
4. **`image_gen/server_fake.py`** (stdlib, bearer-checked, PNG + progress).
   Verify: `python image_gen/server_fake.py --port … --token …` by hand →
   `curl` with/without the token behaves; `/generate` returns N PNGs.
5. **`image/server.rs` + `image/client.rs` + `Krea2Backend` / `Krea2Server`**
   pointed at `server_fake.py`.
   Verify: `cargo test image::` — backend `load` → `ready` handshake →
   `health` ok; protocol-mismatch refused; `shutdown` → `/unload` + no orphan
   (`tasklist` check on Windows CI, else process-gone check).
6. **`lifecycle::backend` `as_image` + `ImageInstance`**, impl on `Krea2Server`;
   register `Krea2Backend` in `lib.rs` under `ModelKind::Image`.
   Verify: `cargo test lifecycle::` still green (additive); a test loads a fake
   Krea 2 model through the real lifecycle manager and gets an `as_image()`.
7. **`image/orchestrator.rs` + the 6 IPC commands + `lib.rs` wiring.**
   Verify: `cargo test image::orchestrator` — with a scripted lifecycle
   (fake LLM + fake Krea 2) and the fake sidecar: evict→load→generate→restore
   emits the expected `ImageEvent` sequence; `image_cancel` mid-generate still
   restores the LLM; a concurrent `image_generate` is rejected; an LLM
   generation in flight blocks eviction.
8. **`ImageGenerator.tsx`** — prompt form, size buttons, LoRA `<select>` +
   weight slider, batch count, Generate, progress bar (phase + step N/M),
   result grid via `image_bytes` blob URLs; Settings gains a read-only LoRA
   list. Config schema **v9** (`image.*`).
   Verify: `ImageGenerator.test.tsx` (mocked IPC) — form → `image_generate`
   with a valid `ImageRequest`; progress renders; results render.
   `node scripts/check.mjs` green.
9. **Docs + indexes** (same change): `docs/contracts.md` (`contracts::image`
   row), `docs/decisions/0019-*.md` (**PROPOSED**), `docs/decisions/README.md`,
   `ARCHITECTURE.md` module map (`image/`, `blob/`), `docs/plan/README.md`
   detail level, this file's stage boxes.
   Verify: `node scripts/check.mjs` green; every 22.A gate item recorded.

### 22.B — Real sidecar + venv + NF4 acquisition  *(blocked: venv deps + Krea 2 asset decisions — see phase-entry questions)*

1. **ADR-0018 amended**: add the image deps to `workers/requirements.txt` (or a
   sibling `image_gen/requirements.txt` installed into the same `.venv`);
   `scripts/setup-venv.mjs` installs them.
   Verify: `uv run python -c "import torch, diffusers, bitsandbytes; from
   diffusers import Krea2Pipeline; print(torch.cuda.is_available())"` → `True`.
2. **`image_gen/server.py`** — the real Krea 2-only sidecar (per Architecture
   notes). **`image_gen/quantize.py`** — the one-time NF4 step.
   Verify: `python image_gen/server.py --help`; a dry import of the load path.
3. **`acquisition` — `FixedModel::Image`**: HF-cache check, `quantize.py` behind
   the RAM pre-check, registry registration; `import_image_lora` /
   `download_image_lora` + `ImageRepo::seed`.
   Verify: `cargo test acquisition::` (mocked); the quantize step invoked
   against a tiny stand-in module if one is feasible, else `NOT EXECUTED` here
   and covered by 22.C.
4. **Docs**: `docs/spec/DEVELOPMENT.md` venv note; `ROADMAP.md` transition log.
   Verify: check suite green.

### 22.C — Live gate on the RTX 5080  *(blocked on 22.B + owner go-ahead)*

1. Run acquisition for Krea 2 end to end (reuse the owner's HF cache + quant
   cache per the phase-entry answer; else quantize live and time it).
   Verify: registry row present; quant cache fingerprint valid.
2. `image::live_tests` (`#[ignore]`, `LOCALAI_RUN_IMAGE_LIVE`): base-model
   generate; realism-LoRA generate; a 2nd LoRA-format file if available;
   `batch_count = 2`; cancel mid-generate; `taskkill` the sidecar mid-generate.
   Verify: each records — image in the blob store, `generated_image` row
   correct, VRAM reserved-then-released (resource-manager snapshot before /
   during / after), LLM evicted then restored, cancel leaves **no** partial
   blob + LLM restored, crash → typed error + GPU released + LLM restored.
3. Record all Performance-notes figures; write the LLM-coexistence answer to
   `ROADMAP.md` §7.
   Verify: `docs/verification/22_phase22_image-generation.md` complete; ADR-0019
   → **ACCEPTED**.

## Verification gate (physically executed — `CLAUDE.md` Article IV)

1. A `image_generate` request produces an image via the sidecar (live, 22.C).
2. VRAM is **reserved before the load and released after** — shown via the
   resource-manager snapshot at three points; the LLM is evicted before and
   restored after.
3. The image bytes are in the blob store at `blobs/<sha[0:2]>/<sha>`; the
   `asset` + `generated_image` rows have the correct hash, path, prompt, seed,
   params, and LoRA.
4. **Cancelling mid-generation** releases VRAM, restores the LLM, and leaves
   **no** partial blob and no `generated_image` row.
5. **Sidecar crash / OOM / invalid params** each produce a typed `AppError`
   (no hang), release VRAM, and restore the LLM.
6. Krea 2 VRAM footprint (idle + peak) and per-image time recorded; the
   **LLM-coexistence answer** recorded in `ROADMAP.md` §7 (drives Phase 23).
7. LoRA registry seeded with all 3 present LoRAs; a generate with the realism
   LoRA applied is verified (adapter registered, visible effect), and a bad
   LoRA id is a typed 400 before any load.
8. Blob store `reconcile()` detects an orphan blob and a dangling `asset` row
   (unit-level is fine).
9. `node scripts/check.mjs` green; rust + vitest counts recorded.

## ADRs / open questions

- **ADR-0019 — Image sidecar: Krea 2-only adaptation** (what is reused from the
  reference vs rewritten; the manual-evict-until-Phase-23 decision; NF4-from-
  acquisition; bearer-auth). PROPOSED at 22.A.9, ACCEPTED at 22.C.3.
- **LLM/image VRAM coexistence** → the Phase 23 trigger. Answer recorded at 22.C.
- **torchao NVFP4 vs NF4** (ROADMAP §7 O7) — a data point at 22.C if cheap;
  otherwise stays deferred, NF4 ships.

## Phase-entry questions for the owner (raise before 22.B)

1. **Krea 2 bf16 weights** — reuse the existing `~/.cache/huggingface/hub/
   models--unsloth--Krea-2-Turbo` (~34 GB), or re-pull fresh?
2. **NF4 quant cache** — reuse the reference project's
   `image_gen/quantized_cache/krea2/` (~9.6 GB, copy into `<models.dir>/image/
   quant_cache/krea2/`), or regenerate it here via `quantize.py` (one-time,
   ~24 GB RAM spike, a few minutes)? Regenerating is cleaner provenance;
   reusing is faster and avoids the RAM spike.
3. **LoRA files** — the 3 `.safetensors` (`krea2-realism`, `krea2-skin`,
   `krea2-lustify-nsfw`): copy the owner's existing files into `<models.dir>/
   image/loras/`, or download fresh (`krea2-realism` ≈ `gokaygokay/Krea-2-Realism`
   on HF; `krea2-lustify-nsfw` is Civitai → needs a token)? Owner confirmed
   2026-09-06 all 3 are in the registry regardless of source.
4. **Venv deps** — OK to add `diffusers @ git`, `bitsandbytes`, `peft`
   (~2–4 GB with CUDA libs already partly present) to the one dev `.venv`
   (ADR-0018)?
