# 06 — Phase 7: Application Contracts

**Date:** 2026-09-06
**Branch:** `main` (local; no remote).
**Method:** new `src-tauri/src/contracts/` module + `ipc::error` extension;
`ts-rs` regeneration; a round-trip + rejection test sweep; a serialization perf
probe; full check suite.

Governing ADRs: **ADR-0002** (IPC design), **ADR-0013** (worker transport).
Plan: `docs/plan/07_application-contracts.md`.

---

## What landed

| Area | Types | File |
| ---- | ----- | ---- |
| IDs | `TaskId` `ModelId` `ConversationId` `MessageId` `ReservationId` `WorkerJobId` `DownloadId` `AssetId` | `contracts/ids.rs` |
| Errors | `AppError` (9 `kind`s) + `ErrorEnvelope` | `ipc/error.rs` |
| Tasks | `TaskKind` `TaskState` `TaskStatus` `CancelRequest` | `contracts/task.rs` |
| Models | `ModelKind` `ModelBackend` `Quant` `ModelCapabilities` `ModelMetadata` `ModelState` | `contracts/model.rs` |
| Generation | `SamplingParams` `GenerationRequest` `StopReason` `GenerationEvent` | `contracts/generation.rs` |
| Conversation | `Role` `MessageContent` `GenerationMeta` `Message` `ConversationKind` `Conversation` | `contracts/conversation.rs` |
| Resource | `ResourceKind` `ReservationState` `Reservation` | `contracts/resource.rs` |
| Worker | `WorkerKind` `WorkerHello` `WorkerRequest` `WorkerResult` `WorkerResponse` + `WORKER_PROTOCOL_VERSION` | `contracts/worker.rs` |

TS: 42 files in `src/bindings/` (was 5); single import surface `src/lib/contracts.ts`.
Docs: `docs/contracts.md` (groups + serialization + evolution rules).

---

## Gate — execution record

| # | Check | Result |
| - | ----- | ------ |
| 1 | All contracts compile — `cargo build` + `npx tsc --noEmit` | **PASS** — both clean, zero warnings |
| 2 | Round-trip serialize/deserialize for every contract type | **PASS** — `contracts::tests` sweep: `ids_round_trip_*`, `task_contracts_round_trip`, `model_contracts_round_trip`, `generation_contracts_round_trip`, `conversation_contracts_round_trip`, `resource_contracts_round_trip`, `worker_contracts_round_trip`, `every_app_error_kind_serializes_to_its_tag`, `error_envelope_carries_task_id` — 66 Rust tests pass |
| 3 | Invalid payloads rejected with a typed error, tested | **PASS** — `empty_id_is_rejected_on_construction_and_deserialization`, `unknown_app_error_kind_is_rejected`, `unknown_task_state_variant_is_rejected`, `unknown_generation_event_tag_is_rejected`, `unknown_worker_result_status_is_rejected`, `message_missing_role_is_rejected`, `task_status_rejects_out_of_range_progress` (incl. NaN), `sampling_params_validate_rejects_bad_ranges` |
| 4 | No model-name literal in non-contract code | **PASS** — `rg -i 'llama|whisper|chatterbox|krea|silero|insightface|flux' src-tauri/src` outside `contracts/` → nothing; inside `contracts/` → nothing (doc comments genericised) |
| 5 | `docs/contracts.md` exists + documents evolution rules | **PASS** — additive-only, `#[non_exhaustive]` on `AppError`, exhaustive-match policy for data enums, worker `protocol_version` bump policy, ID-opacity rule |
| 6 | `src/bindings/` regenerated + committed; `git diff --exit-code` clean | **PASS** — after staging, `scripts/check.mjs` "ipc bindings in sync" ✓ |
| 7 | Perf number for `TokenDelta` round-trip recorded | **PASS** — `token_delta_round_trip_perf`: **~3.3–3.6 µs/op** (10k iters; two runs 3251 / 3576 ns). Ceiling asserted at 50 µs. Negligible vs per-token LLM latency. |
| 8 | No behaviour / no new `invoke` handlers | **PASS** — diff touches only `contracts/`, `ipc/{mod,error}.rs`, `lib.rs` (one `pub mod`), `src/lib/contracts*`, `src/bindings/`, docs. `invoke_handler!` unchanged (`app_ready`, `app_ping`, `frontend_log`). |

Full `node scripts/check.mjs`: **all green** (rust fmt · clippy `-D warnings` · 66 rust tests · bindings in sync · prettier · eslint · tsc · 5 vitest · vite build).

---

## Decisions taken at phase entry

- **`Option<T>` → `T | null` (required key, nullable)**, not an optional key.
  `ts-rs` 10 does not translate `#[serde(skip_serializing_if)]` (it emits a parse
  warning and ignores it), so those attributes are not used. Explicit `null` is a
  clean, unambiguous contract; the hot streaming types (`TokenDelta`, `Done`)
  have no optional fields, so there is no wire-size cost where it matters.
- **ID newtypes hand-implement `Serialize`/`Deserialize`** (bare string +
  validation) rather than `#[serde(try_from/into)]`, so `ts-rs` sees no serde
  attribute it cannot parse — zero build warnings.
- **Worker `payload` / `Ok.data` are `serde_json::Value`** (`#[ts(type =
  "unknown")]`); per-worker body schemas belong to phases 18 / 19 / 27. The
  `serde-json-impl` ts-rs feature was tried and dropped — it emitted a stray
  `src-tauri/bindings/serde_json/JsonValue.ts`.
- **`seed: Option<u32>`** (32-bit, matches common LLM runtimes) to avoid a
  `bigint` in the TS contract.
- Scope held to the plan's list — no character / persona / relationship /
  config / registry contracts (those come with their phases; ARQ-15 relationship
  stages still being tuned).

## Follow-ups (not blocking)

- Re-evaluate a `ts-rs` 11/12 bump at a later docs/tooling pass — it may restore
  optional-key generation from `skip_serializing_if`. Not worth the two-major
  bump mid-Phase-7.
- `duration_ms: u64` in `GenerationMeta` renders as `bigint` in TS — acceptable
  for a read-only field; revisit if it becomes awkward in the UI (Phase 16/30).

**Phase 7 complete.** Pointer → Phase 8 (Configuration).
