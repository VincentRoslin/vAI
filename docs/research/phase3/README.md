# Phase 3 — Architecture Research (working notes)

Per `docs/plan/03_architecture-research.md`. These are **research writeups**, one
area per file — options, trade-offs, and a recommendation each. They are **not
decisions**; ratified decisions become ADRs in `docs/decisions/` at step 3.15 and
are frozen at Phase 5.

**Every area file carries an "Optimizations" section** (owner standing
instruction — always look for the better option; performance + resource
efficiency are first-class). Each optimization is a candidate to validate/measure
in its implementation phase, not a commitment.

Requirements traced: `docs/product/requirements.md` (`FR-*`, `NFR-*`, `ARQ-*`).
Machine: `docs/verification/01_env_audit.md` (Ryzen 9800X3D, RTX 5080 **16 GB**,
32 GB RAM, single NVMe ~215 GB free, Win 11, CUDA runtime 13.3, driver 610.88,
Blackwell **sm_120**).

---

## 3.1 — Decision table

| ID | Decision | File | Status | ADR |
| -- | -------- | ---- | ------ | --- |
| D-1 | Desktop shell + Rust crate structure (O4) | `01_desktop-ipc.md` | drafted | ADR-0001 |
| D-2 | IPC design + typed-contract generation | `01_desktop-ipc.md` | drafted | ADR-0002 |
| D-3 | LLM runtime: integration mode + CUDA build (O3, O5) | `02_llm-runtime.md` | drafted | ADR-0003, ADR-0004 |
| D-4 | Voice pipeline: STT / VAD / TTS / audio I/O | `03_voice.md` | drafted | ADR-0005 |
| D-5 | Image-generation subsystem (Krea 2 Turbo + LoRAs) | `04_image-generation.md` | drafted | ADR-0006 |
| D-6 | Resource manager + VRAM accounting | `05_resource-vram.md` | drafted | ADR-0007 |
| D-7 | Model acquisition (HF, GGUF) | `06_model-acquisition.md` | drafted | ADR-0008 |
| D-8 | Persistence: driver, pool, schema groups, blob store | `07_persistence.md` | drafted | ADR-0009 |
| D-9 | Scheduler / GPU arbitration | `08_scheduler.md` | drafted | ADR-0010 |
| D-10 | Persistent character visual identity | `09_character-identity.md` | drafted | ADR-0011 |
| D-11 | Memory: schema + retrieval | `10_memory.md` | drafted | ADR-0012 |
| D-12 | Subprocess transport: loopback HTTP for model *servers* (LLM, image), stdio JSON-lines for small workers (STT/TTS) | `11_worker-protocol.md` | drafted | ADR-0013 |
| D-13 | Packaging (Windows, workers, native libs) | `12_packaging.md` | drafted | ADR-0014 |
| D-14 | App navigation / shell (3 tabs) (ARQ-16) | `01_desktop-ipc.md` | drafted | ADR-0002 |
| D-15 | Character generation subsystem (ARQ-13/14) | `09_character-identity.md` | drafted | ADR-0011 |
| D-16 | Relationship progression model (ARQ-15) | `10_memory.md` | drafted | ADR-0012 |

**All 12 area files drafted. 14 draft ADRs in `docs/decisions/` (ADR-0001..0014,
all `PROPOSED`).**

### Remaining Phase 3 exit items (before the gate passes)
1. **Runnable `nvml-wrapper` probe** — does per-process VRAM work on driver
   610.88 / sm_120? (throwaway Rust bin, like the env-audit link test)
2. **GGUF header parse** verified against a real `.gguf` file's first ~1–2 MB.
3. **Confirm no WDDM hang** on the 5080 bare-metal compute path (spawn
   `llama-server`, hold VRAM, observe).
4. **Owner sign-off on ADR-0011** — per-character LoRA (option B) cost/feasibility,
   or scope FR-C90 to option-A quality for v1.
5. Phase 4 (adversarial review) then Phase 5 (ratify ADRs, freeze).

---

## Cross-cutting finding: the 16 GB VRAM budget is the dominant constraint

From the research below, approximate loaded-VRAM footprints on this card:

