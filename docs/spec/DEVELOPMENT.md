# DEVELOPMENT.md — LocalAI

**Status:** Frozen at Phase 5, 2026-09-05. Living doc — updated as the toolchain
is wired (Phase 6) and finalized at Phase 39–40.

---

## 1. Prerequisites (Windows)

From `docs/verification/01_env_audit.md`, confirmed present on the reference
machine:

| Tool | Version (reference) | Notes |
| ---- | ------------------- | ----- |
| Rust | 1.98.0 (MSVC host) | `rustup`, `x86_64-pc-windows-msvc` |
| Node | 24.19.0 | npm 24.x (package manager decision below) |
| Python | 3.11.9 | for the AI workers; a managed venv, not the system install |
| Git | 2.52.0 | |
| VS Build Tools 2022 | 17.14 (MSVC 14.44) | + Windows SDK 10.0.26100 |
| CMake | 4.4.2 | for the llama.cpp build |
| **CUDA Toolkit 13.x** | **to install** | required to build `llama-server` from source (ADR-0004); ~3 GB |
| WebView2 Runtime | present | Tauri dependency |

**Package manager:** npm (default; `corepack` available for pnpm if a concrete
reason appears — Phase 6 decision, ADR if changed).
**Python env:** two `uv`-managed venvs (**ADR-0018** + its Phase 22.B two-venv
amendment). Create/refresh:

```
python -m pip install --user uv      # once
node scripts/setup-venv.mjs           # builds .venv AND .venv-image
node scripts/setup-venv.mjs image     # just one, if only that changed
```

- `<repo>/.venv` — workers (STT/TTS), from `workers/requirements.txt` +
  `workers/overrides.txt` (~6 GB — faster-whisper + CUDA-12 libs + torch
  2.11.0+cu128 + Chatterbox; the override forces the Blackwell torch over the
  `chatterbox-tts` pin).
- `<repo>/.venv-image` — the image sidecar only, from `image_gen/requirements.txt`
  + `workers/overrides.txt` (diffusers + Krea 2 deps + its own torch; ~6–8 GB).
  Separate because `chatterbox-tts` hard-pins `torch`/`transformers`/`diffusers`
  against Krea 2's needs.

Both are gitignored. CI running the `worker::` / `voice::` / `image::` unit tests
needs **no** venv — those use stdlib fakes (`stt_fake.py` / `tts_fake.py` /
`image_gen/server_fake.py`); only the `#[ignore]`d live gates need the real
venvs + GPU. The shipped app uses neither — it bundles an embedded CPython + the
same frozen sets (ADR-0014).

## 2. Repository layout (target, post-bootstrap)

```
/                     README.md, CLAUDE.md, ROADMAP.md, .gitattributes
src-tauri/            Rust core (single crate, module per subsystem)
src/                  React/TS frontend
workers/              stt.py, tts.py, image_server.py, embedder.py
docs/spec/            the 7 frozen spec docs (PROJECT, ARCHITECTURE,
                      AI_PIPELINES, SECURITY, PERFORMANCE, UI_GUIDELINES, this file)
docs/plan/            per-phase implementation plans
docs/decisions/       ADRs
docs/design/          visual language (soft lock)
docs/research/        pre-architecture research (historical)
docs/verification/    gate evidence
docs/product/         requirements
docs/contracts.md     the typed contract vocabulary
```

## 3. The dev loop (per `docs/plan/NN`)

```
Read ROADMAP.md §1  →  open docs/plan/<current phase>
  →  (if the "finalized at phase entry" banner is present, finalize the steps)
  →  work the steps in order; each has a Verify line — run it, observe, record
  →  run the full check suite
  →  run / launch the app when runtime behaviour matters
  →  inspect the git diff
  →  when every step + the gate passes with recorded evidence:
        update ROADMAP.md §1 + §5, commit
```

## 4. Check suite (wired in Phase 6)

One command runs all of:

