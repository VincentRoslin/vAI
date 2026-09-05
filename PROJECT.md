# PROJECT.md — LocalAI

**Status:** Officialized at Architecture Freeze (Phase 5), 2026-09-05.
This is the binding product definition. It is assembled from
`docs/product/requirements.md` (owner-confirmed) + the Phase 3–4 outcomes — not
re-invented. *How* it is built: `CLAUDE.md` (rules) + `ARCHITECTURE.md` (design) +
`docs/decisions/` (ADRs). *What is done and next*: `ROADMAP.md`.

Changing anything in this file, or in the architecture that realises it, requires
the STOP → explain → propose → approve → record loop (`CLAUDE.md`).

---

## 1. What LocalAI is

A **Windows-first, local-first, single-user AI desktop application**. It runs and
orchestrates local AI models — LLM chat and voice, image generation, and
persistent AI characters — with no dependence on cloud services during normal
operation. After models are acquired it works fully offline.

The product's centre of gravity is **persistence**: conversations, characters,
memories, relationships, and images are durable and connected — not disposable.

## 2. Structure — three tabs

| Tab | Purpose | AI entity | Images |
| --- | ------- | --------- | ------ |
| **1. Chat / Voice** | Text and voice conversation with a local LLM | **Persona** (behaviour config) | No |
| **2. Image Generator** | Standalone local image generation | — | Yes |
| **3. Discovery** | Swipe-discover, keep, and converse with persistent **Characters** | **Character** (persistent entity) | Yes (character-sent, gallery) |

Plus **Models** and **Settings** surfaces. **Persona ≠ Character** — distinct
concepts. One shared image-generation subsystem serves Tabs 2 and 3; their
galleries stay separate.

## 3. Functional scope (binding)

The full numbered requirement set is `docs/product/requirements.md` §1–2
(`FR-*`, `FR-C-*`). Summary of what LocalAI **does**:

- **Chat (Tab 1):** immediate local LLM chat; token streaming; stop; automatic
  persistence; reopen/continue; select the LLM (switchable mid-conversation,
  context re-truncated); optional Persona (fixed once a conversation starts). No
  images in this tab.
- **Voice (Tab 1):** speak → understand → respond → speak, no manual handoff;
  automatic transcription + endpointing; streamed spoken reply; **interrupt by
  speaking**; one continuous history with text; listening/thinking/speaking states.
- **Personas:** structured data (personality, tone, style, tendencies) that
  measurably changes model behaviour; user-created/edited; verifiable that the
  persona reaches the model; text/voice only.
- **Conversation history & memory:** all conversations listed and reopenable;
  durable; selective cross-conversation memory (**per-Persona** in Tab 1,
  **per-Character** in Tab 3); view + delete memories in v1, edit later.
- **Models — management:** see installed models + capability + status + resource
  need; select; load/unload where supported; plain-language "can't load" reasons;
  the user never launches backend processes.
- **Models — acquisition:** in-app HuggingFace browse + one-shot **resumable**
  download for LLM GGUFs; integrity-verified; disk-budget-guarded;
  auto-registered; cancel/pause/resume/delete. The single STT (faster-whisper) and
  TTS (Chatterbox) models are acquired once via the same path, no picker. All
  models fully usable offline after acquisition.
- **Image Generator (Tab 2):** dedicated surface for the shared image subsystem;
  immediate queued/generating state, progress, cancellation, graceful failure;
  completed images go to Tab 2's own gallery; **generation presets** and **LoRA**
  support (single-select in v1); participates in resource management.
- **GPU / resource management:** the app coordinates GPU workloads; the user never
  reasons about CUDA/VRAM; an image request that needs the GPU makes a managed
  decision (evict the LLM, generate, restore) rather than failing; workload
  switching shows clear state.
- **Characters (Tab 3):** persistent entities with a stable id surviving restart,
  new conversations, new images, relationship changes, model changes. Structured
  identity, appearance, personality, interests, history, memory, relationship
  state, reference images, generated images. Opening a character returns the
  *same* character.
- **Character discovery:** swipe cards (profile + images), pass or keep; a kept
  character is saved and persistent; choosing opens a conversation. The system
  generates characters (LLM profile + reference images) without the user authoring
  each; manual authoring is a later phase.
