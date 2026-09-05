# 04 — Phase 5.10 cross-check

**Date:** 2026-09-05
**Method:** read all binding docs + all ADRs + all plan files + `ROADMAP.md`
together; check for contradictions, ownership gaps, non-executable gates, and
name/number disagreement.

---

## Checks

### 1. Requirement coverage (gate check 2)
`PROJECT.md` §3–4 summarizes the requirements by group and states the numbered
source is `docs/product/requirements.md`. Every requirement **group** (chat,
voice, personas, history, memory, models, acquisition, image, resource mgmt,
characters, discovery, character conversations, relationship, character-sent
images, visual identity; local-first, privacy, content, performance, reliability,
security, architecture, platform) is present in `PROJECT.md`. **Nothing is cut.**
Two items were scoped, not cut, and both are recorded:
- FR-C90/C91 → best-effort prompt-based v1 (portraits/selfies) — `PROJECT.md` §3,
  `requirements.md` FR-C90, ADR-0011.
- FR-54 (memory *edit*) → later phase; view+delete in v1 — `requirements.md` §4.
**PASS.**

### 2. ADR status (gate check 3)
All 15 ADRs are `ACCEPTED`. No `PROPOSED`, no `UNDECIDED`. Deferred *implementation*
choices (NVFP4 vs NF4, `synchronous` level, pipe vs TCP, npm vs pnpm, at-rest
encryption) are listed in `docs/decisions/README.md` as "decide during
implementation" — they do not change the architecture. **PASS.**

### 3. Single-authority map (gate check 4)
`ARCHITECTURE.md` §3 lists one authority per concern. Cross-checked against the
ADRs: one conversation engine (ADR-0002 shell / Phase 17), one lifecycle manager
(ADR-0003/0014… Phase 14), one resource manager (ADR-0007), one scheduler
(ADR-0010), one db layer + dedicated writer (ADR-0009), one config (Phase 8), one
context builder (Phase 20). No concern has two authorities. The image server's
former self-managed VRAM is explicitly moved to the resource manager (ADR-0006,
Phase 4 R-H8). **PASS.**

### 4. Traceability (gate check 4)
`ARCHITECTURE.md` §5 traces chat, voice, image, and character flows end to end;
`AI_PIPELINES.md` gives each pipeline's model/transport/resource/failure detail. A
reader can follow a chat request and an image request without gaps. **PASS.**

### 5. Transport consistency
`CLAUDE.md` Article I §3 = `ADR-0013` = `ARCHITECTURE.md` §2 = `SECURITY.md` C3:
named-pipe / token'd loopback for model servers, stdio JSON-lines for stateless
workers, offline env for all (ADR-0015). **Consistent.**

### 6. VRAM / RAM numbers
`~11.4 GB` image-generating, `~1.6 GB` idle, image ⟂ LLM, ~13–14 GB effective
budget — stated identically in `PROJECT.md` §7, `ARCHITECTURE.md` §4,
`AI_PIPELINES.md` §5, `PERFORMANCE.md`, `docs/research/phase3/README.md`. NVML
per-process unavailable → ledger accounting — `ADR-0007` = `ARCHITECTURE.md` §4/§7
= `docs/verification/02_phase3_probes.md`. **Consistent.**

### 7. Phase numbering
`ROADMAP.md` §4 (Phase 0–40) ↔ `docs/plan/NN_*.md` filenames ↔ ADR "governing"
references. All 06–40 plan files carry the "Architecture frozen at Phase 5" banner
with governing ADRs. `ROADMAP.md` §4 gate summaries match the plan files' gates
(spot-checked 6, 9, 12, 15, 16, 22, 27). **Consistent.**

### 8. `ROADMAP.md` §7 open items
- O1 (memory/RAG) → ADR-0012 (FTS5, embeddings deferred with a trigger). Resolved.
- O2 (single vs multi-user) → single-user (`PROJECT.md` §4, NFR-61). Resolved.
- O3 (transport) → ADR-0013. Resolved.
- O4 (crate structure) → ADR-0001. Resolved.
- O5 (CUDA build / Python env / pkg mgr / storage) → ADR-0004/0014; pkg mgr +
  storage default pinned at Phase 6. Mostly resolved.
- O6 (identity bar) → ADR-0011, owner-confirmed. Resolved.
- O7 (Krea quant) → NF4 v1, benchmark NVFP4 at Phase 22. Deferred, tracked.
§7 updated to a "Resolved / deferred" summary.

### 9. No application code (gate check 7)
`git status` / `git ls-files` — only `.md` files + the `.git` dir. No `Cargo.toml`,
`package.json`, `src/`, `src-tauri/`. The Phase 3 probes were throwaway and
deleted. **PASS.**

### 10. `docs/OVERVIEW.md`
Pre-freeze doc. Header updated to point at `PROJECT.md` + `ARCHITECTURE.md` as the
authoritative successors; kept for history. No contradictions that matter (it is
explicitly non-authoritative).

---

## Result

No blocking contradictions. Ownership is unambiguous. Every phase gate check is a
physically executable command or observation. `ROADMAP.md`, `docs/plan/`, the
ADRs, and the binding docs agree.

**Phase 5 gate: MET.** Architecture is frozen. Pointer → **Phase 6** (Tauri +
React + Rust bootstrap — the first application code).
