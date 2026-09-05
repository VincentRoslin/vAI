# ADR-0015 — Python worker network lockdown

- **Status:** PROPOSED (Phase 4) · **Date:** 2026-09-05
- **Research:** `docs/verification/03_adversarial_review.md` R-C2

## Context
The Python workers pull in `torch`, `diffusers`, `transformers`,
`huggingface_hub`, `bitsandbytes`, `faster-whisper`, `insightface`. Several of
these **send usage telemetry and check for updates by default** (HF Hub, HF
`transformers`). That violates NFR-1 (local-first), NFR-2/3 (no network
dependency), NFR-10 (no silent upload), NFR-12 (no telemetry).

## Options considered
- Trust the libraries' defaults + audit later.
- Firewall the worker processes at the OS level.
- Set every offline / no-telemetry env var when launching each worker.

## Decision
**Every Python worker process is launched by the Rust supervisor with a fixed
"hostile network" environment:**

```
HF_HUB_OFFLINE=1
TRANSFORMERS_OFFLINE=1
HF_HUB_DISABLE_TELEMETRY=1
HF_HUB_DISABLE_IMPLICIT_TOKEN=1
DISABLE_TELEMETRY=1
DO_NOT_TRACK=1
HF_HUB_DISABLE_PROGRESS_BARS=1
no HTTP(S)_PROXY / ALL_PROXY inherited
```

- Workers load models **from local paths only** — they never resolve a repo id.
- The **only** component allowed network access is the Rust acquisition path
  (`hf-hub`, ADR-0008), behind an explicit user action.
- The list of env vars is reviewed against each library's docs at Phase 22/18
  (versions may add new telemetry knobs).
- **Phase 32 (offline audit)** verifies with a full-session packet capture that
  no worker produces any egress.

## Consequences
- Workers cannot accidentally download a missing file — a missing model is a
  hard, explicit error routed to the acquisition flow (correct behaviour).
- New library versions may introduce telemetry not covered by these vars →
  Phase 32's packet capture is the backstop, and the env list is version-checked.
- Zero functional cost; workers only ever needed local files at runtime.
