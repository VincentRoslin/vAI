# ADR-0013 — Subprocess transport: loopback HTTP for servers, stdio for workers

- **Status:** ACCEPTED (Phase 5 freeze, 2026-09-05; superseding attacks folded in via Phase 4) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/11_worker-protocol.md` (D-12)
- **Supersedes** the "TBD — Phase 3" placeholder in `CLAUDE.md` Article I.

## Context
`CLAUDE.md` Article I left subprocess transport open. `llama-server` (ADR-0003)
and the owner's image sidecar (ADR-0006) are both HTTP servers. STT/TTS/embedder
are small scripts we own.

## Options considered
- Uniform stdio JSON-lines for everything.
- Uniform loopback HTTP for everything.
- Two categories.

## Decision
**Two categories, both Rust-supervised, non-authoritative, no direct SQLite, no
independent GPU management, adapter-confined, loopback-only:**

| Category | Members | Transport |
| -------- | ------- | --------- |
| **Model server** | `llama-server`, `image-server` | **Windows named pipe preferred** (ACL-scoped, no TCP port, no firewall prompt). If TCP loopback is required (`llama-server` upstream), bind `127.0.0.1:<free port>` **+ a per-launch bearer token** the Rust core generates and passes (`--api-key`); the server rejects unauthenticated requests. |
| **Stateless worker** | STT, TTS, face-embedder | JSON-lines over stdin/stdout; `stderr` = logs only; binary audio as framed PCM or a short temp file |

**Why the auth requirement (Phase 4 R-C1):** a bare loopback HTTP server is
callable by *any* local process — it could trigger generation or read results.
Named pipes carry ACLs; a bearer token closes the gap for TCP.

Common: free-port selection (bind `:0`, read, drop, pass) or named pipe; Windows
**Job Object** for orphan cleanup; `ready` handshake with a protocol/version
check; health ping; bounded-backoff restart → `Failed` after N; a **shared Rust
worker-supervisor actor** for all types.

Python workers: reader-thread + `queue.Queue` (Windows pipe polling), **flush
stdout every write**.

**`CLAUDE.md` Article I is updated at Phase 5** to state this as ratified.

## Consequences
- Reuses the owner's working image-server pattern — low porting risk.
- `llama-server` keeps native SSE streaming + slots.
- Small workers stay trivially sandboxable/killable with no socket.
- A loopback bind may trigger a Windows firewall prompt → prefer named pipes, or
  pre-authorize in the installer (ADR-0014).
