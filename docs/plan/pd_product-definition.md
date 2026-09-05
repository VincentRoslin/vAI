# Product Definition (step, between Phase 2 and Phase 3)

## Objective
Capture what LocalAI must *be* — features and desired experience — as structured,
numbered requirements the owner confirms. This is the input to Phase 3
architecture research. **No technology choices, no schema, no subsystem design.**

## Depends on
Phase 2 complete.

## Not in this step
- `PROJECT.md` (Phase 5).
- Any architecture, technology selection, or data model.
- Deciding *how* anything is built.

## Steps

### PD.1 — Owner provides the product vision
- **Do:** Owner writes / dictates the full vision. Claude provides this checklist
  of what to cover so nothing is missed:
  - Every feature, and for each, the *experience* the owner wants (not just "it
    exists").
  - The character system in depth: identity, appearance, personality, memory,
    relationship progression, galleries, discovery/swipe flow, character-sent
    images, identity consistency expectations.
  - Concrete user flows ("open app → …").
  - Offline scope: what must work with no network; what may need it once.
  - Performance expectations (perceived latency for chat, voice, image).
  - Resource expectations (this machine: 16 GB VRAM, 32 GB RAM).
  - Explicit non-goals / out of scope.
  - Any content/maturity considerations that affect architecture (e.g. local-only,
    no moderation service).
- **Owner:** owner (Claude only prompts and records).
- **Verify:** the owner's description is captured verbatim in a scratch note
  before PD.2 begins.

### PD.2 — Claude drafts `docs/product/requirements.md`
- **Do:** Write the requirements document with these sections:
  1. **Functional requirements** — `FR-1, FR-2, …`, grouped: Chat · Voice ·
     Models & acquisition · Personas · Memory · Image generation · Characters ·
     Discovery · Settings · Model/resource management. Each `FR` is one testable
     statement of behaviour.
  2. **Non-functional requirements** — `NFR-1, …`: performance budgets, offline
     guarantees, privacy, reliability/recovery, resource ceilings,
     maintainability, accessibility.
  3. **Character system requirements** — its own section (it drives the most
     architecture): entity model expectations, memory expectations, identity-
     consistency expectations, discovery UX.
  4. **Ambiguities & contradictions** — everything under-specified or in tension,
     listed as questions for the owner.
  5. **Architecture research questions** — `ARQ-1, …`, dated. Every requirement
     with an architectural consequence generates at least one `ARQ`. These are
     the Phase 3 agenda.
  6. **Out of scope** — explicit non-goals.
- **Owner:** Claude (Rust/frontend/worker: n/a — documentation).
- **Verify:** `docs/product/requirements.md` exists; every section present; each
  `FR`/`NFR`/`ARQ` is uniquely numbered; a reviewer can map every feature the
  owner named to at least one `FR`.

### PD.3 — Owner review loop
- **Do:** Owner reads the draft. Claude revises until the owner confirms:
  - nothing they asked for is missing,
  - nothing was invented,
  - no hard requirement was softened or dropped for being difficult,
  - the ambiguities list is complete and the resolutions are recorded.
- **Owner:** owner confirms; Claude records the confirmation (date) in
  `ROADMAP.md` §5.
- **Verify:** `ROADMAP.md` §5 has a dated "Product Definition confirmed by owner"
  entry.

## Verification gate
1. `docs/product/requirements.md` exists and separates **functional** from
   **non-functional** requirements, each uniquely numbered. — file check
2. Every feature named by the owner in PD.1 appears as an `FR`. — reviewer maps
   the PD.1 note to `FR` numbers, 1:1 coverage.
3. No `FR`/`NFR` exists that the owner did not ask for. — reviewer check against
   the PD.1 note.
4. No difficult requirement was dropped or weakened. — the ambiguities section
   shows each hard item was *resolved*, not removed.
5. A dated `ARQ-*` list is present and covers every requirement with an
   architectural consequence. — reviewer check.
6. `ROADMAP.md` §5 records the owner's explicit confirmation with a date.

## ADRs / open questions this step raises
- Resolves O1 (memory scope) and O2 (single vs multi-user) if the owner's
  description settles them; otherwise they stay open into Phase 3.
- Produces the `ARQ-*` list — the Phase 3 research agenda.
