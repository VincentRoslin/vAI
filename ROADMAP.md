# ROADMAP — vAI Bootstrap State Machine

> **Status: WORKING DRAFT.** The phase list below (count, names, scope, ordering,
> and gate details) is **provisional and subject to revision** — it has not been
> ratified against a finalized architecture. Treat it as a planning scaffold, not
> a contract. Expect it to be re-derived once the architecture is frozen (see the
> realignment note in §7).
>
> What *is* stable and should be respected: the **state-tracking discipline** —
> the status vocabulary (§2), the transition rules (§3), the current-state pointer
> (§1), and the principle that a phase is not "done" until its verification was
> physically executed and recorded. Those mechanics carry over even if the phases
> themselves change.
>
> If reality and this file disagree about *progress*, fix the file in the same
> change that fixes reality.

---

## 1. Current State

| Field            | Value                                                        |
| ---------------- | ----------------------------------------------------------- |
| **Phase**        | 2 — Core Configuration & Environment                        |
| **Stage**        | 2.1 — `.env.example` / config-surface enumeration           |
| **Status**       | `NOT STARTED`                                               |
| **Blocked by**   | —                                                          |
| **Last updated** | 2026-09-05                                                  |
| **Updated by**   | bootstrap                                                   |

**Phase 0 — Development Environment Audit: `COMPLETE`** — evidence
`docs/verification/01_env_audit.md` (2026-09-05). Machine is READY for the
desktop-app bootstrap; open decisions (CUDA build strategy, Python env, package
manager, model storage) captured for the architecture phase.

**Phase 1 — Repository & Tooling Bootstrap: `COMPLETE`** (see §4, incl. the
deviation note recording which gate checks were physically executed).

**Only one `(Phase, Stage)` pair is ever `IN PROGRESS`.** Advancing the pointer
is itself a state transition and MUST follow the rules in §3.

---

## 2. Status Vocabulary

| Status         | Meaning                                                                              |
| -------------- | ----------------------------------------------------------------------------------- |
| `NOT STARTED`  | No work begun. Entry criteria may or may not be met.                                |
| `IN PROGRESS`  | Actively being worked. At most one stage repo-wide.                                 |
| `BLOCKED`      | Work cannot proceed; `Blocked by` names the phase/stage/external dependency.        |
| `VERIFIED`     | Verification gate ran and passed, evidence recorded. Not yet accepted.              |
| `COMPLETE`     | `VERIFIED` + reviewed + merged to `main`. Immutable unless explicitly reopened.     |

Phase-level status is the **minimum** of its stage statuses (a phase is
`COMPLETE` only when every stage is `COMPLETE`).

---

## 3. Transition Rules

1. **Forward only through the gate.** A stage moves `IN PROGRESS → VERIFIED` only
   when every check in its **Verification Gate** has been executed and its
   evidence (command output, link, or artifact path) is pasted into the stage's
   **Evidence** line.
2. **No skipping.** Stage `N.k` cannot start until `N.(k-1)` is `VERIFIED` or
   `COMPLETE`, unless the stage's **Entry** line explicitly lists it as parallel-safe.
3. **No phase bleed.** Phase `N` cannot start until Phase `N-1` is `COMPLETE` and
   every dependency in Phase `N`'s **Depends on** line is `COMPLETE`.
4. **Regression reopens.** If a later change breaks a `COMPLETE` phase's gate,
   set that phase back to `IN PROGRESS`, move the current pointer back, and record
   the reason in §5.
5. **Blocked is explicit.** Moving to `BLOCKED` requires a named blocker and a
   dated entry in §5.
6. **Every transition updates §1** (Phase, Stage, Status, Last updated, Updated by)
   **and** §5 (the log).

---

## 4. Phase Ledger

**Provisional (working draft).** The phases below are a first-pass decomposition,
not a ratified plan. Names, scope, count, ordering, and gate specifics will change
— expect a re-derivation toward a subsystem-driven structure once the architecture
is frozen (§7). Do not treat an entry here as a committed requirement.

Legend per phase: **Objective** · **Depends on** · **Stages** · **Verification
Gate** (must be explicit + machine-checkable where possible) · **Exit Criteria**.

Every stage row carries its own status, an **Entry** note, and an **Evidence**
slot to be filled at gate time.

---

### Phase 0 — Development Environment Audit
- **Objective**: Know the actual dev machine (toolchains, GPU/VRAM, build tools,
  disk) before committing to an architecture that depends on them.
- **Depends on**: —
- **Status**: `COMPLETE`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 0.1 | Audit OS, CPU, RAM, GPU/VRAM, driver/CUDA, disk | `COMPLETE` | — | `docs/verification/01_env_audit.md` §Findings |
| 0.2 | Audit toolchains: Rust, Node/npm, Python, Git, CMake | `COMPLETE` | — | same doc §Toolchains |
| 0.3 | Audit native build tooling + Tauri prereqs; verify Rust↔MSVC link chain | `COMPLETE` | 0.2 | same doc — `cargo build` scratch test, exit 0 |
| 0.4 | Record open decisions (CUDA strategy, Python env, pkg manager, model storage) | `COMPLETE` | 0.1–0.3 | same doc §"Open decisions surfaced" |

