# ADR-0003 — LLM runtime: `llama-server` as a loopback-HTTP child process

- **Status:** PROPOSED (Phase 3 draft) · **Date:** 2026-09-05
- **Research:** `docs/research/phase3/02_llm-runtime.md` (D-3a) · resolves **O3**

## Context
LocalAI needs local LLM inference with streaming + cancellation. The 16 GB budget
forces frequent model swaps (image generation evicts the LLM). A model crash
(CUDA OOM) must not take down the app.

## Options considered
- `llama-server` child process over loopback HTTP.
- `llama-cpp-2` FFI (in-process).
- stdio proxy around `llama-server`.

## Decision
**`llama-server`, spawned and supervised by the Rust core, bound to
`127.0.0.1:<free port>` (no external interface).** Rust uses a Windows Job Object
so it dies with the app. Streaming via SSE → Tauri Channel. Cancellation = drop
the request + `CancellationToken`. Crash detection via `child.wait()` / `/health`
/ stream error → report to the lifecycle manager, release the VRAM reservation.

llama.cpp-specific detail is confined to one adapter module exposing an
`LlmBackend` trait (`generate`, `stream`, `cancel`, `health`, `shutdown`).

**Fallback:** `llama-cpp-2` FFI on a dedicated thread + watchdog, if `llama-server`
proves unworkable on Windows/Blackwell.

## Consequences
- **Crash isolation** — an LLM OOM kills only the child; the conversation UI,
  scheduler, and any in-flight image job survive.
- A loopback socket exists → `CLAUDE.md` Article I is updated (ADR-0013) to ratify
  "model server = loopback HTTP".
- Port management + a small spawn race (bind `:0`, read, drop, pass) — accepted,
  or use a named pipe (ADR-0013 opt).
- ~few ms JSON/HTTP overhead per request — negligible vs token latency.
