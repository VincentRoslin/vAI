//! Opaque identifier newtypes.
//!
//! Each ID is an opaque, non-empty string on the wire (`#[ts(type = "string")]`).
//! `Serialize` / `Deserialize` are hand-written so the value is a bare JSON
//! string and deserialization is **validated** (empty / whitespace-only is
//! rejected) — and so `ts-rs` sees no `#[serde(...)]` attribute it cannot parse.
//!
//! ID *generation* policy (UUID vs ULID vs DB-rowid-backed) is **not** decided
//! here — it is deferred to Phase 9, where IDs are minted. These types only
//! carry and validate an already-produced value.

use std::fmt;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

use crate::ipc::AppError;

macro_rules! id_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, TS)]
        #[ts(export, export_to = "../../src/bindings/", type = "string")]
        pub struct $name(String);

        impl $name {
            /// Wrap a value that is already known to be valid (e.g. read back
            /// from the database). Boundary input must go through [`TryFrom`].
            #[must_use]
            pub fn from_trusted(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Borrow the underlying string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = AppError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                if value.trim().is_empty() {
                    return Err(AppError::Validation(
                        concat!(stringify!($name), " must not be empty").to_owned(),
                    ));
                }
                Ok(Self(value))
            }
        }

        impl FromStr for $name {
            type Err = AppError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::try_from(s.to_owned())
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                Self::try_from(raw).map_err(D::Error::custom)
            }
        }
    };
}

id_newtype!(
    /// Identifies one asynchronous task: an LLM generation, an image generation,
    /// an STT/TTS/embedding job, or a model download / load / unload.
    TaskId
);
id_newtype!(
    /// Identifies a registered model. Stable across restarts and path changes
    /// (Phase 11).
    ModelId
);
id_newtype!(
    /// Identifies a conversation (Persona tab or Character tab).
    ConversationId
);
id_newtype!(
    /// Identifies a Persona — structured behaviour data for a Tab 1
    /// conversation (Phase 20).
    PersonaId
);
id_newtype!(
    /// Identifies one message within a conversation.
    MessageId
);
id_newtype!(
    /// Identifies one stored memory (Phase 21).
    MemoryId
);
id_newtype!(
    /// Identifies a resource reservation held against the GPU / system RAM
    /// ledger (Phase 13).
    ReservationId
);
id_newtype!(
    /// Identifies one unit of work handed to a stateless worker over stdio.
    WorkerJobId
);
id_newtype!(
    /// Identifies an in-progress or completed model download (Phase 12).
    DownloadId
);
id_newtype!(
    /// Content address of a stored blob: the lowercase hex SHA-256 of the bytes
    /// (`blobs/<sha256[0:2]>/<sha256>`).
    AssetId
);
id_newtype!(
    /// Identifies a realism LoRA in the image-model LoRA registry (Phase 22,
    /// ADR-0006).
    ImageLoraId
);
id_newtype!(
    /// Identifies a saved image-generation parameter preset (Phase 22).
    ImagePresetId
);
id_newtype!(
    /// Identifies one generated image and its provenance row (Phase 22).
    GeneratedImageId
);
id_newtype!(
    /// Identifies an imported cloned TTS voice (Phase 19 follow-up).
    VoiceId
);
