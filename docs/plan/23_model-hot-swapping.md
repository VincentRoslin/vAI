# Phase 23 — Model Hot-Swapping

> **Architecture frozen at Phase 5** (`PROJECT.md`, `ARCHITECTURE.md`, `AI_PIPELINES.md`, ADR-0001..0015). The design below is settled. Concrete implementation specifics (exact modules, crate APIs, filenames) are filled in at phase entry against the frozen ADRs — they do not change the design.

## Objective
Let different GPU workloads share limited VRAM: e.g. LLM loaded → image request →
(if needed) suspend/unload the LLM → load the image model → generate → release →
restore the LLM. Handle runtimes that cannot suspend cleanly (terminate + restart
to reclaim VRAM).

## Depends on
Phase 22 (image gen exists and its VRAM cost is known), Phase 13 (resources),
Phase 14 (lifecycle).

## Not in this phase
- The full scheduler with priorities/fairness (Phase 24 formalizes the
  arbitration this phase introduces ad hoc).
- Concurrent GPU workloads (only one heavy GPU consumer at a time here).

## Architecture notes
- One serialization point for all load/unload/swap operations (from Phase 13).
- Eviction policy (which model to unload) is explicit — start with "only one heavy
  GPU model resident; loading B unloads A" unless the coexistence math (Phase 22)
  says both fit.
- A swap must **not** interrupt an in-flight generation — it queues behind or
  waits for completion.
- After unload, wait for real VRAM `free` to recover (WDDM can lag) before loading
  the next model.

## Performance notes
- Record swap latency (LLM unload → image model ready) — it is user-visible.
- Minimize unnecessary swaps: if the image model fits alongside the LLM, don't
  swap.

## Step outline
1. Coexistence check: given current residents + the request, does it fit without
   eviction?
2. Eviction selection (policy above); user-pinned models exempt.
3. Graceful unload: finish or refuse-to-interrupt in-flight work first.
4. Post-unload VRAM settle wait (poll `free` until recovered or timeout).
5. Load the requested model; on failure, restore the evicted one.
6. Restore path: after the GPU workload finishes, reload the evicted model.
7. Crash-during-swap handling → reconcile, no leaked reservation, known state.
8. Tests (real VRAM measurement): swap succeeds; swap refused when eviction can't
   fit; swap waits for in-flight generation; crash during swap reconciles.

## Verification gate
1. LLM loaded → image request → swap → image generated → LLM restored, each step
   observed via real VRAM measurement.
2. A swap is refused (typed error) when even after eviction the target won't fit.
3. A swap requested during an active generation does not interrupt it (queues /
   waits).
4. A crash during a swap reconciles to a known state with no leaked reservation.
5. Swap latency recorded.
6. When the image model fits alongside the LLM, no swap occurs.

## ADRs / open questions
- Eviction policy (single-resident vs LRU vs pinned-aware) — ADR.
