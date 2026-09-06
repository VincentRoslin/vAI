# Application Contracts

**Status:** Live from Phase 7 (2026-09-06). Governing ADRs: **ADR-0002** (IPC
design), **ADR-0013** (worker transport).

The strongly-typed, serializable vocabulary the Rust core shares across its two
outward boundaries:

- the **typed Tauri IPC** boundary to the React frontend, and
- the **JSON-lines stdio** boundary to stateless Python workers.

Source of truth: `src-tauri/src/contracts/` (Rust). TypeScript is generated from
it by `ts-rs` into `src/bindings/` and re-exported from `src/lib/contracts.ts`.
Python workers consume the same JSON shapes by hand (no generation).

Contracts are **separate from domain types**. The phase that introduces a domain
type adds the explicit `From` / `TryFrom` to its contract form; `contracts/`
never depends on a domain module, and holds no behaviour.

---

## Groups

| Module | Types | Purpose |
| ------ | ----- | ------- |
| `contracts::ids` | `TaskId` · `ModelId` · `ConversationId` · `MessageId` · `ReservationId` · `WorkerJobId` · `DownloadId` · `AssetId` | Opaque identifiers. Bare non-empty JSON string on the wire; `string` in TS. Deserialization rejects empty / whitespace-only. **ID generation policy (UUID/ULID/rowid) is deferred to Phase 9.** |
| `ipc::error` | `AppError` · `ErrorEnvelope` | The one boundary error. `#[serde(tag = "kind", content = "message")]`; `kind` is the stable machine-readable discriminant. `ErrorEnvelope` adds optional `task_id` correlation. Also crosses the worker boundary (inside `WorkerResult` / `GenerationEvent`). |
| `contracts::task` | `TaskKind` · `TaskState` · `TaskStatus` · `CancelRequest` | Every long-running operation is a task with a `TaskId`. `TaskStatus` carries `state` + optional `progress` (0.0..=1.0, validated) + `detail`. |
| `contracts::model` | `ModelKind` · `ModelBackend` · `Quant` · `ModelCapabilities` · `ModelMetadata` · `ModelState` · `RegisteredModel` · `RegistryAvailability` · `Device` | Model description + runtime state; `RegisteredModel` is a registry row as the frontend sees it (Phase 11 — metadata + path + `availability` + devices). **No product model names as string literals** — `display_name` / `backend` are runtime data; logic switches on `ModelKind`. `RegistryAvailability` (`Ready`/`Missing`) is *file presence*, distinct from `ModelState` (*runtime lifecycle*). |
| `contracts::generation` | `SamplingParams` · `GenerationRequest` · `StopReason` · `GenerationEvent` | LLM request + the per-request streaming event (`TokenDelta` / `Done` / `Error` / `Cancelled`), adjacently tagged. `SamplingParams::validate()` range-checks temperature / top_p / max_tokens. |
| `contracts::conversation` | `Role` · `MessageContent` · `GenerationMeta` · `Message` · `ConversationKind` · `Conversation` | One shape for the Persona tab and the Character tab. `MessageContent` is adjacently tagged (`Text` / `Audio` / `Image`). Timestamps are RFC-3339 **strings** — the contract carries no date library. |
| `contracts::resource` | `ResourceKind` · `ReservationState` · `Reservation` · `GpuMemory` · `RamInfo` · `ResourceSnapshot` | A resource-ledger entry plus the resource-manager snapshot: whole-GPU / RAM measurement (`Option` — `None` when the probe is unavailable) + outstanding reservation totals (Phase 13, ADR-0007). |
| `contracts::model` (lifecycle) | `ModelState` (`+ Busy`, Phase 14 — added additively) · `LifecycleStatus` | Runtime state of a model in the lifecycle manager + a per-model status row (`id`, `state`, measured `vram_mb`, last `error`). |
| `contracts::conversation` (engine state, Phase 17) | `GenerationHandle` (`task_id`, `conversation_id`) · `GenerationState` (`generating: Option<GenerationHandle>`) | The conversation engine's streaming state — `chat_state`. |
| `ipc::commands::ChatSendRequest` | `ChatSendRequest` (`conversation_id`, `model_id`, `text`) | Body of `chat_send` (Phase 16); the reply streams back over a `Channel<GenerationEvent>`. |
| `contracts::acquisition` | `HfModelSummary` · `HfGgufFile` · `DownloadState` · `DownloadInfo` · `DownloadProgress` | HF search results, GGUF file listings, and download queue / progress state (Phase 12). `DownloadProgress` is the per-download Tauri Channel payload. |
| `contracts::worker` | `WorkerKind` · `WorkerHello` · `WorkerRequest` · `WorkerResult` · `WorkerResponse` | The JSON-lines envelope every worker speaks. `payload` / `Ok.data` bodies are `serde_json::Value` — their schema belongs to each worker's own phase (STT 18, TTS 19, embedder 27). `WorkerResult` is adjacently tagged (`Ok` / `Err` / `Progress`). |
| STT worker payload (Phase 18, `voice::SttResult`) | request `{ audio_path: string, language: string \| null }` · `Ok.data` `{ text, language, duration_s, avg_logprob, no_speech_prob }` | The STT worker body inside the `contracts::worker` envelope. `audio_path` is a 16 kHz mono `s16le` WAV the Rust core wrote and owns. Rust drops the turn when `text` is empty, `no_speech_prob > 0.6`, or `avg_logprob < -1.0` (ADR-0005 low-confidence policy). Not `ts-rs`-exported — the worker consumes JSON by hand. |
| voice IPC (Phase 18) | `VoiceState` (`Idle`/`Warming`/`Listening`/`Speech`/`Transcribing`/`Error`, tagged `kind`) · `InputDevice` (`name`, `is_default`) | `voice_start` streams `VoiceState` over a Tauri Channel; `voice_input_devices` lists `InputDevice`s. `ts-rs`-exported from `voice::` (like `config::AppConfig`). |

