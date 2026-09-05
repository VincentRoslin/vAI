# Phase 3 · Desktop shell, Rust structure, IPC, app navigation

Covers steps 3.2, 3.3 and D-1, D-2, D-14. Requirements: FR-1..5, FR-11..13,
FR-18, FR-25, NFR-50, NFR-51, ARQ-16.

---

## D-1 — Desktop shell & Rust crate structure

### Shell: Tauri v2
Already the chosen direction. It fits: native Windows window + WebView2 (present
per env audit), Rust core, small binary, built-in bundler/updater. No alternative
(Electron, native) is worth revisiting — Electron bundles Chromium (~150 MB) and
puts logic in Node; a native UI toolkit loses the web ecosystem the character/chat
UI wants. **Confirm Tauri v2.**

### Rust structure: single crate vs Cargo workspace (O4)
- **Single crate, module-per-subsystem** (`config`, `db`, `models`, `resources`,
  `scheduler`, `llm`, `voice`, `image`, `characters`, `memory`, `ipc`): simplest,
  one `Cargo.toml`, fastest to navigate, no premature boundary.
- **Workspace**: separate crates for e.g. `worker-protocol`, `contracts`,
  `resource-manager`. Justified only when a crate is reused by an independent
  consumer (a separate test binary, a CLI, a second frontend).

**Recommendation:** **single crate** with strict module boundaries. Split later
only when a concrete reuse appears. Record the split triggers in the ADR:
(a) a subsystem needs its own integration-test binary that would otherwise pull
the whole app; (b) the worker protocol gets a standalone conformance harness;
(c) compile times exceed a set budget and a leaf crate would parallelize.

→ **ADR-0001**: single crate, module boundaries, documented split triggers.

---

## D-2 — IPC design

Tauri v2 gives three primitives (confirmed against v2 docs):

| Primitive | Use here |
| --------- | -------- |
| **Command** (`invoke`) | request/response: send message, load model, start download, CRUD on personas/characters, get config |
| **Event** (`emit`, bidirectional) | lifecycle broadcasts: model-state changed, resource pressure, download progress, scheduler state |
| **Channel** (`tauri::ipc::Channel<T>`) | **per-request streams**: LLM token deltas, image-generation progress, STT partial transcripts, TTS playback state |

**Rules for this project:**
- Token/partial streams → **Channel** (v2's recommended streaming mechanism;
  lower overhead than N events, naturally request-scoped, dies with the request).
- Broadcast state → **Events**, always carrying a `taskId` / entity id.
- Commands are thin: deserialize → call a service → serialize. **No business
  logic in `#[tauri::command]` functions** (NFR-50/51).
- Cancellation = a dedicated `cancel(taskId)` command tripping a
  `tokio_util::sync::CancellationToken`; never an out-of-band signal.
- Every command's error type is one boundary enum `AppError`
  (`#[serde(tag = "kind")]`), mapped from per-subsystem `thiserror` enums. Full
  chains are logged, not serialized to the UI. Each error carries a `taskId` where
  one exists so the UI can point at a log entry.

### Typed-contract generation
- **`tauri-specta`** (2.0.0-rc.25, May 2026 — still RC): generates TS types for
  commands **and** events from `specta::Type` derives. Keeps the IPC surface a
  generated artifact. Risk: RC status; churn.
- **`ts-rs`** (stable): generates TS interfaces from `#[derive(TS)]`; you
  hand-write the `invoke` call sites against them.
- **Hand-written shared types**: zero tooling, drifts immediately.

**Recommendation:** **`ts-rs` for the DTO types + a thin hand-written typed
`invoke` wrapper layer** (one function per command, ~3 lines each). Rationale:
`ts-rs` is stable; the wrapper layer is small, explicit, and reviewable; we avoid
depending on an RC crate for the whole IPC surface. Re-evaluate `tauri-specta`
when it hits stable — if it does before Phase 7, prefer it.
DTOs are their own types (`XxxDto`), separate from domain types, with explicit
`From`/`TryFrom`. Every DTO gets serialize + deserialize + reject-garbage tests
(Phase 7).

→ **ADR-0002**: Channel for streams, Events for broadcast, one `AppError`,
`ts-rs` + typed invoke wrappers.

---

## D-14 — App navigation / shell (ARQ-16)

Three primary tabs + two utility surfaces (FR-4):

```
┌─────────────────────────────────────────────┐
│  [Chat/Voice] [Image Generator] [Discovery]  ·  [Models] [Settings]  │
└─────────────────────────────────────────────┘
```

- **Client-side routing** (one route per tab + sub-routes: a conversation, a
  character, a download). A 404 route.
- **Per-tab state is isolated**; only truly shared state (active model status,
  resource pressure, download queue) is global, sourced from Rust via events and
  a small store. The frontend never caches domain data the core owns (NFR-51) —
  it holds view state + a cache of the last server snapshot, invalidated on the
  relevant event.
- Shared UI primitives (loading/empty/error/streaming states, message list, image
  card, model-status chip) are components reused across tabs (Phase 30 formalizes
  this; establish the pattern at Phase 6).
- Tab 1 and Tab 3 both render "a conversation" — the **same conversation
  component** bound to different sources (persona vs character). This mirrors the
  one-engine rule (ARQ-2): one conversation UI, one conversation engine.

→ folded into **ADR-0002** (or a small ADR-0002b if it grows).

---

## Failure modes considered
- WebView2 missing on a target machine → Tauri bundler can embed the bootstrapper
  (Phase 37); env audit confirms it's present for dev.
- IPC payload flood / oversized payload → size limits + backpressure on Channels
  (bounded buffer, coalesce token deltas); Phase 4 attacks this.
- Frontend holds stale domain data after an event is missed → events carry a
  version/seq; the frontend re-fetches on reconnect/focus.
- `invoke` of an unknown command → compile-time prevented by the typed wrapper
  layer; runtime still returns a typed `AppError::NotFound`.

## Open sub-questions for Phase 4 / Phase 5
- Exact Channel backpressure policy (drop-oldest vs coalesce vs bounded-block).
- Whether the typed invoke wrappers are generated (small codegen) or hand-written.
- Event bus: raw Tauri events vs a thin typed pub/sub helper on each side.
