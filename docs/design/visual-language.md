# Visual Language — soft lock

**Status:** soft lock, owner-agreed 2026-09-05. Elaborates `UI_GUIDELINES.md`.
"Soft" = the **firm** parts (shell, chat geometry, the scale) are the baseline
every screen builds against; the **flexible** parts are decided per feature phase.
A full visual overhaul is expected later — this exists so the first real screens
(Phase 16 chat onward) aren't retrofitted.

**Non-negotiable:** a UI or CSS change **never** alters behaviour, state, or IPC.
The frontend is presentation only (`CLAUDE.md` Article I); styling/layout edits
touch markup + CSS only, and the check suite (Vitest, and from Phase 30 the
keyboard + a11y tests) must still pass. See `UI_GUIDELINES.md` §1.

Philosophy: **flat and architectural, geometry does the work, not decoration.**
Dark theme first (light supported per `UI_GUIDELINES.md` §4). No floating panels,
nested cards, heavy shadows, or decorative containers.

---

## 1. Firm — the shell

Three-zone *capable* desktop layout:

```
┌────────────┬─────────────────────────────┬──────────────┐
│ left nav   │  center workspace           │ right panel  │
│ ~260 px    │  (the visual focus)         │ ~300 px      │
│            │                             │ FLEXIBLE     │
│            │                             │ (see §4)     │
└────────────┴─────────────────────────────┴──────────────┘
     1px divider              1px divider
```

- **Left nav** — fixed width ~260 px, collapsible to an icon rail.
  - Logo + "LocalAI" + a collapse toggle at the top.
  - Primary nav items: icon + label, an active item gets a rounded filled pill
    (`--accent` bg). One row each: Chat/Voice · Image · Discovery.
  - A "Recent" list below (conversation / character titles), scrollable.
  - **Settings pinned at the bottom as a plain nav item** — a gear + "Settings",
    same style as the others. **No collapse chevron on it.** It routes to
    `/settings`, it is not a collapsible panel.
- **Center workspace** — takes the majority of the width; expands to fill when
  either sidebar is collapsed (no dead space).
- **Right panel** — see §4. Optional, per surface, ~300 px, collapsible.
- Panels align to a shared grid; thin 1px `--border` dividers between zones.

## 2. Firm — the scale

| Token | Value | Use |
| ----- | ----- | --- |
| `--radius-window` | 14px | large containers, the outer content card |
| `--radius-card` | 12px | cards, the right-panel sections, message bubbles' outer feel |
| `--radius-control` | 9px | nav items, buttons, inputs |
| `--radius-bubble` | 14px | chat message bubbles |
| spacing | **4 / 8 / 12 / 16 / 24 / 32** | padding + gaps; nothing off-scale |
| `--content-max` | **680px** | max width of message text (see §3) |
| divider | 1px `--border` | between zones and list rows |
| shadow | none / barely-there | flat; elevation comes from `--bg-elevated` not shadow |

The outer **window** frame corners are the OS's — no custom titlebar, Windows
min/max/close stay visible (`UI_GUIDELINES.md` §8). "Rounded outer corners" in any
earlier draft is dropped.

## 3. Firm — chat geometry

- Header: conversation/character **title** left, an `⋮` overflow menu right, thin
  divider under it.
- **User messages** align right, avatar to the **right** of the bubble.
- **Assistant / character messages** align left, avatar to the **left**.
- Bubbles: `--radius-bubble`, padding `12 16`, moderate — **not** oversized pills.
  User bubble `--bg-elevated`; assistant bubble `--bg-sunken` (or vice-versa,
  decide in Phase 16 — the point is a clear tonal difference, not colour).
- Timestamp: small, `--text-muted`, under the bubble.
- Message **text column caps at `--content-max`** even when the window is huge —
  the bubble may be wider than the text, the text is not.
- Streaming: tokens append to the last bubble without reflowing the transcript
  (`UI_GUIDELINES.md` §2).

## 4. Flexible — the right panel (soft, per feature phase)

The right panel's **existence, width, and contents are decided per surface** when
that surface is built. It is **not** a persistent system/hardware monitor —
technical info recedes (`UI_GUIDELINES.md` §1 hierarchy). Model info, GPU/VRAM
status, temperature, etc. live in **Settings** or a small status popover, never an
always-on rail.

Working assumptions (revise at the phase):

| Surface | Right panel likely holds | Or |
| ------- | ------------------------ | -- |
| Plain chat (Tab 1, no persona) | nothing — panel hidden, center full-width | a thin "conversation settings" drawer on demand |
| Persona chat (Tab 1) | the active persona summary | hidden |
| Character conversation (Tab 3) | character identity + relationship stage + a memory peek + a gallery strip | collapsible |
| Image Generator (Tab 2) | **n/a** — this tab is a params form + a results grid, not the three-zone layout |
| Discovery (Tab 3 feed) | **n/a** — swipe cards, its own full-bleed layout |

## 5. Flexible — per-tab layout

- **Chat / Voice** — the three-zone layout above.
- **Image Generator** — a generation form (prompt, preset, LoRA, size, seed) +
  progress + a results gallery grid. Left nav stays; no center "conversation", no
  right panel by default.
- **Discovery** — a centred swipe-card stack (profile + images + pass/keep), its
  own layout. Choosing a character opens that character's conversation (§3 + the
  Tab 3 right panel).
- **Models / Settings** — a standard settings layout (sections, forms). Model
  acquisition (the HF picker) is a list + detail here.

## 6. Firm — required states & a11y

All from `UI_GUIDELINES.md` (§2, §5) and unchanged: every data view has
loading / empty / error / streaming / offline states; full keyboard path; screen-
reader labels + live regions; `prefers-reduced-motion`; axe scan zero critical.
A visual change that regresses any of these is a bug.

## 7. Implementation notes

- Colours are **always** `--*` tokens (`src/styles/theme.css`). No hex in
  components.
- The shell (`AppShell`) owns the zones; pages render into the center; a page may
  contribute a right-panel node via a well-defined slot (design in Phase 16).
- Shared state components (loading skeleton, empty state, error panel, streaming
  indicator, message bubble, avatar, nav item, card, section header) are built
  once and reused — Phase 30 formalizes the set; establish them as they're first
  needed.
