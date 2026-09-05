# 02 — Phase 3 runnable probes

**Date:** 2026-09-05
**Method:** throwaway probes in the scratch dir (deleted after). No application
code. Results feed ADR-0007 and ADR-0008.

---

## Probe 1 — NVML per-process VRAM on this driver

**Question:** does `nvml-wrapper` report per-process VRAM on driver 610.88 / WDDM
/ Blackwell sm_120? (The env audit showed `N/A` in `nvidia-smi`.)

**Method:** `nvml-wrapper` 0.10 Rust bin — `memory_info()`,
`running_compute_processes()`, `running_graphics_processes()`,
`process_utilization_stats()`.

**Result — EXECUTED:**
```
NVML version: 13.610.88   driver: 610.88
GPU 0: NVIDIA GeForce RTX 5080   compute cap 12.0
memory_info: total=16303 MiB, used=1490 MiB, free=14812 MiB      ✅ works
running_compute_processes:  18 found, used_gpu_memory = Unavailable (ALL)   ❌
running_graphics_processes: 18 found, used_gpu_memory = Unavailable (ALL)   ❌
process_utilization_stats: 2 entries (utilization %, not memory)
```

**Conclusion (CONFIRMED):** whole-GPU `memory_info` (total/used/free) works and is
accurate. **Per-process VRAM attribution does NOT work** on this driver + WDDM —
every process returns `Unavailable`.

**Impact on ADR-0007:** the "whole-GPU accounting + our own reservation ledger"
path is **the** path, not a fallback. The resource manager:
- reads `free`/`used` from NVML for the ground truth,
- tracks what *we* loaded in the ledger,
- cannot identify which external process holds VRAM — only observe the shortfall
  and reconcile.

This is workable (the ledger + trusting measured `free` was already the design) —
now confirmed rather than assumed.

---

## Probe 2 — GGUF header parsing from a range request

**Question:** can the model picker read quant / context / size from a GGUF
**header only** (HTTP range request), without downloading the whole file?

**Method:** Python — `Range: bytes=0-2097151` (2 MiB) against
`bartowski/Qwen2.5-0.5B-Instruct-GGUF/.../Q4_K_M.gguf` (a HF `resolve` URL);
hand-parse the GGUF v3 header.

**Result — EXECUTED:**
```
HTTP 206 (partial content)  ✅ range requests work on HF resolve URLs
magic "GGUF" OK   version=3   tensor_count=290   metadata_kv_count=38
parsed 25/38 KV before the 2 MiB window ended
extracted:
  general.architecture       = qwen2
  general.file_type          = 15  (Q4_K_M)         ← the quant
  qwen2.context_length       = 32768
  qwen2.block_count          = 24
  qwen2.embedding_length     = 896
  qwen2.attention.head_count = 14
  qwen2.attention.head_count_kv = 2
```

**Conclusion (CONFIRMED):** header-only parsing works. Every field the picker +
the VRAM estimator need (architecture, quant, context length, layer/head counts
for KV-cache sizing) appears **before** the large tokenizer arrays. The 2 MiB
window ran out mid-KV only because of the tokenizer token list near the end —
**bump the range to ~8 MiB**, or stop once the needed keys are seen, or `HEAD`
first and fetch the whole file if it's small.

**Impact on ADR-0008:** confirmed. Use an ~8 MiB range (or adaptive), parse until
the required keys are present. HF `resolve` URLs support HTTP 206 → resumable
downloads (FR-71) also confirmed working.

---

## Probe 3 — WDDM hang risk (`llama-server` sustained CUDA) — PARTIAL

**Question:** the reported Blackwell WDDM hang (5090 / PRO 6000 under GPU
paravirtualization) — does it affect this 5080?

**Method (PowerShell, host inspection only):**
```
HypervisorPresent : True
Hyper-V feature   : Enabled
Msvm_PartitionableGpu : 2 entries (AMD iGPU + NVIDIA 5080) — "GPUPARAV" devices
GPU driver model  : WDDM
```

**Conclusion — INCONCLUSIVE, carried to Phase 15:**
- Hyper-V is **enabled on the host** (common on Win 11 — VBS/HVCI, WSL2, Docker,
  Sandbox all enable it). The `GPUPARAV` devices exist because Hyper-V is present,
  **not** because a VM is using the GPU — no VM is configured.
- Host (root-partition) CUDA on WDDM with Hyper-V enabled is the config millions
  of machines run inference on without issue. But it *is* nominally the condition
  the bug report described, and it **cannot be conclusively cleared without a
  sustained real CUDA compute workload** (llama.cpp / torch), which doesn't exist
  until Phase 15.
- **Phase 15 verification item:** on the first sustained `llama-server` generation,
  watch for a WDDM TDR / hang. If it occurs, mitigations in order:
  1. disable VBS / memory integrity (Core Isolation) — often enough;
  2. disable the Hyper-V platform feature if the user doesn't need WSL2/Docker;
  3. fall back to `llama-cpp-2` FFI on a dedicated thread (ADR-0003 fallback).
- Recorded as a Phase 4 risk and a Phase 15 gate check.

---

## Phase 3 exit status

| Probe | Status |
| ----- | ------ |
| NVML per-process VRAM | ✅ done — per-process unavailable; ADR-0007 confirmed |
| GGUF header parse | ✅ done — works; ADR-0008 confirmed (use ~8 MiB range) |
| WDDM hang | ⚠ inconclusive from host inspection — **Phase 15 watch item** + Phase 4 risk |

Remaining before the Phase 3 gate: none that block. Proceed to Phase 4
(adversarial review) → Phase 5 (ratify ADRs, freeze).
