# LocalAI — Product & Architecture Overview

Reference context: what LocalAI is meant to be and the technical shape it is
expected to take. This is **not a specification and not a decision record** — the
binding versions are `PROJECT.md` and `ARCHITECTURE.md` (written at Phase 5),
`CLAUDE.md` (rules), and `ROADMAP.md` (plan/state). Where this doc and a repo
document disagree, the repo document wins.

Distilled from the owner's product vision and prior planning discussions
(2026-09-05); rephrased for clarity, guide/process material removed.

---

## 1. What LocalAI is

A **Windows-first, local-first AI desktop application** — one coherent interface
for running and interacting with local AI models, with no dependence on cloud AI
services during normal operation.

The ambition is a full local AI platform, not just a chat window: it manages
multiple AI modalities, models, hardware resources, conversations, personas,
memory, and persistent AI characters, and should feel like a single product
rather than several tools bolted together.

## 2. Feature set (product requirements)

- Local text chat with streaming LLM responses
- Voice conversations: speech-to-text and text-to-speech, with interruption
- Personas
- Conversation history and persistent memory
- Local image generation
- Local model management: discovery, registration, load/unload, hot-swapping
- GPU / VRAM resource management across competing AI workloads
- Persistent AI characters: structured profiles, galleries, discovery
  (swipe-style), conversations, per-character memory, and
  character-consistent image generation

Quality priorities: local execution, privacy, offline capability, reliability,
good UX, clear architecture, resource efficiency, maintainability, extensibility,
and behaviour that can be verified rather than assumed.

## 3. The character system (the defining capability)

A character is a **persistent entity with structured state**, not a large prompt
or a selected system message. It stays the same conceptual entity across
conversations and across generated media.

Structured character state (exact schema decided during data-model research):
identity, name, personality, appearance, interests, relationship state,
conversation history, memory, reference images, generated images.

- **Discovery**: browse AI-generated characters, view profile and images, choose
  one, and begin a conversation bound to that persistent entity. Returning later
  restores the relationship, memory, and history.
- **Conversations** run on the shared conversation engine (§4), with the
  character's identity assembled into model context by the persona/context
  builder.
- **Memory**: conversation → extraction → importance/validation → storage →
  relevant retrieval → context construction. SQLite-based first; a vector store
  only if measurements justify it.
- **Character-consistent images**: reference identity → conditioning → generation
  → identity verification → accept or regenerate. Using the same text prompt does
  not guarantee identity consistency; the pipeline must actively check.

## 4. Runtime ownership model

Three runtime categories with fixed responsibilities:

### Rust application core — the authoritative nucleus
Owns application state, orchestration, configuration, persistence and all SQLite
access, model lifecycle, resource/VRAM management, scheduling, task management,
process supervision, IPC, security-sensitive operations, and authoritative
business logic. Subsystem boundaries within Rust are finalized during architecture
research.

### React / TypeScript frontend — presentation only
Owns rendering, UI, user interaction, and transient presentation state. It never
accesses SQLite, AI workers, llama.cpp, or arbitrary processes directly, and is
not an authoritative persistence layer. It communicates only through typed,
validated Tauri IPC contracts. Frontend validation is for UX; authoritative
validation happens in Rust.

### AI subprocesses — isolated, non-authoritative
llama.cpp / `llama-server`, plus workers for STT, TTS, image generation, and
embeddings. All are spawned and supervised only by the Rust core. They never hold
authoritative state, never touch SQLite, never talk directly to the frontend, and
never independently manage global GPU resources or launch other processes. Binary
assets move via controlled filesystem paths the core dictates. Backend-specific
detail is confined to a single adapter module in Rust.

**One authority per concern.** There must not be independent frontend, Python,
Rust, and SQLite state each deciding what the application believes. Clear
single ownership for models, GPU resources, processes, configuration,
conversations, characters, memory, tasks, and persistence.

## 5. Key technical areas

### IPC
`React → Tauri IPC → Rust → managed subsystem`. Never React → llama.cpp / Python /
arbitrary process. Contracts are typed, validated, documented, explicit, and
independent of internal implementation.

### LLM runtime
Initial backend: llama.cpp. Likely integration is to run `llama-server` as a
supervised local process and talk to its local API — to be confirmed in
architecture research. Requirements regardless of mechanism: local inference,
streaming, cancellation that reaches the real job, process lifecycle management,
crash detection and recovery, model load/unload, resource awareness, and no
direct frontend process control.

