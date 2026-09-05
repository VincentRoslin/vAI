# Phase 3 — Architecture Research (working notes)

Per `docs/plan/03_architecture-research.md`. These are **research writeups**, one
area per file — options, trade-offs, and a recommendation each. They are **not
decisions**; ratified decisions become ADRs in `docs/decisions/` at step 3.15 and
are frozen at Phase 5.

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
| D-7 | Model acquisition (HF, GGUF) | `06_model-acquisition.md` | pending | ADR-0008 |
| D-8 | Persistence: driver, pool, schema groups, blob store | `07_persistence.md` | pending | ADR-0009 |
| D-9 | Scheduler / GPU arbitration | `08_scheduler.md` | pending | ADR-0010 |
| D-10 | Persistent character visual identity | `09_character-identity.md` | pending | ADR-0011 |
| D-11 | Memory: schema + retrieval | `10_memory.md` | pending | ADR-0012 |
| D-12 | Worker protocol (stdio JSON-lines) | `11_worker-protocol.md` | pending | ADR-0013 |
| D-13 | Packaging (Windows, workers, native libs) | `12_packaging.md` | pending | ADR-0014 |
| D-14 | App navigation / shell (3 tabs) (ARQ-16) | `01_desktop-ipc.md` | drafted | ADR-0002 |
| D-15 | Character generation subsystem (ARQ-13/14) | `09_character-identity.md` | pending | ADR-0011 |
| D-16 | Relationship progression model (ARQ-15) | `10_memory.md` | pending | ADR-0012 |

---

## Cross-cutting finding: the 16 GB VRAM budget is the dominant constraint

From the research below, approximate loaded-VRAM footprints on this card:

| Workload | Quant | ~VRAM (weights + working set) | Fits alongside LLM? |
| -------- | ----- | ---------------------------- | ------------------- |
| LLM (llama.cpp, ~8–14B GGUF) | Q4_K_M | ~6–10 GB (model + KV cache @ 8k ctx) | — |
| STT (faster-whisper `large-v3`) | float16 (**not** int8 on Blackwell) | ~3–4 GB | usually yes |
| VAD (Silero) | — | <0.1 GB (or CPU) | yes |
| TTS (Chatterbox Turbo) | fp16 | ~1–3 GB | yes |
| Image (Krea 2 Turbo, 12B) | **NVFP4** | ~10–12 GB + LoRAs + activations | **no — requires LLM eviction** |
| Image (Krea 2 Turbo) | fp8 | ~18 GB | **does not fit at all** |

**Consequences that shape the whole architecture:**
1. Image generation (Tab 2 and character-sent images) **cannot coexist** with a
   loaded LLM → the hot-swap subsystem (Phase 23) is not optional, it is on the
   critical path for the Discovery experience and Tab 2.
2. Krea 2 Turbo must run in **NVFP4** (Blackwell-native 4-bit). fp8 is out.
3. STT must use `compute_type=float16` (CTranslate2 int8 is broken on sm_120).
4. Voice (STT+VAD+TTS ≈ 5–7 GB) **can** run with a mid-size LLM loaded — voice
   does not need a swap; image does.

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