- **Verification Gate** — execution record (Article IV of `CLAUDE.md`):
  1. Rust / Node / Python / Git present with versions. — **EXECUTED, PASS**
     (rustc 1.98.0, node 24.19.0, python 3.11.9, git 2.52.0).
  2. MSVC build tools + Windows SDK + WebView2 present. — **EXECUTED, PASS**
     (VS Build Tools 2022 17.14, MSVC 14.44, SDK 10.0.26100, WebView2 152.x).
  3. GPU + NVIDIA driver detected; VRAM known. — **EXECUTED, PASS**
     (RTX 5080, 16 GB, driver 610.88, CUDA runtime 13.3, sm_120).
  4. Rust → MSVC → linker chain actually builds+runs an exe. — **EXECUTED, PASS**
     (throwaway `cargo new`/`cargo build`, exit 0, ran "Hello, world!").
  5. Disk headroom recorded. — **EXECUTED** (C: 215 GB free — noted as a planning
     constraint, not a blocker).
- **Non-blocking gaps** (decisions, not prerequisites): CUDA Toolkit / `nvcc` not
  installed; no pnpm; no Python venv/uv tooling; `git core.autocrlf=true`; RAM
  under rated speed; PowerShell 5.1 only.
- **Exit Criteria**: Machine confirmed READY for the desktop bootstrap; gaps
  triaged. **Met** 2026-09-05.

---

### EPOCH A — Foundation (Phases 1–6)

> Note: **Phase 0** (above) precedes this epoch — it was added after the initial
> ledger draft to fill the environment-audit gap noted in §7.

---

### Phase 1 — Repository & Tooling Bootstrap
- **Objective**: A clean repo any contributor can clone, install, lint, format,
  and build with one documented command each.
- **Depends on**: —
- **Status**: `COMPLETE`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 1.1 | Initialize git repo, default branch, branch-naming & PR strategy | `COMPLETE` | — | Repo on `main`; first commit created 2026-09-05 containing `ROADMAP.md` + `CLAUDE.md`. Branch/PR strategy: trunk-based on `main`, still to be written into `DEVELOPMENT.md` (tracked in Phase 14.4). |
| 1.2 | Documentation skeleton + ROADMAP state machine (this file) | `COMPLETE` | 1.1 | `ROADMAP.md` (this file) + `CLAUDE.md` Engineering Constitution authored and committed. Remaining root `*.md` files exist as empty placeholders, to be filled by their owning phases. |
| 1.3 | Choose language / runtime / package manager; commit lockfile | `COMPLETE` | 1.2 | Stack fixed by `CLAUDE.md` Article I: **Tauri (Rust) core + React/TypeScript frontend + isolated Python workers + local SQLite**, local-first desktop app (Article II). Package managers: Cargo (Rust), npm/pnpm (TS), uv/pip (Python). Lockfiles land with the first code in each runtime (Phases 3–5); **not yet committed**. |
| 1.4 | Editor config, formatter, linter, type checker wired | `COMPLETE` | 1.3 | **Not physically verified** — no source tree yet. Toolchain decided (`rustfmt`+`clippy`, ESLint+Prettier+`tsc`, `ruff`); wiring + first clean run deferred to Phase 6.1 (CI) and the first code commit of each runtime. Accepted by project-owner directive 2026-09-05. |
| 1.5 | Pre-commit hooks + commit-message convention enforced | `COMPLETE` | 1.4 | **Not physically verified** — deferred with 1.4 to Phase 6.1. Accepted by project-owner directive 2026-09-05. |
| 1.6 | `README.md` quickstart: clone → install → run in ≤5 commands | `COMPLETE` | 1.4 | **Not physically verified** — `README.md` is an empty placeholder; quickstart cannot exist before the app runs. Content deferred to Phase 5 / Phase 14.4. Accepted by project-owner directive 2026-09-05. |

- **Verification Gate** — execution record (Article IV of `CLAUDE.md`):
  1. `git log --oneline` shows ≥1 commit; working tree clean. — **EXECUTED, PASS** (initial commit 2026-09-05).
  2. Fresh clone + documented install command exits 0. — **NOT EXECUTED** (no install command yet).
  3. Formatter check command reports no diffs. — **NOT EXECUTED** (no formatter wired).
  4. Linter + type checker exit 0 with zero warnings. — **NOT EXECUTED** (none wired).
  5. A deliberately mis-formatted commit is rejected by the pre-commit hook. — **NOT EXECUTED** (no hook).
  6. Every root `*.md` file is non-empty and listed in `README.md`. — **NOT EXECUTED** (placeholders still empty).
- **Deviation note**: Phase 1 was marked `COMPLETE` by **project-owner directive on
  2026-09-05** with gate checks 2–6 unexecuted. The tooling/verification work for
  stages 1.4–1.6 is **carried forward to Phase 6.1 (CI pipeline)** and the first
  code commit of each runtime; Phase 6's own gate re-covers it. This deviation is
  recorded here and in §5 so the state machine stays honest.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.
  Met per the deviation note above.

---

### Phase 2 — Core Configuration & Environment
- **Objective**: One typed, validated configuration surface; no secret or
  environment value read ad hoc from `process.env` / OS env outside it.
- **Depends on**: Phase 1
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 2.1 | `.env.example` enumerating every variable with description + default | `NOT STARTED` | Phase 1 | — |
| 2.2 | Config loader with schema validation; fails fast on missing/invalid | `NOT STARTED` | 2.1 | — |
| 2.3 | Environment tiers (local / test / staging / prod) resolved deterministically | `NOT STARTED` | 2.2 | — |
| 2.4 | Secret handling: sourced from a vault/secret store, never committed | `NOT STARTED` | 2.2 | — |

