# Phase 30 — UI / UX Pass

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Bring every user flow — chat, voice, characters, gallery, discovery, settings,
model picker — to the bar in `UI_GUIDELINES.md`: loading / empty / error /
streaming states, keyboard navigation, accessibility.

## Depends on
The feature phases whose UI it polishes (16–29). Best done once features are
functionally complete.

## Not in this phase
- New features or behaviour changes — presentation only.
- Visual redesign beyond the guidelines.

## Architecture notes
- Still presentation only; no new data ownership.
- Shared components for the recurring states (loading skeleton, empty state, error
  panel, streaming indicator).

## Performance notes
- No layout thrash on streaming updates.
- Large lists (galleries, discovery, model list) virtualized.

## Step outline
1. Inventory every flow and its current state coverage (loading/empty/error/
   streaming).
2. Build/standardize the shared state components.
3. Apply them to every flow; remove ad-hoc handling.
4. Keyboard navigation + focus management across all flows.
5. Screen-reader labels, roles, live regions for streaming output.
6. Contrast, reduced-motion, responsive breakpoints.
7. Automated a11y scan in the check suite.
8. Manual audit pass with sign-off notes.

## Verification gate
1. Every primary flow has explicit loading, empty, and error states.
2. Each primary flow can be completed keyboard-only.
3. Automated a11y scan reports no critical violations (in the check suite).
4. Light and dark both pass contrast.
5. Streaming output is announced to a screen reader without flooding.
6. Large lists are virtualized (no jank measured on a big dataset).

## ADRs / open questions
- None expected; raise one if a guideline proves impractical.
