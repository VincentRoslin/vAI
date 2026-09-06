# Phase 17 — Conversation Engine

> **Status: COMPLETE** (2026-09-06) — all 6 gate items pass. Evidence
> `docs/verification/16_phase17_engine.md`. `ConversationEngine` is the one
> formalized path; the Phase 16 live gate re-ran on it with no regression
> (TTFT ≈ 44 ms).

> **Architecture frozen at Phase 5.** Governing: **ADR-0002** (IPC), **ADR-0009**
> (persistence), Phase 7 contracts (`Conversation` / `Message` / `MessageContent`
> / `GenerationEvent` / `GenerationMeta`), `docs/spec/AI_PIPELINES.md`.
> **No new ADR** — the message-content taxonomy is already the frozen Phase 7
> contract; this phase ratifies it in practice.

## Objective
Turn the Phase 16 thin service into **the one conversation engine** that voice
(18/19), personas (20), memory (21), and characters (25/26) all use. Formalize:
conversation lifecycle · **typed** turns · an explicit streaming state machine ·
first-class cancellation · generation metadata · persistence · restart. There is
**exactly one** engine.

## Depends on
Phase 16 (the vertical slice proved the flow — this reshapes it).

## Not in this phase
- Voice / persona / memory / character *behaviour* — they call the engine later.
- New UI beyond re-hosting text chat unchanged.
- Configurable sampling / context window (Phase 20).
- A generation **queue** — one at a time stays (Phase 24 scheduler).

## Architecture notes
- **The module stays `src-tauri/src/conversation/`.** `ConversationService` is
  **renamed `ConversationEngine`**; the doc comment stops calling it "thin". No
  parallel path exists (Phase 16 built one path) — nothing to delete; a
  `git grep ConversationService` returns nothing after the rename (gate 2).
- **Split `send` into two seams** so a voice caller can drive it:
  - `add_user_turn(conversation_id, content: MessageContent) -> Message` —
    persist one user turn. **Typed content** (`Text` now; `Audio { asset,
    transcript }` when Phase 18 lands — no schema break).
  - `generate(conversation_id, model_id, sink) -> TaskId` — run one LLM
    generation over the conversation's history, stream events to `sink`, persist
    the assistant turn on the terminal frame.
  - `send(...)` stays as a **text convenience** = `add_user_turn(Text) +
    generate` (what `chat_send` calls; the text UI is unchanged).
- **Explicit streaming state machine**, owned by the engine:
  `Idle → Generating { task_id, conversation_id } → Idle` (the terminal frame,
  `Done`/`Cancelled`/`Error`, returns it to `Idle`). Exposed as
  `engine.generation_state() -> GenerationState`; `is_generating()` kept as a
  convenience. A `chat_state` IPC command returns it. (Barge-in in Phase 19 will
  add a push event; a query is enough here.)
- **Prompt rendering** (`prompt::render_chatml`) reads `MessageContent::Text` and
  `MessageContent::Audio { transcript: Some(t), .. }` — so a voice turn renders
  as its transcript with no engine change later. `Image` and untranscribed audio
  are skipped for now.
- **Truncated turns**: no separate flag — `GenerationMeta.stop_reason`
  (`Cancelled` / `Error`) is the truncation signal. Recorded as the decision.
- Backend-agnostic: the engine calls `LlmInstance` via `lifecycle.instance(id)
  .as_llm()`; nothing llama-specific leaks in.

## Performance notes
- Re-hosting must not regress the Phase 16 numbers: end-to-end TTFT ≈ 33 ms,
  cancel-to-stop prompt, no throughput change. The live gate re-measures.

## Steps

**17.1 — Rename + module framing**
    Do:     `ConversationService` → `ConversationEngine` (type, `new`, all refs
            in `lib.rs`, `ipc/commands.rs`, tests). Module doc: "the one shared
            conversation engine". `Arc<ConversationEngine>` in managed state.
    Verify: `cargo build`; `git grep -n ConversationService` → nothing.

