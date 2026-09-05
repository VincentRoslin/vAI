# Phase 3 · Resource manager & VRAM accounting

Covers step 3.7 and D-6. Requirements: FR-80..83, NFR-35, NFR-36; ARQ-7, ARQ-8.

---

## The problem, concretely for this machine

16 GB total, shared with the Windows desktop compositor (~1–2 GB fluctuating).
Effective budget for AI ≈ **13–14 GB**. From the other research files:

| Scenario | Components | ~VRAM | Verdict |
| -------- | --------- | ----- | ------- |
| Chat (text) | LLM Q4_K_M ~8B + 8k KV | 6–8 GB | fits, lots of headroom |
| Voice | above + faster-whisper fp16 + Chatterbox + VAD | 11–15 GB | fits, tight — resource manager may pick a smaller LLM or STT model |
| Image idle (sidecar alive) | Krea 2 Turbo NF4, CPU-offloaded | **~1.6 GB** | coexists with anything |
| Image generating | Krea 2 Turbo NF4, active | **~11.4 GB peak** (measured, owner's impl) | fits **only with LLM + TTS evicted** |
| Image + LLM both active | — | ~18–20 GB | **impossible** |

**Design consequences (feed the scheduler ADR D-9):**
1. **LLM ↔ active image generation are mutually exclusive** on the GPU. Every
   image *generation* (Tab 2 or FR-C80) requires evicting the LLM (+ TTS) and
   restoring after. The image *sidecar can stay resident* (~1.6 GB) — only the
   ~11.4 GB generation spike conflicts.
2. **Voice coexists with the LLM** but leaves little slack — the resource manager
   must be able to say "not enough for `large-v3` fp16 alongside this LLM, use
   `medium`".
3. Only **one** heavy GPU workload at a time. No concurrent image jobs.
4. LLM reload after an image job costs time (model-size dependent, ~10–30 s) —
   the scheduler should batch queued image requests before restoring the LLM.

---

## D-6 — Design

### Measurement: `nvml-wrapper`
- Supports NVML v12; `Device::memory_info()` → `{ total, free, used }`.
- **Per-process VRAM on Blackwell + WDDM is unconfirmed** — the env audit showed
  `N/A` for compute processes in `nvidia-smi`, and consumer WDDM often doesn't
  report per-process memory. **Phase 3 exit gate requires a runnable probe** (a
  tiny Rust bin calling `running_compute_processes()` / `process_utilization`
  while `llama-server` holds VRAM) to confirm what's available on driver 610.88.
- **Also flag:** a known WDDM hang issue on RTX 5090 / RTX PRO 6000 under
  paravirtualization — verify the 5080 bare-metal path is unaffected (it should
  be; note it for Phase 4).
- **Fallback if per-process is N/A:** whole-GPU accounting — `free`/`used` from
  NVML + **our own reservation ledger** for what *we* loaded. External apps
  grabbing VRAM are handled by trusting the measured `free` and reconciling.

### Estimation (never trust it as truth — NFR-35)
Per model type, a closed-form estimate:
- **LLM:** GGUF file size (weights) + KV cache
  (`2 · n_layers · n_kv_heads · head_dim · ctx · n_parallel · 2 bytes`) + CUDA
  context (~400–700 MB) + compute buffers + **margin (~10%)**.
- **Image:** quantized checkpoint size + LoRA sizes + a fixed activation estimate
  (calibrated) + margin.
- **STT/TTS:** ~fixed per model (small), calibrated once.
Store `(estimated, measured)` per commit in SQLite; learn a per-model correction
factor.

### Lifecycle (from the plan)
```
request(estimate) → reserve (ledger entry, before spawn)
                  → commit (backend ready; replace estimate with first real Δ)
                  → observe (poll ~1–2 s while loaded)
                  → release (unload/crash/cancel)
                  → reconcile (periodic: ledger vs measured; recover stale)
```
- **One async mutex / actor** serializes every reserve/commit/release/swap. No two
  load or swap operations in flight.
- Safety margin is config (`vram_safety_margin_mb`, default ~1500).
- `reserve` decision uses **last observed `free` − margin − outstanding
  reservations**, not a blocking NVML call on the request path.

### Interaction with the scheduler (D-9)
The resource manager **accounts**; the scheduler **decides order and eviction**.
`can_fit(request)` and `what_to_evict(request)` are resource-manager queries; the
scheduler acts on them.

→ **ADR-0007**: `nvml-wrapper` + reservation ledger, whole-GPU accounting with
per-process as a bonus if the probe confirms it, closed-form estimate + learned
correction, single serialization point, config safety margin.

---

## Failure modes
- Estimate too low → OOM on load → child dies → release + bump correction factor +
  typed error; do not retry same config.
- NVML unavailable / driver mismatch → the app must still run (degraded: no
  fine-grained accounting, conservative single-model policy). Not a hard crash.
- Ledger drifts from reality (external app, missed release) → `reconcile` trusts
  the measurement, logs the drift, recovers stale reservations after a timeout.
- WDDM free-memory lag after unload → the swap path waits for `free` to actually
  recover (poll, with timeout) before loading the next model.

## Optimizations
1. **KV-cache quantization** (see `02`) — the cheapest way to buy VRAM headroom
   for voice-alongside-LLM.
2. **Keep the image sidecar resident** at ~1.6 GB rather than fully stopping it —
   only the generation spike needs the LLM evicted, and only when a job is
   actually queued.
3. **Batch queued image jobs** — if the user requests 3 images (or a character
   sends several), generate all before restoring the LLM. One eviction/restore
   instead of three.
4. **Predictive eviction** — when the user opens the Discovery or Image tab,
   pre-shrink the LLM (or pre-evict) so the first image doesn't wait the full
   unload+reload on the critical path.
5. **Learned correction factors** — persist `(estimated, measured)` per model and
   converge the estimate; over time `reserve` decisions get tighter and fewer
   safety-margin GB are wasted.
6. **`-ngl` auto-tuning** from measured free VRAM (see `02`).
7. **Idle unload** — unload STT/TTS after N minutes of no voice; unload the LLM
   after N minutes of no chat *if* an image or discovery session is active.

## Phase 3 exit items for this area
- [ ] Runnable `nvml-wrapper` probe: does per-process VRAM work on driver 610.88 /
  sm_120? (record output)
- [ ] Confirm no WDDM hang on the 5080 bare-metal compute path.
- [ ] First-pass calibration numbers for the estimate constants (from the
  `02`/`03`/`04` model choices).

## Sources
- [nvml-wrapper docs](https://docs.rs/nvml-wrapper/latest/nvml_wrapper/) · [crate](https://crates.io/crates/nvml-wrapper)
- [vLLM #30707 — RTX 5080 SM120 pre-flight memory check](https://github.com/vllm-project/vllm/issues/30707)
- [Blackwell WDDM hang note (5090/PRO 6000)](https://allenkuo.medium.com/vllm-or-ollama-on-blackwell-benchmarks-landmines-and-what-agents-actually-need-5dc539bb28ef)
