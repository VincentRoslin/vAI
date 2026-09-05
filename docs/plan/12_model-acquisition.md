# Phase 12 — Model Acquisition & Picker

## Objective
An in-app HuggingFace model picker with one-shot resumable download for **LLM
GGUF** files, plus a shared download/verify path used to acquire the single pinned
**faster-whisper** and **Chatterbox** models on first run (no picker for those).
Rust owns the download; the UI is presentation only.

> ⚠ Step detail assumes the Phase 3.5 acquisition ADR (crate choice, checksum
> source, GGUF parsing). Finalize at phase entry.

## Depends on
Phase 11 (Model Registry — an acquired model is registered), Phase 9 (persistence
for download state), Phase 8 (config: model dir, disk budget), Phase 10
(observability).

> **Before triggering any download at phase entry / testing:** ask the owner
> whether they already have the specific file locally (Krea 2, LoRAs, NF4 cache,
> STT/TTS models, test GGUFs). List exact files/paths, wait for their answer.
> `CLAUDE.md` → "Model / asset downloads — always ask first".

## Not in this phase
- Loading or running models (Phase 14/15).
- A picker for STT/TTS/image models — those are fixed single models.
- Any image-model acquisition (FLUX.1 Krea comes with the owner's implementation).

## Architecture notes
- **Rust core owns**: HF API calls (read-only network — the only allowed runtime
  egress, isolated here), the download, checksum verification, filesystem writes
  (path-confined to the configured model dir), and Model Registry updates.
- **Frontend owns**: browsing, selection, progress display, download management
  UI — via typed IPC only.
- Downloads are resumable and survive app restart (state in SQLite).
- This is a network feature; it must be **fully skippable** and everything it
  produces must work offline afterwards (`CLAUDE.md` Art. II).

## Performance notes
- Download throughput should saturate the connection (chunked, possibly parallel
  ranges — decide in Phase 3); measure MB/s.
- Checksum verification streamed during/after download, not a second full read
  where avoidable.
- GGUF metadata read from the header only (no full-file parse).

## Steps

### 12.1 — HF model search / listing
Do: Rust command to query the HF API for models + their file lists; return
results as typed data (id, files, sizes, tags). Read-only; no auth required for
public models.
Verify: searching a known term returns results; the response is typed; offline →
a typed "offline" error, not a hang.

### 12.2 — GGUF-aware file listing
Do: for a selected model, list its `.gguf` files with parsed quant level, tensor
count / size, and context length from the GGUF header (range-request the first
KBs, don't download the whole file).
Verify: for a real multi-quant GGUF repo, the quants and context sizes shown match
the files; only header bytes were fetched (log the byte count).

### 12.3 — Resumable download
Do: download to `<model_dir>/<repo>/<file>.part` using HTTP range requests;
persist progress (offset, total, etag) in SQLite; retry transient failures with
backoff; rename to final name on completion.
Verify: start a download, kill the app mid-transfer, relaunch → it resumes from
the saved offset and completes; the final file size matches `Content-Length`.

### 12.4 — Integrity verification
Do: compute SHA256 while writing; compare against the HF-provided hash (LFS
pointer / API). On mismatch: delete the file, mark the download failed, surface a
typed error.
Verify: a download of a real file verifies OK; a deliberately corrupted `.part`
(flip a byte before the final chunk) is rejected and cleaned up.

### 12.5 — Disk-budget guard
Do: before starting, check `free_space >= file_size + margin` **and**
`model_dir_usage + file_size <= configured_budget`. Refuse with a typed error if
either fails.
Verify: set a tiny budget in config → a download is refused pre-transfer with a
clear message; free-space check refuses when the file wouldn't fit.

### 12.6 — Register on completion
Do: on successful verified download, create a Model Registry (Phase 11) entry:
stable id, path (under the confined model dir), backend `llama.cpp`, parsed
capabilities/context/quant, estimated resource need.
Verify: after a download, the model appears in the registry with correct metadata
and a valid path; the path is inside the configured model dir (no traversal).

### 12.7 — Download control
Do: cancel (delete `.part`), pause (stop, keep `.part` + state), resume, delete a
completed model (remove file + registry entry, with confirmation).
Verify: each control works; cancel/delete leave no orphan files or registry
entries; pause+resume produces a byte-identical file to an uninterrupted download.

### 12.8 — Fixed STT/TTS model acquisition
Do: reuse the 12.3–12.5 download/verify path to fetch the pinned faster-whisper
and Chatterbox model files on first run (triggered explicitly, e.g. from
settings, not silently). No picker UI — a fixed repo/file per the Phase 3.9 ADR.
Verify: triggering acquisition downloads + verifies both models to their expected
locations; re-triggering when present is a no-op; both are then usable offline.

### 12.9 — Model-picker UI
Do: a screen to search, view a model's GGUF files, pick one, start a download,
watch progress, and manage existing/paused/failed downloads. Presentation only;
all actions via typed IPC.
Verify: the full pick→download→appears-in-list flow works from the UI; progress
updates live; a failed download shows the error and a retry.

### 12.10 — Offline behaviour
Do: with no network — the picker shows already-downloaded models and a clear
"offline" state for search; all downloaded models remain fully usable.
Verify: disable the network → picker still lists local models, search shows the
offline state (no crash), and a previously downloaded model still loads once
Phase 15 exists.

## Verification gate
1. Pick + download + verify + register a small real GGUF from HF. — end-to-end,
   evidence: registry entry + file on disk + logged SHA256 match.
2. Resume an interrupted download to a byte-identical result. — kill/relaunch test.
3. Checksum mismatch is rejected and cleaned up. — corrupted-`.part` test.
4. Insufficient disk / over-budget is refused **before** transfer starts. — config
   test.
5. faster-whisper + Chatterbox models acquired via the same download/verify path.
6. A downloaded model appears in the Model Registry with correct metadata and a
   path confined to the model dir.
7. Picker UI works read-only with the network disabled; downloaded models stay
   usable.
8. Download throughput recorded (MB/s) for the Phase 31 baseline.

## ADRs / open questions this phase resolves
- Confirms the Phase 3.5 acquisition ADR in practice.
- May raise an ADR on parallel-range downloading vs single-stream if throughput
  is inadequate.