- **Character conversations:** run on the shared conversation engine with the
  character's identity/personality/memory/relationship in context; text-only in
  v1; important info becomes character memory; relationship state evolves.
- **Relationship progression:** persistent **discrete stages** (Stranger →
  Acquaintance → Friend → Close Friend → Romantic Interest → Partner, tunable)
  with **behavioural consequences** and gating of what character images may be
  sent — not a bare number.
- **Character-sent images (Tab 3 only):** at a natural point a character sends an
  image via a **typed, validated action** the app checks and executes; the image
  is identity-consistent-checked and saved to that character's gallery. Personas
  (Tab 1) have no image capability.
- **Character visual identity:** target is **recognisable continuity** — a viewer
  concludes "same character". **v1 is prompt-based** (multi-view reference sheet +
  an LLM-captioned canonical appearance block + fixed seed + realism LoRA + a
  face-embedding similarity gate). It holds for **portraits and selfies** (the
  expected image range); it will drift under large pose/scene changes. Pixel-perfect
  identity is not promised. Revisit if the image model gains reference
  conditioning.

## 4. Non-functional scope (binding)

Full set: `docs/product/requirements.md` §3 (`NFR-*`). Load-bearing points:

- **Local-first / offline:** after acquisition, every core feature works with no
  network. The only permitted network operations are optional, explicit, and
  skippable (model/dependency/update download). The app is never a non-functional
  shell offline.
- **Privacy:** no silent upload of anything; no mandatory moderation service; no
  telemetry or analytics — including from bundled Python libraries (ADR-0015).
- **Content:** no censoring and **no content controls of any kind**. NSFW/explicit
  is allowed; moderation is entirely the user's responsibility. The architecture
  neither depends on nor contains content-filtering infrastructure.
- **Performance (experience targets, benchmarked not guaranteed):** UI responsive
  immediately; generation begins ~1–2 s once the model is ready; streaming as soon
  as usable output exists; UI never appears frozen; voice reply audible in ~1–3 s;
  interruption feels immediate; long image generations never look broken;
  load/unload/swap always shows clear state. Real numbers are measured per
  hardware+model and recorded (`PERFORMANCE.md`).
- **Reliability:** backend/worker failures produce predictable, recoverable
  behaviour — never a crash, hang, orphan process, or silent data loss.
  Persistence survives restart and unclean shutdown.
- **Security:** model/character output is untrusted data; it never becomes a
  command, filesystem op, process launch, or DB command; every AI-triggered effect
  is a typed, validated application action. Local services are ACL/token-scoped
  loopback only; filesystem access is confined; no secrets in logs or the frontend
  bundle.
- **Architecture:** one conversation engine, one model-lifecycle authority, one
  resource manager, one DB layer, one config system — no competing authorities.
  The frontend is presentation only. LocalAI orchestrates proven local runtimes
  (llama.cpp, faster-whisper, Chatterbox, Krea 2 Turbo); it implements none of
  them.
- **Platform:** Windows-first desktop; single local user; not designed around
  accounts, orgs, sync, or multi-user.

## 5. Non-goals

- Cloud/SaaS: hosted inference, cloud databases, mandatory accounts, remote AI
  APIs.
- A general AI agent with unrestricted computer control.
- Browser-first delivery.
- Multi-user / accounts / sync / tenancy.
- Building our own LLM runtime, CUDA backend, or image/STT/TTS engine.
- Mathematically perfect identity preservation in generated images.
- Any mandatory external content-moderation service.

## 6. Confirmed technology (frozen — see ADRs)

Tauri v2 (single Rust crate) + React/TypeScript · Rust authoritative core ·
`llama-server` (GGUF, CUDA, built from source) · diffusers image sidecar running
**Krea 2 Turbo** (bitsandbytes NF4, cached) · **faster-whisper** (fp16) +
**Silero VAD** + **Chatterbox** TTS · **SQLite** (rusqlite) + content-addressed
blob store · NVIDIA CUDA. Subprocess transport: named pipe / token'd loopback for
model servers, stdio JSON-lines for small workers.

## 7. Reference machine

Ryzen 7 9800X3D · **RTX 5080 16 GB** (Blackwell sm_120) · 32 GB RAM · ~215 GB free
NVMe · Windows 11. The 16 GB VRAM budget is the dominant design constraint:
**image generation and the LLM cannot be resident together** — every image
generation evicts the LLM and restores it after.
