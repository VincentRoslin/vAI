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
| 1 | Pick + download + verify + register a small **real** GGUF from HF | **NOT EXECUTED — HF blocked from this network.** `curl`/`reqwest` to `huggingface.co/api/*`, `/models/*`, and `/resolve/*` all return **HTTP 401** from this machine (even `/api/whoami-v2`; the site root is 200) — a path-level proxy/firewall block, not an auth requirement. The full code path is proven offline (gates 2, 3, 6). A `#[ignore]`d live test (`live_download_qwen_0_5b`, owner-confirmed file `unsloth/Qwen2.5-0.5B-Instruct-GGUF` → `…-Q4_K_M.gguf`) is committed; run `cargo test -p localai --lib -- --ignored live_download_qwen` on a network where HF is reachable. |
| 2 | Resume an interrupted download → byte-identical result | **PASS** — `resume_completes_from_a_partial_part_file`: 30 KB pre-written to `.part`, engine `Range`-requests `bytes=30000-`, appends, verifies — final bytes exactly match the whole body. |
| 3 | Checksum mismatch rejected + cleaned up | **PASS** — `checksum_mismatch_fails_and_cleans_up`: a wrong `sha256_expected` → state `Failed`, `.part` deleted, no final file. |
| 4 | Insufficient disk / over-budget refused **before** transfer | **PASS** — `service_refuses_an_over_budget_download` + `budget::tests::*`: a 2 GB file vs a 1 GB budget → `ResourceExhausted("…budget…")`; a huge `min_free_gb` → `ResourceExhausted("…free-space…")`; the check runs before any HTTP request. |
| 5 | faster-whisper + Chatterbox Turbo acquired via the same path | **NOT EXECUTED — same HF block.** `acquire_fixed` is implemented with the correct owner-confirmed repos + file lists; its multi-file orchestration shares the gate-2/3/6 engine code. Run live when HF is reachable. |
| 6 | A downloaded model appears in the registry, metadata correct, path confined | **PASS** — `a_completed_gguf_download_is_registered`: a GGUF-header fixture served via the mock → on completion the registry has one `Llm` entry with the parsed `context_tokens = 4096`, `quant = Q4_K_M`, and a path inside the model dir. |
| 7 | Picker UI works read-only with the network disabled; local models stay usable | **PASS** — `Models.test.tsx`: renders the installed list from mocked IPC; a search that rejects with `BackendUnavailable` shows an "offline" state, no crash. `tauri dev`: `/models` loads, `schema_version: 3`, no errors. |
| 8 | Download throughput recorded (MB/s) | **NOT EXECUTED** — needs a live transfer (gate 1/5). The `#[ignore]`d test prints MB/s when run. |
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
  phase). Public models need none; a `#[ignore]`d live test can't currently
  verify the gated-repo path from this network anyway.

## Open item (→ `ROADMAP.md` §6)

Gates 1, 5, 8 are **NOT EXECUTED** solely because HuggingFace is unreachable
(HTTP 401 on every model/API/resolve path) from this development machine. Nothing
in the code is blocked — the live test is committed and ready. Run it from a
network where `huggingface.co/api/*` responds, or once the block is lifted.

**Phase 12 code + offline gates complete.** Pointer → Phase 13 (Resource Manager).
