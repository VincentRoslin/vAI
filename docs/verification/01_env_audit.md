# 01 — Development Environment Audit

**Date:** 2026-09-05
**Machine:** Vincent's Windows workstation
**Method:** read-only inspection (`Get-CimInstance`, `nvidia-smi`, `--version` probes)
plus one throwaway `cargo new` + `cargo build` in the scratch dir to physically
verify the Rust → MSVC → linker chain. No application code written. Nothing
installed. Scratch project deleted after use.

---

## Verdict

| Area | Status |
| ---- | ------ |
| Tauri + React + Rust bootstrap prerequisites | ✅ **READY** — all verified |
| GPU / driver for local LLM inference | ✅ Present (RTX 5080, 16 GB, CUDA 13.3 runtime) |
| Build-from-source of CUDA llama.cpp | ⚠️ **CUDA Toolkit (nvcc) not installed** — decision needed |
| Python worker environment strategy | ⚠️ Bare Python 3.11, no venv/uv — decision needed |
| Package manager (pnpm) | ⚠️ Not installed; npm works, corepack available |
| Disk headroom for models | ⚠️ 215 GB free — workable, not generous |

**Nothing blocks starting the desktop-app bootstrap.** The ⚠️ items are decisions
to record (ADRs), not missing hard prerequisites.

---

## Findings

### OS / Shell
| Item | Value |
| ---- | ----- |
| OS | Windows 11 Pro, 10.0.26200 (build 26200), 64-bit |
| PowerShell | 5.1.26100.8875 — **Windows PowerShell only; no PowerShell 7 (`pwsh`)** |

### CPU / Memory
| Item | Value |
| ---- | ----- |
| CPU | AMD Ryzen 7 9800X3D — 8 cores / 16 threads, 4.7 GHz base |
| RAM | 32 GB total — 2 × 16 GB Kingston Fury (KF560C30-16, rated DDR5-6000) |
| RAM speed | Running at **4800 MT/s** (below the 6000 rating — EXPO/XMP likely off; perf note only) |

### GPU
| Item | Value |
| ---- | ----- |
| Primary GPU | **NVIDIA GeForce RTX 5080** |
| VRAM | 16303 MiB (~16 GB); ~14984 MiB free at audit |
| Driver | 610.88 (WDDM), KMD 610.88 |
| CUDA (driver/runtime) | UMD 13.3, `compute_cap` **12.0** (Blackwell, sm_120) |
| Secondary | AMD Radeon(TM) Graphics (Ryzen iGPU) — present, not for inference |
| Also listed | "Virtual Desktop Monitor" (Virtual Desktop streaming app virtual display) |
| CUDA Toolkit (`nvcc`) | **NOT installed**; `CUDA_PATH` unset |

Note: `Win32_VideoController.AdapterRAM` misreports (capped ~4 GB) — `nvidia-smi`
is the authoritative VRAM source and is used above.

### Disk
| Volume | Size | Free |
| ------ | ---- | ---- |
| C: | 1862 GB | **215 GB (~11.5%)** |

Single volume. Local model sets (LLM GGUF + image-gen + STT + TTS) can easily
consume 100+ GB; 215 GB is enough to start but model storage location / budget
should be planned.

### Toolchains (all on PATH)
| Tool | Version | Path |
| ---- | ------- | ---- |
| rustc | 1.98.0 (2026-08-18) | `C:\Users\Vincent\.cargo\bin` |
| cargo | 1.98.0 | `C:\Users\Vincent\.cargo\bin` |
| rustup | 1.29.0 | host `x86_64-pc-windows-msvc`; only target installed |
| rustc LLVM | 22.1.8 | — |
| node | v24.19.0 | `C:\Program Files\nodejs` |
| npm | 11.17.0 | — |
| corepack | 0.35.0 | present (can provision pnpm/yarn) |
| pnpm | **not installed** | — |
| python | 3.11.9 | `C:\Program Files\Python311` (real install ahead of WindowsApps alias on PATH) |
| pip | 24.0 | — |
| uv / virtualenv | **not installed** | — |
| git | 2.52.0.windows.1 | `C:\Program Files\Git` |
| cmake | 4.4.2 | `C:\Program Files\CMake\bin` (needed for llama.cpp source builds) |

### Native build tooling
| Item | Value |
| ---- | ----- |
| Visual Studio Build Tools 2022 | 17.14.37614.0 at `...\2022\BuildTools` |
| MSVC toolset | 14.44.35207 — `cl.exe` + `link.exe` present (Hostx64/x64) |
| Windows SDK | 10.0.26100.0 (Include + Lib) |
| **Rust link-chain test** | ✅ **`cargo new` + `cargo build` succeeded (exit 0), ran "Hello, world!"** — physically executed in scratch, then deleted |

### Tauri prerequisites
| Item | Value |
| ---- | ----- |
| WebView2 Runtime | ✅ 152.0.4191.62 (HKLM) |
| MSVC / Windows SDK | ✅ (above) |
| Rust | ✅ (above) |
| Node | ✅ (above) |
| Tauri CLI | not installed (expected — added per-project) |

### Git config (repo-relevant)
| Item | Value |
| ---- | ----- |
| core.autocrlf | `true` — source of the `LF will be replaced by CRLF` warnings on commit |
| core.eol | unset |

---

## Open decisions surfaced (for ADRs / architecture phase)

1. **CUDA llama.cpp:** build from source (needs CUDA Toolkit 13.x + `nvcc`, ~3 GB,
   Blackwell sm_120 requires recent llama.cpp) **vs.** ship prebuilt CUDA-enabled
   binaries. → `AI_PIPELINES.md` / ADR.
2. **Python worker environment:** bare `Program Files` Python vs. per-worker venv
   vs. `uv`-managed vs. bundled/embedded Python for distribution. → ADR.
3. **JS package manager:** npm (works now) vs. pnpm via corepack. → `DEVELOPMENT.md`.
4. **Model storage:** where models live, disk budget, and behavior when C: runs low
   (215 GB free today). → `ARCHITECTURE.md` / `PERFORMANCE.md`.
5. **Line endings:** add `.gitattributes` and consider `core.autocrlf=input` for a
   mixed Rust/TS repo. → `DEVELOPMENT.md`.
6. **Dev scripts target:** Windows PowerShell 5.1 only on this machine (no `pwsh`).
7. **RAM below rated speed** (4800 vs 6000) — enable EXPO in BIOS for a free perf
   gain; not a blocker.

## Not blocking

Rust, Node, Python, Git, MSVC build tools, Windows SDK, WebView2, and a working
Rust/MSVC link chain are all present and verified. The desktop-app bootstrap can
proceed without installing anything.
