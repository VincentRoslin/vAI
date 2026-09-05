# ADR-0002 — IPC design: Channels, Events, one AppError, ts-rs

- **Status:** ACCEPTED (Phase 5 freeze, 2026-09-05; superseding attacks folded in via Phase 4) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/01_desktop-ipc.md` (D-2, D-14)

## Context
React ↔ Rust must be typed, streaming-capable, and keep all logic in Rust
(NFR-50/51). Tauri v2 offers Commands, Events, and Channels.

## Options considered
- Streaming: many Events vs one Channel per request.
- Typed contracts: `tauri-specta` (RC), `ts-rs` (stable), or hand-written.

## Decision
- **Commands** (`invoke`) for request/response; thin — deserialize → service →
  serialize, no logic.
- **Channels** (`tauri::ipc::Channel<T>`) for per-request streams: LLM tokens,
  image progress, STT partials, TTS state.
- **Events** for broadcast state (model status, resource pressure, download
  progress), always carrying an entity/task id.
- **Cancellation**: a `cancel(taskId)` command → `CancellationToken`.
- **Errors**: one boundary enum `AppError` (`#[serde(tag="kind")]`), mapped from
  per-subsystem `thiserror` enums; full chains logged, not serialized; each error
  carries a `taskId` where one exists.
- **Typed contracts**: `ts-rs` for DTO types + a thin hand-written typed `invoke`
  wrapper layer (one fn per command). DTOs are their own types, separate from
  domain types, with explicit `From`/`TryFrom`.
- **Shell**: 3 tabs (Chat/Voice, Image Generator, Discovery) + Models + Settings;
  client-side routing; per-tab isolated state; one shared conversation component
  for Tab 1 and Tab 3.

## Consequences
- No dependency on an RC crate for the whole IPC surface. Re-evaluate
  `tauri-specta` if it reaches stable before Phase 7.
- Every DTO gets serialize/deserialize/reject-garbage tests (Phase 7).
- The typed wrapper layer is ~3 lines per command — small, explicit, reviewable.
- Channel backpressure policy (coalesce token deltas) decided at Phase 7/15.