- **Verification Gate**:
  1. Booting with a required var unset prints a named error and exits non-zero.
  2. Booting with an out-of-range value is rejected by schema validation.
  3. `git grep` for direct env access outside the config module returns nothing.
  4. Secret-scanning tool over full history reports no findings.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 3 — Data Model & Schema Foundation
- **Objective**: Canonical domain schema with forward-only, reversible migrations.
- **Depends on**: Phase 2
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 3.1 | Entity-relationship model documented in `ARCHITECTURE.md` | `NOT STARTED` | Phase 2 | — |
| 3.2 | Migration tool selected; migration 0001 creates baseline schema | `NOT STARTED` | 3.1 | — |
| 3.3 | Seed / fixture data for local + test | `NOT STARTED` | 3.2 | — |
| 3.4 | Schema-to-type generation wired into build | `NOT STARTED` | 3.2 | — |

- **Verification Gate**:
  1. `migrate up` from empty DB succeeds; `migrate down` returns to empty.
  2. `migrate up` is idempotent (second run is a no-op).
  3. Generated types compile and match the live schema (drift check exits 0).
  4. Seed command produces a queryable dataset in a throwaway DB.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 4 — Backend Service Skeleton
- **Objective**: A running service with health, readiness, versioned routing, and
  graceful shutdown — no business logic yet.
- **Depends on**: Phase 3
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 4.1 | HTTP server boots, binds configured port, `/healthz` + `/readyz` | `NOT STARTED` | Phase 3 | — |
| 4.2 | Router with `/api/v1` prefix; 404 + error envelope standardized | `NOT STARTED` | 4.1 | — |
| 4.3 | Request-scoped context (request id, timeout, cancellation) | `NOT STARTED` | 4.2 | — |
| 4.4 | Graceful shutdown drains in-flight requests within deadline | `NOT STARTED` | 4.1 | — |

- **Verification Gate**:
  1. `GET /healthz` → 200; `GET /readyz` → 200 only after DB reachable.
  2. Unknown route → 404 with the standard error envelope.
  3. SIGTERM during a slow request: request completes, then process exits 0.
  4. Every response carries a propagated/generated request id header.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 5 — Frontend Application Shell
- **Objective**: A deployable front-end shell: routing, layout, theming, error
  boundary, and a wired (mockable) API client.
- **Depends on**: Phase 4
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 5.1 | App builds; client-side routing with a 404 route | `NOT STARTED` | Phase 4 | — |
| 5.2 | Layout primitives + theme tokens per `UI_GUIDELINES.md` (light/dark) | `NOT STARTED` | 5.1 | — |
| 5.3 | Top-level error boundary + loading states | `NOT STARTED` | 5.1 | — |
| 5.4 | Typed API client generated from Phase 4 contracts | `NOT STARTED` | 5.1, Phase 4 | — |

- **Verification Gate**:
  1. Production build succeeds with zero type errors and zero console errors on load.
  2. Unknown client route renders the 404 view.
  3. Forced render error is caught by the boundary, not a white screen.
  4. Theme toggle switches tokens with no unstyled flash; both themes pass a
     contrast check.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 6 — CI/CD Pipeline
- **Objective**: Every push runs the full gate; `main` is always releasable.
- **Depends on**: Phases 1–5
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 6.1 | CI: install → lint → typecheck → test → build on every PR | `NOT STARTED` | Phase 1 | **Carries forward Phase 1 stages 1.4–1.6**: formatter/linter/type-checker wiring, pre-commit hooks, commit-message convention, and their first clean run must be delivered and gate-verified here. |
| 6.2 | Branch protection: green CI + review required to merge `main` | `NOT STARTED` | 6.1 | — |
| 6.3 | Build artifacts / images published on merge to `main` | `NOT STARTED` | 6.1 | — |
| 6.4 | Deploy to staging automated; prod deploy gated on manual approval | `NOT STARTED` | 6.3, Phase 2 | — |

- **Verification Gate**:
  1. A PR with a failing test cannot be merged.
  2. A green merge to `main` produces a versioned, retrievable artifact.
  3. Staging auto-deploys from that artifact and passes a post-deploy smoke check.
  4. Pipeline run is reproducible: same commit → same artifact digest.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### EPOCH B — Core Platform (Phases 7–14)

---

### Phase 7 — Authentication & Authorization
- **Objective**: Verified identity on every protected route; deny-by-default authz.
- **Depends on**: Phases 4, 6
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 7.1 | Auth provider / flow chosen; login + logout + token refresh | `NOT STARTED` | Phase 4 | — |
| 7.2 | Session/token validation middleware on all `/api/v1` except allowlist | `NOT STARTED` | 7.1 | — |
| 7.3 | Role/permission model + enforcement helper | `NOT STARTED` | 7.2, Phase 3 | — |
| 7.4 | Front-end auth guard + authenticated API client | `NOT STARTED` | 7.1, Phase 5 | — |

- **Verification Gate**:
  1. Unauthenticated request to a protected route → 401.
  2. Authenticated-but-unauthorized request → 403, action not performed.
  3. Expired token is rejected; refresh issues a working token.
  4. Automated test matrix covers {anon, user, admin} × {allowed, denied} routes.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 8 — User & Account Management
- **Objective**: Full account lifecycle: create, read, update, deactivate, delete.
- **Depends on**: Phase 7
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 8.1 | Profile CRUD endpoints + validation | `NOT STARTED` | Phase 7 | — |
| 8.2 | Account deactivation + reactivation | `NOT STARTED` | 8.1 | — |
| 8.3 | Hard delete / data export honoring retention policy | `NOT STARTED` | 8.1, Phase 3 | — |
| 8.4 | Front-end account settings screens | `NOT STARTED` | 8.1, Phase 5 | — |

