# UI_GUIDELINES.md — LocalAI

**Status:** Frozen at Phase 5, 2026-09-05. Binding.
The bar every user-facing surface must meet. Phase 30 (UI/UX pass) applies these
across all flows; Phase 30's gate is the check.

---

## 1. Principles

- **The frontend is presentation only.** It renders Rust-owned state and sends
  typed IPC. It never owns domain data, never blocks on heavy work.
- **Never look frozen.** Every operation over ~200 ms shows a state; long ones
  show progress and a way to cancel.
- **Persistent, not disposable.** Conversations, characters, images always look
  like they'll still be there tomorrow — no "unsaved" ambiguity, no silent loss.
- **Plain language for AI infrastructure.** The user never sees "CUDA OOM",
  "process exited 1", a port number, or a stack trace. They see "not enough
  graphics memory — free up ~3 GB or pick a smaller model", with an action.

## 2. Required states — every data-driven view

| State | Requirement |
| ----- | ----------- |
| **Loading** | A skeleton or a labelled spinner within ~200 ms; never a blank frame |
| **Empty** | An explanation + the primary action ("No conversations yet — start one") |
| **Error** | Plain-language cause + a recovery action (retry / open settings / free space); the raw error is available behind a "details" affordance for logs |
| **Streaming** | An unambiguous in-progress indicator; a **stop** control; incremental content that does not reflow the whole view |
| **Offline** | Where a feature needs the network (model search), a clear offline state — the rest of the app stays fully usable |

## 3. Structure

- **Three primary tabs** (Chat/Voice · Image Generator · Discovery) + Models +
  Settings, always reachable. Tab state is isolated; switching tabs never loses
  in-progress work in another tab.
- **One shared conversation component** renders Tab 1 and Tab 3 conversations
  (different sources, same behaviour) — no divergent chat UIs.
- Navigation is obvious and shallow. A 404 route exists.

## 4. Theming

- **Light and dark**, following the OS setting by default, user-overridable.
- All colours are **tokens**; no hard-coded colours in components.
- Both themes meet **WCAG AA contrast** for text and interactive elements.
- No unstyled flash on load or theme switch.

## 5. Accessibility (the bar)

- **Full keyboard path** through every primary flow — start a chat, send a
  message, stop generation, open a character, generate an image, change a setting
  — all doable without a mouse. Visible focus indicators.
- Screen-reader labels/roles on all controls; **live regions** for streaming
  output announced without flooding (announce meaningful chunks, not every token).
- `prefers-reduced-motion` respected — swipe/transition animations reduce to
  fades/instant.
- Automated a11y scan (axe-core class) in the check suite → **zero critical
  violations** on the key pages.
- Manual audit pass with recorded sign-off (Phase 30).

## 6. Feedback for AI operations

| Operation | The user sees |
| --------- | ------------- |
| Model loading | "Loading <model>…" with progress where available; the chat composer disabled with a reason |
| Model swap for an image | "Preparing image generation…" (they don't need to know the LLM unloaded) |
| Image generating | Queued → generating with step/percent where available; a cancel control; result appears automatically |
| Voice | Distinct **listening / thinking / speaking** states |
| Download | Progress, speed, pause/resume/cancel; survives an app restart |
| Driver reset (rare) | One "the graphics driver reset — recovering" notice, then normal |

## 7. Content

- **No content warnings, blurring, or age gates** — NSFW/explicit is allowed and
  unfiltered (NFR-15). The UI presents generated content plainly.
- Galleries and discovery cards are lazy-loaded and virtualized; full-res only on
  explicit view.

## 8. Responsiveness

- The window opens **maximized** (windowed — the Windows title bar and
  minimize/maximize/close controls stay visible; not exclusive fullscreen). It is
  **resizable** down to a sensible minimum (720×520) without breakage. The
  un-maximized restore size is 1280×800, centred. (Persisting the last
  size/position across launches — via `tauri-plugin-window-state` — is a small
  later add, not v1-critical.)
- Relative units; flex/grid layout.
- Wide content (long messages, image grids) scrolls within its own container —
  the window never scrolls horizontally.
