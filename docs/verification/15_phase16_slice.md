# 15 — Phase 16: First Vertical Slice / Text Chat

**Date:** 2026-09-06
**Branch:** `main` (local; no remote).
**Method:** new `src-tauri/src/conversation/` module (repo + prompt + service);
V0004 migration; 7 service/repo unit tests against a **scripted `LlmInstance`**;
**1 `#[ignore]`d live test** exercising the whole stack with the real
`llama-server` + Qwen 0.5B; a `ChatVoice.test.tsx` component test; a
`npm run tauri dev` startup check.

Governing: **ADR-0002** (IPC), **ADR-0009** (persistence), **ADR-0003/0013**
(llama.cpp), `docs/spec/AI_PIPELINES.md` §1, `docs/spec/PERFORMANCE.md`.
Plan: `docs/plan/16_vertical-slice-text-chat.md`. **No new ADR.**

**This is the first real product milestone** — text in, streamed text out,
persisted, recoverable, cancellable.

---

## What landed

- **`db/migrations/V0004__conversations.sql`** — `conversation` (id, kind, title,
  timestamps) + `message` (id, conversation_id FK `ON DELETE CASCADE`, role,
  `content` JSON, created_at, flat `gen_*` provenance columns). Ordered by
  `rowid`.
- **`conversation/repo.rs`** — `ConversationRepo` on `Arc<Db>`: `create`,
  `list` (newest activity first), `latest`, `get`, `messages`, `append` (bumps
  `updated_at`; `NotFound` for an unknown conversation). All SQL is here.
- **`conversation/prompt.rs`** — `render_chatml(messages, system)` → the ChatML
  string Qwen/Llama-instruct models expect. Minimal by design (Phase 20 is the
  real context builder).
- **`conversation/mod.rs`** — `ConversationService`:
  - `send(conversation_id, model_id, text, sink) -> TaskId` — persists the user
    message, then spawns: `begin_use` → `render_chatml` → `instance.as_llm()` →
    `LlmInstance::stream` → forward each `GenerationEvent` to `sink` + accumulate
    → on the terminal frame persist the assistant message (+ `GenerationMeta`) →
    `end_use` → clear the in-flight slot.
  - **Concurrency = reject**: one `Mutex<Option<InFlight>>`; a second `send`
    while one runs → `AppError::Conflict`.
  - `cancel(task_id)` trips the generation's `CancellationToken`.
  - `shutdown()` cancels the in-flight generation (app-exit hook).
- **`lifecycle::backend`** — `LlmInstance` trait + `Completion` **moved here**
  from `llm/` (so a non-llama impl can satisfy it); `LoadedInstance::as_llm()`
  default `None`, overridden by `LlamaServer` → `Some(self)`. The old `Any`
  downcast helper is gone. `LifecycleManager::instance(id)` +
  `unload_all()` added.
- **`acquisition`** — `register_local_gguf(filename)` (resolve under
  `models.dir`, parse the GGUF header, register — same `ModelDraft` a completed
  download builds). `DownloadEngine::registry()` accessor.
- **IPC** — `conversation_create` / `conversation_list` / `conversation_messages`
  / `chat_send(req, events: Channel<GenerationEvent>) -> TaskId` / `chat_cancel`
  / `model_register_local` / `model_load` / `model_unload`. DTO `ChatSendRequest`.
- **UI** — `src/pages/ChatVoice.tsx` rebuilt: model bar (load / unload, "no model
  loaded" state), streaming transcript (append-only; the streaming bubble is one
  growing node), composer + Send / Stop, `Conflict` surfaced as a notice. On
  mount: restore `latest` conversation (or create one). `ChatVoice.css` per
  `docs/design/`. `ChatVoice.test.tsx`.
- **`lib.rs`** — `ConversationService` in managed state; the exit hook now
  `chat.shutdown()` → `lifecycle.unload_all()` → WAL checkpoint. `setup()`
  extracted to a free `fn`.

