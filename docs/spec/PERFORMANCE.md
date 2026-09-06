# PERFORMANCE.md — LocalAI

**Status:** Frozen at Phase 5, 2026-09-05. Binding targets; **actual numbers are
measured and recorded**, not assumed.
Method: baseline → identify bottleneck → apply the highest-value fix → re-measure
→ record before/after. No intuition-only optimization (`CLAUDE.md` Article IV).

Reference machine: Ryzen 7 9800X3D · RTX 5080 16 GB · 32 GB RAM · NVMe.
Numbers depend on the selected model + quant + context — budgets are experience
targets, validated per hardware+model combination.

---

## 1. Budgets (experience targets)

| Metric | Target | Measured at |
| ------ | ------ | ----------- |
| Cold app start → window interactive | < ~3 s | Phase 6 baseline, tracked every phase |
| User message appears in UI | immediate (< ~50 ms) | Phase 16 |
| LLM time-to-first-token (model loaded) | < ~1–2 s | Phase 15 (backend), Phase 16 (end-to-end). **Backend measured (15.D):** Qwen 0.5B Q4_K_M on RTX 5080 → **TTFT ≈ 23 ms**; model load + `/health` ready ≈ **775 ms** |
| LLM tokens/sec | model-dependent; recorded per model | Phase 15. **Measured (15.D):** Qwen 0.5B Q4_K_M `-ngl -1` ctx 4096 → **≈ 278 tok/s** (a larger chat model sets the real baseline at Phase 16) |
| UI during streaming | never frozen; no dropped frames | Phase 16, Phase 30 |
| Voice: speech-end → first audio | ~1–3 s | Phase 18+19 |
| Voice: barge-in trigger → silence | < ~200 ms | Phase 19 |
| Image generation (1024², 8 steps, loaded) | ~15–20 s | Phase 22 |
| Image model cold load (NF4 cached) | ~15–25 s | Phase 22 |
| LLM reload after an image job | ~10–30 s (model-dependent) | Phase 23 |
| Model load/unload/swap UI feedback | state visible within ~200 ms | Phase 14, Phase 23 |
| DB single indexed read | < ~1 ms | Phase 9 |
| DB 100-row transaction | < ~10 ms | Phase 9 |
| Memory retrieval (FTS5, per scope) | < ~10 ms | Phase 21 |
| Model download throughput | saturates the connection | Phase 12 |
| Resource `request` (cached snapshot, no probe) | < ~1 ms | Phase 13 |
| Hardware probe (NVML + `sysinfo`), off the request path | ~1–15 ms, on a ~1.5 s poll | Phase 13 |
| Lifecycle state transitions (excl. the backend load itself) | < ~1 ms | Phase 14 |
| Peak system RAM, normal use | well under 32 GB | Phase 22, Phase 31 |

## 2. Known bottlenecks and the decisions that address them

| Bottleneck | Decision |
| ---------- | -------- |
| Krea 2 NF4 re-quantization (~90 s) + 24 GB bf16 in RAM | **NF4 quant cached once at acquisition** (ADR-0006/0008); NF4 loaded directly (~6 GB) after |
| LLM ↔ image swap thrash (chat + image alternating) | scheduler **batches** queued image jobs behind one eviction; "image/discovery mode" keeps the LLM unloaded while in those tabs; predictive eviction on tab open |
| Token-stream IPC overhead | Tauri **Channels** + coalesce deltas (~2–4 tokens / ~16 ms per message) |
| Logging on the hot path | non-blocking, bounded, **lossy** writer (Phase 10) — a log burst drops lines with a count, never blocks generation; token-throughput comparison at Phase 16 |
| STT int8 unavailable on Blackwell | faster-whisper `float16`; VAD-gated (transcribe speech only) |
| Voice perceived latency | pipeline overlap — synthesize clause 1 while the LLM generates clause 2; play chunk 1 while chunk 2 synthesizes |
| KV-cache VRAM pressure | flash-attention; optional `q8_0` KV-cache quantization; `-ngl` auto-tuned from measured free VRAM |
| VRAM accounting must not stall an interactive load | the resource manager's `request` reads the **last cached measurement** + the ledger — never the driver; only the ~1.5 s observe loop touches NVML (Phase 13) |
| A slow backend load blocking other work | `LifecycleManager::load` is `async` and holds the transition lock only for state edits, never across the backend `load`/`shutdown`/`health` call (Phase 14, ADR-0010) |
| WDDM free-memory lag after unload | scheduler polls measured `free` until it recovers before the next load |
| Discovery card generation latency | pre-generated pool + idle top-up; lower-res cards (768²/896×1152); on-demand fallback |
| Large image/character lists in the UI | virtualized lists; thumbnail cache; lazy image loading |
| DB write contention | dedicated write connection (serialized); prepared-statement cache; batch inserts in one transaction |

## 3. Measurement discipline

- Every phase that adds a GPU/DB/IO path **records its baseline** into this doc's
  companion `docs/verification/` entry for that phase.
- Phase 31 (Performance Audit) assembles the full baseline table, compares to §1,
  profiles every miss to a concrete bottleneck, applies the highest-value fix with
  recorded before/after, and re-measures the whole table with no regression.
- Phase 38 spot-checks the key metrics on the **packaged** build.
- A metric that cannot meet its budget is **explicitly accepted with a rationale**
  in the Phase 31 audit doc — never silently missed.

## 4. Non-negotiable

- The UI must **never** appear frozen — all heavy work is off the UI thread and
  off the IPC command thread; long operations return a task id + stream progress.
- No optimization may violate an ownership boundary (`CLAUDE.md` Article I) or add
  hidden state. Optimizations that change architecture get an ADR and a note to
  the Phase 36 maintainability audit.
