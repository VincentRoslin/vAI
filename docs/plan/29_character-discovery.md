# Phase 29 — Character Discovery (swipe UX)

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Browse AI-generated characters, view profile + images, choose one (swipe) → start
a conversation **bound to the persistent character entity** (not a temporary
prompt). The relationship persists on return.

## Depends on
Phase 25 (character data), Phase 26 (character conversations), Phase 27
(characters have images).

## Not in this phase
- Bulk character generation pipelines (generating the discovery pool) beyond a
  minimum to populate it — the generation approach is its own decision.
- Recommendation / ranking algorithms (simple ordering is fine for v1).

## Architecture notes
- Discovery reads the character list (Phase 25) — presentation only; selecting a
  character opens a Phase 26 conversation with that `character_id`.
- A half-generated character (missing required fields or images) is not shown.
- "Swipe past" vs "choose" is UI state; choosing creates/opens the bound
  conversation.

## Performance notes
- The discovery feed is paginated and image-lazy (thumbnails first).
- Prefetch the next card's assets while the user views the current one.

## Step outline
1. Discovery feed: paginated character list with profile summary + primary image.
2. Card detail view: full profile + image gallery.
3. Readiness filter: only fully-formed characters appear.
4. Choose → open a Phase 26 conversation bound to the character id.
5. Return behaviour: re-opening a chosen character restores relationship + memory
   + history.
6. Swipe/skip UI state (no persistence needed beyond "seen" if desired).
7. Tests: feed renders from stored data; incomplete characters excluded; choosing
   binds to the entity id; leave + return restores state.

## Verification gate
1. The discovery feed renders characters from stored data, paginated.
2. An incomplete character does not appear.
3. Selecting a character opens a conversation bound to that character's stable id.
4. Leaving the conversation and returning restores relationship + memory +
   history exactly.
5. Feed scrolling stays smooth with image lazy-loading (no jank measured).

## ADRs / open questions
- How the discovery pool of characters is generated/seeded — separate decision,
  flagged for the owner.
