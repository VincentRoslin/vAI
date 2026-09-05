# ADR-0008 — Model acquisition: reqwest transfer + downloads table

- **Status:** ACCEPTED (Phase 5 freeze, 2026-09-05; superseding attacks folded in via Phase 4)
- **Amended:** 2026-09-06 (Phase 12 entry, owner-approved) — **transfer client
  changed from `hf-hub` to hand-rolled `reqwest` range requests.** Everything else
  stands.
- **Research:** `docs/research/phase3/06_model-acquisition.md` (D-7)

## Context
FR-70..77: in-app HuggingFace picker + one-shot resumable download for LLM GGUF;
fixed STT/TTS/image models acquired once via the same path; fully offline after.

## Options considered
- Transfer: `hf-hub` (official Rust crate) vs hand-rolled `reqwest` range requests.
- GGUF metadata: header-only range request vs full download.

## Decision
- **Transfer: hand-rolled `reqwest` range requests** + a SQLite `model_downloads`
  table for queue/pause/resume/cancel state that survives a hard kill.
  - *Amendment rationale (2026-09-06):* `hf-hub` 1.0 pulls **+121 transitive
    crates** (incl. `aws-lc-sys` — a C build needing cmake — and `hf-xet`);
    `hf-hub` 0.3 pulls +66. `reqwest` adds **0 net crates** (its whole tree is
    already present via Tauri). We need `reqwest` regardless for the HF search API
    and the GGUF header range request, so `hf-hub` would mean two HTTP stacks. The
    resume logic it provides is ~30 lines given the `model_downloads` state table
    we build anyway (`Range` header from the persisted `downloaded_bytes`). This
    was the ADR's own documented runner-up; Article IV (fewer deps) decides it.
  - `reqwest` features: `json`, `rustls-tls` (ring, **not** aws-lc), `stream`.
  - HF **Xet** transfer is dropped for v1 (it rode on `hf-hub`/`hf-xet`); plain
    LFS range requests over the `resolve` URL. Revisit only if large-file
    throughput is measured as inadequate.
- **GGUF picker**: parse the **header only** via a range request to show quant /
  context / size. **Probe-confirmed** (2026-09-05): GGUF v3 header parses fine
  from a range request; architecture / file_type (quant) / context_length /
  layer & head counts all appear before the big tokenizer arrays. Use an **~8 MiB
  range** (or adaptive: stop once the needed keys are seen), since a large
  tokenizer vocab can push the KV block past 2 MiB. HF `resolve` URLs return
  HTTP 206 → resumable downloads confirmed.
- **Integrity**: stream SHA256 during write, compare to HF's LFS hash, reject +
  delete on mismatch.
- **Disk budget**: pre-transfer check `file + margin <= free` **and**
  `dir_usage + file <= budget` (config); refuse with a typed error.
- **Register on completion**: Model Registry row, path confined to `model_dir`.
- **Fixed models** (faster-whisper, Chatterbox, `unsloth/Krea-2-Turbo`, the
  `QuadView_krea2_v1` + realism LoRAs, the InsightFace embedder): same path,
  triggered from Settings, no picker. After Krea 2 downloads, **run the one-time
  NF4 quantization and persist the NF4 weights** — this is a **required** step of
  acquisition, not deferred (Phase 4 R-C3); it is gated by a free-system-RAM
  pre-check (~24 GB bf16 transient).
- **Optional HF token** (Settings, OS credential store) for gated repos.
- HF calls are the only egress here, behind explicit user action; offline →
  typed "offline" state, local models still listed + usable.

## Consequences
- **During development/testing, Claude asks the owner before fetching any model,
  LoRA, or cache** — they have pre-downloaded files in related projects. The
  shipped app's first-run flow also offers a "point at an existing models folder"
  option (R also covered by FR-77 / packaging). `CLAUDE.md` has the standing rule.
- Resume is our code: `Range: bytes=<downloaded_bytes>-` on relaunch, append to
  `.part`, streaming SHA-256 continued from a persisted mid-state is *not* done —
  on resume the digest is recomputed over the whole `.part` before the final
  verify (a `.part` is at most one model file; a full re-hash is seconds).
- A content-addressed local model store stays a possible optimization (research
  file §Optimizations); **Xet is out for v1** (see amendment).
- Krea 2 first-run acquisition is ~34 GB + a one-time quant — explicit, with clear
  progress.
