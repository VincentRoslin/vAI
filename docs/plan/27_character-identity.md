# Phase 27 — Persistent Character Identity (image)

> **Architecture frozen at Phase 5.** Governing: **ADR-0011** (prompt-based
> identity — `QuadView_krea2_v1` reference sheet + LLM-captioned canonical
> appearance block + fixed seed + realism LoRA + face-embedding similarity gate;
> **no LoRA training**; FR-C90 scoped to portraits/selfies), `AI_PIPELINES.md` §7.
> Adopt IP-Adapter/reference conditioning for Krea 2 immediately if it ships.

## Objective
A character stays the *same conceptual entity* across generated images: reference
identity → conditioning → generation → identity verification → accept or
regenerate. Accepts up front that perfect identity preservation is not
guaranteed by current image models.

## Depends on
Phase 25 (character has reference images), Phase 22 (image generation),
Phase 3.11 (approach).

## Not in this phase
- The LLM requesting images (Phase 28).
- Discovery (Phase 29).

## Architecture notes
- Reference images are character-linked blob-store entries.
- Conditioning approach per the Phase 3.11 ADR (IP-Adapter / reference / LoRA).
- The identity-similarity check (face or image embedding) gates accept vs
  regenerate; threshold + max-retries are config.
- Accepted images are linked to the character; rejected ones are discarded (not
  stored as the character's).

## Performance notes
- Similarity check cost recorded; it runs once per candidate.
- Regeneration budget (max attempts) bounded so a hard case doesn't loop forever.

## Step outline
1. Reference-image management for a character (add / remove / set primary).
2. Conditioning integration into the Phase 22 generation worker per the ADR.
3. Identity-similarity check (embedding + distance) with a config threshold.
4. Accept/regenerate loop: generate → check → accept (link to character) or
   regenerate up to N → then surface "couldn't match" with the best candidate.
5. Storage: accepted images linked to the character with the similarity score
   recorded.
6. Tests: conditioned generation runs; the check accepts a good match and
   rejects a poor one; the retry budget is enforced; accepted images are linked.

## Verification gate
1. Reference images can be stored and linked to a character.
2. Generation is conditioned on the character's reference identity (per the ADR).
3. The identity-similarity check accepts a matching result and rejects a
   non-matching one (scripted with known-good / known-bad candidates).
4. The regenerate loop is bounded by the configured max attempts.
5. Accepted images are linked to the character in the DB with their similarity
   score; rejected candidates are not stored as the character's.
6. Similarity-check cost recorded.

## ADRs / open questions
- Conditioning method and similarity model/threshold — ADRs from Phase 3.11,
  validated here with real output.
