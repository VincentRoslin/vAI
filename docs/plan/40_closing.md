# Phase 40 — Git Strategy · Context Efficiency · Final Release Gates

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.
> (Merges the ChatGPT guide's phases 37–39 — three short checklists.)

## Objective
Formalize git practice, make the repo efficient for a fresh Claude session, and
run the release checklist. After this phase the first release ships.

## Depends on
Phase 39 (permanent workflow defined) and everything before it.

## Not in this phase
- Post-release feature work (that uses the Phase 39 loop).

## Part A — Git strategy
Steps:
1. Write the branch model, commit-message convention, and tagging/release scheme
   in `DEVELOPMENT.md`.
2. Enforce commit-message + branch rules with a hook.
3. Decide and record the **remote / push policy** (where the repo lives, who
   pushes, what's private) — this has been deferred the whole project.
4. Produce a dry-run release tag + generated changelog.
Gate: rules written + hook-enforced; a bad commit message / branch name is
rejected; remote/push policy recorded; dry-run tag + changelog produced.

## Part B — Context efficiency
Steps:
1. Review every doc for staleness and redundancy; delete or merge.
2. Check `CLAUDE.md` + `ROADMAP.md` are within a sane size budget.
3. Ensure a clear entry path: `CLAUDE.md` → `ROADMAP.md` → current `docs/plan/`.
4. **Fresh-session test**: a new Claude session, given only the repo, correctly
   states the project status and the next action.
Gate: no stale/contradictory docs; size budgets met; the fresh-session test
passes (recorded).

## Part C — Final release gates
Steps:
1. Confirm every phase in `ROADMAP.md` §4 is `COMPLETE`.
2. Re-run the Phase 16 (reliability), 32 (offline), 33 (fault injection), 37
   (packaging) gates on the release build.
3. Verify `PROJECT.md`'s feature list against the running app, item by item.
4. Publish a known-issues list.
5. Tag the release.
Gate: all phases `COMPLETE`; the four re-run gates pass on the release build;
`PROJECT.md` feature list verified 1:1 against the app; known-issues list
published; release tagged.

## Overall verification gate
Parts A, B, and C gates all pass, each with recorded evidence. This is the last
gate of the course.

## ADRs / open questions
- Remote/push/hosting policy — an ADR (it's been open since the start).
