# Phase 32 — Offline Audit

> ⚠ Step detail finalized at phase entry (after Phase 5). Outline only.

## Objective
Prove the runtime is genuinely local-first: every core feature works with the
network disabled, and the only network code paths are the explicit, isolated,
skippable acquisition/update ones.

## Depends on
Features complete (through Phase 29). `SECURITY.md` (Phase 5).

## Not in this phase
- Changing the acquisition/update features — only auditing that nothing else
  touches the network.

## Architecture notes
- Enumerate every network call site in the codebase (Rust + frontend + workers +
  their dependencies).
- Each must be one of: HF model search/download (Phase 12), STT/TTS model
  acquisition (Phase 12.8), an optional update check. Anything else is a bug.

## Performance notes
- N/A (correctness audit).

## Step outline
1. Static sweep: grep for HTTP/socket/DNS across the codebase; list every call
   site.
2. Dependency sweep: check for phone-home behaviour in dependencies (fonts, CDNs,
   analytics, telemetry).
3. Runtime capture: run the app fully (chat, voice, memory, characters, image gen)
   with a traffic monitor; record every packet's destination.
4. Disable the network entirely; repeat every core flow.
5. Confirm the acquisition/update paths are each isolated behind an explicit
   user action and skippable.
6. Write `docs/verification/04_offline_audit.md`.

## Verification gate
1. With the network disabled, every core feature works (chat, voice, memory,
   characters, image generation) — each executed and recorded.
2. A full-session traffic capture shows **zero** external egress during normal
   use.
3. Every network call site is one of the sanctioned acquisition/update paths;
   the list is in the audit doc.
4. Each sanctioned path is behind an explicit user action and can be skipped.
5. No telemetry, analytics, or CDN fetch anywhere (static + dependency sweep
   clean).

## ADRs / open questions
- Update-check mechanism (if any) — ADR: default off, opt-in, no auto-download.
