# 01 — Tauri IPC: command bindings, serialization contracts, typed errors

Scope: how the React/TS frontend talks to the Rust core. Assumes **Tauri v2**.

---

## 1. Command bindings

### Baseline (built-in)
- Rust: `#[tauri::command] async fn foo(state: State<'_, App>, req: FooReq) -> Result<FooResp, AppError>`.
  Register via `tauri::generate_handler![...]` in the builder.
- TS: `import { invoke } from '@tauri-apps/api/core'; await invoke('foo', { req })`.
- Args are passed as a JSON object keyed by parameter name; **camelCase in JS maps
  to snake_case in Rust** by default (configurable with `rename_all`).

### The typing gap
`invoke` is `invoke<T>(cmd: string, args?): Promise<T>` — `T` is unchecked. Nothing
stops the frontend calling a command that doesn't exist or passing the wrong shape;
it fails at runtime. Options to close the gap:

| Approach | What it does | Cost |
| -------- | ------------ | ---- |
| **`tauri-specta`** | Derives a `.ts` file of typed command wrappers + event types from `specta::Type` derives on Rust structs | Extra derive on every DTO; build step; keeps TS/Rust in lockstep automatically |
| **`ts-rs`** | Derives `.ts` interfaces from Rust types (`#[derive(TS)]`), no command wrappers | Lighter; you still hand-write the `invoke` call sites but against generated types |
| **Hand-written shared types** | A `contracts` doc + manually mirrored `.ts` | Zero tooling; drifts the moment someone forgets |

**Recommendation:** `tauri-specta` for command + event signatures, so the IPC
surface is a generated artifact (matches the "contracts are data / generated"
intent). **SPIKE**: confirm `tauri-specta` v2 + `specta` v2 API stability and
build ergonomics on Windows.

### Command surface discipline
- One module per subsystem (`commands::conversation`, `commands::models`, …); the
  `generate_handler!` list is the whole IPC surface in one place — easy to audit.
- Commands are thin: deserialize → call into a service → serialize. **No business
  logic in `#[tauri::command]` functions.**
- Long-running work returns immediately with a task id and streams via events
  (see §4); commands never block for a generation.

---

## 2. Serialization contracts

- Transport is JSON (serde_json) in v2 for `invoke`. (v2 can use a faster raw
  channel for bytes — see §4 Channels — but control messages stay JSON.)
- **DTOs are their own types**, not domain types with `Serialize` bolted on.
  Domain ↔ DTO conversion is explicit (`From`/`TryFrom`). This stops a schema
  change silently becoming an IPC break.
- Enums: use `#[serde(tag = "type")]` (internally tagged) or `tag`+`content` for
  discriminated unions the TS side can `switch` on. Avoid untagged enums — they
  make TS narrowing and error messages bad.
- Every DTO gets: a serialize test, a deserialize test, and a
  deserialize-rejects-garbage test (guide's Phase 7 intent).
- Field evolution rules to write down: additive-only without a version bump;
  `#[serde(default)]` on new optional fields; never repurpose a field name.
- IDs: newtypes (`TaskId(Uuid)`, `ModelId(String)`) not bare strings, with
  `Display`/`FromStr`; serialize as plain string/uuid.
- Timestamps: RFC3339 UTC strings (or epoch millis) — pick one, document it.

---

## 3. Typed error propagation

### The core problem
`#[tauri::command]` requires the error type to be `Serialize`. `anyhow::Error`
isn't; `std::error::Error` trait objects aren't. So you need a concrete app error
enum for the boundary.

### Pattern
```
// sketch, not final
pub enum AppError {
  NotFound { resource: String },
  Validation { field: String, message: String },
  ResourceExhausted { detail: String },   // e.g. VRAM
  Backend { subsystem: String, message: String },
  Cancelled,
  Internal { message: String },           // opaque; logged with full context
}
```
- `#[serde(tag = "kind")]` so TS gets `{ kind: 'Validation', field, message }`.
- Internally, services use `thiserror` per-subsystem error enums; the boundary
  layer maps them into `AppError` (explicit `From`). Full chains + backtraces are
  **logged** at the boundary, not serialized to the UI.
- `AppError` carries a stable **machine-readable `kind`** + a human `message`;
  optionally a `code` string for i18n on the frontend.
- TS side: a generated union type + a helper that turns a rejected `invoke` into a
  typed `AppError` (Tauri rejects the promise with the serialized value).

### Things to decide (ADR)
- Do we expose a `correlationId`/`taskId` on every error so the UI can point the
  user at a log entry? (Recommended — matches observability phase.)
- Validation errors: one-at-a-time or a list? (List is friendlier for forms.)

---

## 4. Streaming (generation tokens, progress)

Not a "contract" question but drives the DTO design.

| Mechanism | Use | Notes |
| --------- | --- | ----- |
| **Events** (`app.emit` / `emit_to`) | progress, lifecycle, token deltas | Global or windowed; JSON; simple. Payload should include `taskId`. |
| **Channels** (`tauri::ipc::Channel<T>`) | per-request token stream | v2 feature: frontend passes a `Channel` as a command arg, Rust pushes `T` into it; lower overhead, naturally scoped to the request, no manual topic strings. **Preferred for token streaming.** |
| Raw response (`Response`) | large binary blobs (image bytes) | Avoids base64; returns `tauri::ipc::Response`. |

**Recommendation:** Channels for token/streaming output, Events for
broadcast lifecycle signals (model state changed, resource pressure). Cancellation
= a separate `cancel(taskId)` command that trips a `CancellationToken` on the Rust
side (see topic 02/03).

**SPIKE:** end-to-end Channel test — 10k rapid messages, does the webview keep up;
what happens on window close mid-stream.

---

## 5. Open questions for the architecture phase
- `tauri-specta` vs `ts-rs` vs both — and whether the generated file is committed
  or built.
- One `AppError` for the whole boundary vs per-domain error unions exposed
  directly.
- Event bus naming convention + whether we wrap Tauri events in a typed
  pub/sub helper on each side.
- Backpressure policy on token Channels.
