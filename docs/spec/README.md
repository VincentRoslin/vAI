# docs/spec — the frozen specification

The seven binding documents, **frozen at Phase 5** (2026-09-05). Together with
`CLAUDE.md` (the constitution) and `docs/decisions/` (the ADRs) they are the
authoritative record of *what* LocalAI is and *how* it is built. Changing any of
them is a STOP → propose → approve step, recorded in `ROADMAP.md` §5 and, where a
decision changes, a new/amended ADR.

| File | Authoritative for |
| ---- | ----------------- |
| `PROJECT.md` | The officialized product definition — what LocalAI is / does / is not; the binding FR/NFR summary (the numbered requirements are in `docs/product/requirements.md`). |
| `ARCHITECTURE.md` | The frozen system design — runtimes, Article I boundaries, the single-authority map, the VRAM constraint, cross-cutting flows. |
| `AI_PIPELINES.md` | Each AI pipeline end to end — LLM · STT · VAD · TTS · image · identity · memory · relationship. |
| `SECURITY.md` | Threat model + controls. |
| `PERFORMANCE.md` | Performance budgets + the measurement method; the running baseline table. |
| `UI_GUIDELINES.md` | The UX / theming / accessibility bar (visual soft-lock detail is in `docs/design/`). |
| `DEVELOPMENT.md` | Dev prerequisites, the dev loop, the check suite, git conventions. The one "living" doc here — updated as tooling lands. |

Not in this folder: `CLAUDE.md` and `ROADMAP.md` stay at the repo root (Claude
Code auto-loads `CLAUDE.md`; `ROADMAP.md` is the per-session entry point).
`docs/OVERVIEW.md` is the superseded early overview, kept for history.

**Moved here from the repo root at Phase 15** (2026-09-06) — organisational only,
no content change. Prose elsewhere may still name a doc without the `docs/spec/`
prefix; the names are unique.
