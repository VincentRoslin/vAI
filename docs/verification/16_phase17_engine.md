# 16 — Phase 17: Conversation Engine

**Date:** 2026-09-06
**Branch:** `main` (local; no remote).
**Method:** reshaped `src-tauri/src/conversation/` — `ConversationService` →
`ConversationEngine`; `send` split into `add_user_turn` + `generate`; an explicit
`GenerationState`; typed prompt rendering. 12 unit/contract tests + the Phase 16
`#[ignore]`d live test re-run on the re-hosted engine.

Governing: **ADR-0002** (IPC), **ADR-0009** (persistence), Phase 7 contracts.
Plan: `docs/plan/17_conversation-engine.md`. **No new ADR.**

---

## What changed

- **`ConversationService` → `ConversationEngine`** (type, `new`, every ref in
  `lib.rs` / `ipc/commands.rs` / tests). Module doc: "the one engine that every
  conversational surface runs on". `git grep ConversationService` → **nothing**
  (gate 2).
- **`send` split into two seams** (so voice/STT can drive them separately at
  Phase 18):
  - `add_user_turn(conversation_id, content: MessageContent) -> Message` —
    persist one **typed** user turn. `Text` must be non-empty; `Audio` / `Image`
    accepted as-is.
  - `generate(conversation_id, model_id, sink) -> TaskId` — run one LLM
    generation over the history, stream events, persist the assistant turn on the
    terminal frame. Rejects with `Conflict` if one is already running; `NotFound`
    for an unknown conversation (checked before marking busy).
  - `send(id, model, text, sink)` stays as the **text convenience** =
    `add_user_turn(Text) + generate` — the chat UI is unchanged.
- **Explicit streaming state machine.** `Mutex<Option<Running>>` where `Running`
  carries a `GenerationHandle { task_id, conversation_id }`.
  `generation_state() -> GenerationState { generating: Option<GenerationHandle> }`
  — `None` at rest, `Some` mid-stream, back to `None` on the terminal frame.
  `is_generating()` kept as a convenience. New `chat_state` IPC command +
  `src/lib/ipc.ts` `chatState()`.
- **Typed prompt rendering.** `render_chatml` now contributes text for
  `MessageContent::Text` **and** `Audio { transcript: Some(_) }`; skips
  `Image` and untranscribed audio. Voice turns (Phase 18) render with no engine
  change.
- Contracts (additive): `GenerationHandle`, `GenerationState` in
  `contracts::conversation`.

---

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | Engine unit tests pass — lifecycle, streaming, cancellation, persistence, restart, **state machine** | **PASS** — `conversation::tests` (19, scripted `LlmInstance`): `generation_state_tracks_idle_then_generating_then_idle` (Idle → `Generating{task,convo}` → Idle), `add_user_turn_then_generate_streams_and_persists`, `add_user_turn_rejects_empty_text_and_a_missing_conversation`, `send_streams_deltas_and_persists_the_assistant_message`, `a_second_send_while_generating_is_a_conflict`, `cancel_mid_stream_persists_the_partial_turn`, `a_backend_error_persists_a_truncated_turn_and_recovers`, `repo_round_trips_a_conversation_with_messages` (reopen). |
| 2 | Text chat runs entirely on the engine; the old service name is gone | **PASS** — `git grep -n ConversationService -- '*.rs' '*.ts' '*.tsx'` → nothing. There was only ever one path (Phase 16 built one); this was a rename, not a delete. |
| 3 | The full Phase 16 verification gate still passes on the re-hosted chat | **PASS (live)** — `conversation::live_tests::live_send_streams_persists_and_recovers_on_restart` on the RTX 5080: `send("Reply with exactly: pong")` → **`generation_state` reports `Generating` for the right conversation** → real `llama-server` stream → assistant "pong" persisted (`EndOfText`) → `generation_state` back to `None` → **fresh engine over the same DB restores the 2-message transcript + `latest`** → a 500-word-essay generation cancelled mid-stream → `Cancelled` frame, partial persisted with `stop_reason = Cancelled` → a following `send` → `Done` (model reusable). |
| 4 | No performance regression vs the Phase 16 baseline | **PASS** — end-to-end TTFT (engine `send` → first delta) **≈ 44 ms** (Phase 16 was ≈ 33 ms; both are timer-granularity noise, far under the ~1–2 s budget). Same single-`Channel` streaming path — no extra hop. |
| 5 | `add_user_turn` accepts typed content — a non-`Text` turn persists and renders from its transcript | **PASS** — `chatml_renders_transcribed_audio_and_skips_the_rest`: a `[Text, Audio(Some), Audio(None), Image]` transcript renders `text` + the one transcript in order; the untranscribed audio + image turns are skipped. `add_user_turn` persists any `MessageContent`. |
| 6 | Full check suite green; bindings regenerated | **PASS** — `node scripts/check.mjs` all green: `cargo fmt` / `clippy -D warnings` / **226 rust tests** (6 `#[ignore]`d live) / `tsc` / eslint / prettier / **9 vitest** / `vite build`. New bindings: `GenerationHandle`, `GenerationState`. |

### 17.6 re-run recipe

```
LOCALAI_LLAMA_SERVER=<repo>/runtime/llama-server/llama-server.exe \
LOCALAI_TEST_GGUF=<repo>/models/qwen2.5-0.5b-instruct-q4_k_m.gguf \
cargo test --manifest-path src-tauri/Cargo.toml conversation::live_tests -- --ignored --nocapture --test-threads=1
```

---

## Decisions taken this phase

- **Message content taxonomy** (the plan's open question) — **resolved by the
  frozen Phase 7 contract**, no ADR needed:
  `MessageContent::{ Text{text}, Audio{asset: AssetId, transcript}, Image{asset:
  AssetId, caption} }`. Media attaches as a **content-addressed blob** referenced
  by `AssetId`; the blob store (`blobs/<sha256[0:2]>/<sha256>`) lands with the
  first blob feature — Phase 18 (voice audio) or Phase 22 (images). The engine
  renders a turn from its text / transcript today and is unchanged when audio
  arrives.
- **No "truncated" flag** — `GenerationMeta.stop_reason` (`Cancelled` / `Error`)
  is the truncation signal; the UI already reads it.
- **`send` keeps its own up-front `Conflict` check** — so a rejected send never
  leaves an orphan user turn (the check is before `add_user_turn`).
- **`chat_state` is a query, not a push event** — the per-request `Channel` drives
  the UI; a broadcast event for cross-subsystem observers (barge-in) is Phase 19.

---

## Not done here (by design)

- Voice / persona / memory / character behaviour — they *call* the engine later.
- Configurable sampling / context window — Phase 20.
- A generation queue — one at a time stays (Phase 24 scheduler).

**Phase 17 COMPLETE** — all 6 gate items pass. The conversation engine is the
single formalized path. Pointer → Phase 18.