- **Verification Gate**:
  1. CRUD round-trip test passes; invalid input rejected with field errors.
  2. Deactivated account cannot authenticate; reactivation restores access.
  3. Delete removes or anonymizes all owned rows (verified by query) and export
     produces a complete archive.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 9 — Persistence Layer & Migrations Hardening
- **Objective**: Safe, observable data access: pooling, transactions, retries,
  and a migration process safe to run against production.
- **Depends on**: Phases 3, 4
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 9.1 | Connection pool sized + timeouts + health probe | `NOT STARTED` | Phase 4 | — |
| 9.2 | Transaction helper with rollback-on-error + nesting rules | `NOT STARTED` | 9.1 | — |
| 9.3 | Migration runbook: backup, apply, verify, rollback | `NOT STARTED` | Phase 3 | — |
| 9.4 | Query logging + slow-query threshold | `NOT STARTED` | 9.1, Phase 11 | — |

- **Verification Gate**:
  1. Load test holds connections ≤ pool size; no leak after run (pool returns to idle).
  2. Forced mid-transaction error leaves zero partial writes.
  3. Migration runbook executed end-to-end on a staging clone, including rollback.
  4. A seeded slow query appears in logs tagged over threshold.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 10 — API Gateway & Contracts
- **Objective**: A published, versioned API contract that clients and tests
  generate from; breaking changes are detectable in CI.
- **Depends on**: Phases 4, 6
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 10.1 | Machine-readable API spec checked into repo | `NOT STARTED` | Phase 4 | — |
| 10.2 | Server request/response validated against spec | `NOT STARTED` | 10.1 | — |
| 10.3 | Client SDK + test fixtures generated from spec | `NOT STARTED` | 10.1 | — |
| 10.4 | CI contract-diff blocks undeclared breaking changes | `NOT STARTED` | 10.1, Phase 6 | — |

- **Verification Gate**:
  1. Response not matching the spec fails a contract test.
  2. Removing/renaming a field without a version bump fails CI.
  3. Generated client compiles and round-trips against a running server.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 11 — Observability
- **Objective**: Structured logs, metrics, and traces correlated by request id;
  dashboards and alerts exist before load.
- **Depends on**: Phase 4
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 11.1 | Structured JSON logging with levels + request id | `NOT STARTED` | Phase 4 | — |
| 11.2 | Metrics: RED (rate, errors, duration) per route + runtime metrics | `NOT STARTED` | 11.1 | — |
| 11.3 | Distributed tracing spanning HTTP → DB → external calls | `NOT STARTED` | 11.1 | — |
| 11.4 | Dashboards + alert rules for error rate, latency, saturation | `NOT STARTED` | 11.2 | — |

- **Verification Gate**:
  1. One request id ties together its log lines, trace, and metric labels.
  2. Induced error surfaces on the dashboard and fires the alert within SLA.
  3. Logs contain no secrets or full PII (redaction test passes).
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 12 — Secrets & Configuration Management
- **Objective**: Rotatable secrets, least-privilege access, zero secrets in code
  or images.
- **Depends on**: Phases 2, 6
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 12.1 | All secrets in a managed store; app reads at boot/runtime only | `NOT STARTED` | Phase 2 | — |
| 12.2 | Rotation procedure documented + rehearsed | `NOT STARTED` | 12.1 | — |
| 12.3 | Per-environment least-privilege credentials | `NOT STARTED` | 12.1 | — |
| 12.4 | CI secret-scanning + image scanning gate | `NOT STARTED` | 12.1, Phase 6 | — |

- **Verification Gate**:
  1. Secret scan over history + built image → zero findings, enforced in CI.
  2. Rotating a secret with no code deploy keeps the app healthy.
  3. Staging credentials cannot access prod resources (verified by attempt).
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 13 — Error Handling & Resilience
- **Objective**: Predictable failure behavior: timeouts, retries with backoff,
  circuit breaking, and idempotency where it matters.
- **Depends on**: Phases 4, 11
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 13.1 | Standard error taxonomy (client / server / dependency / transient) | `NOT STARTED` | Phase 4 | — |
| 13.2 | Outbound calls: timeout + capped retry + backoff + jitter | `NOT STARTED` | 13.1 | — |
| 13.3 | Circuit breaker on flaky dependencies with fallback | `NOT STARTED` | 13.2 | — |
| 13.4 | Idempotency keys on unsafe/retryable mutations | `NOT STARTED` | 13.1, Phase 3 | — |

- **Verification Gate**:
  1. Fault-injection test: dependency 500s → bounded retries, then typed error.
  2. Dependency made unavailable → breaker opens, fallback served, breaker recovers.
  3. Same mutation replayed with one idempotency key produces one effect.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 14 — Local Dev Environment Parity
- **Objective**: `one command` brings up the whole stack locally, close enough to
  prod that most bugs reproduce.
- **Depends on**: Phases 2, 3, 4, 5
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 14.1 | Containerized/orchestrated local stack (app, DB, deps) | `NOT STARTED` | Phase 4 | — |
| 14.2 | Hot reload for backend + frontend | `NOT STARTED` | 14.1 | — |
| 14.3 | Deterministic reset command (wipe + migrate + seed) | `NOT STARTED` | 14.1, Phase 3 | — |
| 14.4 | `DEVELOPMENT.md` documents the full loop + troubleshooting | `NOT STARTED` | 14.1 | — |

