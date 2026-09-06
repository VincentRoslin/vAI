# Phase 16 — First Vertical Slice / Text Chat

> **Status: COMPLETE** (2026-09-06) — all 9 gate items pass. Evidence
> `docs/verification/15_phase16_slice.md`. A live full-stack Rust test on the
> RTX 5080 (register → real `llama-server` → send → stream → persist → restart
> recovery → cancel → reuse); end-to-end TTFT ≈ 33 ms. First real milestone.

> **Architecture frozen at Phase 5.** Governing: **ADR-0002** (IPC —
> Commands/Events/Channels, one `AppError`), **ADR-0009** (persistence),
> **ADR-0003/0013** (llama.cpp adapter), `docs/spec/AI_PIPELINES.md` §1,
> `docs/spec/PERFORMANCE.md` (TTFT budget). Concrete step detail below.

## Objective
The whole stack does one useful thing reliably: React → typed IPC → Rust →
**conversation service** → `LlmInstance` → `llama-server` → streamed tokens → UI,
with working cancellation, SQLite persistence, and restart recovery. **Text chat
only.** This is the first real product milestone.

## Depends on
Phase 15 (`llm` adapter, `LlmInstance`), Phase 14 (`lifecycle` — load/unload/
`begin_use`), Phase 13 (resources — reservation on load), Phase 11 (registry),
Phase 9 (`db`), Phase 7 (`conversation` + `generation` contracts), Phase 6 (shell).

## Not in this phase
- Voice, images, personas, memory, characters, discovery, embeddings, RAG.
- The **shared** conversation engine (Phase 17 generalizes this thin service).
- Multi-conversation UX polish — one working conversation is the gate.
- Model hot-swap / eviction (Phase 23), a scheduler / generation queue (Phase 24).
- A real context/prompt builder (Phase 20) — Phase 16 renders a minimal ChatML
  prompt inline.
- HF token, image/audio message content.

## Architecture notes
- **New module `src-tauri/src/conversation/`** — a *thin service*, not the
  engine. `mod.rs` (`ConversationService` + the in-flight-generation registry),
  `repo.rs` (all SQL — Rust owns it, ADR-0009 / Article I), `prompt.rs` (ChatML
  rendering), `tests.rs`.
- **Ownership:** the service owns conversation/message state (in SQLite via
  `repo`). The lifecycle manager owns the loaded model; the service calls
  `begin_use`/`end_use` around a generation. The UI renders; it holds only view
  state.
- **Concurrency policy (v1): reject.** One model, one `llama-server` slot →
  `chat_send` while any generation is in flight returns `AppError::Conflict`.
  A queue is Phase 24. **Not an ADR** — a documented v1 simplification.
- **Generation flow** (`chat_send`):
  1. validate; reject if a generation is already running.
  2. `repo.append_message(User, text)` — persisted before the command returns.
  3. spawn a task: `begin_use(model)` → render prompt from the conversation's
     messages → `LlmInstance::stream(prompt, params, tx, cancel)`; forward each
     `GenerationEvent` to the command's `Channel`; on `Done` /
     `Error` / `Cancelled`, `repo.append_message(Assistant, accumulated_text,
     GenerationMeta{ stop_reason, tokens, duration_ms })`, `end_use`, drop the
     in-flight entry.
  4. return the `TaskId` immediately.
- **Cancellation:** `Mutex<Option<InFlight { task_id, cancel: CancellationToken
  }>>` on the service. `chat_cancel(task_id)` trips the token → the `stream`
  future returns → the partial assistant text is persisted with
  `StopReason::Cancelled`.
- **Failure:** a backend crash mid-stream → `GenerationEvent::Error` → persist
  the partial assistant message with `StopReason::Error` (truncated), forward the
  error frame, `end_use`. The Phase-14 liveness monitor moves the model to
  `Failed`; the user re-loads it for the next turn.
- **Model registration:** `models/` holds the Qwen GGUF but the app DB doesn't
  know it. New `AcquisitionService::register_local_gguf(filename)` — resolve
  `<models.dir>/<filename>`, parse the GGUF header (`acquisition::gguf`), build
  the same `ModelDraft` as a completed download, `registry.register`. Exposed as
  `model_register_local`.
