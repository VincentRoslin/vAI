# Phase 37 — Packaging (Windows installer)

> **Architecture frozen at Phase 5.** Governing: **ADR-0014** (MSI/WiX, embedded
> CPython + one shared frozen venv, deterministic sibling layout, first-run model
> onboarding, offline install, signing stubbed), **ADR-0015** (worker network
> env), `SECURITY.md` C10, `DEVELOPMENT.md` §1. Concrete steps filled in at phase
> entry; the design does not change. Re-check the Tauri NSIS stale-sidecar bug.

## Objective
A Windows installer that bundles the app, native dependencies, and the AI workers,
handles first-run model acquisition, and produces an app that runs fully offline
afterwards.

## Depends on
Feature + audit complete (through Phase 36). Phase 3.14 (packaging approach),
Phase 12 (acquisition).

## Not in this phase
- Auto-update infrastructure (a manual "check for updates" per the offline ADR is
  the ceiling).
- Code signing certificate procurement (flag it; the pipeline should support
  signing when a cert exists).

## Architecture notes
- Python workers ship per the ADR (embedded Python / `uv` / frozen). CUDA runtime,
  llama.cpp binary, CTranslate2 bundled or acquired per the ADR.
- The installed layout is deterministic; Rust locates workers + binaries by a
  known relative path, not PATH guessing.
- Model files are NOT bundled (too large) — acquired on first run, skippable.

## Performance notes
- Installer size recorded. First-run acquisition time recorded.
- Cold start from the installed build compared to the dev baseline.

## Step outline
1. Configure the Tauri bundler (MSI/NSIS per ADR); app metadata, icons.
2. Bundle native deps (llama.cpp, CTranslate2, CUDA runtime) into the layout.
3. Bundle the Python worker runtime per the ADR; verify workers launch from the
   installed layout.
4. First-run flow: detect missing models → offer acquisition (Phase 12) →
   skippable → app usable (with a clear "no model" state) if skipped.
5. Uninstaller: removes the app; asks about user data + downloaded models.
6. Build the installer in a clean environment; install on a clean Windows VM.
7. Run the full smoke suite on the installed app.
8. Run the Phase 32 offline gate on the installed app.
9. Record sizes + timings.

## Verification gate
1. A clean-machine install produces a launchable app.
2. All workers + native runtimes launch from the installed layout (no dev paths).
3. First-run model acquisition works and is skippable; skipping leaves a usable
   app with a clear no-model state.
4. Uninstall is clean; user-data handling is explicit.
5. The installed app passes the Phase 32 offline gate.
6. The installed app passes the Phase 16 reliability gate.
7. Installer size + first-run acquisition time + cold start recorded.

## ADRs / open questions
- Code signing (deferred until a cert exists; pipeline hook present).
