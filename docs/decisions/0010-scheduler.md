# ADR-0010 — Scheduler: priority queue, interactive LLM never preempted

- **Status:** PROPOSED (Phase 3 draft) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/08_scheduler.md` (D-9)

## Context
One GPU, 16 GB. LLM ⟂ active image generation. LLM reload after eviction costs
~10–30 s. Consumers: LLM gen, image gen, STT/TTS, per-character LoRA training,
character/pool generation.

## Options considered
- Preemptive vs cooperative scheduling of GPU work.
- Where eviction logic lives (resource manager vs scheduler).

## Decision
- **Scheduler = a priority queue + dispatcher**, running behind the resource
  manager's single serialization mutex. Resource manager **accounts**; scheduler
  **orders and drives eviction/restore**.
- **Priority**: (1) interactive LLM generation — never preempted; (2) STT/TTS —
  coexist; (3) user-initiated image generation; (4) character/pool generation;
  (5) per-character LoRA training — idle-only.
- A background GPU job needing an LLM eviction **waits for no interactive
  generation in flight**, then evicts, **drains all queued same-kind jobs**,
  restores the LLM, resumes queued interactive work.
- Documented **evict/restore sequence** with a **WDDM free-memory settle wait**
  (poll `free` until it recovers past the estimate, or timeout).
- **Cancellation** by job id (queued → removed; running → token fired, resources
  released; still restore the LLM if an image job was mid-spike).
- **Reconciliation** on startup/crash: rebuild from lifecycle + resource state;
  queued jobs are lost (acceptable — user re-requests).

## Consequences
- Chat/voice stay responsive; image generation is a visible background operation
  ("waiting for image generation…" rather than a frozen UI).
- Batching N images behind one eviction is the key throughput optimization.
- Introduced ad hoc at Phase 23 (hot-swap), formalized at Phase 24.
- LoRA training may be deferred a long time on a heavy user → a max-defer prompt.
