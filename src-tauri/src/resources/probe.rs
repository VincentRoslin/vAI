//! Hardware measurement behind a trait.
//!
//! [`HardwareProbe`] reads **whole-GPU** VRAM and system RAM. Per-process VRAM
//! attribution is confirmed unavailable on this driver (ADR-0007,
//! `docs/verification/02_phase3_probes.md`) — the resource manager pairs these
//! whole-device readings with its own reservation ledger.
//!
//! Two implementations:
//! - [`NvmlProbe`] — `nvml-wrapper` for the GPU, `sysinfo` for RAM (the default).
//! - [`MockProbe`] — settable values, drives every test without real hardware.

use std::sync::Mutex;
use std::time::Duration;

use crate::contracts::resource::{GpuMemory, RamInfo};
use crate::ipc::AppError;

/// Why a probe could not produce a reading.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// The measurement backend is not available (no NVIDIA driver, NVML init
    /// failed, GPU index absent).
    #[error("hardware probe unavailable: {0}")]
    Unavailable(String),
    /// The backend is present but a specific query failed.
    #[error("hardware probe failed: {0}")]
    Query(String),
}

impl From<ProbeError> for AppError {
    fn from(err: ProbeError) -> Self {
        Self::BackendUnavailable(err.to_string())
    }
}

/// Reads whole-device GPU and system-RAM memory. Cheap enough to call on a
/// ~1.5 s poll; never call it on the `request` path (ADR-0007).
pub trait HardwareProbe: Send + Sync {
    /// Current whole-GPU memory.
    ///
    /// # Errors
    /// [`ProbeError::Unavailable`] when there is no usable GPU probe.
    fn gpu(&self) -> Result<GpuMemory, ProbeError>;

    /// Current system RAM.
    ///
    /// # Errors
    /// [`ProbeError`] when the reading cannot be taken.
    fn ram(&self) -> Result<RamInfo, ProbeError>;
}

// ---------------------------------------------------------------- real

/// The production probe: `nvml-wrapper` (GPU 0) + `sysinfo` (RAM).
///
/// NVML is initialised lazily and, on failure, [`NvmlProbe::gpu`] returns
/// [`ProbeError::Unavailable`] forever — the app still runs, but loads that need
/// VRAM accounting are refused with a clear message.
pub struct NvmlProbe {
    nvml: Result<nvml_wrapper::Nvml, String>,
    system: Mutex<sysinfo::System>,
}

impl NvmlProbe {
    /// Initialise NVML and `sysinfo`. Never fails — a dead NVML is remembered
    /// and surfaced per-call.
    #[must_use]
    pub fn new() -> Self {
        let nvml = nvml_wrapper::Nvml::init().map_err(|e| e.to_string());
        if let Err(err) = &nvml {
            tracing::warn!(%err, "NVML init failed — GPU VRAM accounting disabled");
        }
        Self {
            nvml,
            system: Mutex::new(sysinfo::System::new()),
        }
    }
}

impl Default for NvmlProbe {
    fn default() -> Self {
        Self::new()
    }
}

const BYTES_PER_MB: u64 = 1024 * 1024;

impl HardwareProbe for NvmlProbe {
    fn gpu(&self) -> Result<GpuMemory, ProbeError> {
        let nvml = self
            .nvml
            .as_ref()
            .map_err(|e| ProbeError::Unavailable(e.clone()))?;
        let device = nvml
            .device_by_index(0)
            .map_err(|e| ProbeError::Unavailable(format!("GPU 0: {e}")))?;
        let mem = device
            .memory_info()
            .map_err(|e| ProbeError::Query(e.to_string()))?;
        Ok(GpuMemory {
            total_mb: mem.total / BYTES_PER_MB,
            used_mb: mem.used / BYTES_PER_MB,
            free_mb: mem.free / BYTES_PER_MB,
        })
    }

    fn ram(&self) -> Result<RamInfo, ProbeError> {
        let mut system = self
            .system
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        system.refresh_memory();
        Ok(RamInfo {
            total_mb: system.total_memory() / BYTES_PER_MB,
            available_mb: system.available_memory() / BYTES_PER_MB,
        })
    }
}

// ---------------------------------------------------------------- mock

/// A settable probe for tests. `gpu` / `ram` return whatever was last set;
/// [`MockProbe::fail_gpu`] simulates a machine with no usable GPU probe, and
/// [`MockProbe::set_delay`] simulates a slow driver call.
#[derive(Debug)]
pub struct MockProbe {
    state: Mutex<MockState>,
}

#[derive(Debug, Clone)]
struct MockState {
    gpu: Option<GpuMemory>,
    ram: Option<RamInfo>,
    delay: Duration,
}

impl MockProbe {
    /// A 16 GB GPU with 2 GB already used, 32 GB RAM with 24 GB available.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(MockState {
                gpu: Some(GpuMemory {
                    total_mb: 16_384,
                    used_mb: 2_048,
                    free_mb: 14_336,
                }),
                ram: Some(RamInfo {
                    total_mb: 32_768,
                    available_mb: 24_576,
                }),
                delay: Duration::ZERO,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MockState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Set the whole-GPU reading. `used` and `free` are derived to sum to `total`.
    pub fn set_gpu(&self, total_mb: u64, used_mb: u64) {
        self.lock().gpu = Some(GpuMemory {
            total_mb,
            used_mb,
            free_mb: total_mb.saturating_sub(used_mb),
        });
    }

    /// Raise `used` by `delta_mb` (an external process grabbing VRAM).
    pub fn add_gpu_used(&self, delta_mb: u64) {
        let mut state = self.lock();
        if let Some(gpu) = state.gpu {
            let used = gpu.used_mb.saturating_add(delta_mb).min(gpu.total_mb);
            state.gpu = Some(GpuMemory {
                total_mb: gpu.total_mb,
                used_mb: used,
                free_mb: gpu.total_mb - used,
            });
        }
    }

    /// Set the system-RAM reading.
    pub fn set_ram(&self, total_mb: u64, available_mb: u64) {
        self.lock().ram = Some(RamInfo {
            total_mb,
            available_mb,
        });
    }

    /// Make [`HardwareProbe::gpu`] report an unavailable probe.
    pub fn fail_gpu(&self) {
        self.lock().gpu = None;
    }

    /// Make every probe call block for `delay` first.
    pub fn set_delay(&self, delay: Duration) {
        self.lock().delay = delay;
    }
}

impl Default for MockProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl HardwareProbe for MockProbe {
    fn gpu(&self) -> Result<GpuMemory, ProbeError> {
        let (gpu, delay) = {
            let state = self.lock();
            (state.gpu, state.delay)
        };
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        gpu.ok_or_else(|| ProbeError::Unavailable("mock GPU probe disabled".to_owned()))
    }

    fn ram(&self) -> Result<RamInfo, ProbeError> {
        let (ram, delay) = {
            let state = self.lock();
            (state.ram, state.delay)
        };
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
        ram.ok_or_else(|| ProbeError::Unavailable("mock RAM probe disabled".to_owned()))
    }
}
