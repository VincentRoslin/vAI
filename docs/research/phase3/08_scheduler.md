# Phase 3 · Scheduler / GPU arbitration

Covers step 3.8 and D-9. Requirements: FR-80..83, FR-C82; ARQ-8. Depends on the
VRAM analysis in `05_resource-vram.md`.

---

## What the scheduler must arbitrate

One GPU, 16 GB. Consumers:
- **LLM generation** (chat, voice, character) — interactive, latency-critical.
- **Image generation** (Tab 2, character-sent) — background, ~11.4 GB spike,
  requires LLM + TTS eviction.
- **STT / TTS** — small, coexist with the LLM; only evicted for an image job.
- **Per-character LoRA training** (Phase 27, if chosen) — background, exclusive,
  minutes-long.
- **Character generation** (Phase 25/29) — LLM (profile) + image (references),
  background.

Key facts from `05`:
- LLM ⟂ active image generation. Everything else can share with the LLM.
- Only one heavy GPU job at a time.
- LLM reload after eviction costs ~10–30 s → batch before restoring.

---

## D-9 — Design

### Split of responsibility
- **Resource manager** (`05`): *accounts* — `can_fit(req)`, `what_to_evict(req)`,
  reserve/commit/release, measured usage.
- **Scheduler**: *orders and dispatches* — a priority queue of GPU jobs; decides
  what runs next; drives the eviction/restore dance via the resource manager and
  lifecycle manager.
- Both sit behind the **one serialization mutex** — no two GPU transitions at
  once.

### Job model
`{ id, kind (llm_gen | image_gen | stt | tts | lora_train | char_gen),
   priority, resource_estimate, cancel_token, submitted_at }`

### Priority (starting policy — ADR)
1. **Interactive LLM generation** (a user is waiting on a chat/voice reply) —
   highest. Never preempted.
2. **STT / TTS** — coexist, effectively immediate.
3. **User-initiated image generation** (Tab 2, or a character-sent image the user
   is watching for) — high background.
4. **Character generation / discovery pool top-up** — low background.
5. **Per-character LoRA training** — lowest; only when the user is idle.

Rule: a background GPU job that needs an LLM eviction **waits until no interactive
LLM generation is in flight**, then evicts, runs (draining the queue of same-kind
jobs), and restores the LLM. An interactive generation submitted *during* an image
job queues behind it (the image is already mid-spike; interrupting wastes the
~11.4 GB of work) — but a **new** image request while an interactive reply is
streaming waits for the reply to finish.

### The eviction/restore sequence (image job)
```
1. mark LLM "evicting"; let any in-flight token stream finish (or hard-cancel if
   the user cancelled)
2. unload LLM (+ TTS if resident); resource manager releases
3. poll measured `free` until it recovers past the image estimate (WDDM lag) or
   timeout
4. run the image job(s) — drain all queued image_gen jobs
5. release image VRAM (sidecar drops to ~1.6 GB)
6. reload LLM (+ TTS if voice was active); resource manager reserves + commits
7. resume any queued interactive generations
```

### Cancellation
- Queued job → removed from the queue, `cancel_token` fired, nothing ran.
- Running job → `cancel_token` fired; the worker/backend aborts; resources
  released; if it was an image job mid-spike, still restore the LLM.

### Reconciliation
On startup / after a crash: query the lifecycle manager + resource manager for
what's actually loaded, rebuild the queue picture (queued jobs are lost — that's
acceptable; the user re-requests), ensure the GPU is in a known state (unload
anything unexpected).

→ **ADR-0010**: scheduler = priority queue + dispatcher over the one
serialization mutex; resource manager accounts, scheduler orders; interactive LLM
never preempted; background GPU jobs batch behind interactive work; documented
eviction/restore sequence with a WDDM free-memory settle wait.

---

## Optimizations
1. **Batch same-kind GPU jobs** behind one eviction — N images = one
   evict/restore, not N.
2. **Predictive eviction** — opening the Discovery/Image tab signals "images
   likely soon"; pre-shrink or pre-evict the LLM so the first image skips the
   unload on the critical path.
3. **Keep TTS resident during image jobs if it fits** — only evict TTS when the
   image estimate genuinely needs its ~1–3 GB. Avoids a TTS reload for voice
   users.
4. **Coalesce discovery pool top-up** into idle windows only; never contend with
   a user-visible job.
5. **Warm-restore** — start the LLM reload as soon as the last queued image job
   *starts its final step*, overlapping the reload with the tail of generation
   where VRAM allows.
6. **Skip the swap entirely** when the resource manager says the image job fits
   alongside the current (small) LLM — don't evict reflexively.
7. **LoRA training only on true idle** (no input for N minutes, screen not on a
   generation view) and checkpoint it so it can be interrupted by any real job.

## Failure modes
- Crash during eviction/restore → reconcile to a known GPU state; queued jobs
  lost (acceptable); no leaked reservation.
- WDDM `free` never recovers past the estimate → timeout → typed "GPU didn't free
  memory, try restarting the app" (rare; note for Phase 4).
- Interactive generation submitted repeatedly during a long image job → they
  queue; UI shows "waiting for image generation to finish" rather than spinning.
- LoRA training job starves forever (user never idle) → a max-defer with a
  user prompt ("train now? this will pause chat for ~3 min").
