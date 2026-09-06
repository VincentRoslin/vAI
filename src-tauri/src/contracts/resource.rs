//! Resource-reservation contracts.
//!
//! The resource manager (Phase 13) accounts for the GPU and system RAM with a
//! reservation ledger. This is the shared shape of a ledger entry.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::contracts::ids::{ReservationId, TaskId};

/// Which pool a reservation draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ResourceKind {
    /// GPU VRAM.
    Gpu,
    /// System RAM.
    SystemRam,
}

/// Lifecycle of a reservation. `Released` and `Denied` are terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ReservationState {
    /// Asked for, not yet granted.
    Requested,
    /// Granted and currently counted against the pool.
    Held,
    /// Given back to the pool.
    Released,
    /// Refused — the pool could not satisfy it.
    Denied,
}

/// One entry in the resource ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct Reservation {
    /// Stable identifier.
    pub id: ReservationId,
    /// Which pool.
    pub kind: ResourceKind,
    /// Amount reserved, in MB.
    pub amount_mb: u32,
    /// The task this reservation serves, when it is tied to one.
    pub task_id: Option<TaskId>,
    /// Current state.
    pub state: ReservationState,
}

/// A measured GPU memory reading (whole-GPU — per-process attribution is
/// unavailable on this driver, ADR-0007). All values in MB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct GpuMemory {
    /// Total VRAM on the device.
    pub total_mb: u64,
    /// VRAM in use across the whole system.
    pub used_mb: u64,
    /// VRAM currently free.
    pub free_mb: u64,
}

/// A measured system-RAM reading. All values in MB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct RamInfo {
    /// Total physical RAM.
    pub total_mb: u64,
    /// RAM available to new allocations.
    pub available_mb: u64,
}

/// The resource manager's current view: last measurement + what the ledger
/// holds. `gpu` / `ram` are `None` when the hardware probe is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ResourceSnapshot {
    /// Last GPU measurement, if the probe works.
    pub gpu: Option<GpuMemory>,
    /// Last system-RAM measurement, if the probe works.
    pub ram: Option<RamInfo>,
    /// Sum of outstanding GPU reservations (committed where known, else estimate).
    pub reserved_gpu_mb: u32,
    /// Sum of outstanding system-RAM reservations.
    pub reserved_ram_mb: u32,
}
