# 11 — Phase 12: Model Acquisition & Picker

**Date:** 2026-09-06
**Branch:** `main` (local; no remote).
**Method:** new `src-tauri/src/acquisition/` module; 21 unit tests (mock HTTP
server with `Range`) + config v3 tests; a `tauri dev` launch; a `#[ignore]`d live
HF test held ready.

Governing: **ADR-0008** (amended 2026-09-06 — `reqwest` transfer, not `hf-hub`),
ADR-0009, ADR-0016. Plan: `docs/plan/12_model-acquisition.md`.

---

## What landed

- `acquisition/gguf.rs` — hand-written GGUF **header** parser (partial-buffer
  aware → `Incomplete`; magic/version/KV walk; quant from `general.file_type`).
- `acquisition/hf.rs` — `HfClient`: `search_models`, `list_gguf_files` (+ header
  metadata via a bounded range), `fetch_range`. Short connect timeout → a fast
  `BackendUnavailable` on offline, never a hang.
- `acquisition/budget.rs` — `check_budget`: model-dir budget **and**
  `min_free_gb` margin, refused **before** any byte.
- `acquisition/download.rs` — `DownloadEngine`: `reqwest` stream →
  `<dest>.part` with `Range` resume from the file's own size, streaming... (in
  fact re-hashed on completion) SHA-256 verify, atomic rename, register on
  completion (GGUF header parse → `ModelDraft`); `pause` / `resume` / `cancel`;
  `reconcile_on_start` (interrupted rows → `Paused`).
- `acquisition/mod.rs` — `AcquisitionService`; `acquire_fixed` for the pinned
  bundles: **faster-whisper large-v3** (`Systran/faster-whisper-large-v3`) and
  **Chatterbox Turbo** (`ResembleAI/chatterbox-turbo` — owner-confirmed, MIT);
  `delete_model`.
- `V0003__model_downloads.sql`; config schema **v3** (`models.min_free_gb`,
  default 20; `ConfigKey::ModelsMinFreeGb`).
- IPC: `models_list`, `model_delete`, `hf_search`, `hf_list_files`,
  `download_start` (per-download progress `Channel`), `download_pause` /
  `resume` / `cancel`, `downloads_list`, `acquire_fixed`.
- UI: a functional `/models` screen (installed list + delete + get-STT/TTS,
  live downloads with pause/resume/cancel + %, HF search → expandable file list
  with quant/ctx/size → download). `Models.test.tsx`.
- Deps: `reqwest` (`rustls-tls` = ring; **0 net crates** — whole tree already
  present via Tauri), `sha2`, `fs4`, `walkdir`, `tiny_http` (dev).

