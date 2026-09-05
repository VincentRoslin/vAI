# Phase 12 — Model Acquisition & Picker

> **Architecture frozen at Phase 5.** Step detail **finalized at phase entry, 2026-09-06**. Governing: **ADR-0008** (*amended 2026-09-06*: `reqwest` transfer, not `hf-hub`; + a SQLite `model_downloads` table, header-only GGUF parse, SHA-256 verify, pre-transfer budget guard, auto-register, same path for fixed models), **ADR-0009** (persistence), **ADR-0016** (config — model dir + budget), **ADR-0011** (image models come with the owner's impl — *not* this phase), `SECURITY.md` (HF token → OS credential store, path confinement), `CLAUDE.md` Art. II (fully skippable, offline after).

## Owner sign-off (2026-09-06)
`CLAUDE.md` → "Model / asset downloads — always ask first". Owner answered:
- **Engine tests: local mock HTTP server only** (deterministic, offline) — resume,
  checksum, budget, register, control all run against a `tiny_http` fixture server.
- **Gate 1 (a real HF GGUF): a small one is fine** — Claude names the exact
  repo/file and waits for a final yes before fetching (~a few hundred MB).
- **Fixed models (12.8): download now** — **faster-whisper** (size is nominally a
  Phase 18 call — use `large-v3` CT2 as the default, revisitable) and
  **Chatterbox Turbo** (the build the owner uses). Exact repos/files listed and
  confirmed at 12.8 before the transfer.

## Objective
An in-app HuggingFace picker + one-shot **resumable** download for **LLM GGUF**
files, and a shared download/verify path used to acquire the pinned faster-whisper
and Chatterbox files (no picker). Rust owns HF calls, the download, verification,
and filesystem writes; the frontend is presentation only. Fully skippable; all
acquired models work offline afterwards.

## Depends on
Phase 11 (registry — an acquired model is registered), Phase 9 (download state),
Phase 8 (model dir + `models.budget_gb`), Phase 10 (observability, redaction of a
possible HF token), Phase 7 (`DownloadId`, `TaskId`, `AppError`).

## Not in this phase
- Loading / running models (Phase 14–15).
- A picker for STT/TTS/image models (fixed single models).
- Any image-model acquisition or the Krea 2 NF4 quant step (Phase 22 / owner impl).
- The HF-token entry UI + credential-store wiring — a stub `token: None` now; the
  Settings field + OS credential store land with the Settings UI phase.

## Architecture notes
- **New module `src-tauri/src/acquisition/`** (`mod.rs`, `hf.rs` — HF API +
  transfer, `gguf.rs` — header parser, `download.rs` — the engine + state,
  `tests.rs`). The **only** runtime network egress in the app lives here, behind
  an explicit user action.
- **Crates (ADR-0008 amended):** `reqwest` (`json`, `rustls-tls`, `stream` — 0
  net crates, whole tree already present via Tauri) for **all** HF traffic —
  search API, GGUF header range, and the file transfer; `sha2` (streaming
  SHA-256). **No `hf-hub`.** Resume = a `Range: bytes=<downloaded_bytes>-` header
  from the persisted `model_downloads` row, appending to `.part`; on resume the
  digest is recomputed over the whole `.part` before the final verify. GGUF
  header is **hand-parsed** (~120 lines — stable format; the crates.io options
  are immature and want a full file).
- **`model_downloads` table** (migration `V0003`): `id` (`DownloadId`), `repo`,
  `filename`, `kind`, `dest_path`, `total_bytes`, `downloaded_bytes`, `sha256_expected`,
  `etag`, `state` (Queued/Downloading/Paused/Verifying/Failed/Complete),
  `error`, `created_at`, `updated_at`. Survives a hard kill → resume on next launch.
- **Path confinement:** every write target is `<models.dir>/<repo owner>/<repo
  name>/<filename>` resolved + checked with the Phase 11 `validate_model_path`
  rules (no `..`, inside the dir). `.part` during transfer, atomic rename on
  verified completion.
- **Budget guard (pre-transfer):** `file_size + min_free_margin <= free_space`
  **and** `dir_usage + file_size <= models.budget_gb`. `min_free_margin` is a new
  config key `models.min_free_gb` (default 20) — schema **v3**.
- **Events / progress:** a Tauri **Channel** per download for byte progress
  (coalesced ~4×/s); Events for state changes. `cancel(taskId)` → stop + clean.
- **Offline:** search → a typed `AppError::BackendUnavailable("offline")`, never a
  hang (short connect timeout). The picker still lists local + in-progress models.

## Performance notes
- Download throughput: let `hf-hub` do multi-chunk; expose no tuning yet. Record
  MB/s from a real transfer (gate 8) or from the mock-server transfer as a floor.
- SHA-256 **streamed during write** (`sha2` update per chunk), not a second read.
- GGUF: fetch an adaptive range (start 1 MiB, extend to at most 8 MiB) — stop once
  `general.architecture`, `*.context_length`, `general.file_type` are seen. Log
  the byte count.

## Steps

**12.1 — Deps + module skeleton + `V0003` migration**
    Do:     Add `reqwest` (features `json`, `rustls-tls`, `stream`), `sha2`,
            `tiny_http` (dev-dep). `acquisition/` skeleton. `V0003__model_downloads.sql`
            + config schema **v3** (`models.min_free_gb` default 20,
            `ConfigKey::ModelsMinFreeGb`, `step_forward` handles it). Register in
            `lib.rs`, `README`s, `ARCHITECTURE`.
    Verify: `cargo build`; `cargo tree -d` — no new duplicate; `cargo test` —
            `V0003` applies, config v2→v3 migration test passes.

**12.2 — GGUF header parser (`gguf.rs`)**
    Do:     Parse magic (`GGUF`), version (2/3), tensor & KV counts, then the KV
            metadata typed values. Extract `general.architecture` (String),
            `general.file_type` (u32 → quant label via a fixed table),
            `<arch>.context_length` (u64), `<arch>.block_count` (u64). `parse_header(&[u8])
            -> Result<GgufHeader, AppError>` — returns `Incomplete` if the slice
            is too short (caller extends the range).
    Verify: `cargo test acquisition::gguf` — against a **committed tiny fixture**
            header (hand-built or the first bytes of a real GGUF, ≤ 64 KiB): the
            arch / quant / context parse correctly; a truncated slice → `Incomplete`;
            garbage magic → `Err`.

**12.3 — HF API client (`hf.rs`)**
    Do:     `search_models(query, limit) -> Result<Vec<HfModel>, AppError>`
            (`GET /api/models?search=&filter=gguf&limit=`); `list_gguf_files(repo)
            -> Result<Vec<HfFile>, AppError>` (`GET /api/models/{repo}` →
            `siblings`, keep `*.gguf`, pull `size` + LFS `sha256` from
            `/api/models/{repo}/tree/main?recursive=1`); `fetch_header_range(repo,
            file, len) -> Result<Vec<u8>, AppError>` (range GET on the `resolve`
            URL). Short connect timeout → offline error.
    Verify: `cargo test acquisition::hf_offline` — with an unroutable base URL the
            calls return `BackendUnavailable`, fast, no hang. (Live calls: gate,
            owner-gated.)

**12.4 — Download engine (`download.rs`) against a local mock server**
    Do:     `DownloadEngine` — `start(spec) -> DownloadId` (writes a `Queued`
            row), a worker that streams a `reqwest` GET (`Range: bytes=<off>-`
            when `off > 0`) → append to `<dest>.part`, updating `downloaded_bytes`
            + a progress Channel; `resume(id)` re-issues from the persisted offset;
            on completion recompute SHA-256 over the `.part`, verify the digest,
            atomic-rename, set `Complete`. Transient errors → bounded backoff.
    Verify: `cargo test acquisition::download_*` — a `tiny_http` (dev-dep) server
            serving a fixture with `Range` support: full download → byte-identical
            + digest match; kill mid-way (drop the engine) then `resume` → same
            bytes; a served body that mismatches the declared sha256 → `Failed`,
            `.part` deleted.

**12.5 — Disk-budget guard**
    Do:     `check_budget(file_size, models_dir, budget_gb, min_free_gb) ->
            AppResult<()>` — `dir_usage` via a walk (cached briefly); free space
            via `fs4`/`sysinfo` (or a `std`+winapi call). Refuse **before** any
            byte is fetched, naming the shortfall.
    Verify: `cargo test acquisition::budget_*` — a 1 GB budget + a 2 GB file →
            `Err("over budget")`; a huge `min_free_gb` → `Err("insufficient free
            space")`; a fitting file → `Ok`.

**12.6 — Register on completion**
    Do:     On verified `Complete`: parse the now-local GGUF header (full file
            available — reuse `gguf.rs`), build a `ModelDraft` (kind `Llm`,
            backend `"llama.cpp"`, quant/context/streaming from the header,
            `estimated_vram_mb` from a first-cut formula: `file_size_mb +
            kv_cache_estimate + overhead`), `registry.register(draft, models_dir)`.
    Verify: `cargo test acquisition::registers_on_complete` — after a mock
            download, the registry has one `Llm` entry with the parsed context +
            quant and a path inside the model dir.

**12.7 — Download control**
    Do:     `pause(id)` (stop worker, keep `.part` + row `Paused`), `resume(id)`,
            `cancel(id)` (stop, delete `.part`, row `Failed`/removed),
            `delete_model(model_id)` (remove file + registry row + any download
            row). `cancel(taskId)` wired to the running task.
    Verify: `cargo test acquisition::control_*` — pause+resume → byte-identical to
            an uninterrupted download; cancel leaves no `.part` and no row; delete
            removes file + registry entry.

**12.8 — Fixed STT/TTS acquisition**
    Do:     `acquire_fixed(FixedModel::Stt | Tts)` — pinned `{repo, revision,
            files[], kind, backend}` constants: **faster-whisper `large-v3` CT2**
            (`model.bin` + `config.json` + `tokenizer.json` + `vocabulary.*`;
            size revisitable at Phase 18) and **Chatterbox Turbo** (the owner's
            build — exact repo confirmed at this step). Through the 12.4–12.6 path
            (no picker, no GGUF parse — a fixed `ModelDraft` per model, multi-file).
            Idempotent when all files present.
    Verify: gate item 5 — run **live** once the owner confirms the exact repos
            (see below); the *code path* is unit-tested first with the mock server
            + fake fixed specs.

**12.9 — Typed IPC + picker UI**
    Do:     Commands: `hf_search`, `hf_list_files`, `download_start`,
            `download_pause`/`resume`/`cancel`, `downloads_list`, `model_delete`,
            `acquire_fixed`. A progress **Channel** per download. `src/lib/ipc.ts`
            wrappers + `contracts.ts`. UI: a `/models` screen — search, a model's
            GGUF files with quant/size/context, start/track/manage downloads,
            local models list. Presentation only.
    Verify: `npx tsc`, `vitest` (a component test with mocked IPC: renders the
            list, shows a progress bar from a Channel event, shows a failed
            download's error + retry). Bindings regenerated.

**12.10 — Offline behaviour + wiring**
    Do:     `lib.rs setup()` — on launch, reconcile `model_downloads`: `Downloading`
            rows → `Paused` (offer resume). Picker search offline → typed state.
    Verify: `cargo test acquisition::reconcile_on_start`; `npm run tauri dev` with
            no network — `/models` lists local models, search shows "offline", no
            crash.

**12.11 — Gate run + docs + commit**
    Do:     `node scripts/check.mjs`; `docs/verification/11_phase12_acquisition.md`
            (record which gate items ran live vs `NOT EXECUTED`);
            `src-tauri/README.md`, `ARCHITECTURE.md` §2/§9, `docs/contracts.md`,
            `SECURITY.md` (egress isolated here), `PERFORMANCE.md` (throughput),
            `ROADMAP.md` §1/§4/§5. Commit.
    Verify: check suite green; verification doc complete + honest about NOT-EXECUTED.

## Verification gate
1. Pick + download + verify + register a small real GGUF from HF — **live**
   (owner OK'd a small file; exact repo/file confirmed just before fetching).
2. Resume an interrupted download to a byte-identical result. *(12.4 mock)*
3. Checksum mismatch is rejected and cleaned up. *(12.4 mock)*
4. Insufficient disk / over-budget is refused **before** transfer. *(12.5)*
5. faster-whisper + Chatterbox Turbo acquired via the same path — **live**
   (owner: "download them now"; repos confirmed at 12.8).
6. A downloaded model appears in the registry, metadata correct, path confined.
   *(12.6 mock + the live download from gate 1)*
7. Picker UI works read-only with the network disabled; local models stay usable.
   *(12.10)*
8. Download throughput recorded (MB/s) from the live transfers. *(gates 1, 5)*
9. Full check suite green; `src/bindings` regenerated + committed. *(12.11)*

## ADRs / open questions
- **ADR-0008 amended 2026-09-06** (owner-approved): transfer client `hf-hub` →
  hand-rolled `reqwest`. Xet dropped for v1.
- Raise a further ADR only if throughput needs parallel-range (unlikely for GGUF).
- faster-whisper model *size* stays a Phase 18 decision; `large-v3` CT2 is the
  Phase 12 default.
