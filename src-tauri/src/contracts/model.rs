//! Model metadata + model-state contracts.
//!
//! Model *names* are runtime data (`display_name`, `backend`), never string
//! literals in logic — the Phase 7 gate greps for that.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::contracts::ids::ModelId;

/// The role a model plays in a pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ModelKind {
    /// A large language model (chat / instruct).
    Llm,
    /// Speech-to-text.
    Stt,
    /// Text-to-speech.
    Tts,
    /// Text-to-image diffusion.
    Image,
    /// Produces embedding vectors.
    Embedder,
}

/// A transparent string newtype: bare JSON string on the wire, `string` in TS,
/// no `#[serde(...)]` attribute for `ts-rs` to trip over.
macro_rules! str_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, TS)]
        #[ts(export, export_to = "../../src/bindings/", type = "string")]
        pub struct $name(pub String);

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                Ok(Self(String::deserialize(d)?))
            }
        }
    };
}

str_newtype!(
    /// Which runtime executes a model, as an opaque identifier. Opaque on
    /// purpose: logic switches on [`ModelKind`], not on the backend string.
    ModelBackend
);
str_newtype!(
    /// Quantization label as reported by the model file (e.g. a `"Q4_K_M"`- or
    /// `"NF4"`-style tag). Absent for unquantized weights.
    Quant
);

/// What a model can do. Minimal for Phase 7; extended additively as backends
/// land.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ModelCapabilities {
    /// Whether the model can stream output incrementally.
    pub streaming: bool,
    /// Maximum context length in tokens, when it applies (LLM/embedder).
    pub context_tokens: Option<u32>,
}

/// Static description of a registered model. Persistence + `ModelId` stability
/// are Phase 11; this is only the shared shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct ModelMetadata {
    /// Stable identifier.
    pub id: ModelId,
    /// Human-facing name (from the model card / file).
    pub display_name: String,
    /// Pipeline role.
    pub kind: ModelKind,
    /// Executing runtime.
    pub backend: ModelBackend,
    /// Quantization, if any.
    pub quant: Option<Quant>,
    /// Declared capabilities.
    pub capabilities: ModelCapabilities,
    /// Rough VRAM footprint in MB when loaded, when known (Phase 13 refines it).
    pub estimated_vram_mb: Option<u32>,
}

/// Runtime state of a model in the lifecycle manager (Phase 14). Terminal only
/// in the sense that `Failed` needs an explicit recovery transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ModelState {
    /// Not in memory.
    Unloaded,
    /// Being brought into memory.
    Loading,
    /// Ready to serve requests.
    Loaded,
    /// Loaded **and** currently serving a generation — cannot be unloaded until
    /// it returns to `Loaded` (Phase 14).
    Busy,
    /// A load or a running backend failed; needs recovery.
    Failed,
    /// Being released from memory.
    Unloading,
}

/// Whether a registered model's file is actually present on disk. Computed at
/// read time (Phase 11) — **not** the same as [`ModelState`], which is runtime
/// lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum RegistryAvailability {
    /// The file exists where the registry says it is.
    Ready,
    /// The registry row is intact but the file is gone.
    Missing,
}

/// One row of the lifecycle manager's runtime view (Phase 14): a model it is
/// tracking, its [`ModelState`], and — once loaded — its measured VRAM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct LifecycleStatus {
    /// The model.
    pub id: ModelId,
    /// Its runtime state.
    pub state: ModelState,
    /// Measured VRAM footprint in MB, when loaded and the backend reports it.
    pub vram_mb: Option<u32>,
    /// The last error string, when `state` is `Failed`.
    pub error: Option<String>,
}

/// A compute device a model can run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub enum Device {
    /// An NVIDIA CUDA GPU.
    Cuda,
    /// The CPU.
    Cpu,
}

/// A model registry row as the frontend sees it: its [`ModelMetadata`] plus the
/// on-disk facts (Phase 11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct RegisteredModel {
    /// Static description.
    pub metadata: ModelMetadata,
    /// Absolute path to the model file (confined to the configured model dir).
    pub path: String,
    /// Whether the file is present right now.
    pub availability: RegistryAvailability,
    /// Devices this model can run on.
    pub devices: Vec<Device>,
}
