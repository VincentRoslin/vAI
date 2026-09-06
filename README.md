# LocalAI

A Windows-first, local-first AI desktop application — local LLM chat & voice,
image generation, and persistent AI characters. Runs fully offline after models
are acquired.

**Status:** core platform (Phase 15 of the 41-step course). LLM adapter in place;
first end-to-end chat is Phase 16.

## Quickstart

Prerequisites (see `docs/spec/DEVELOPMENT.md` §1): Rust ≥ 1.98 + MSVC build tools,
Node ≥ 22, WebView2 runtime.

```bash
git clone <this repo>
cd vAI
npm install
git config core.hooksPath .githooks   # enable the commit hooks
npm run tauri dev                      # launch the app
```

## Scripts

| Command | Does |
| ------- | ---- |
| `npm run tauri dev` | Launch the app (Vite + Rust, hot reload) |
| `npm run check` | The full check suite — format, lint, types, tests, build (CI runs this) |
| `npm run lint` / `npm run format` | ESLint / Prettier on the frontend |
| `npm run typecheck` | `tsc --noEmit` |
| `npm run test` | Vitest (frontend) |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Rust tests + regenerate `src/bindings/` |
| `npm run tauri build` | Production build |

## Layout

| Path | What |
| ---- | ---- |
| `src/` | React/TypeScript frontend — **presentation only** (`src/README.md`) |
| `src-tauri/` | Rust application core — the authoritative nucleus (`src-tauri/README.md`) |
| `src/bindings/` | TypeScript types generated from Rust by `ts-rs` (committed; regen'd by `cargo test`) |
| `scripts/` | `check.mjs` — the check suite |
| `.githooks/` | pre-commit (fmt/lint) + commit-msg convention |
| `docs/` | Project docs — start at `CLAUDE.md` and `ROADMAP.md` |
| `docs/spec/` | The 7 frozen spec docs (`docs/spec/README.md`) |

## Documentation

`CLAUDE.md` (rules + a "looking for X → go to Y" map) · `ROADMAP.md` (state +
plan) · `docs/spec/PROJECT.md` (what it is) · `docs/spec/ARCHITECTURE.md` (how
it's built) · `docs/spec/AI_PIPELINES.md` · `docs/decisions/` (ADRs). Every
`docs/` subdirectory has a `README.md` index.
