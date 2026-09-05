# Phase 3 · Model acquisition (HuggingFace, GGUF)

Covers step 3.5 and D-7. Requirements: FR-70..77, FR-60; ARQ-6.

---

## D-7 — Design

### Download client: `hf-hub` (official HuggingFace Rust crate)
- Async + sync interfaces; repo metadata queries; file download **with automatic
  resume**; tokio backend uses a single task with multiple chunks (desktop-network
  friendly, 2026 update).
- Covers FR-71 (resumable, survives restart — `hf-hub` resumes from the partial
  file) with minimal code.
- Public models need no token. A user-supplied HF token (Settings) unlocks gated
  repos — optional, stored via the OS credential store, never in config/logs.

Alternative: hand-rolled `reqwest` range requests. More control (explicit
`.part` + SQLite progress rows for a UI that survives a hard kill), but
re-implements what `hf-hub` gives. **Recommendation: `hf-hub` for the transfer,
plus our own SQLite `downloads` table** for the queue/pause/resume/cancel UI
state and cross-restart visibility (FR-75). Best of both.

### GGUF metadata (FR-70, ARQ-6)
- GGUF has a well-defined header (magic, version, tensor count, KV metadata
  including `general.architecture`, `*.context_length`, quantization type per
  tensor). Parse the **header only** via an HTTP range request for the first
  ~1–2 MB — no full download to show quant/context/size in the picker (FR-C10 of
  the acquisition plan doc, step 12.2).
- Crate: `gguf` / `gguf-rs` exists, or a ~100-line hand parser (the header format
  is stable and simple). **Verify against a real GGUF header in Phase 3** (plan
  gate item).

### Integrity (FR-72)
- HF exposes a SHA256 for LFS files (in the file's LFS pointer / the
  `/api/models/{id}/tree` response). Compute SHA256 while writing; compare;
  reject + delete on mismatch.
- `hf-hub` may verify the ETag; we additionally verify the content hash.

### Disk budget (FR-73)
- Config: `model_dir` (default under the app data dir), `model_dir_budget_gb`
  (default e.g. 150 on this 215 GB-free machine), `min_free_gb` (default 20).
- Pre-check: `file_size + margin <= free_space` **and**
  `dir_usage + file_size <= budget`. Refuse before the transfer starts with a
  typed error naming the shortfall.

### Registration (FR-74)
- On verified completion → create a Model Registry row (Phase 11): stable id
  (`<repo>/<file>` or a hash), path (confined to `model_dir`), backend
  `llama.cpp`, parsed capabilities/context/quant, estimated VRAM (from the
  Phase 5 estimation formula).

### Fixed STT/TTS/image models (FR-76)
- Pinned repo+file per the voice/image ADRs (faster-whisper `large-v3` or the
  chosen size; Chatterbox Turbo; `unsloth/Krea-2-Turbo`). Acquired **through the
  same `hf-hub` + verify + budget path**, triggered explicitly from Settings, no
  picker. The Krea 2 download is ~34 GB — the budget check matters here.
- After Krea 2 download: run the **one-time NF4 quantization** and cache the
  result (see `04` optimization 1) as part of the acquisition step, so the first
  generation isn't a 300 s wait.

### Offline (FR-77, NFR-1)
- `hf-hub` calls are the **only network egress** in this subsystem, gated behind
  an explicit user action (browse / download). With no network: search returns a
  typed "offline" state; the picker lists locally-present models; everything
  downloaded is fully usable.

→ **ADR-0008**: `hf-hub` for transfers + a SQLite `downloads` table for UI state;
header-only GGUF parsing; SHA256 verification; pre-transfer budget guard;
auto-register; same path for fixed STT/TTS/image models; one-time NF4 quant cache
for Krea 2.

---

## Optimizations

1. **Header-only GGUF parsing** (range request ~1–2 MB) to populate the picker —
   no full download to show quant/context/size.
2. **Cache the NF4-quantized Krea 2 weights** on first acquisition → first
   generation drops from ~300 s to ~15–25 s. Highest-value single optimization in
   the image path.
3. **HuggingFace Xet transfer** — HF's dedup-based content-addressed transfer
   (2026) is faster than LFS for large files and resumes at chunk granularity.
   Check `hf-hub`'s Xet support; prefer it when available.
4. **Content-addressed local model store** (`model_dir/blobs/<sha256>` + a
   name→hash index): re-downloading the same file from a different repo/mirror is
   a no-op; identical LoRAs/models dedupe on disk.
5. **Background prefetch** of the fixed STT/TTS models during first-run while the
   user explores the UI (explicit consent on first launch, then silent) — so
   voice is ready without a separate wait. Krea 2 (~34 GB) stays explicit.
6. **Concurrent chunk tuning** — `hf-hub`'s multi-chunk single-task default is
   desktop-friendly; expose a "max download bandwidth / connections" setting for
   users on metered connections rather than saturating by default.
7. **Verify-during-write** (streaming SHA256) instead of a second full read after
   download.

---

## Failure modes
- Interrupted download → `hf-hub` resumes; our `downloads` row tracks state for
  the UI across a hard kill.
- Hash mismatch → delete, mark failed, typed error, offer retry.
- HF API down / rate-limited → typed error, retry with backoff; local models
  unaffected.
- Disk fills mid-download despite the pre-check (another process) → abort, keep
  the `.part` for resume, surface the space problem.
- Gated model without a token → clear "needs a HuggingFace token in Settings"
  message.

## Sources
- [huggingface/hf-hub](https://github.com/huggingface/hf-hub) · [docs.rs/hf-hub](https://docs.rs/hf-hub/latest/hf_hub/) · [releases](https://github.com/huggingface/hf-hub/releases)