- **Verification Gate**:
  1. Fresh machine → documented command → working app in browser, no manual steps.
  2. Reset command returns the stack to a known state in under a set time budget.
  3. A version-mismatch bug reproduced locally that also occurs in staging.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### EPOCH C — AI Pipeline (Phases 15–22)

---

### Phase 15 — Model Provider Integration
- **Objective**: A single provider-abstraction module; models referenced by
  logical name, swappable without touching call sites.
- **Depends on**: Phases 2, 12, 13
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 15.1 | Provider client wrapper: auth, base URL, timeout, retry (reuses Phase 13) | `NOT STARTED` | Phase 13 | — |
| 15.2 | Logical-model registry (name → provider + model id + params) | `NOT STARTED` | 15.1 | — |
| 15.3 | Token counting + request/response logging (redacted) | `NOT STARTED` | 15.1, Phase 11 | — |
| 15.4 | Fallback / secondary model on provider failure | `NOT STARTED` | 15.2, Phase 13 | — |

- **Verification Gate**:
  1. Swapping a logical model's target in config changes behavior with no code change.
  2. Provider outage triggers the documented fallback; caller still gets a response
     or a typed error.
  3. Logged prompts/responses are redacted per policy; token counts recorded per call.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 16 — Prompt Management & Versioning
- **Objective**: Every prompt is a versioned, reviewable asset with tests — no
  inline prompt strings in business logic.
- **Depends on**: Phase 15
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 16.1 | Prompt store (files/templates) with explicit version ids | `NOT STARTED` | Phase 15 | — |
| 16.2 | Templating with typed variables + injection-safe rendering | `NOT STARTED` | 16.1 | — |
| 16.3 | Per-prompt fixture tests (input → expected shape/assertions) | `NOT STARTED` | 16.2, Phase 20 | — |
| 16.4 | `AI_PIPELINES.md` catalogs each prompt, purpose, owner, version | `NOT STARTED` | 16.1 | — |

- **Verification Gate**:
  1. `git grep` finds no prompt literal outside the prompt store.
  2. Rendering with a missing/typo variable fails at build/test, not at runtime.
  3. Untrusted input containing template/delimiter tokens cannot alter prompt structure.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 17 — Retrieval / Context Assembly
- **Objective**: Deterministic, bounded context construction with provenance for
  every included chunk.
- **Depends on**: Phases 3, 15
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 17.1 | Source ingestion + chunking + (if used) embedding pipeline | `NOT STARTED` | Phase 3 | — |
| 17.2 | Retrieval query with top-k, filters, and score threshold | `NOT STARTED` | 17.1 | — |
| 17.3 | Context packer: token budget, dedup, ordering, provenance tags | `NOT STARTED` | 17.2, Phase 16 | — |
| 17.4 | Empty-result + low-confidence handling path | `NOT STARTED` | 17.2 | — |

- **Verification Gate**:
  1. Same query + same corpus → identical assembled context (byte-stable).
  2. Assembled context never exceeds the configured token budget.
  3. Every chunk in the context is traceable to a source id + offset.
  4. Query with no relevant matches yields the defined no-context behavior, not a guess.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 18 — Inference Orchestration
- **Objective**: A named, testable pipeline graph (retrieve → assemble → call →
  post-process) with per-step observability.
- **Depends on**: Phases 15, 16, 17
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 18.1 | Pipeline definition: ordered steps, typed IO between them | `NOT STARTED` | Phase 17 | — |
| 18.2 | Per-step tracing, timing, and token/cost attribution | `NOT STARTED` | 18.1, Phase 11 | — |
| 18.3 | Multi-step / tool-calling loop with a hard iteration cap | `NOT STARTED` | 18.1 | — |
| 18.4 | Deterministic replay from a recorded pipeline run | `NOT STARTED` | 18.1 | — |

- **Verification Gate**:
  1. A pipeline run emits one trace with a span per step and total cost.
  2. Tool loop cannot exceed the iteration cap (test forces runaway → bounded).
  3. Recorded run replays to the same final output with providers mocked.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 19 — Streaming & Response Handling
- **Objective**: Token streaming end to end with cancellation, partial-failure
  recovery, and clean client rendering.
- **Depends on**: Phases 5, 18
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 19.1 | Server streams model output over the wire (SSE/websocket) | `NOT STARTED` | Phase 18 | — |
| 19.2 | Client renders incremental tokens without layout thrash | `NOT STARTED` | 19.1, Phase 5 | — |
| 19.3 | User cancellation aborts the upstream model call | `NOT STARTED` | 19.1, Phase 13 | — |
| 19.4 | Mid-stream error yields a partial result + error marker, not a hang | `NOT STARTED` | 19.1 | — |

- **Verification Gate**:
  1. Client shows first token before full completion (measured TTFT under budget).
  2. Cancel button stops billing/generation upstream (verified in provider logs).
  3. Killing the stream mid-response leaves the client in a recoverable state.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 20 — Evaluation Harness
- **Objective**: Repeatable offline evals gating prompt/model/pipeline changes in CI.
- **Depends on**: Phases 6, 16, 18
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 20.1 | Golden dataset(s) with labeled expectations, versioned | `NOT STARTED` | Phase 16 | — |
| 20.2 | Scorers (exact / rubric / model-graded) with thresholds | `NOT STARTED` | 20.1 | — |
| 20.3 | `eval` command produces a scored report + diff vs. baseline | `NOT STARTED` | 20.2 | — |
| 20.4 | CI runs evals on pipeline/prompt changes; regression blocks merge | `NOT STARTED` | 20.3, Phase 6 | — |

