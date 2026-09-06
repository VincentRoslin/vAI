# workers/ — Python AI worker scripts

Stateless subprocesses the Rust core spawns, supervises, and kills
(`CLAUDE.md` Article I, ADR-0013). Each speaks **one JSON object per line** over
stdin/stdout (`stderr` = logs only); the envelope is `contracts::worker`
(`WorkerHello` → `WorkerRequest` / `WorkerResponse`). Every worker is launched
with the ADR-0015 "hostile network" environment and loads models **from local
paths only** — never a repo id, never the network.

| File | Worker | Phase | Notes |
| ---- | ------ | ----- | ----- |
| `stt.py` | STT (`WorkerKind::Stt`) | 18 | faster-whisper `large-v3`, `compute_type=float16`, CUDA. Loads `models/stt/` once; transcribes a WAV path per request. Prepends the venv's `nvidia/*/bin` dirs to `PATH` before importing `ctranslate2` (CUDA 12 / cuDNN 9 DLLs). |
| `stt_fake.py` | STT (fake) | 18 | Stdlib only. Honours the protocol, echoes a canned transcript. Used by `worker::` + `voice::` unit tests so they need no venv/GPU. |
| `tts.py` | TTS | 19 | _(later)_ Chatterbox Turbo. |
| `embedder.py` | Embed | 27 | _(later)_ face / image identity vectors. |

## Environment

- **Dev:** `<repo>/.venv` (`uv`-managed), from `requirements.txt`. Recreate with
  `node scripts/setup-venv.mjs`. The venv is gitignored (~2.2 GB).
- **Shipped:** embedded CPython + one shared frozen venv, same pinned set
  (ADR-0014). Resolved as a sibling of the executable, never PATH.
- Isolated test run: `python -m uv run --python .venv workers/<name>.py` with the
  ADR-0015 env set (the Rust supervisor sets it in `worker::env`).

## Governing docs

`docs/decisions/0013-subprocess-transport.md`,
`docs/decisions/0015-python-worker-network-lockdown.md`,
`docs/decisions/0018-python-worker-dev-environment.md`,
`docs/plan/18_voice-in.md`, `docs/contracts.md` (worker payload schemas).