- **Clean shutdown:** extend the `RunEvent::ExitRequested` hook — cancel any
  in-flight generation, `unload` every loaded model (the Job Object already
  guarantees no orphan), then the existing WAL checkpoint.

## Performance notes
- **TTFT budget** (`docs/spec/PERFORMANCE.md`): first token visible in the UI
  within ~1–2 s with the model loaded. Backend TTFT is ~23 ms (Phase 15) — the
  budget covers IPC + React append. Measure end-to-end (send click → first
  delta rendered) and record it.
- The transcript **appends**; it never re-renders the whole list on a delta
  (key by message id; the streaming message is a single growing node).

## Steps

**16.1 — V0004 migration: `conversation` + `message`**
    Do:     `db/migrations/V0004__conversations.sql` — `conversation` (id, kind,
            title, created_at, updated_at) + `message` (id, conversation_id FK
            `ON DELETE CASCADE`, role, content JSON, created_at, gen_model,
            gen_stop_reason, gen_tokens, gen_duration_ms) STRICT; index on
            `message(conversation_id, rowid)`.
    Verify: `cargo test db::` — migrate from V0003 → V0004 idempotent; FK
            cascade works (delete conversation → messages gone).

**16.2 — `conversation/repo.rs`**
    Do:     `ConversationRepo` on `Arc<Db>`: `create(kind) -> Conversation`,
            `list() -> Vec<Conversation>` (newest `updated_at` first),
            `get(id)`, `messages(id) -> Vec<Message>` (rowid order),
            `append(id, role, content, Option<GenerationMeta>) -> Message`
            (bumps `conversation.updated_at`), `latest() -> Option<Conversation>`.
            IDs = UUIDv4 (ADR-0017).
    Verify: `cargo test conversation::repo` — round-trip a conversation + 3
            messages incl. one with `GenerationMeta`; `list` ordering; `latest`;
            survives a reopen.

**16.3 — `conversation/prompt.rs`**
    Do:     `render_chatml(messages: &[Message], system: &str) -> String` —
            `<|im_start|>{role}\n{text}<|im_end|>\n` per message +
            `<|im_start|>assistant\n` tail. Only `MessageContent::Text` (skip
            others for now). A default system string.
    Verify: `cargo test conversation::prompt` — a 2-turn transcript renders the
            exact expected string; empty transcript still has the system + tail.

**16.4 — `ConversationService`: create / list / messages**
    Do:     `mod.rs` — `ConversationService::new(db, lifecycle)`; thin wrappers
            over `repo`. `InFlight` state (`Mutex<Option<..>>`).
    Verify: `cargo test conversation::service_basic` — create → list → messages.

**16.5 — `send` + streaming + persistence (fake `LlmInstance`)**
    Do:     `send(conversation_id, model_id, text, sink: impl Fn(GenerationEvent))
            -> AppResult<TaskId>`. Reject if `InFlight` is `Some`. Persist the
            user message. Spawn: `begin_use` → `render_chatml` → resolve the
            `LlmInstance` from the lifecycle manager (`llm::as_llm`) → `stream`
            → forward events + accumulate text → on terminal, persist the
            assistant message + `end_use` + clear `InFlight`.
    Verify: `cargo test conversation::send_persists` — with a **fake
            `LlmInstance`** (reuse the pattern from `llm`/`lifecycle` tests):
            deltas arrive in order; assistant message persisted with the right
            `stop_reason` + token count; a 2nd concurrent `send` → `Conflict`.

**16.6 — Cancellation**
    Do:     `cancel(task_id)`. Trip the token; the fake instance's `stream`
            observes it and stops; persist the partial text with
            `StopReason::Cancelled`.
    Verify: `cargo test conversation::cancel` — mid-stream cancel → a `Cancelled`
            frame, partial text persisted, `InFlight` cleared, a following `send`
            works.

