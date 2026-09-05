# src/ — React / TypeScript frontend

**Presentation only** (`CLAUDE.md` Article I, `ARCHITECTURE.md` §2). No SQLite, no
process spawning, no filesystem, no network. All communication with the Rust core
goes through typed IPC in `lib/ipc.ts`.

| Path | What |
| ---- | ---- |
| `main.tsx` | Entry — mounts the router inside the error boundary; subscribes to `app://ready` |
| `router.tsx` | Hash routes: 3 primary tabs (`/chat` `/images` `/discovery`) + `/models` + `/settings` + 404 |
| `components/AppShell.tsx` | Top nav + routed `<Outlet/>` |
| `components/ErrorBoundary.tsx` | Top-level render-error recovery panel |
| `components/Placeholder.tsx` | "not built yet — Phase N" stub for unbuilt surfaces |
| `pages/*.tsx` | One per route (placeholders until their phase) |
| `lib/ipc.ts` | **Typed wrappers over Tauri IPC** — the only place `invoke`/`listen` is called |
| `lib/log.ts` | Frontend logger → forwards to the Rust structured log via `frontend_log` |
| `styles/theme.css` | Colour tokens (light/dark, OS-following) — no hard-coded colours in components |
| `bindings/` | Types generated from Rust by `ts-rs` — **do not hand-edit**; `cargo test` regenerates |
| `test/setup.ts` | Vitest setup — stubs the Tauri bridge for jsdom |

Design: `docs/decisions/0002-ipc-design.md`, `UI_GUIDELINES.md`.
