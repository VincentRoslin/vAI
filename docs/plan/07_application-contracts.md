# Phase 7 — Application Contracts

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Strongly-typed, serializable contracts shared across the IPC and worker
boundaries: tasks, model metadata + state, generation requests + events,
streaming events, cancellation, structured errors, conversations, messages,
resource reservations, worker jobs. No model names embedded in business logic.

## Depends on
Phase 6.

## Not in this phase
- Behaviour that uses the contracts (no inference, no persistence logic).
- Worker or backend implementations.

## Architecture notes
- Contracts are their own types, distinct from domain types; domain↔DTO
  conversion is explicit.
- IDs are newtypes (`TaskId`, `ModelId`, …), not bare strings.
- Errors are structured with a stable machine-readable `kind` + human message.
- Cancellation and progress are representable in the contract, not out-of-band.

## Performance notes
- Serialization format per the Phase 3 IPC ADR; measure round-trip cost for a
  representative token-stream event (target: negligible vs token latency).

## Step outline
1. Define the ID newtypes + their serialization.
2. Define the error taxonomy (boundary enum + `kind` discriminant).
3. Define task + task-status contracts (unique ids, progress, cancellation).
4. Define model metadata + model-state contracts.
5. Define generation request + streaming event contracts (token delta, done,
   error, cancelled).
6. Define conversation + message contracts.
7. Define resource-reservation + worker-job contracts.
8. Generate / write the TypeScript side; ensure it compiles.
9. Serialization + deserialization round-trip tests for every contract.
10. Invalid-payload rejection tests (missing field, wrong type, unknown variant).
11. Document every contract (purpose, fields, evolution rules).

## Verification gate
1. All contracts compile (Rust + TS).
2. Round-trip serialize/deserialize tests pass for every contract.
3. Invalid payloads are rejected with a typed error, tested.
4. `git grep` finds no model-name string literal in non-contract code.
5. Contract documentation exists and covers evolution rules (additive-only, etc.).

## ADRs / open questions
- May raise an ADR on internally-tagged vs adjacently-tagged enums if TS
  narrowing is awkward.
