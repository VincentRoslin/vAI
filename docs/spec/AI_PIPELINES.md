# AI_PIPELINES.md — LocalAI

**Status:** Frozen at Phase 5, 2026-09-05. Binding.
Each pipeline end to end: model, transport, resource interaction, failure
handling. Design rationale: `docs/research/phase3/` + `docs/decisions/`.

---

## 1. LLM (chat / voice / character)

| | |
| --- | --- |
| Runtime | `llama-server`, built from source with CUDA (sm_120), pinned commit (ADR-0004) |
| Transport | named pipe / token'd loopback HTTP; SSE for streaming (ADR-0013) |
| Models | GGUF, user-acquired from HuggingFace; Q4_K_M / Q5_K_M for ~8–14B on 16 GB |
| Owns model load? | No — the lifecycle manager does; `-ngl` from the resource manager estimate |
| VRAM | 6–8 GB typical; **evicted for every image generation** |

**Flow:** context builder assembles the prompt → `LlmBackend.stream()` → SSE
deltas → per-request Tauri Channel (deltas coalesced ~2–4 tokens). Cancellation:
`CancellationToken` + drop the HTTP request (server frees the slot — verified in
Phase 15). Mid-conversation model switch: history re-truncated to the new context
window; each model advertises its chat template via the adapter.

**Failure:** CUDA OOM on load → child exits → reservation released → typed error
("free X GB / smaller quant"), **no auto-retry same config**. Crash mid-stream →
detected (wait/health/stream error) → lifecycle `Failed` → bounded restart. Hang
(no tokens) → generation timeout → kill + restart. Orphan → Job Object + startup
sweep. WDDM hang (Blackwell + Hyper-V) → Phase 15 watch item, mitigation ladder in
`docs/verification/02_phase3_probes.md`.

## 2. STT — speech to text

| | |
| --- | --- |
| Model | **faster-whisper**, single pinned model, `compute_type=float16` (int8 is broken on sm_120) |
| Runtime | CTranslate2 **CUDA-12** runtime, bundled alongside the CUDA-13 llama.cpp |
| Transport | stdio JSON-lines worker |
| VRAM | ~3–4 GB (`large-v3` fp16); coexists with the LLM |

**Flow:** `cpal` capture (16 kHz mono) → **Silero VAD in the Rust core** decides
onset + endpoint → the endpointed segment is sent to the worker → partial
(optional) + final transcript → the final transcript becomes a user turn on the
conversation engine (identical path to typed input). Not true streaming —
transcribe per VAD segment.

**Failure:** worker crash mid-transcription → typed error, recovers next
utterance. Missing device / missing model → clear error, no crash. VAD never
endpoints → max-utterance timeout forces a cut. Empty / low-confidence transcript
→ dropped per policy, never sent as a blank turn.

## 3. VAD — voice activity detection

Silero VAD (ONNX, ~tens of MB, CPU or GPU), **in the Rust core** (not a worker) —
it owns utterance boundaries and the barge-in trigger. Tunables (config):
speech-onset threshold, min-silence-to-endpoint (~500–800 ms), min-utterance
duration. During TTS playback, VAD sensitivity is ducked to reduce echo
false-triggers.

## 4. TTS — text to speech

| | |
| --- | --- |
| Model | **Chatterbox Turbo**, single pinned model |
| Transport | stdio JSON-lines worker (framed PCM out) |
| VRAM | ~1–3 GB; coexists with the LLM; evicted only for an image job |

**Flow:** as LLM tokens arrive, the Rust core **chunks the assistant text by
clause/sentence** and streams chunks to the worker → synthesized audio chunks →
`cpal` playback with a **small (~100–200 ms) buffer**. Target first-audio < ~500 ms
after the first clause is ready.

**Barge-in:** VAD speech-onset (or an explicit stop) → conversation engine
`Interrupting` state → (1) trip the LLM `CancellationToken`, (2) cancel the TTS
worker, (3) stop playback, (4) persist the assistant turn as `truncated` with the
spoken token count, (5) → `Listening`. All four before the transition completes.
Target trigger→silence < ~200 ms.

**Failure:** worker crash / missing model / lost output device → typed error,
conversation continues in text.

## 5. Image generation (Tab 2 + character-sent)

