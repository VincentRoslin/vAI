# LocalAI — Product Requirements

**Status:** CONFIRMED by owner 2026-09-05 (Product Definition step complete).
Open points A1–A11 resolved (§4). Two details deferred to their implementation
phase by owner request: Image Generator preset/LoRA specifics → Phase 22; exact
relationship-stage set → ARQ-15 / Phase 3.
**Source:** `docs/product/vision.md` (owner draft, 2026-09-05) + owner clarifications
in chat 2026-09-05.
**Rule:** This document captures *what* and *the experience*. It does **not**
choose technology or design architecture — that is Phase 3. Items needing a
decision are in §4 (Ambiguities) and §5 (Architecture Research Questions).

Legend: **FR** = functional requirement · **NFR** = non-functional requirement ·
**FR-C** = character-system functional requirement. Each is one testable
statement. "Eventually" in the vision = in scope for the product, not necessarily
the first slice (sequencing is `ROADMAP.md`).

---

## 0. Product Structure (owner clarification 2026-09-05)

The application has **three primary tabs, kept separate**, plus supporting
surfaces (Models, Settings):

| Tab | Purpose | AI entity | Images? |
| --- | ------- | --------- | ------- |
| **1. Chat / Voice** | Text and voice conversation with a local LLM | **Persona** (behaviour/personality config) | **No** |
| **2. Image Generator** | Standalone local image generation | — | Yes (this is its job) |
| **3. Discovery (Tinder-style)** | Swipe/discover, keep, and converse with persistent AI **Characters** | **Character** (persistent entity: identity, appearance, personality, memory, relationship, gallery) | Yes (character-sent images, character gallery) |

- **Persona** and **Character** are different concepts. Personas live only in Tab 1
  and never generate or send images. Characters live only in Tab 3.
- Character conversations, character memory, relationship progression, character
  galleries, and character-sent images are **all within Tab 3**.
- **One shared image-generation subsystem** serves both Tab 2 (standalone) and
  Tab 3 (character profile images + character-sent images). The tabs and their
  galleries stay separate; the underlying generation engine is the same
  (FR-90, FR-C82, ARQ-9).

---

## 1. Functional Requirements

### 1.1 Application shell & first run
- **FR-1** The application is a Windows desktop app that launches to a usable main
  interface without the user configuring AI infrastructure.
- **FR-2** On first launch the app starts its local services, checks available
  models and resources, and shows the user what is and isn't ready.
- **FR-3** If no usable LLM is present, the app guides the user to acquire one
  (FR-70) rather than silently failing. With no model downloaded, the UI and
  Settings remain usable; generation features are unavailable until a model
  exists, with a clear explanation.
- **FR-4** The main interface has three primary tabs — **Chat/Voice**,
  **Image Generator**, **Discovery** — plus Models and Settings surfaces.
- **FR-5** Errors are presented as understandable, recoverable messages, never as
  raw backend/process failures.

### 1.2 Tab 1 — Chat / Voice (text)
- **FR-10** The user can start a text conversation with a local LLM immediately.
- **FR-11** Assistant responses stream token-by-token into the UI.
- **FR-12** The UI stays responsive during generation (no freeze).
- **FR-13** The user can stop generation mid-response; the partial response is
  kept.
- **FR-14** Conversations are persisted automatically, with no explicit "save".
- **FR-15** The user can reopen any previous conversation and continue it.
- **FR-16** The user can select which LLM a conversation uses, where multiple are
  available. Switching the LLM mid-conversation is allowed; prior context is
  carried over, truncated to the new model's context window.
- **FR-17** A Persona is optional for a Tab 1 conversation (it works with a default
  assistant). Once a conversation has started, its Persona is fixed.
- **FR-18** Text chat runs on the single shared conversation engine — not a
  separate isolated chat system. The same engine serves voice (FR-20) and
  character conversations (FR-C70).
- **FR-19** This tab never generates or displays generated images.

### 1.3 Tab 1 — Chat / Voice (voice modality)
- **FR-20** In the same tab, the user can hold a spoken conversation: speak → the
  AI understands → the AI responds → the AI speaks, with no manual handoff between
  STT, LLM, and TTS.
- **FR-21** The microphone is captured and speech transcribed automatically;
  end-of-utterance is detected without a manual "stop".
- **FR-22** The spoken reply begins as soon as usable output exists (streamed TTS
  where possible).