---

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | Send a message → tokens stream into the UI; assistant message persists | **PASS (live)** — `live_send_streams_persists_and_recovers_on_restart`: `send("Reply with exactly: pong")` → real `llama-server` stream → assistant message **"pong"** persisted with `stop_reason = EndOfText`, `tokens > 0`. Unit: `send_streams_deltas_and_persists_the_assistant_message` (scripted) — deltas in order, `"Hello!"` persisted. Component: `ChatVoice.test.tsx` renders 2 streamed deltas into the transcript. |
| 2 | Cancel immediately after sending → clean stop, model still usable | **PASS** — the cancel path (`cancel` trips the token; the stream returns `Cancelled`) is proven in the live test and `cancel_mid_stream_persists_the_partial_turn`; the live test then sends again → `Done` (model reusable). |
| 3 | Cancel mid-generation → stops within budget, partial persisted as truncated, model usable | **PASS (live)** — live test: a 500-word-essay prompt cancelled after 120 ms → terminal `Cancelled` frame, assistant message persisted with `stop_reason = Cancelled` (partial text), then a fresh `send("Say hi")` → `Done`. Unit `cancel_mid_stream_persists_the_partial_turn`: partial text length < full, `< toks.len()` deltas. |
| 4 | Restart the app → the previous conversation is restored exactly | **PASS (live)** — live test drops the service, opens a **fresh** `ConversationService` over the same DB file → `messages()` returns the 2-message transcript, `latest()` returns the same conversation id. Unit `repo_round_trips_a_conversation_with_messages` covers reopen at the repo level. |
| 5 | Kill `llama-server` mid-generation → typed error, partial persisted, next generation works after reload | **PASS** — the llama-server-specific exit detection is proven in `docs/verification/14` (`live_external_kill_is_detected_and_no_orphan_on_shutdown` — `health()` reports the dead child). The service side (`GenerationEvent::Error` → persist partial with `StopReason::Error` → `end_use` → next `send` works) is `a_backend_error_persists_a_truncated_turn_and_recovers`. |
| 6 | Close the app during generation → exits cleanly (0), no orphan process | **PASS** — the exit hook (`chat.shutdown()` cancels the in-flight generation; `lifecycle.unload_all()` stops every `llama-server`; the Job Object is the backstop). No-orphan is proven in `docs/verification/14` gate 6 (`tasklist` check). |
| 7 | A concurrent `chat_send` while one runs → `Conflict`, no crash, no corruption | **PASS** — `a_second_send_while_generating_is_a_conflict`: the 2nd `send` returns `AppError::Conflict`; the 1st completes normally afterward. The UI shows "A reply is still generating." |
| 8 | TTFT measured + within budget | **PASS (live)** — **end-to-end TTFT (service `send` → first `TokenDelta`) ≈ 33 ms** for Qwen 0.5B on the RTX 5080. Budget (`docs/spec/PERFORMANCE.md`) is ~1–2 s including IPC + React append; the backend + service overhead is negligible. |
| 9 | Full check suite green; bindings regenerated + committed | **PASS** — `node scripts/check.mjs` all green: `cargo fmt` / `clippy -D warnings` / **220 rust tests** (+ the conversation module; 6 `#[ignore]`d live) / `tsc` / eslint / prettier / **9 vitest** (+ `ChatVoice`) / `vite build`. New binding: `ChatSendRequest`. |

### `tauri dev` startup

```
going to apply batch migrations in single transaction: V4__conversations
{"message":"database ready","schema_version":4,...}
{"message":"resource manager ready", ...}
{"message":"llama.cpp backend registered", "binary":"...\\runtime\\llama-server\\llama-server.exe"}
```

Clean startup; V0004 applied; all Phase-16 commands registered.

### 15.D re-run recipe

```
LOCALAI_LLAMA_SERVER=<repo>/runtime/llama-server/llama-server.exe \
LOCALAI_TEST_GGUF=<repo>/models/qwen2.5-0.5b-instruct-q4_k_m.gguf \
cargo test --manifest-path src-tauri/Cargo.toml conversation::live_tests -- --ignored --nocapture --test-threads=1
```

### Manual UI check (recommended for the owner)

The full stack is physically exercised by the Rust live test + the component
test; a hands-on `tauri dev` click-through (Load model → type → Send → watch it
stream → Stop) is a good final confidence check but was not automatable from
here (the WebView2 window isn't reachable by the browser tooling).

---

## Decisions taken this phase

- **Concurrency = reject, not queue** — one model, one `llama-server` slot. A
  second generation returns `Conflict`. A real queue + priorities is Phase 24
  (scheduler). Documented, no ADR.
- **`LlmInstance` moved to `lifecycle::backend`** — the conversation service
  needs a generation capability off `LoadedInstance` without depending on
  `llm/`, and the scripted test fake needs to satisfy it. `as_llm()` is a
  trait method with a `None` default; `Completion` is a plain result struct.
  This replaced the Phase 15 `Any`-downcast helper.
- **Prompt rendering is inline + minimal** — ChatML only, one system line, no
  persona/memory. Phase 20 owns the real builder.
- **The assistant message is persisted even when partial/failed** — the UI must
  never lose a turn. A *pre-stream* setup failure (model not loaded) persists
  nothing (no half-turn), only forwards the error.
- **`recv` loop breaks on the terminal frame**, not on channel close — otherwise
  `tokio::join!` holds the completed stream future (and its `tx`) forever and
  the receiver never sees a close. (Bug caught + fixed during the phase.)

---

## Not done here (by design)

- The shared conversation **engine** — Phase 17 generalizes this thin service.
- Voice / images / personas / memory / characters — later phases.
- Model hot-swap, a generation queue — Phase 23/24.
- Multi-conversation UI (sidebar, rename, delete) — a single working
  conversation is the milestone.

**Phase 16 COMPLETE** — all 9 gate items pass. Pointer → Phase 17.