| Check | Tool |
| ----- | ---- |
| Rust format | `cargo fmt --check` |
| Rust lint | `cargo clippy -- -D warnings` |
| Rust build + test | `cargo check` / `cargo test` |
| Frontend lint | ESLint |
| Frontend types | `tsc --noEmit` (strict) |
| Frontend test | Vitest |
| Production build | `tauri build` (or the frontend build in CI) |
| Dep audit (Phase 35+) | `cargo audit`, `npm audit`, Python audit |
| a11y (Phase 30+) | axe-core scan of key pages |

CI runs the suite on every push; `main` stays releasable.

## 5. Running subsystems locally

- **Full app (dev):** `npm run tauri dev` (vite dev server + a debug binary).
- **Full app (real / release, Phase 18.5):** `node scripts/deploy-local.mjs` —
  `npm run build` + `cargo build --release`, then launches
  `src-tauri/target/release/localai.exe` in place against the real
  `%APPDATA%\com.localai.app\config.json`. Prints the git SHA and the
  `config` / `logs` / `diagnostics` paths. `--no-launch` builds + prints only.
  This is for hands-on live-testing; the **installer** is Phase 37.
  - **Logs:** a rotating JSON-lines file at
    `%APPDATA%\com.localai.app\logs\localai.jsonl.<date>` (redacted, swept to
    7 files / ~50 MB). `tail` the newest for a session you want to inspect.
  - **Diagnostics:** **Settings → Export diagnostics** writes
    `%APPDATA%\com.localai.app\diagnostics\diag-<ts>.json` (build + config +
    registry + resources + lifecycle + conversation *metadata* + recent logs +
    host facts — redacted, no conversation content). Share that file for
    debugging. Everything stays local (ADR-0015).
- **`llama-server`:** the app spawns it from `<runtimes.dir>/llama-server.exe`
  (config `runtimes.dir`, default `%APPDATA%\com.localai.app\runtimes`; on the
  reference machine it points at `<repo>/runtime/llama-server/`). Do not run it
  by hand. **In use: the pinned prebuilt** (ADR-0004 amended, Phase 15.D) —
  release **`b10819`**, CUDA 13.3, sm_120-capable:

  ```powershell
  # from https://github.com/ggml-org/llama.cpp/releases/tag/b10819
  #   llama-b10819-bin-win-cuda-13.3-x64.zip   (binaries)
  #   cudart-llama-bin-win-cuda-13.3-x64.zip   (CUDA 13.3 runtime DLLs)
  # extract BOTH into  <runtimes.dir>/llama-server/   (SHA-256s in
  #   docs/verification/14_phase15_llama.md). ~540 MB, gitignored.
  ```

  Source-build fallback (ADR-0004, needs CUDA Toolkit 13.x — `nvcc` on PATH):

  ```powershell
  git clone https://github.com/ggml-org/llama.cpp
  cd llama.cpp; git checkout b10819
  cmake -B build -DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=120 `
        -DLLAMA_CURL=OFF -DCMAKE_BUILD_TYPE=Release
  cmake --build build --config Release --target llama-server -j
  ```

  `scripts\build-llama.ps1` will wrap the chosen path (Phase 37 packaging).
- **Python workers:** the app spawns them from the venv; for isolated testing,
  `uv run workers/<name>.py` with the offline env (ADR-0015) set.
- **DB:** `%APPDATA%\com.localai.app\localai.db` (+ `-wal` / `-shm`), created and
  migrated on first run (ADR-0009). Pre-migration backups: `db-backups/`
  (last 5). Delete `localai.db*` to reset. `scripts/db-reset.ps1` (Phase 14) will
  wipe + re-migrate + seed.
- **Config:** `%APPDATA%\com.localai.app\config.json` (ADR-0016, schema **v5**).
  Absent on a fresh machine — the app runs on defaults and only writes the file
  when a value changes. Delete it to reset to defaults; an unparseable file is
  auto-backed-up to `config.json.corrupt-<unix>` and defaults are used.
  - `models.dir` (default `<app_data>/models`) and `runtimes.dir` (default
    `<app_data>/runtimes`) can point anywhere. The reference machine sets both
    to repo-local paths (`<repo>/models`, `<repo>/runtime/llama-server`) — both
    `.gitignore`d — so no large runtime blob lives outside the project. The DB +
    `config.json` themselves stay in `%APPDATA%` (small, conventional).

## 6. Line endings

`.gitattributes` normalizes to LF (`* text=auto eol=lf`, binaries excluded,
`*.ps1`/`*.cmd`/`*.bat` keep CRLF). Added in Phase 6 — CRLF warnings are gone.

## 6a. Tooling (wired in Phase 6)

| Tool | Config | Command |
| ---- | ------ | ------- |
| rustfmt | `rustfmt.toml` | `cargo fmt` |
| clippy | `[lints]` in `src-tauri/Cargo.toml` (deny warnings; `all` + `pedantic`) | `cargo clippy --all-targets -- -D warnings` |
| ESLint 9 (flat) | `eslint.config.js` (`no-console` error; logs via `lib/log.ts`) | `npm run lint` |
| Prettier | `.prettierrc.json` / `.prettierignore` | `npm run format` |
| tsc (strict) | `tsconfig.json` (`noUncheckedIndexedAccess`) | `npm run typecheck` |
| Vitest (jsdom) | `vitest.config.ts` / `src/test/setup.ts` | `npm run test` |
| ts-rs bindings | `#[ts(export)]` on DTOs → `src/bindings/` (committed) | `cargo test` regenerates; `npm run check` fails on drift |
| Git hooks | `.githooks/` — enable once: `git config core.hooksPath .githooks` | pre-commit (fmt+lint), commit-msg (subject ≤ 72, blank line 2) |
| Full suite | `scripts/check.mjs` | `npm run check` |

