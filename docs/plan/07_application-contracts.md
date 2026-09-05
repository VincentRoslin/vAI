# Phase 7 — Application Contracts

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0015). Step detail below **finalized at phase entry, 2026-09-06** against the freeze. Governing ADRs: **ADR-0002** (IPC design — Commands/Events/Channels, one `AppError`, `ts-rs`), **ADR-0013** (subprocess transport — worker JSON-lines framing).

## Objective
Strongly-typed, serializable contracts shared across the IPC and worker
boundaries: tasks, model metadata + state, generation requests + events,
streaming events, cancellation, structured errors, conversations, messages,
resource reservations, worker jobs. No model names embedded in business logic.

## Depends on
Phase 6 (`ipc` module, `AppError`, `ts-rs` binding pipeline, check suite).

## Not in this phase
- Behaviour that uses the contracts (no inference, no persistence, no scheduling
  logic, no `invoke` handlers beyond what Phase 6 already registered).
- Worker or backend implementations.
- Character / persona / relationship / image-action contracts — those are defined
  with their own phases (20, 25–28); ARQ-15 (relationship stage set) is still
  being tuned. Phase 7 stops at the list above.
- Config contract — Phase 8. Model *registry* persistence — Phase 11.

## Architecture notes
- New top-level module `src-tauri/src/contracts/` — **not** under `ipc`, because
  the same types cross the worker (stdio JSON-lines) boundary too.
- Contracts are their own types, distinct from domain types; domain↔DTO
  conversion is explicit (`From`/`TryFrom`), added by the phase that introduces
  the domain type.
- IDs are newtypes (`TaskId`, `ModelId`, …), not bare strings. Opaque value,
  `#[ts(type = "string")]`, non-empty on `TryFrom`.
- `AppError` stays the one boundary enum (ADR-0002): `#[serde(tag = "kind", content = "message")]`, machine-readable `kind`, human `message`, optional `task_id` correlation added where the discriminant alone is not enough.
- Cancellation and progress are representable in the contract (a `TaskStatus`
  with `state` + `progress`), not out-of-band.
- Streaming events (`GenerationEvent`, worker stream frames) are adjacently
  tagged for clean TS discriminated-union narrowing.

## Performance notes
- Serialization is serde_json (ADR-0002 transport). Measure round-trip cost for a
  representative `GenerationEvent::TokenDelta` (target: << per-token LLM latency,
  i.e. < ~50 µs serialize+deserialize on the dev machine). Record the number.
- `TokenDelta` must be cheap to construct and small on the wire (single `String`
  + `u32` index, no per-token allocation beyond the token text).
- Contract structs derive `Clone`; keep them shallow (no nested `Vec` of structs
  in the hot streaming types).

## Steps (atomic; each independently verifiable)

**7.1 — ID newtypes**
    Do:     `contracts/ids.rs` — `TaskId, ModelId, ConversationId, MessageId,
            ReservationId, WorkerJobId, DownloadId, AssetId`. Each: `struct X(String)`,
            `Serialize`/`Deserialize` transparent, `TS` with `#[ts(type="string")]`,
            `TryFrom<String>` rejecting empty/whitespace, `Display`, `AsRef<str>`,
            `FromStr`. A `new_random()` behind `#[cfg(test)]` or a small helper is
            fine but generation policy is not frozen here.
    Owner:  Rust core (contracts)
    Verify: `cargo test contracts::ids` — round-trips each id through JSON as a
            bare string; `TryFrom::try_from(String::new())` is `Err`.

**7.2 — Error taxonomy**
    Do:     Extend `ipc/error.rs` `AppError` to the frozen `kind` set:
            `NotFound, Validation, Conflict, ResourceExhausted, Timeout, Cancelled,
            BackendUnavailable, WorkerCrashed, Internal`. Keep `#[serde(tag="kind",
            content="message")]`. Add `AppError::with_task(self, TaskId)` producing
            a correlated form (a sibling `ErrorEnvelope { error, task_id }` DTO, not
            a new variant). Document each `kind`: meaning, retriable? , who raises it.
    Owner:  Rust core (ipc)
    Verify: `cargo test` — every `kind` serializes to its documented tag string;
            an unknown `kind` fails to deserialize; `ErrorEnvelope` round-trips.