`contracts::WORKER_PROTOCOL_VERSION` (currently `1`) is the version a worker
reports in `WorkerHello`; the supervisor refuses a mismatch.

---

## Serialization

- Format: `serde_json` (ADR-0002).
- Enums carrying data are **adjacently tagged** (`{ "type": "X", "data": { … } }`,
  or `{ "status": … , "body": … }` for `WorkerResult`) so the generated
  TypeScript is a discriminated union that narrows on the tag.
- Unit enums are external-tagged (bare `"Variant"` string).
- `Option<T>` fields serialize as `"field": null` when absent and appear in TS as
  `field: T | null` (required key, nullable value) — deliberately explicit
  rather than an optional key. `ts-rs` 10 does not translate
  `#[serde(skip_serializing_if)]`, so those attributes are not used here.
- Perf: a `GenerationEvent::TokenDelta` round-trip (serialize + deserialize) is
  **~3.3 µs** on the dev machine (`token_delta_round_trip_perf`, ceiling 50 µs) —
  negligible against per-token LLM latency.

---

## Evolution rules

**Additive-only.** Existing field names, types, and enum tag strings never
change meaning once shipped.

1. **New optional field** → fine. Add `Option<T>`; old payloads omit it.
2. **New required field** → breaking. Avoid; if unavoidable it is a coordinated
   Rust + TS + worker change in one commit.
3. **New enum variant** → *source-breaking for exhaustive `match`*. `AppError` is
   `#[non_exhaustive]`; add a `_` arm in non-`contracts` code that matches it.
   Data-carrying contract enums (`GenerationEvent`, `MessageContent`,
   `WorkerResult`, …) are matched exhaustively on purpose — adding a variant is a
   deliberate breaking change that the compiler flags at every call site.
   *Precedent:* `ModelState::Busy` was added at Phase 14 — a simple state enum
   the frozen contract had reserved for "Phase 14"; nothing matched it
   exhaustively, the TS union just grew, no version bump.
4. **Removing / renaming anything** → not allowed without a migration and a
   version bump.
5. **Worker envelope change** → bump `WORKER_PROTOCOL_VERSION`; the supervisor
   rejects workers that report a different version. Promote the bump to an ADR
   only when a second version actually ships.
6. **ID format** → the string stays opaque. Nothing may parse structure out of an
   ID; if a real format is chosen at Phase 9 it is still validated only for
   non-emptiness here.

Every contract type has a serialize→deserialize round-trip test and every
`validate()` has a rejection test in `src-tauri/src/contracts/tests.rs`; the
`git diff --exit-code src/bindings` check keeps the generated TS in sync.