**16.7 — Failure handling**
    Do:     Fake instance emits `GenerationEvent::Error`. Persist partial text
            with `StopReason::Error`; forward the error; `end_use`; clear
            `InFlight`.
    Verify: `cargo test conversation::backend_error` — error frame forwarded,
            truncated assistant message persisted, next `send` works.

**16.8 — `register_local_gguf` + model IPC**
    Do:     `AcquisitionService::register_local_gguf(filename)`. IPC:
            `model_register_local(filename) -> ModelId`, `model_load(id)`,
            `model_unload(id)` (→ `lifecycle`), keep `lifecycle_status`.
    Verify: `cargo test acquisition::register_local` — a fixture GGUF in a temp
            model dir → registered `Llm` entry with parsed quant/context.

**16.9 — Chat IPC**
    Do:     `ipc/commands.rs` — `conversation_create`, `conversation_list`,
            `conversation_messages`, `chat_send(req, events: Channel<GenerationEvent>)
            -> TaskId`, `chat_cancel(task_id)`. `DTO`s (`ChatSendRequest {
            conversation_id, model_id, text }`). `ConversationService` in managed
            state. `src/lib/ipc.ts` + `contracts.ts`.
    Verify: `cargo build`; bindings regenerate; `npm run typecheck`.

**16.10 — Chat UI**
    Do:     Replace `src/pages/ChatVoice.tsx` with the real slice:
            - a model bar: `lifecycle_status` + `models_list`; buttons to
              `model_register_local` (the known Qwen filename if present in
              `models_list` it's already there), `model_load` / `model_unload`;
              a clear "no model loaded" state.
            - transcript: message list, the streaming assistant message a single
              growing node; auto-scroll.
            - composer: textarea + Send; Stop button while generating.
            - on mount: `conversation_list` → if empty `conversation_create`,
              else open `latest`; `conversation_messages`.
            Styling per `docs/spec/UI_GUIDELINES.md` + `docs/design/`. Presentation
            only — all state from IPC.
    Verify: `npm run test` — a component test: renders the composer; a mocked
            `chat_send` streams 2 deltas into the transcript; Stop calls
            `chat_cancel`. `npm run tauri dev` — real end-to-end (see gate).

**16.11 — Restart recovery + clean shutdown**
    Do:     UI already restores `latest` on mount (16.10). Rust: extend the exit
            hook — `ConversationService::shutdown()` cancels the in-flight gen;
            `lifecycle.unload_all()`; then the WAL checkpoint.
    Verify: covered by gate items 4 + 6 (below).

**16.12 — Gate run + docs + commit**
    Do:     `node scripts/check.mjs`; run the manual gate script (below) on
            `tauri dev` with the real Qwen model; `docs/verification/15_phase16_slice.md`;
            `src-tauri/README.md`, `docs/spec/ARCHITECTURE.md` §2/§3/§5,
            `docs/contracts.md`, `docs/spec/PERFORMANCE.md`, `ROADMAP.md`. Commit.
    Verify: check suite green; every gate item recorded with evidence.

## Verification gate (physically executed, `tauri dev` + real Qwen 0.5B)
1. Send a message → tokens stream into the UI; the assistant message persists
   (visible after `conversation_messages` refetch).
2. Cancel immediately after sending → clean stop; the model is still usable
   (next send works).
3. Cancel mid-generation → stops within budget; partial text persisted as
   truncated (`StopReason::Cancelled`); model still usable.
4. Restart the app → the previous conversation + messages are restored.
5. Kill `llama-server` mid-generation (`taskkill`) → typed error in the UI;
   partial text persisted (`StopReason::Error`); after `model_load` again the
   next generation works.
6. Close the app during generation → exits cleanly (code 0), no orphan
   `llama-server` (`tasklist`).
7. A concurrent `chat_send` while one runs → `Conflict` surfaced in the UI, no
   crash, no state corruption.
8. End-to-end TTFT (send click → first delta rendered) measured + within budget.
9. Full check suite green; bindings regenerated + committed.

## ADRs / open questions
- **Concurrency = reject** (documented here; Phase 24 scheduler may revisit to
  queue). No new ADR.
- No new ADR. `conversation` schema (V0004) is minimal and grows in Phase 17/21.