## 7. Git

- Branch off `main` for any change; `main` is always releasable.
- Small, verified commits — one logical change each. Inspect the diff, confirm the
  changed files, run the relevant checks, ensure no secrets / generated junk /
  unrelated changes before committing.
- Commit message: short subject, then a body explaining what and why; end with the
  `Co-Authored-By` trailer.
- **Local commits only.** No push / remote / GitHub without an explicit
  instruction — the remote/hosting policy is decided at Phase 40.
- The full branch model + commit convention + tagging scheme + a commit-msg hook
  are formalized in **Phase 40**.

## 8. Model / asset downloads

Before any step that downloads a model, LoRA, or cache: **ask the owner whether
the file already exists locally** — he has pre-downloaded caches in related
projects (`CLAUDE.md` → "Model / asset downloads"). Do not read from his other
projects.

## 9. Keep it traceable (applies to code too)

The repo must stay navigable without scanning it. When you add anything:

- **A new Rust module / subsystem** → add or update its entry in
  `ARCHITECTURE.md` (§2/§3 single-authority map) and, if it's a new top-level
  concern, the `CLAUDE.md` Document Map.
- **A new directory** (`workers/`, `scripts/`, a crate) → give it a short
  `README.md` naming what's inside and pointing at the governing doc/ADR.
- **A new ADR** → add the row to `docs/decisions/README.md`.
- **A new gate-evidence or research file** → add the row to that directory's
  `README.md`.
- **A module that owns a concern** → it appears exactly once in the
  `ARCHITECTURE.md` single-authority map.

The Phase 36 audit gate is "a trace of each subsystem vs `ARCHITECTURE.md` — no
undocumented component"; the Phase 40 gate is "a fresh session states status +
next action from the repo alone". Don't let those find gaps — close them as you
go.

## 10. Verification honesty

`CLAUDE.md` Article IV. "Passed" means a command was run and its output observed.
A `ROADMAP.md` gate is met only when every check ran with recorded evidence. If a
check cannot be executed here, record `NOT EXECUTED` with the reason — never as
passed.