**7.3 — Task + task status**
    Do:     `contracts/task.rs` — `TaskKind` (LlmGeneration, ImageGeneration,
            Stt, Tts, Embedding, ModelDownload, ModelLoad, ModelUnload),
            `TaskState` (Queued, Running, Succeeded, Failed, Cancelled),
            `TaskStatus { id: TaskId, kind, state, progress: Option<f32> (0..=1),
            detail: Option<String> }`, `CancelRequest { task_id: TaskId }`.
    Owner:  Rust core (contracts)
    Verify: `cargo test contracts::task` — round-trip; `progress` outside 0..=1 is
            rejected by a `validate()` (unit-tested); `TaskState` unknown variant
            rejected.

**7.4 — Model metadata + state**
    Do:     `contracts/model.rs` — `ModelKind` (Llm, Stt, Tts, Image, Embedder),
            `ModelBackend` (opaque string newtype — *not* an enum of product names),
            `Quant` (`Option<String>` newtype), `ModelCapabilities { streaming: bool,
            context_tokens: Option<u32>, … minimal set }`, `ModelState` (Unloaded,
            Loading, Loaded, Failed, Unloading), `ModelMetadata { id: ModelId,
            display_name: String, kind, backend, quant, capabilities,
            estimated_vram_mb: Option<u32> }`. No product model names as literals.
    Owner:  Rust core (contracts)
    Verify: `cargo test contracts::model` — round-trip; `git grep -niE
            'llama|whisper|chatterbox|krea|silero|insightface'
            src-tauri/src/contracts` → nothing.

**7.5 — Generation request + streaming events**
    Do:     `contracts/generation.rs` — `SamplingParams { temperature, top_p,
            top_k, max_tokens, stop: Vec<String>, seed: Option<u64> }` with a
            `validate()`; `GenerationRequest { task_id: TaskId, model: ModelId,
            prompt: String, params: SamplingParams }`; `StopReason` (EndOfText,
            MaxTokens, StopSequence, Cancelled, Error); `GenerationEvent`
            (adjacently tagged) — `TokenDelta { index: u32, text: String }`,
            `Done { stop_reason: StopReason, tokens: u32 }`, `Error { error:
            AppError }`, `Cancelled`.
    Owner:  Rust core (contracts)
    Verify: `cargo test contracts::generation` — round-trip every `GenerationEvent`
            variant; TS discriminated union narrows (compile check in 7.8);
            `SamplingParams::validate` rejects `temperature < 0`, `top_p > 1`.

**7.6 — Conversation + message**
    Do:     `contracts/conversation.rs` — `Role` (System, User, Assistant),
            `MessageContent` (adjacently tagged: `Text { text }`, `Audio { asset:
            AssetId, transcript: Option<String> }`, `Image { asset: AssetId,
            caption: Option<String> }`), `Message { id: MessageId, conversation_id:
            ConversationId, role, content, created_at: String (RFC-3339),
            generation: Option<GenerationMeta> }`, `GenerationMeta { model: ModelId,
            stop_reason: StopReason, tokens: u32, duration_ms: u64 }`,
            `ConversationKind` (Persona, Character), `Conversation { id, kind,
            title: Option<String>, created_at, updated_at }`.
    Owner:  Rust core (contracts)
    Verify: `cargo test contracts::conversation` — round-trip; a `Message` missing
            `role` fails to deserialize; timestamps are strings (no chrono in the
            contract).

**7.7 — Resource reservation + worker job**
    Do:     `contracts/resource.rs` — `ResourceKind` (Gpu, SystemRam),
            `ReservationState` (Requested, Held, Released, Denied), `Reservation
            { id: ReservationId, kind, amount_mb: u32, task_id: Option<TaskId>,
            state }`.
            `contracts/worker.rs` — the JSON-lines envelope (ADR-0013):
            `WorkerRequest<T> { id: WorkerJobId, kind: WorkerKind, payload: T }`,
            `WorkerResponse<T> { id: WorkerJobId, result: WorkerResult<T> }`,
            `WorkerResult` (adjacently tagged: `Ok { data }`, `Err { error:
            AppError }`, `Progress { progress: f32, detail: Option<String> }`),
            `WorkerKind` (Stt, Tts, Embed), `WorkerHello { protocol_version: u32,
            worker: WorkerKind }` handshake.
    Owner:  Rust core (contracts)
    Verify: `cargo test contracts::resource contracts::worker` — round-trip;
            `WorkerHello` with a mismatched `protocol_version` is representable and
            an equality check distinguishes it; unknown `WorkerResult` tag rejected.