**17.2 — `GenerationState` contract + engine state machine**
    Do:     `contracts::conversation` — `GenerationState` (`{ generating:
            Option<GenerationHandle> }`, `GenerationHandle { task_id,
            conversation_id }`). Engine: replace the bare `Mutex<Option<InFlight>>`
            read path with `generation_state()`; `Idle`/`Generating` transitions
            logged. `chat_state` IPC command + `src/lib/ipc.ts` wrapper.
    Verify: `cargo test conversation::` — `state` is `Idle` at rest, `Generating`
            with the right ids mid-stream, back to `Idle` after Done/Cancel/Error.

**17.3 — Split `send` → `add_user_turn` + `generate`**
    Do:     `add_user_turn(id, content: MessageContent) -> AppResult<Message>`
            (rejects an empty `Text`; `NotFound` for a missing conversation).
            `generate(id, model, sink) -> AppResult<TaskId>` (rejects if
            `Generating`; spawns `run_generation`). `send` = the two, for text.
    Verify: `cargo test conversation::` — `add_user_turn` persists a typed turn;
            `generate` alone (turn already added) streams + persists; `send`
            still works; a 2nd `generate` while one runs → `Conflict`.

**17.4 — Typed prompt rendering**
    Do:     `render_chatml` handles `Audio { transcript: Some(_), .. }` (renders
            the transcript) in addition to `Text`; skips `Image` /
            untranscribed audio.
    Verify: `cargo test conversation::prompt` — a mixed `[Text, Audio(Some),
            Audio(None), Image]` transcript renders text + the one transcript,
            in order.

**17.5 — Engine unit tests (lifecycle / streaming / cancel / persistence / restart)**
    Do:     Re-point the Phase 16 tests at the new API; add the state-machine
            assertions. Keep the scripted `LlmInstance` fake.
    Verify: `cargo test conversation::` all green.

**17.6 — Re-run the Phase 16 gate on the re-hosted chat**
    Do:     `conversation::live_tests` updated to the new API. Run it with the
            real `llama-server` + Qwen 0.5B. Manual `tauri dev` spot check.
    Verify: send → stream → persist; cancel mid-gen → partial persisted, model
            reusable; restart recovery; concurrent → `Conflict`; TTFT ≈ 33 ms
            (no regression).

**17.7 — Docs + gate + commit**
    Do:     `node scripts/check.mjs`; `docs/verification/16_phase17_engine.md`;
            `src-tauri/README.md`, `docs/spec/ARCHITECTURE.md` §2/§3,
            `docs/contracts.md`, `ROADMAP.md`. Commit.
    Verify: check suite green; every gate item recorded.

## Verification gate (physically executed)
1. Engine unit tests pass — lifecycle, streaming, cancellation, persistence,
   restart, **state machine**. *(17.2, 17.5)*
2. Text chat runs entirely on the engine; the old `ConversationService` name is
   gone (`git grep`). *(17.1)*
3. The full Phase 16 verification gate still passes on the re-hosted chat
   (live test). *(17.6)*
4. No performance regression vs the Phase 16 baseline (TTFT ≈ 33 ms). *(17.6)*
5. `add_user_turn` accepts typed content — a non-`Text` turn persists and renders
   from its transcript. *(17.3, 17.4)*
6. Full check suite green; bindings regenerated. *(17.7)*

## ADRs / open questions
- **Message content taxonomy** (the plan's open question): **resolved by the
  frozen Phase 7 contract** — `MessageContent::{Text, Audio{asset: AssetId,
  transcript}, Image{asset: AssetId, caption}}`. Media is a content-addressed
  blob referenced by `AssetId`; the blob store lands with the first blob feature
  (Phase 18 audio / Phase 22 image). No ADR — recorded in the verification doc.
- No new ADR.