| Workload | Quant | ~VRAM (weights + working set) | Fits alongside LLM? |
| -------- | ----- | ---------------------------- | ------------------- |
| LLM (llama.cpp, ~8–14B GGUF) | Q4_K_M | ~6–10 GB (model + KV cache @ 8k ctx) | — |
| STT (faster-whisper `large-v3`) | float16 (**not** int8 on Blackwell) | ~3–4 GB | usually yes |
| VAD (Silero) | — | <0.1 GB (or CPU) | yes |
| TTS (Chatterbox Turbo) | fp16 | ~1–3 GB | yes |
| Image (Krea 2 Turbo, 12B) | bitsandbytes **NF4** + `enable_model_cpu_offload` | **~11.4 GB peak** during generation, **~1.6 GB at rest** (measured — owner's impl) | **no — requires LLM + TTS eviction** |
| Image (Krea 2 Turbo) | fp8 / bf16 full | ~18–24 GB | **does not fit** |

**Consequences that shape the whole architecture:**
1. Image generation (Tab 2 and character-sent images) **cannot coexist** with a
   loaded LLM → the hot-swap subsystem is **on the critical path** for Discovery
   and Tab 2 (not an optimization). The owner's existing impl already does this
   (`exclusive_vram` unloads LLM + TTS first).
2. Krea 2 Turbo runs quantized: owner uses **bitsandbytes NF4 + CPU offload**
   (~11.4 GB peak, ~1.6 GB idle, ~17–18 s/image, but ~90 s to (re)load).
   Optimization to evaluate: **cache the NF4-quantized weights** (kill the ~90 s
   re-quant) and/or **torchao NVFP4** (Blackwell-native, faster). See `04`.
3. Because idle VRAM is only ~1.6 GB, the image sidecar can stay **alive**
   between generations; only the generation itself needs the LLM evicted.
4. STT must use `compute_type=float16` (CTranslate2 int8 is broken on sm_120).
5. Voice (STT+VAD+TTS ≈ 5–7 GB) **can** run with a mid-size LLM loaded — voice
   does not need a swap; image does.
6. **Biggest open risk:** character visual identity (FR-C90..94). Krea 2 in
   diffusers is text-to-image only; the owner's impl has no reference/identity
   mechanism. "High visual continuity" likely needs **per-character LoRAs**
   (trained locally). See `04` §identity.

---

## Sources (this phase)

- Krea 2 / Krea 2 Turbo: [unsloth/Krea-2-Turbo](https://huggingface.co/unsloth/Krea-2-Turbo),
  [Krea 2 VRAM — WillItRunAI](https://willitrunai.com/image-models/krea-2),
  [Krea 2 open-source review](https://www.buildfastwithai.com/blogs/krea-2-open-source-review-raw-turbo)
- Blackwell diffusion quant: [PyTorch: MXFP8/NVFP4 with Diffusers + TorchAO](https://pytorch.org/blog/faster-diffusion-on-blackwell-mxfp8-and-nvfp4-with-diffusers-and-torchao/),
  [sayakpaul/diffusers-blackwell-quants](https://github.com/sayakpaul/diffusers-blackwell-quants)
- faster-whisper on Blackwell: [faster-whisper #1431 (CUDA 13)](https://github.com/SYSTRAN/faster-whisper/issues/1431),
  [SubtitleEdit #10180 (sm_120 cuBLAS)](https://github.com/SubtitleEdit/subtitleedit/issues/10180)
- Chatterbox: [Chatterbox Turbo — Resemble](https://www.resemble.ai/learn/models/chatterbox-turbo),
  [chatterbox-streaming](https://github.com/davidbrowne17/chatterbox-streaming),
  [latency issue #193](https://github.com/resemble-ai/chatterbox/issues/193)
- llama.cpp Blackwell: [llama.cpp #22696](https://github.com/ggml-org/llama.cpp/issues/22696),
  [sm_120 Windows prebuilt](https://github.com/Andgihat/llama-cpp-mtp-turboquant-sm120-blackwell-windows)
- Tauri v2 IPC: [Calling Rust from the Frontend](https://v2.tauri.app/develop/calling-rust/),
  [IPC concept](https://v2.tauri.app/concept/inter-process-communication/)
- tauri-specta: [releases](https://github.com/specta-rs/tauri-specta/releases) (2.0.0-rc.25, May 2026)
- hf-hub: [huggingface/hf-hub](https://github.com/huggingface/hf-hub)
- nvml-wrapper: [docs.rs/nvml-wrapper](https://docs.rs/nvml-wrapper/latest/nvml_wrapper/)