- **Verification Gate**:
  1. `eval` on a known-good build meets or exceeds baseline thresholds.
  2. A deliberately worse prompt drops the score and fails the CI gate.
  3. Eval run is reproducible within a stated variance band.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 21 — Guardrails & Safety Filters
- **Objective**: Input and output pass policy checks; unsafe content is blocked,
  logged, and surfaced safely.
- **Depends on**: Phases 18, 20
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 21.1 | Input checks: prompt-injection, PII, disallowed requests | `NOT STARTED` | Phase 18 | — |
| 21.2 | Output checks: policy classifier + schema/format validation | `NOT STARTED` | 21.1 | — |
| 21.3 | Blocked-path UX + audit log entry (no silent drop) | `NOT STARTED` | 21.2, Phase 11 | — |
| 21.4 | Red-team suite in `SECURITY.md` run against the pipeline | `NOT STARTED` | 21.2, Phase 20 | — |

- **Verification Gate**:
  1. Known injection strings from the red-team suite do not alter system behavior.
  2. Output violating policy is blocked before reaching the user and is audited.
  3. Malformed structured output is rejected/repaired, never passed downstream.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 22 — Caching & Cost Controls
- **Objective**: Bounded, observable spend: caching, rate limits, quotas, and
  budget alerts.
- **Depends on**: Phases 11, 15, 18
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 22.1 | Response/embedding cache keyed on normalized input + model version | `NOT STARTED` | Phase 18 | — |
| 22.2 | Per-user + per-endpoint rate limits and quotas | `NOT STARTED` | 22.1, Phase 7 | — |
| 22.3 | Cost metering per request/user/feature | `NOT STARTED` | 22.1, Phase 11 | — |
| 22.4 | Budget thresholds with alerts + optional hard cutoff | `NOT STARTED` | 22.3 | — |

- **Verification Gate**:
  1. Cache hit for a repeated request avoids the provider call (verified in logs).
  2. Exceeding a quota returns 429; usage resets on the defined schedule.
  3. Simulated spend crossing a threshold fires the alert (and cutoff if enabled).
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### EPOCH D — Product Surface (Phases 23–28)

---

### Phase 23 — Conversation UI
- **Objective**: The primary chat surface: compose, send, stream, stop, retry,
  render rich content.
- **Depends on**: Phases 19, 21
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 23.1 | Composer: multiline, submit, disabled/loading states | `NOT STARTED` | Phase 19 | — |
| 23.2 | Message list: streaming, markdown/code rendering, copy | `NOT STARTED` | 23.1 | — |
| 23.3 | Stop / regenerate / edit-and-resend | `NOT STARTED` | 23.2, Phase 19 | — |
| 23.4 | Error + guardrail-block message states | `NOT STARTED` | 23.2, Phase 21 | — |

- **Verification Gate**:
  1. End-to-end: type → send → streamed answer → stop mid-stream all work in a UI test.
  2. Code blocks render with correct escaping; copy yields exact source.
  3. A guardrail block renders the safe state, not a crash or blank bubble.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 24 — Session & History Management
- **Objective**: Durable conversations: list, resume, rename, delete, and search.
- **Depends on**: Phases 8, 23
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 24.1 | Persist conversations + messages per user | `NOT STARTED` | Phase 23 | — |
| 24.2 | Sidebar: list, resume, rename, delete | `NOT STARTED` | 24.1 | — |
| 24.3 | Search across a user's history | `NOT STARTED` | 24.1 | — |
| 24.4 | Pagination / windowing for long conversations | `NOT STARTED` | 24.1 | — |

- **Verification Gate**:
  1. Reload restores the exact conversation state from storage.
  2. Delete removes all messages for that conversation (query-verified) and only that one.
  3. Search returns the expected conversation for a known unique phrase.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 25 — File / Artifact Handling
- **Objective**: Safe upload, storage, retrieval, and rendering of user files and
  generated artifacts.
- **Depends on**: Phases 12, 21, 23
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 25.1 | Upload with type/size limits + malware/type scan | `NOT STARTED` | Phase 23 | — |
| 25.2 | Object storage with scoped, expiring access URLs | `NOT STARTED` | 25.1, Phase 12 | — |
| 25.3 | Rendering/preview with sanitization | `NOT STARTED` | 25.2, Phase 21 | — |
| 25.4 | Lifecycle: retention, deletion, quota per user | `NOT STARTED` | 25.2 | — |

- **Verification Gate**:
  1. Disallowed file type/oversize upload is rejected before storage.
  2. Access URL for user A's file cannot be used by user B (403 / not found).
  3. A crafted malicious file (EICAR / script-in-SVG) is blocked or neutralized.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 26 — Notifications & Real-time Updates
- **Objective**: Timely, reliable delivery of async results and system messages.
- **Depends on**: Phases 13, 19
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 26.1 | Real-time channel (websocket/SSE) with reconnect + backfill | `NOT STARTED` | Phase 19 | — |
| 26.2 | Async job completion → user notification | `NOT STARTED` | 26.1, Phase 13 | — |
| 26.3 | Notification center UI: unread, mark read, clear | `NOT STARTED` | 26.2 | — |
| 26.4 | Optional email/push for offline users | `NOT STARTED` | 26.2 | — |