**7.8 — TypeScript side**
    Do:     `cargo test` regenerates `src/bindings/`. Add `src/lib/contracts.ts`
            re-exporting the generated types under a single import surface. Add a
            `src/lib/contracts.test-d.ts` (or a Vitest `expectTypeOf`) that
            exercises discriminated-union narrowing on `GenerationEvent` and
            `MessageContent`.
    Owner:  Frontend
    Verify: `npx tsc --noEmit` clean; `git diff --exit-code src/bindings` clean
            after commit; narrowing test passes.

**7.9 — Round-trip + rejection test sweep**
    Do:     One `contracts/tests.rs` (or per-module `#[cfg(test)]`) covering: every
            contract type serialize→deserialize equality; every enum rejects an
            unknown variant/tag; every `validate()` rejects at least one bad input;
            the `ErrorEnvelope` carries `task_id`.
    Owner:  Rust core
    Verify: `cargo test contracts` — all green; count assertions ≥ one per
            contract type.

**7.10 — Perf probe**
    Do:     A `#[test]` (or `cargo bench`-free timed test) that serializes +
            deserializes 10k `GenerationEvent::TokenDelta` and prints ns/op.
            Record the number in `docs/verification/06_phase7_contracts.md`.
    Owner:  Rust core
    Verify: number recorded; < ~50 µs/round-trip (else note + raise an ADR).

**7.11 — Documentation**
    Do:     `docs/contracts.md` — one section per contract group: purpose, field
            table, **evolution rules** (additive-only; new enum variants are a
            breaking change for exhaustive matches → add `#[non_exhaustive]` where
            appropriate; `protocol_version` bump policy for the worker envelope).
            Link it from `CLAUDE.md` Document Map and `src-tauri/README.md`.
            Register the `contracts` module in `ARCHITECTURE.md` §2.
    Owner:  Docs
    Verify: file exists; every type from 7.1–7.7 appears; `CLAUDE.md` +
            `src-tauri/README.md` + `ARCHITECTURE.md` updated.

**7.12 — Gate run + commit**
    Do:     `node scripts/check.mjs`; write `docs/verification/06_phase7_contracts.md`;
            update `ROADMAP.md` §1 + §5; commit.
    Owner:  —
    Verify: check suite all green; verification doc records each gate check.

## Verification gate
1. All contracts compile — `cargo build` + `npx tsc --noEmit` clean.
2. Round-trip serialize/deserialize tests pass for **every** contract type
   (7.9 sweep, one+ assertion per type).
3. Invalid payloads rejected with a typed error, tested — unknown enum
   variant/tag, missing required field, out-of-range `validate()` input.
4. `git grep -niE 'llama|whisper|chatterbox|krea|silero|insightface|flux'` in
   `src-tauri/src` **outside** `contracts/` and its tests → nothing.
5. `docs/contracts.md` exists and documents evolution rules (additive-only,
   `#[non_exhaustive]`, worker `protocol_version` policy).
6. `src/bindings/` regenerated and committed; `git diff --exit-code` clean.
7. Perf number for `TokenDelta` round-trip recorded.
8. No new `invoke` handlers / no behaviour — `git diff` touches only
   `contracts/`, `ipc/error.rs`, `src/lib/contracts*`, `src/bindings/`, docs.

## ADRs / open questions this phase resolves or raises
- May raise an ADR on internally- vs adjacently-tagged enums if TS narrowing is
  awkward (expectation: adjacently tagged, decided here, no ADR needed).
- Worker `protocol_version` starts at `1`; bump policy documented in
  `docs/contracts.md`, promoted to an ADR only if a second version ever ships.
- ID generation policy (UUID vs ULID vs DB rowid-backed) is **not** frozen here —
  deferred to Phase 9 (persistence) where IDs get minted.
