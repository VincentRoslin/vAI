# Phase 3 · Subprocess transport & protocol

Covers step 3.13 and D-12. Requirements: NFR-40..42, NFR-50..52; `CLAUDE.md`
Article I. Resolves the transport question left open in `CLAUDE.md` Article I.

---

## D-12 — Transport

The owner's existing project already runs the image generator as a **FastAPI
loopback server** driven by Rust, and `llama-server` is a loopback HTTP server
too. This tilts the decision toward **uniform loopback HTTP for the model
servers**, with lightweight stdio for the small always-our-code workers.

### Recommendation: two categories

| Category | Members | Transport | Why |
| -------- | ------- | --------- | --- |
| **Model server** | `llama-server` (LLM), `image-server` (diffusers/Krea 2) | Rust-supervised child; **loopback HTTP** on `127.0.0.1:<free port>`, no external interface | Both are long-lived servers holding a big model; both already exist as HTTP servers (upstream / owner's project); HTTP gives clean streaming (SSE), health, concurrent requests, and language independence |
| **Stateless worker** | STT (faster-whisper), TTS (Chatterbox), face-embedder, LoRA-trainer job | Rust-supervised child; **JSON-lines over stdin/stdout**; binary audio as framed bytes or short files in a controlled temp dir | Small, our code, one request at a time, trivially killable; no benefit to a socket; keeps them maximally sandboxable |

Both categories are identical on everything else (`CLAUDE.md` Article I):
Rust-spawned & supervised, non-authoritative, no direct SQLite, no independent
GPU management (all via the resource manager / scheduler), backend detail confined
to one Rust adapter module, **loopback only — a `127.0.0.1` socket is not a
network call** (Article II).

**Update `CLAUDE.md` Article I** at Phase 5 to state this two-category split as
the ratified decision (it currently lists all three as "TBD — Phase 3 ADR").

### Why not stdio for everything
- `llama-server` and the diffusers server are HTTP-native; wrapping them in a
  stdio proxy is pure overhead and loses SSE streaming + concurrent slots.
- The owner's working image server is already HTTP — reusing it is faster and
  lower-risk than re-architecting it to stdio.

### Why not HTTP for everything
- STT/TTS/embedder are small scripts we own; a socket adds port management and a
  bind surface for no gain. stdio is simpler and the process is trivially bounded.

→ **ADR-0013**: model servers = supervised loopback HTTP; small workers = stdio
JSON-lines; update Article I.

---

## Protocol details

### Model server (HTTP)
- Rust picks a free `127.0.0.1` port (bind `:0`, read, drop, pass), spawns the
  server bound to it, `tokio::process` + `kill_on_drop` + **Windows Job Object**
  (orphan cleanup).
- Readiness: poll `GET /health` until 200 with a generous timeout.
- LLM: OpenAI-compatible `/v1/chat/completions` + native `/completion` (SSE) for
  streaming.
- Image: `POST /generate {model, prompt, width, height, seed, lora, lora_weight,
  …}` → progress events (SSE or poll) → result = a blob path Rust reads.
- Cancellation: drop the request (connection close) **and** an explicit
  `/cancel/{id}` where supported.

### Stateless worker (stdio JSON-lines)
- One compact JSON object per line, UTF-8. `stderr` = logs only, never parsed.
- Messages: `→ {type:"request", id, op, params}` · `→ {type:"cancel", id}` ·
  `← {type:"ready", protocol:1, capabilities:[…]}` (once at startup) ·
  `← {type:"progress"|"chunk"|"result"|"error", id, …}`.
- Handshake: worker emits `ready` with a protocol version; Rust refuses a
  mismatch (guards a stale venv).
- **Python side**: a reader thread + `queue.Queue` (Windows `selectors` can't poll
  pipes reliably) so a `cancel` is read while work is running; **flush stdout on
  every write** (Python buffers when not a TTY).
- Binary handoff: framed PCM for short audio; a short-lived file in a
  Rust-dictated temp dir for anything larger, path returned in `result`.

### Common
- Health: periodic ping / `/health`; no response within a timeout → kill +
  bounded-backoff restart → park in `Failed` after N.
- Crash: `child.wait()` resolves → fail in-flight tasks (typed error), release
  resource reservations, restart policy.
- Shutdown: send shutdown (`SIGTERM`-equivalent / `/shutdown` / `{type:"shutdown"}`),
  wait briefly, then kill. Job Object ensures nothing survives an app crash.

---

## Optimizations
1. **Reuse one `llama-server` across conversations** (slots) instead of one per
   chat — no reload on persona/conversation switch when the model is unchanged.
2. **Keep the image server warm** (`~1.6 GB` idle) while the Image/Discovery tab
   is open — see `08`.
3. **Framed PCM over stdio** for short STT/TTS audio — no disk round-trip for a
   2-second utterance.
4. **Unix-domain-socket-style named pipe** instead of a TCP loopback socket on
   Windows (`\\.\pipe\…`) for the model servers — avoids a TCP port entirely and
   the localhost firewall prompt. Evaluate; TCP loopback is the fallback.
5. **Shared worker supervisor** in Rust — one actor pattern for spawn / health /
   restart / route, reused across all subprocess types.
6. **Lazy spawn** — start STT/TTS on first voice use, the image server on first
   image use; not at app launch.

## Failure modes
- Free-port race (something grabs the port between drop and spawn) → retry with a
  new port, bounded.
- Windows firewall prompt on first loopback bind → named pipes (opt 4) avoid it;
  otherwise document it / pre-authorize in the installer.
- Worker never sends `ready` → timeout → kill + restart; after N, `Failed` with a
  clear message.
- Orphaned model server after a hard crash → Job Object kills it; a startup sweep
  also kills any stray `llama-server` / `image-server` we didn't spawn.
- Stale sidecar after an app upgrade (known Tauri NSIS bug) → see `12_packaging.md`.

## Sources
- [Tauri v2 sidecar / externalBin](https://v2.tauri.app/develop/sidecar/) · [embed Python for sidecar (discussion #2759)](https://github.com/orgs/tauri-apps/discussions/2759)