- **FR-23** The user can interrupt the assistant by speaking (or an explicit
  control); the assistant stops speaking promptly and listens.
- **FR-24** An interrupted assistant turn is preserved in history as truncated,
  with what was actually spoken.
- **FR-25** A voice exchange is part of the same conversation record as text — one
  continuous history regardless of modality.
- **FR-26** The UI clearly shows the current state: listening / thinking /
  speaking.

### 1.4 Personas (Tab 1)
- **FR-30** The user can select a Persona for a conversation.
- **FR-31** The user can create and edit Personas.
- **FR-32** A Persona is structured data (personality, tone, communication style,
  preferences, behavioural tendencies), not free prose only.
- **FR-33** A Persona measurably changes the model's behaviour — vocabulary, tone,
  humour, reactions — not just displayed profile text.
- **FR-34** The app provides a way to verify that the active Persona's content
  actually reached the model for a given generation.
- **FR-35** Personas are text/voice only and have **no image capability**.

### 1.5 Conversation history (Tab 1 + Tab 3)
- **FR-40** All conversations (persona chat/voice, character) are listed and
  reopenable within their own tab.
- **FR-41** Reopening a conversation restores its full message history in order.
- **FR-42** Conversation history persists across app restarts.

### 1.6 Memory
- **FR-50** The AI can retain relevant information across conversations so the user
  does not have to repeat important facts.
- **FR-51** Memory retention is selective — useful/important information, not every
  sentence stored indefinitely.
- **FR-52** The user can view what is remembered.
- **FR-53** The user can delete a memory.
- **FR-54** The user can correct/edit a memory. In scope, but a later phase — view
  + delete come first.
- **FR-55** Memory maintains continuity without injecting entire historical
  conversations into every request.
- **FR-56** Tab 1 cross-conversation memory is **per-Persona** (each Persona has
  its own memory store). Character memory is character-scoped (FR-C32).

### 1.7 Models — management
- **FR-60** The user can see installed models and, for each, its capability
  (LLM / STT / TTS / image), status, and resource requirement.
- **FR-61** The user can select a model for a task.
- **FR-62** The user can load and unload models where the backend supports it;
  model status (loading / unloading / preparing / ready / failed) is visible.
- **FR-63** When a model cannot be loaded (resources or configuration), the app
  explains why in plain terms.
- **FR-64** The user never manually launches or kills backend processes.

### 1.8 Models — acquisition
- **FR-70** From inside the app the user can browse and download LLM models
  (GGUF) from HuggingFace.
- **FR-71** A download is resumable and survives an app restart.
- **FR-72** A downloaded file is integrity-verified; a corrupt download is
  rejected and not registered.
- **FR-73** Before a download starts, the app checks free disk / model-storage
  budget and refuses clearly if it won't fit.
- **FR-74** A completed download is automatically registered and usable.
- **FR-75** The user can cancel, pause, resume, and delete downloads/models.
- **FR-76** The single STT model (faster-whisper) and single TTS model
  (Chatterbox) are acquired once through the same download/verify path — no model
  picker for these.
- **FR-77** After acquisition, all models are fully usable with no network.

### 1.9 GPU / resource management & hot-swapping
- **FR-80** The app coordinates AI GPU workloads so the user never has to reason
  about CUDA processes or VRAM.
- **FR-81** If an image request (Tab 2 or a character-sent image in Tab 3) needs
  GPU resources currently held by the LLM, the app makes a managed decision (keep
  both if they fit, else suspend/unload the LLM, generate, then restore) — it does
  not crash with CUDA OOM.
- **FR-82** Model/workload switching is presented as an app feature with clear
  state feedback, not a technical operation the user drives.
- **FR-83** A character sending an image (FR-C80) triggers hot-swap transparently —
  the user experiences one continuous conversation.

### 1.10 Tab 2 — Image Generator (standalone)
- **FR-90** The user can generate an image locally from a dedicated Image
  Generator tab. Tab 2 is the user-facing surface for the **shared
  image-generation subsystem** (also used by Tab 3 for character images).
- **FR-91** Image generation shows an immediate queued/generating state, progress
  where available, cancellation, and graceful failure.
- **FR-92** A completed image appears automatically and is saved to the Image
  Generator's own gallery, separate from character galleries.