- **Verification Gate**:
  1. Dropping the connection and reconnecting backfills missed events exactly once.
  2. A completed background job reliably notifies the initiating user.
  3. No duplicate notifications under retry (idempotent delivery test).
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 27 — Admin & Analytics Dashboard
- **Objective**: Operators can see usage, cost, errors, and eval trends, and take
  scoped admin actions.
- **Depends on**: Phases 7, 11, 20, 22
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 27.1 | Admin-only area behind Phase 7 role checks | `NOT STARTED` | Phase 7 | — |
| 27.2 | Usage + cost + error dashboards (from Phase 11/22 data) | `NOT STARTED` | 27.1, Phase 22 | — |
| 27.3 | Eval-trend view over time (from Phase 20) | `NOT STARTED` | 27.1, Phase 20 | — |
| 27.4 | Scoped admin actions (disable user, revoke session) with audit log | `NOT STARTED` | 27.1, Phase 11 | — |

- **Verification Gate**:
  1. Non-admin cannot load or call any admin route (401/403).
  2. Dashboard numbers reconcile with raw metrics/logs for a sample window.
  3. Every admin action writes an attributable audit record.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 28 — Accessibility & UI Polish
- **Objective**: The product meets the accessibility bar in `UI_GUIDELINES.md`
  and behaves well under real conditions.
- **Depends on**: Phases 23, 24, 25, 26
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 28.1 | Keyboard navigation + focus management across all flows | `NOT STARTED` | Phase 23 | — |
| 28.2 | Screen-reader labels, roles, live regions for streaming | `NOT STARTED` | 28.1 | — |
| 28.3 | Contrast, reduced-motion, responsive breakpoints, empty/loading/error states | `NOT STARTED` | 28.1 | — |
| 28.4 | Automated a11y checks in CI + manual audit sign-off | `NOT STARTED` | 28.2, Phase 6 | — |

- **Verification Gate**:
  1. Automated a11y scan of key pages reports zero critical violations in CI.
  2. Full primary flow completed with keyboard only.
  3. Streaming responses are announced to a screen reader without flooding.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### EPOCH E — Hardening & Launch (Phases 29–32)

---

### Phase 29 — Security Review & Penetration Testing
- **Objective**: Documented threat model, resolved findings, and a repeatable
  security test pass.
- **Depends on**: Phases 7, 12, 21, 25
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 29.1 | Threat model in `SECURITY.md` (assets, actors, entry points, mitigations) | `NOT STARTED` | Phase 21 | — |
| 29.2 | Dependency + container + SAST scans clean or triaged | `NOT STARTED` | 29.1, Phase 6 | — |
| 29.3 | Pen test (internal or third-party) against staging | `NOT STARTED` | 29.1 | — |
| 29.4 | All high/critical findings fixed + re-tested; rest tracked with owners | `NOT STARTED` | 29.3 | — |

- **Verification Gate**:
  1. No open high/critical findings; each has a fix commit + retest note.
  2. OWASP-style checks (authz, injection, SSRF, IDOR) pass on staging.
  3. Security scans are wired into CI and green.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 30 — Performance & Load Testing
- **Objective**: Documented SLOs met under expected peak load, with headroom.
- **Depends on**: Phases 9, 11, 22
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 30.1 | SLOs defined in `PERFORMANCE.md` (latency, throughput, error rate) | `NOT STARTED` | Phase 11 | — |
| 30.2 | Load-test scenarios modeling realistic traffic mix | `NOT STARTED` | 30.1 | — |
| 30.3 | Bottlenecks profiled + addressed; results recorded | `NOT STARTED` | 30.2 | — |
| 30.4 | Autoscaling / capacity plan validated under sustained + spike load | `NOT STARTED` | 30.3 | — |

- **Verification Gate**:
  1. At target peak RPS, p95 latency and error rate stay within SLO for a sustained run.
  2. Spike test recovers to baseline within the stated time.
  3. Resource use leaves the documented headroom margin.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 31 — Documentation & Runbooks
- **Objective**: Anyone on call can operate the system; anyone new can contribute.
- **Depends on**: Phases 1–30
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 31.1 | `ARCHITECTURE.md` reflects the built system (diagrams current) | `NOT STARTED` | Phase 18 | — |
| 31.2 | Operational runbooks: deploy, rollback, incident, on-call, backup/restore | `NOT STARTED` | Phase 30 | — |
| 31.3 | Contributor guide + local setup verified by a new contributor | `NOT STARTED` | Phase 14 | — |
| 31.4 | User-facing docs / help content | `NOT STARTED` | Phase 23 | — |

- **Verification Gate**:
  1. A person who has not touched the repo follows the contributor guide to a
     merged change with no undocumented steps.
  2. A rollback is executed on staging using only the runbook.
  3. Architecture diagram matches an independent trace of a live request.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; merged to `main`.

---

### Phase 32 — Production Deployment & Launch Gate
- **Objective**: The system is live in production, monitored, and reversible.
- **Depends on**: Phases 6, 11, 29, 30, 31
- **Status**: `NOT STARTED`

| Stage | Description | Status | Entry | Evidence |
| ----- | ----------- | ------ | ----- | -------- |
| 32.1 | Production environment provisioned from code (IaC), reviewed | `NOT STARTED` | Phase 6 | — |
| 32.2 | Progressive rollout (canary / blue-green) with automated rollback triggers | `NOT STARTED` | 32.1, Phase 30 | — |
| 32.3 | Launch checklist signed off: security, perf, docs, on-call, backups | `NOT STARTED` | 32.1, Phases 29–31 | — |
| 32.4 | Post-launch monitoring window with defined success/abort criteria | `NOT STARTED` | 32.2, Phase 11 | — |