| | |
| --- | --- |
| Model | **Krea 2 Turbo** (12B DiT, 8-step distilled), only image model |
| Quant | **bitsandbytes NF4**, quantized **once at acquisition** and cached; NF4 loaded directly (~6 GB) thereafter (ADR-0006, R-C3). torchao NVFP4 benchmarked at Phase 22 |
| Runtime | diffusers `Krea2Pipeline` + `enable_model_cpu_offload()`; a FastAPI-style sidecar |
| Transport | named pipe / token'd loopback; `generate` endpoint only — **no self-managed VRAM** |
| Params | 8 steps, `guidance_scale 0.0`, no negative prompt, FlowMatch Euler; resolution 1024²–1664×928 snapped to /16 |
| LoRA | single-select in v1 (`{id, weight}` list in the contract for future multi); Kohya→diffusers converter; LoRA + preset registries in SQLite |
| VRAM | ~1.6 GB idle (sidecar resident), **~11.4 GB generating** → LLM + TTS evicted |
| Time | ~17–18 s / 1024² once loaded; NF4 load ~15–25 s (cached) |

**Flow:** `request → scheduler.enqueue(image_gen) → wait for no interactive LLM
gen → acquire GPU mutex → unload LLM (+ TTS) → settle-wait → sidecar.generate →
[Tab 3: identity gate, §7] → write blob + metadata/params/seed/hash to SQLite →
release mutex → restore LLM (+ TTS) → deliver`. Restore runs on crash too. Queued
image jobs are **batched** behind one eviction.

**Failure:** NF4/bitsandbytes breakage on a torch upgrade → re-quantize (slow) or
bf16 + full CPU offload (very slow) → typed error. OOM after eviction (big LoRA /
large res) → reduce res / drop LoRA / fewer steps → typed error. Sidecar crash
mid-generation → GPU released, no partial blob, scheduler restores the LLM.

## 6. Persona / character context building

One **context builder**, used by every generation path (ADR-0004… actually
Phase 20). Assembles: `system instructions → persona/character (structured) →
relationship stage guidance (characters) → retrieved memory → conversation
history → runtime context`. Deterministic; token-budgeted with truncation +
provenance; all untrusted sections rendered inside fixed delimiters stripped from
the content (injection-safe). A test harness captures the exact string sent to the
LLM — persona/character presence is asserted, not assumed.

## 7. Character visual identity (v1, prompt-based)

No LoRA training (owner). Per character:
1. **Reference sheet** — `QuadView_krea2_v1` CharacterSheet LoRA generates a face
   close-up + 3 body views in one consistent image; cropped into the gallery.
2. **Canonical appearance block** — an LLM captions the primary view in detail;
   merged with the structured appearance fields → the character's **immutable
   identity prompt prefix**, byte-identical on every generation.
3. **Fixed per-character seed**, reused for the character's lifetime.
4. **Realism LoRA always on** (`gokaygokay/Krea-2-Realism` + Skin).
5. **Batch-and-pick** — generate N (3–4), score each with the **face-embedding
   identity gate** (InsightFace-style cosine vs the reference set; threshold +
   bounded retries, config), deliver the best; on repeated failure deliver the
   best candidate + "couldn't closely match".

**Scope:** holds for portraits/selfies (the expected range); drifts under large
pose/scene changes. Revisit if Krea 2 gains reference conditioning (IP-Adapter /
PuLID) — adopt immediately if it ships.

## 8. Character generation (discovery pool)

`LLM (schema-constrained) → structured profile → completeness/coherence gate →
reference sheet (§7) → LLM caption → canonical block → commit character (profile +
N images in one transaction) → char_pool`. Feed serves the pool; the scheduler
tops it up on idle; on-demand generation with a "finding someone…" state when
empty. Only `ready` characters are shown.

## 9. Memory

`turn completes → (async, off the response path) schema-constrained LLM extraction
→ candidates → importance-threshold + FTS5-dedup + size-cap gate → store (mem_* +
FTS5) with provenance`. Retrieval: BM25 keyword query from the current turn,
filtered by scope (`persona:<id>` / `character:<id>`), top-k → context-builder
memory section (budgeted). Embeddings deferred; added only on a measured recall
gap (> ~500 memories/scope + FTS5 misses) via `sqlite-vec` in-process.

## 10. Relationship progression

Discrete stages; **hybrid** transitions — rule-based signed points per turn (cheap
signals) for the moment-to-moment value, an occasional LLM assessment (~every 20
turns / on a big event) for what rules miss + to write the `history` reason.
Hysteresis prevents oscillation. The current stage is injected into conversation
context and **gates the `send_image` action's allowed `image_type`/`scene` set**
(a character-behaviour rule enforced in Rust — not a content filter; NFR-15
unaffected).
