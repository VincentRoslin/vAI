# SECURITY.md — LocalAI

**Status:** Frozen at Phase 5, 2026-09-05. Binding.
Threat model + the controls that address it. Findings from Phase 4
(`docs/verification/03_adversarial_review.md`) are folded in. The Phase 34
security audit re-verifies every control against the shipped code.

**Scope note:** LocalAI has **no content policy and no content-filtering
infrastructure** (NFR-15). "Security" here means *the app cannot be made to harm
the user's system or exfiltrate their data* — not moderation of generated content.

---

## 1. Assets

- The user's conversations, personas, characters, memories, relationship history.
- Generated and reference images / audio (may be explicit — the user's private
  data).
- The local SQLite database and the blob store.
- The user's filesystem and OS (must not be reachable by AI output).
- A HuggingFace token, if the user provides one (Settings).

## 2. Actors / entry points

| Actor | Entry point | Trust |
| ----- | ----------- | ----- |
| The user | The UI | trusted |
| An LLM / image model | Generated text, structured output | **untrusted data** |
| A character/persona definition | Its stored text | **untrusted data** |
| Retrieved memory / history | Injected into prompts | **untrusted data** |
| Another local process | The model servers' loopback endpoint | **untrusted** |
| A downloaded model file | Parsed + loaded | **untrusted** (verified) |
| A Python dependency | Bundled in the venv | semi-trusted (network-locked) |
| The network | The acquisition path only | untrusted |

## 3. Controls

### C1 — Untrusted AI output containment (`CLAUDE.md` Article III)
- Model/character output is only ever a message string or a **typed action from a
  fixed allow-list**, schema- and rule-validated in Rust before any effect.
- `send_image` (the first action): schema + allow-list + `character_id` matches
  the conversation + relationship-stage gate → else rejected + audit-logged,
  nothing runs.
- **No code path deserializes raw model text into a command/DTO type.** Verified
  by review (Phase 28) and the Phase 34 audit.
- No general-purpose "run this" primitive is exposed to any model.

### C2 — Prompt-injection resistance
- The context builder renders every untrusted section (persona, character,
  memory, history) inside fixed delimiters that are **stripped/escaped from the
  content itself**; the system block is always first and structurally separate.
- Injected "ignore previous instructions" text cannot change the assembled
  prompt's structure (Phase 20 test).
- Semantic persuasion of the model is **out of scope** — there is no content
  policy; structural safety is the boundary.

### C3 — Local service exposure
- Model servers (`llama-server`, `image-server`) bind **loopback only**, and:
  - **named pipe preferred** (Windows ACL-scoped, no TCP port, no firewall
    prompt), OR
  - `127.0.0.1:<free port>` **+ a per-launch bearer token** the Rust core
    generates and passes; unauthenticated requests are rejected.
- Rationale: a bare loopback HTTP server is callable by any local process
  (Phase 4 R-C1).
- Stateless workers use stdio only — no socket.

### C4 — Filesystem confinement
- Every path the app writes or reads (model dir, blob store, downloads, config,
  temp audio) is validated to resolve **under the app data root**; `..` and
  absolute escapes are rejected.
- The blob store is content-addressed; filenames are hashes, not model-supplied.
- Workers receive/emit files only at paths the Rust core dictates.

### C5 — Process argument safety
- Every child process is spawned with an **argument vector**, never a shell
  string. Model paths, ports, prompts as args are passed as discrete argv entries.
- Hostile inputs (spaces, quotes, `;`, unicode, newlines) are tested (Phase 34).

### C6 — Network lockdown (offline-first)
- Runtime egress is forbidden except the Rust `hf-hub` acquisition path (explicit
  user action) and an optional **default-off** update check.
- Every Python worker launches with `HF_HUB_OFFLINE=1`, `TRANSFORMERS_OFFLINE=1`,
  `HF_HUB_DISABLE_TELEMETRY=1`, `HF_HUB_DISABLE_IMPLICIT_TOKEN=1`,
  `DISABLE_TELEMETRY=1`, `DO_NOT_TRACK=1`, no inherited proxy (ADR-0015).
- Frontend: no CDN fonts/scripts, no analytics SDK, a restrictive CSP in the
  Tauri webview.
- Phase 32 verifies with a full-session packet capture — **zero external egress
  during normal use**.

### C7 — Secrets
- No secrets in source (history scanned, Phase 34).
- No secrets in logs (redaction at the logging boundary; workers log operation
  metadata, not payloads). **Mechanism (Phase 10):** the `logging` module's write
  boundary runs a regex redaction pass on **every** line — `hf_…` tokens,
  `Bearer …` credentials, and secret-ish JSON fields / `key=value` pairs
  (`token`, `secret`, `password`, `api_key`, `authorization`) → `***`. Message /
  prompt / transcript text is logged only at `trace!`; `info`-level breadcrumbs
  use `logging::content_preview` (length + hash, never the text).
- No secrets in the frontend bundle (inspected, Phase 34).
- A HuggingFace token (if provided) is stored in the **OS credential store**,
  never in `config` or logs; passed to `hf-hub` only.

### C8 — Downloaded-file integrity
- Every downloaded model file is **SHA256-verified** against HuggingFace's hash;
  a mismatch → deleted, not registered (ADR-0008).
- GGUF headers are parsed defensively (bounded reads, typed).

### C9 — Data-at-rest
- v1: files sit in the app data dir with normal OS permissions (single-user
  machine).
- Optional at-rest encryption of the DB + blob store is a **later phase** (not v1)
  — flagged in `ROADMAP.md`, decided when multi-device or shared-machine use is
  considered.

### C10 — Supply chain
- Rust deps + npm deps + Python deps inventoried and vulnerability-scanned
  (Phase 35); `cargo audit` / `npm audit` / a Python audit in the check suite.
- `llama.cpp` is built from a pinned source commit (ADR-0004), not a random
  binary. The `QuadView_krea2_v1` + realism LoRAs and the InsightFace model are
  from named HF repos, hash-verified on download.

## 4. Explicitly accepted residual risks

| Risk | Rationale |
| ---- | --------- |
| The user (or a model, at the user's prompting) plants false "memories" that steer character behaviour | Single-user local sandbox — it is the user's world to shape; not a security boundary |
| A model can be *persuaded* by conversation content to produce any content | No content policy by design (NFR-15); structural injection is prevented |
| DB / blobs are unencrypted at rest on the local disk | Single-user machine; encryption deferred to a later phase |
| Unsigned installer → SmartScreen warning | Deferred until a signing certificate exists; documented |

## 5. Audit

Phase 34 runs the OWASP-style checks (command execution, action validation, path
traversal, worker-input validation, arg construction, secret scanning, loopback
binding) against the shipped code and closes any high/critical finding before
release. Phase 32 runs the offline audit. Phase 38 re-runs both on the packaged
build.