- **FR-93** Image generation participates in resource management (FR-80/81), never
  blindly competing for VRAM.
- **FR-94** The UI does not appear broken during a 10–30s+ generation.
- **FR-95** Tab 2 provides **generation presets** (reusable parameter sets) the
  user can save, select, and manage.
- **FR-96** Tab 2 supports **LoRAs**: the user can add LoRA models and apply one
  or more to a generation, with adjustable weight. *(Exact UX + capabilities to be
  detailed from the owner's other project at Phase 22 — ARQ-9.)*
- **FR-97** The Tab 3 character-image pipeline (FR-C82) uses this same subsystem —
  the same backend, presets, and LoRA mechanism are available to it.

---

## 2. Character System Requirements (FR-C) — Tab 3 (Discovery)

All character functionality lives in the Discovery tab.

### 2.1 Identity & persistence
- **FR-C1** A character is a persistent entity with a stable identifier, not a
  prompt.
- **FR-C2** A character's identity survives app restart, new/different
  conversations, new generated images, relationship changes, and model changes.
- **FR-C3** Opening a character later returns the user to the *same* character —
  same identity, appearance, personality, history, memory, relationship, gallery.
- **FR-C4** A character has: identity, structured appearance, personality,
  interests, history, memory, relationship state, reference images, generated
  images.

### 2.2 Appearance
- **FR-C10** Appearance is structured data (face, hair, eyes, skin, body,
  height/build, distinctive features, clothing/style, accessories, other visual
  traits) — exact schema is ARQ-10.
- **FR-C11** Structured appearance is usable as input to image generation for
  consistency, not just displayed.

### 2.3 Personality
- **FR-C20** Personality is structured and influences actual conversation
  behaviour: vocabulary, tone, humour, emotional responses, interests,
  conversational habits, preferences, reactions to the user.
- **FR-C21** A character must not read as the same generic model with a different
  profile.

### 2.4 Character memory
- **FR-C30** Character memory is associated with the user↔character relationship.
- **FR-C31** Things the user tells a character remain available in later
  conversations with that character.
- **FR-C32** Character memory retrieval is scoped to that character.
- **FR-C33** Character memory provides continuity without replaying the entire
  conversation history each request.

### 2.5 Relationship progression
- **FR-C40** A character has persistent relationship state that develops based on
  interactions and persists across sessions.
- **FR-C41** Relationship progresses through **discrete stages** (e.g.
  Stranger → Acquaintance → Friend → Close Friend → Romantic Interest → Partner —
  exact set is ARQ-15). Each stage has **behavioural consequences** — it is not a
  bare visible number.
- **FR-C42** The character "remembers how the relationship developed."
- **FR-C43** The current relationship stage influences both conversation behaviour
  and character-image generation (what the character will send). *(→ ARQ-15.)*

### 2.6 Galleries
- **FR-C50** Each character has a persistent gallery; images are associated with
  that specific character, not a loose folder.
- **FR-C51** The gallery may contain profile images, portraits, photos, scene
  images, and character-sent images.
- **FR-C52** Gallery images can serve as reference material for future generation
  for that character.

### 2.7 Discovery / swipe
- **FR-C60** The Discovery surface presents characters one at a time as
  cards/profiles with images.
- **FR-C61** The user can inspect a character's profile and images, then swipe
  (pass) or choose (keep).
- **FR-C62** A chosen character is saved to the user's collection and becomes
  persistent — it does not regenerate into a different entity next launch.
- **FR-C63** Choosing a character opens its profile and lets the user start a
  conversation with it.
- **FR-C64** The system generates characters without the user authoring each: an
  LLM writes the profile, the shared image subsystem (FR-90) generates reference
  images. A small pool is pre-generated and topped up in the background. A quality
  gate keeps incomplete/broken characters out of the feed. *(Exact approach →
  ARQ-13/14.)*
- **FR-C65** The user can also author a character manually with full control over
  appearance and personality — a **later phase**; discovery-generated characters
  come first.

### 2.8 Character conversations
- **FR-C70** A character conversation runs on the shared conversation engine, with
  the character's identity, personality, relevant character memory, relationship
  state, and history assembled into context.
- **FR-C71** Character conversations are **text-only for v1**. Voice with a
  character is a later addition.
- **FR-C72** During a character conversation, important information may become
  character memory (FR-C30) and relationship state may evolve (FR-C40).

### 2.9 Character-sent images (Discovery only)
- **FR-C80** In a character conversation, a character can, at a natural point,
  send an image representing what they are saying/doing.
- **FR-C81** The character never issues raw image-generation commands. It emits a
  **structured, typed image action**; the app validates it (schema, allow-list,
  correct character) and decides whether/how to run it.
- **FR-C82** Using the shared image-generation subsystem (FR-90), the pipeline
  loads the character's identity/reference, generates, runs an
  identity-consistency check, and on acceptance delivers the image and saves it to
  that character's gallery; on failure it regenerates (bounded) or fails
  gracefully.
- **FR-C83** Character-sent images exist **only** in the Discovery tab.
  Personas (Tab 1) have no image capability.

### 2.10 Persistent visual identity (major requirement)
- **FR-C90** Target: **high visual continuity** — a viewer looking at multiple
  images of a character reasonably concludes "that is the same character."
- **FR-C91** Recognisable across images: face, hair, general physique,
  distinctive features, overall visual identity — across different poses,
  clothing, environments, lighting, scenes.
- **FR-C92** The system uses references + conditioning + verification; it does not
  assume identical prompts produce identical people.
- **FR-C93** Obvious identity failures are detected and the image is
  rejected/regenerated.
- **FR-C94** Pixel-perfect identity is explicitly not promised (see Non-goals).

---

## 3. Non-Functional Requirements

### 3.1 Local-first & offline
- **NFR-1** After required models/assets are acquired, all core functionality
  works with no network: launch, UI, chat, voice, LLM inference, history,
  personas, characters + their conversations/memory/relationship/galleries,
  standalone image generation, character image generation, STT, TTS, model
  management, resource management, hot-swap, local DB, local files, settings.
- **NFR-2** The only permitted network operations are optional and explicit:
  downloading models, downloading app updates, obtaining optional external
  resources the user explicitly chooses.
- **NFR-3** No permitted network operation may become a dependency of normal local
  operation. The app is never a non-functional shell offline.

### 3.2 Privacy
- **NFR-10** The app never silently uploads conversations, character data,
  memories, images, audio, or prompts to any third party.
- **NFR-11** No mandatory external moderation service; core conversations and
  generations are not sent to a cloud moderation API.
- **NFR-12** No telemetry or analytics.

### 3.3 Content
- **NFR-15** No censoring, and **no content controls of any kind** — no filter, no
  age-of-appearance logic, no content warnings. NSFW/explicit content is allowed;
  moderation is entirely the user's responsibility. The architecture must not
  depend on, or include, content-filtering infrastructure.

### 3.4 Performance (experience targets — validated later, not guarantees)
- **NFR-20** The interface feels responsive immediately; the user's message
  appears essentially instantly.
- **NFR-21** Once the model is ready, generation begins in roughly 1–2 s;
  streaming starts as soon as the backend produces usable output.
- **NFR-22** The UI never appears frozen during generation.
- **NFR-23** Voice: STT starts promptly after the user stops speaking; the user
  hears the start of the reply in roughly 1–3 s (hardware-dependent).
- **NFR-24** Voice interruption feels immediate.
- **NFR-25** Image generation gives immediate "started" feedback and progress
  where available; long generations (10–30s+) never look broken.
- **NFR-26** Model load/unload/swap always shows a clear state; the user never
  wonders if the app froze.
- **NFR-27** Actual TPS / latency / generation-time numbers are benchmarked on
  real hardware+model combinations and recorded — not fixed universal targets.

### 3.5 Reliability
- **NFR-30** Backend and worker failures (LLM crash, worker crash, missing/corrupt
  model, corrupt config/DB, VRAM exhaustion, cancellation, shutdown mid-generation,
  worker timeout, unexpected process death) produce predictable, recoverable
  behaviour — never a crash, hang, orphan process, or silent data loss.
- **NFR-31** Persistence is durable: conversations, personas, characters, memory,
  relationship state, galleries, settings survive restart and unclean shutdown.

### 3.6 Resource-awareness
- **NFR-35** The app tracks estimated / reserved / observed GPU usage separately
  and never treats an estimate as a measurement.
- **NFR-36** Only one authority manages GPU/VRAM; all AI GPU consumers go through
  it.

### 3.7 Security / untrusted AI output
- **NFR-40** Model and character output is untrusted data. It never becomes a
  shell command, executable, arbitrary filesystem operation, process launch,
  database command, or unrestricted application action.
- **NFR-41** Every action an AI can trigger is an explicitly defined, typed,
  validated application action.
- **NFR-42** Local services bind to loopback only; no secrets in logs or the
  frontend bundle; filesystem access is confined and path-traversal-safe.

### 3.8 Architecture & maintainability
- **NFR-50** One conversation engine, one model-lifecycle authority, one resource
  manager, one database access layer, one config system — no competing
  authorities.
- **NFR-51** The frontend is presentation only; it never accesses the DB, AI
  processes, the filesystem, or the network directly.
- **NFR-52** The app orchestrates proven local runtimes (llama.cpp, faster-whisper,
  Chatterbox, FLUX.1 Krea); it does not implement its own LLM runtime, CUDA
  backend, or STT/TTS/image engines.
- **NFR-53** Development is incremental; the first vertical slice is text chat
  end-to-end (UI → IPC → Rust → conversation engine → local LLM → streaming → UI),
  then capabilities are added systematically.

### 3.9 Platform
- **NFR-60** Windows-first desktop application. A web UI may exist later for other
  purposes but is not the core product.
- **NFR-61** Single local user. The core architecture is not designed around
  accounts, organisations, cloud sync, tenancy, or multi-user permissions.

---

## 4. Resolved Decisions (owner-confirmed 2026-09-05)

All Product-Definition open points are resolved. Folded into the FR/NFR above;
recorded here for traceability.

| # | Decision | Where |
| --- | -------- | ----- |
| Structure | Three separate tabs — Chat/Voice, Image Generator, Discovery — + Models + Settings | §0, FR-4 |
| Persona vs Character | Distinct concepts. Persona = Tab 1, text/voice, no images. Character = Tab 3, full persistent entity. | §0 |
| A1 | Persona optional per Tab 1 conversation (works with default assistant); fixed once the conversation starts | FR-17 |
| A2 | Memory *edit/correct* is in scope but a later phase; view + delete first | FR-52–54 |
| A3 | Switching the LLM mid-conversation is allowed; context carried over, truncated to the new window | FR-16 |
| A4 | Tab 1 memory is **per-Persona** | FR-56 |
| A5 | **One shared image-generation subsystem**; Tab 2 and Tab 3 keep separate galleries; Tab 2 is not character-coupled but characters use the same engine to make their images | §0, FR-90, FR-97, FR-C82 |
| A5b | Tab 2 has **presets** and **LoRA** support (detail from owner's other project at Phase 22) | FR-95, FR-96 |
| A6 | Relationship = **discrete stages** with behavioural + image-generation consequences | FR-C41, FR-C43 |
| A7 | Discovery characters: LLM-authored profile + shared image subsystem for reference images; small pre-generated pool topped up in background; quality gate | FR-C64 |
| A8 | Manual character authoring: **later phase**, discovery-generated first | FR-C65 |
| A9 | Character conversations **text-only for v1**; voice with characters later | FR-C71 |
| A10 | **No content controls of any kind** — fully user-moderated | NFR-15 |
| A11 | First run with no model: UI + Settings usable; generation unavailable until a model exists | FR-3 |

---

## 5. Architecture Research Questions (ARQ) — for Phase 3

Dated 2026-09-05. Each maps to FR/NFR items and to a
`docs/plan/03_architecture-research.md` step.

- **ARQ-1** LLM runtime: llama.cpp integration mode (supervised `llama-server`
  over loopback HTTP vs FFI), streaming, cancellation reaching the job, parallel
  slots, CUDA 13 / Blackwell sm_120 build strategy. *(FR-10..13, FR-64, NFR-21,
  NFR-40; O3, O5.)*
- **ARQ-2** One conversation engine serving Tab 1 text, Tab 1 voice, and Tab 3
  character conversations — shape, message/content model, streaming + cancellation
  first-class, persistence. *(FR-18, FR-25, FR-C70; NFR-50.)*
- **ARQ-3** Voice pipeline: faster-whisper (model size/quant for 16 GB, streaming
  vs batch), Silero VAD (endpointing), Chatterbox (latency, clause chunking for
  barge-in), audio I/O, echo handling; end-to-end latency budget; coexistence with
  a loaded LLM. *(FR-20..26, NFR-23/24.)*
- **ARQ-4** Persona + context builder: structured Persona → model context, and a
  verifiable way to confirm Persona content reached the model. *(FR-32..34.)*
- **ARQ-5** Memory: extraction, importance/validation, storage, retrieval (FTS5
  keyword first; embeddings only on measured need); scoping is decided
  (per-Persona for Tab 1, per-Character for Tab 3) — research the schema + retrieval
  + the later edit/correct capability. *(FR-50..56, FR-C30..33; O1.)*
- **ARQ-6** Model registry + HuggingFace GGUF acquisition: search, GGUF metadata,
  resumable download, checksum source, disk budget, registration. *(FR-60,
  FR-70..77.)*
- **ARQ-7** Resource manager: VRAM estimation, `nvml` per-process availability on
  this driver (env audit showed N/A), reservation ledger, reconcile. *(FR-80,
  NFR-35/36.)*
- **ARQ-8** Model hot-swap + scheduler: eviction policy, swap without interrupting
  in-flight generation, VRAM-settle wait, arbitration of LLM vs image (Tab 2 and
  Tab 3) vs STT/TTS. *(FR-81..83, FR-C82.)*
- **ARQ-9** Image generation subsystem (one engine, serving Tab 2 + character
  images): adapt the owner's existing FLUX.1 Krea implementation (**request it**),
  worker boundary, resource-manager integration, storage, cancellation, **preset
  model**, **LoRA loading + weighting**. *(FR-90..97, FR-C80..82.)*
- **ARQ-10** Character data model / schema: identity, structured appearance,
  personality, interests, history, memory links, relationship state, image links.
  *(FR-C1..4, FR-C10, FR-C20, FR-C50.)*
- **ARQ-11** Persistent visual identity: reference-image conditioning for FLUX.1
  Krea, identity-similarity check (embedding + threshold) for accept-vs-regenerate,
  bounded retry. *(FR-C90..94.)*
- **ARQ-12** Typed image actions from the LLM: extraction mechanism, schema,
  allow-list, cross-character refusal, audit logging. *(FR-C81, NFR-40/41.)*
- **ARQ-13** Character generation subsystem: LLM-authored profiles + reference
  images via the shared image subsystem; pre-generated pool + background top-up;
  quality gate; storage. *(FR-C64.)*
- **ARQ-14** Discovery feed: pool sizing + top-up trigger, pagination, image
  lazy-loading, seen/kept/passed state. *(FR-C60..64.)*
- **ARQ-15** Relationship stages: the exact stage set, the transition rules
  (what advances/regresses it), and each stage's concrete effect on (a)
  conversation behaviour and (b) what character images depict. *(FR-C40..43.)*
- **ARQ-16** App navigation / shell: the three tabs + Models + Settings; routing,
  per-tab state, shared vs isolated components. *(FR-4.)*
- **ARQ-17** Persistence design: SQLite schema groups (core, conversations,
  personas, models, characters, memory, assets), migration tooling,
  content-addressed blob store for images/audio, backup. *(FR-14, FR-31, FR-C50;
  NFR-31.)*
- **ARQ-18** Offline enforcement: enumerate every network call site; the
  sanctioned acquisition/update paths; how "skippable + isolated" is guaranteed.
  *(NFR-1..3, NFR-10.)*
- **ARQ-19** Packaging: bundling Python workers, llama.cpp, CUDA runtime,
  CTranslate2; first-run model acquisition; fully-offline install. *(NFR-1,
  NFR-52.)*
- **ARQ-20** Implications of "no content controls": logging/redaction choices for
  explicit content, storage-at-rest handling, and confirming no code path assumes
  a filter exists. *(NFR-15.)*

---

## 6. Out of Scope (Non-Goals)

- **OOS-1** Cloud/SaaS platform: hosted inference, cloud databases, mandatory
  accounts, remote AI APIs.
- **OOS-2** A general AI agent with unrestricted computer control. AI actions are
  typed and validated only.
- **OOS-3** Browser-first delivery. Desktop/local is the core product.
- **OOS-4** Multi-user: accounts, organisations, cloud sync, server tenancy,
  multi-user permissions.
- **OOS-5** Building our own LLM runtime, CUDA backend, or image/STT/TTS engine.
- **OOS-6** Mathematically perfect identity preservation in generated images.
- **OOS-7** Any mandatory external content-moderation service.