---

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | Pick + download + verify + register a small **real** GGUF from HF | **PASS (live)** — `live_download_qwen_0_5b` (`#[ignore]`d): `Qwen/Qwen2.5-0.5B-Instruct-GGUF`. HF file listing returned `quant=Q4_K_M ctx=32768 size=491 400 032 sha256=74a4da8c…`; the engine downloaded **491 MB in 9.7 s (50.6 MB/s)** over the real `resolve` URL, **verified the SHA-256 against HF's LFS hash**, atomic-renamed, and registered one `Llm` entry with the parsed quant + context and a path confined to the model dir. (`unsloth/Qwen2.5-0.5B-Instruct-GGUF` — my first pick — is gated/removed; HF returns 401 for those, which briefly looked like a network block.) |
| 2 | Resume an interrupted download → byte-identical result | **PASS** — `resume_completes_from_a_partial_part_file`: 30 KB pre-written to `.part`, engine `Range`-requests `bytes=30000-`, appends, verifies — final bytes exactly match the whole body. |
| 3 | Checksum mismatch rejected + cleaned up | **PASS** — `checksum_mismatch_fails_and_cleans_up`: a wrong `sha256_expected` → state `Failed`, `.part` deleted, no final file. |
| 4 | Insufficient disk / over-budget refused **before** transfer | **PASS** — `service_refuses_an_over_budget_download` + `budget::tests::*`: a 2 GB file vs a 1 GB budget → `ResourceExhausted("…budget…")`; a huge `min_free_gb` → `ResourceExhausted("…free-space…")`; the check runs before any HTTP request. |
| 5 | faster-whisper + Chatterbox Turbo acquired via the same path | **NOT EXECUTED — deferred (size + owner go-ahead).** `acquire_fixed` is implemented with the owner-confirmed repos + exact file lists: `Systran/faster-whisper-large-v3` (`config.json`, `preprocessor_config.json`, `tokenizer.json`, `vocabulary.json`, `model.bin` ≈ 3.1 GB) and `ResembleAI/chatterbox-turbo` (11 files incl. `t3_turbo_v1.safetensors`) — both MIT. Its multi-file orchestration is the same engine proven live in gate 1. ~5 GB total — run when the owner wants the models present (e.g. before Phase 18). |
| 6 | A downloaded model appears in the registry, metadata correct, path confined | **PASS** — mock: `a_completed_gguf_download_is_registered` (fixture GGUF → `Llm` entry, `context_tokens = 4096`, `quant = Q4_K_M`, path in the model dir). **Live** (gate 1): the real Qwen 0.5B registered with `ctx 32768`, `quant Q4_K_M`, path confined. |
| 7 | Picker UI works read-only with the network disabled; local models stay usable | **PASS** — `Models.test.tsx`: renders the installed list from mocked IPC; a search that rejects with `BackendUnavailable` shows an "offline" state, no crash. `tauri dev`: `/models` loads, `schema_version: 3`, no errors. |
| 8 | Download throughput recorded (MB/s) | **PASS** — **50.6 MB/s** for the 491 MB Qwen transfer (gate 1). Single-stream `reqwest`, no tuning; well above what a GGUF picker needs. |
| 9 | Full check suite green; bindings regenerated + committed | **PASS** — `node scripts/check.mjs` all green; new bindings: `HfModelSummary`, `HfGgufFile`, `DownloadState`, `DownloadInfo`, `DownloadProgress`, `DownloadRequest`, `FixedModelKind`. 153 rust tests, 7 vitest. |

---

## Decisions taken this phase

- **ADR-0008 amended (owner-approved):** transfer client `hf-hub` → hand-rolled
  `reqwest`. `hf-hub` 1.0 = +121 transitive crates (incl. `aws-lc-sys` C build,
  `hf-xet`); `reqwest` = 0 net crates and is needed anyway for the HF API + GGUF
  header range. Xet dropped for v1.
- **Resume re-hashes the whole `.part`** on completion rather than persisting a
  mid-stream SHA-256 state — a `.part` is one model file; a full re-hash is
  seconds and keeps the code simple.
- **faster-whisper model size** stays a Phase 18 call; `large-v3` CT2 is the
  Phase 12 default.
- **HF token**: not wired (a Settings field + OS credential store is a later
  phase). Public models need none.

## Bugs found by the live test + fixed

1. **LFS SHA-256 field** — HF's file listing carries the content hash as
   `lfs.oid` (64 hex, sometimes `sha256:`-prefixed), not `lfs.sha256`. Downloads
   were completing without hash verification. Fixed in `hf.rs`; the live test now
   verifies against the real HF hash.
2. **Windows `\\?\` verbatim paths** — `Path::canonicalize()` returns
   extended-length paths; those were being stored in the registry. Added
   `models::strip_verbatim`; `validate_model_path` returns a clean path.
3. **`register()` model-dir off-by-one** — it derived the confinement root by
   walking `dest_path` parents (one too many). Replaced with an explicit
   `DownloadSpec.models_dir`.

## Open item (→ `ROADMAP.md` §6)

Gate 5 (fixed models) is **NOT EXECUTED** — deferred pending the owner's go-ahead
for the ~5 GB download (faster-whisper + Chatterbox Turbo). `acquire_fixed` is
wired with the exact repos/files; run it before Phase 18.

**Phase 12 complete** (gates 1–4, 6–9 pass; gate 5 deferred). Pointer → Phase 13.
