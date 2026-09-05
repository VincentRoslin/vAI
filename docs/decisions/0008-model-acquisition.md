# ADR-0008 — Model acquisition: hf-hub + downloads table

- **Status:** PROPOSED (Phase 3 draft) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/06_model-acquisition.md` (D-7)

## Context
FR-70..77: in-app HuggingFace picker + one-shot resumable download for LLM GGUF;
fixed STT/TTS/image models acquired once via the same path; fully offline after.

## Options considered
- Transfer: `hf-hub` (official Rust crate) vs hand-rolled `reqwest` range requests.
- GGUF metadata: header-only range request vs full download.

## Decision
- **`hf-hub`** for transfers (async, automatic resume, desktop-friendly chunking)
  **+ a SQLite `model_downloads` table** for queue/pause/resume/cancel UI state
  that survives a hard kill.
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
- Minimal download code (hf-hub does resume); our table adds the UI/restart story.
- Evaluate HF **Xet** transfer + a content-addressed local model store as
  optimizations (research file §Optimizations).
- Krea 2 first-run acquisition is ~34 GB + a one-time quant — explicit, with clear
  progress.