### Model management
Discover, register, load, unload, track state, select, handle failure, associate
capabilities, and track resource requirements. Model names must not leak
throughout business logic; model-specific behaviour stays behind boundaries. Build
the abstraction the first real integration needs, then generalize when a second
runtime proves the need — no speculative generic model layer.

### GPU / VRAM management
GPU is shared between LLM, image generation, STT, TTS, and other workloads.
**Model file size ≠ runtime VRAM usage** — actual use depends on architecture,
quantization, context length, KV cache, batch size, GPU offload, temporary
buffers, backend overhead, and concurrency. The system distinguishes **estimated**,
**reserved**, and **observed** usage, and never treats an estimate as a
measurement. Conceptual resource lifecycle: request → reserve → commit → observe →
release → reconcile.

### Model hot-swapping
Different GPU workloads share limited VRAM: e.g. LLM loaded → image request →
scheduler suspends/unloads the LLM if needed → image model loads → generates →
resources released → LLM restored. Not every runtime can suspend gracefully;
process termination and restart may be required to reclaim VRAM.

### Persistence
`Rust → SQLite`, never frontend or workers. Migrations for all schema changes;
schema versioning. Large binaries (images, audio) live on the filesystem; SQLite
holds metadata, relationships, and paths.

### Voice
A **modality of the conversation system**, not a separate app:
mic → STT → user message → conversation engine → LLM → TTS → playback. Supports
interruption: if the user speaks during TTS, the current response stops and the
system returns to listening; an interrupted assistant turn is persisted as
truncated.

### Personas / context building
Structured persona data plus a predictable context builder: system instructions +
persona + character info + conversation + relevant memory + runtime context. It
must be verifiable that persona/character content actually reaches the model — the
UI displaying it is not proof.

### AI output is untrusted
Model and character output is data, never instructions. It must never become a
shell command, executable path, filesystem operation, database command, or process
launch. Actions a model requests (e.g. a character sending an image) are expressed
as **typed, validated actions** — `send_image { character_id, image_type, scene,
mood, clothing, context }` — that Rust validates against a schema and allow-list
and then executes through the appropriate subsystem. An action referencing another
character's id, or any malformed/unknown action, is rejected and logged.

### Security (local ≠ safe)
Avoid arbitrary command execution; validate model-generated actions; confine
filesystem access and protect against path traversal; validate worker inputs;
construct process arguments safely; keep secrets out of logs and the frontend
bundle; bind local services to loopback only.

### Offline
Core features (chat, voice, memory, characters, image generation) work with the
network disabled. The only network paths are explicit, isolated, and skippable:
initial model/dependency acquisition and an optional update check. Acquiring
something once is different from requiring the network to run.

### Performance
Measured, not guessed: baseline → identify bottleneck → improve → re-measure →
compare. Relevant metrics: startup, UI responsiveness, LLM latency and token
throughput, image generation, VRAM usage, model switching, DB operations, memory
retrieval.

### Reliability
Failure handling is part of the architecture. The system must behave predictably
when llama.cpp or a worker crashes, a model file is missing or corrupt, config or
the DB is corrupt, VRAM is exhausted, a generation is cancelled, the app closes
mid-generation, a worker times out, or a process dies unexpectedly.

## 6. Technology direction vs decisions

**Direction (expected, not yet ratified):** Tauri, React, TypeScript, Rust,
llama.cpp, Python workers, SQLite, NVIDIA CUDA.

**Decided by architecture research (Phase 3) and frozen at Phase 5:** subsystem
boundaries, IPC contract design, process architecture, worker transport protocol,
database schema, resource manager design, scheduler design, model lifecycle
design, image-generation and identity architecture, packaging strategy, testing
strategy, security boundaries, and whether the Rust side is one crate or a
workspace (default: one crate, split only for a concrete reason).

Architecture research stays genuinely open — technology being *expected* is not the
same as an implementation design being *approved*.

## 7. Product mental model

```
          React / UI  (Chat · Voice · Characters · Gallery · Settings)
                              │  Tauri IPC
                              ▼
          Rust Core  (App state · Conversations · Characters · Memory ·
                      Configuration · Scheduling · Resources ·
                      Process lifecycle · Persistence)
                        │                       │
                        ▼                       ▼
                   llama.cpp               AI Workers
                  (Local LLM)        (Image · STT · TTS · Embeddings)
```

Direction and ownership philosophy — not a substitute for the formal architecture.