- **Verification Gate**:
  1. Production deploy completed via the pipeline with zero manual server access.
  2. A canary regression triggers automated rollback in a rehearsal.
  3. Every launch-checklist item has a named owner and a "done" link.
  4. Post-launch window closes with all success criteria met and no open Sev-1/2.
- **Exit Criteria**: All stages `COMPLETE`; gate evidence recorded; production stable
  through the monitoring window. **Bootstrap complete.**

---

## 5. Transition Log

Newest first. One line per state transition (§3 rule 6).

| Date       | From                | To                        | By        | Note |
| ---------- | ------------------- | ------------------------- | --------- | ---- |
| 2026-09-05 | Phase 0 `NOT STARTED` | Phase 0 `COMPLETE` | audit | Dev-environment audit run; evidence `docs/verification/01_env_audit.md`. Gate checks 1–5 physically executed. Phase inserted ahead of Epoch A to fill the §7 gap. Pointer stays at Phase 2 / 2.1. |
| 2026-09-05 | Phase 1 `IN PROGRESS` | Phase 2 / 2.1 `NOT STARTED` | owner | Pointer advanced. Phase 1 closed (see below). |
| 2026-09-05 | 1.4–1.6 (`COMPLETE` by directive) | Phase 6.1 (carry-forward) | owner | Tooling/hook/README-quickstart work not physically verified; re-covered by Phase 6.1's gate. Deviation recorded in §4 Phase 1. |
| 2026-09-05 | Phase 1 stages 1.1–1.6 | Phase 1 `COMPLETE` | owner | Marked complete by project-owner directive. Gate check 1 EXECUTED/PASS; checks 2–6 NOT EXECUTED (no code/tooling yet). |
| 2026-09-05 | 1.3 `NOT STARTED`   | 1.3 `COMPLETE`            | owner     | Stack fixed by `CLAUDE.md` Article I–II: Tauri/Rust core + React/TS frontend + isolated Python workers + local SQLite, local-first. Lockfiles deferred to first code per runtime. |
| 2026-09-05 | 1.2 `IN PROGRESS`   | 1.2 `COMPLETE`           | owner     | `ROADMAP.md` + `CLAUDE.md` (Engineering Constitution) authored and committed. |
| 2026-09-05 | 1.1 `IN PROGRESS`   | 1.1 `VERIFIED`           | bootstrap | Repo initialized on `main`; branch strategy to be documented in `DEVELOPMENT.md`. |
| 2026-09-05 | 1.2 `NOT STARTED`   | 1.2 `IN PROGRESS`        | bootstrap | ROADMAP state machine established; current pointer set to Phase 1 / Stage 1.2. |

---

## 6. Open Blockers

_None._ (Phase 6.1 carries an inherited debt from Phase 1.4–1.6 — tracked, not blocking.)

---

## 7. Assumptions

### Resolved (2026-09-05, via `CLAUDE.md` Engineering Constitution)

- **R1. vAI is a local-first AI *desktop* application.** Stack: Tauri (Rust)
  core + React/TypeScript frontend + isolated stateless Python workers + local
  SQLite. No backend service, no cloud, no remote DB, no telemetry
  (`CLAUDE.md` Articles I–II).
- **R2. Untrusted AI output is contained** — mediated only through allow-listed
  typed Rust operations (`CLAUDE.md` Article III).
- **R3. Verification honesty + smallest-correct-implementation** are binding
  (`CLAUDE.md` Article IV).

### Ledger realignment required (before serious Phase 2 work)

The phase ledger was drafted before R1, still uses web-service / cloud vocabulary,
and is an explicit **working draft** (see the banner at the top of this file). It
is expected to be re-derived toward a subsystem-driven structure once the
architecture is frozen. Phase count, names, and ordering are all open. What should
survive the re-derivation: **state-machine semantics (§2–§3) and one explicit,
physically-executable verification gate per phase.** Known corrections to fold in:

- "HTTP server / `/api/v1` / request id header" (Phase 4, 10, 11, 13) → **typed
  Tauri IPC commands + events**; "backend service skeleton" → **Rust core boot,
  IPC surface, worker supervisor**.
- "staging / prod deploy / autoscaling / IaC / canary" (Phase 6, 30, 32) →
  **signed desktop release builds, auto-update channel, release-candidate
  soak**.
- "external model providers / provider outage / billing" (Phase 15, 19, 22) →
  **local model runtime + user-controlled local endpoint**; cost controls become
  **local resource (CPU/RAM/disk) budgets**.
- "object storage / expiring access URLs" (Phase 25) → **Rust-mediated local
  file vault with path confinement**.
- Phase 7–8 (auth / accounts): reduce to **local single-user profile + optional
  at-rest encryption** unless multi-user is later required.

### Still open

- **O1. Retrieval/RAG in scope?** (Phase 17). If not, mark it `COMPLETE` as N/A
  with a note and re-point dependents.
- **O2. Multi-user vs single-user** (affects Phases 7, 8, 24, 27).

### Closed

- ~~Missing: a dev-machine/environment inspection gate~~ → **added as Phase 0**,
  completed 2026-09-05 (`docs/verification/01_env_audit.md`).
