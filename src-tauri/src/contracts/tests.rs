//! Round-trip + rejection sweep for every contract type (Phase 7 gate items
//! 2 & 3), plus the `TokenDelta` serialization perf probe (gate item 7).

use std::fmt::Debug;

use serde::de::DeserializeOwned;
use serde::Serialize;

use super::conversation::{
    Conversation, ConversationKind, GenerationHandle, GenerationMeta, GenerationState, Message,
    MessageContent, Role,
};
use super::generation::{GenerationEvent, GenerationRequest, SamplingParams, StopReason};
use super::ids::{
    AssetId, ConversationId, DownloadId, GeneratedImageId, ImageLoraId, ImagePresetId, MessageId,
    ModelId, ReservationId, TaskId, WorkerJobId,
};
use super::image::{
    GeneratedImageRow, ImageEvent, ImageLora, ImagePhase, ImagePreset, ImagePresetParams,
    ImageProgress, ImageRequest, LoraSelection,
};
use super::model::{
    Device, LifecycleStatus, ModelBackend, ModelCapabilities, ModelKind, ModelMetadata, ModelState,
    Quant, RegisteredModel, RegistryAvailability,
};
use super::resource::{
    GpuMemory, RamInfo, Reservation, ReservationState, ResourceKind, ResourceSnapshot,
};
use super::task::{CancelRequest, TaskKind, TaskState, TaskStatus};
use super::worker::{WorkerHello, WorkerKind, WorkerRequest, WorkerResponse, WorkerResult};
use super::WORKER_PROTOCOL_VERSION;
use crate::ipc::error::ErrorEnvelope;
use crate::ipc::AppError;

/// Serialize → deserialize → assert equal.
fn round_trip<T>(value: &T)
where
    T: Serialize + DeserializeOwned + PartialEq + Debug,
{
    let json = serde_json::to_string(value).expect("serialize");
    let back: T = serde_json::from_str(&json).unwrap_or_else(|e| panic!("deserialize {json}: {e}"));
    assert_eq!(*value, back, "round-trip changed the value (json: {json})");
}

fn task_id() -> TaskId {
    TaskId::from_trusted("task-1")
}
fn model_id() -> ModelId {
    ModelId::from_trusted("model-1")
}
fn conversation_id() -> ConversationId {
    ConversationId::from_trusted("conv-1")
}

// ---------------------------------------------------------------- ids

#[test]
fn ids_round_trip_as_bare_strings() {
    assert_eq!(serde_json::to_string(&task_id()).unwrap(), "\"task-1\"");
    round_trip(&task_id());
    round_trip(&model_id());
    round_trip(&conversation_id());
    round_trip(&MessageId::from_trusted("m-1"));
    round_trip(&ReservationId::from_trusted("r-1"));
    round_trip(&WorkerJobId::from_trusted("w-1"));
    round_trip(&DownloadId::from_trusted("d-1"));
    round_trip(&AssetId::from_trusted("a-1"));
}

#[test]
fn empty_id_is_rejected_on_construction_and_deserialization() {
    assert!(TaskId::try_from(String::new()).is_err());
    assert!(TaskId::try_from("   ".to_owned()).is_err());
    assert!("".parse::<ModelId>().is_err());
    assert!(serde_json::from_str::<TaskId>("\"\"").is_err());
    assert!(serde_json::from_str::<TaskId>("\"  \"").is_err());
}

// ---------------------------------------------------------------- errors

#[test]
fn every_app_error_kind_serializes_to_its_tag() {
    let cases = [
        (AppError::NotFound("x".into()), "NotFound"),
        (AppError::Validation("x".into()), "Validation"),
        (AppError::Conflict("x".into()), "Conflict"),
        (AppError::ResourceExhausted("x".into()), "ResourceExhausted"),
        (AppError::Timeout("x".into()), "Timeout"),
        (AppError::Cancelled, "Cancelled"),
        (
            AppError::BackendUnavailable("x".into()),
            "BackendUnavailable",
        ),
        (AppError::WorkerCrashed("x".into()), "WorkerCrashed"),
        (AppError::Internal, "Internal"),
    ];
    for (err, tag) in cases {
        let v: serde_json::Value = serde_json::to_value(&err).unwrap();
        assert_eq!(v["kind"], tag);
        round_trip(&err);
    }
}

#[test]
fn unknown_app_error_kind_is_rejected() {
    assert!(serde_json::from_str::<AppError>(r#"{"kind":"Meltdown","message":"x"}"#).is_err());
}

#[test]
fn error_envelope_carries_task_id() {
    let env = AppError::Timeout("slow".into()).with_task(task_id());
    assert_eq!(env.task_id, Some(task_id()));
    round_trip(&env);
    round_trip(&ErrorEnvelope {
        error: AppError::Internal,
        task_id: None,
    });
}

// ---------------------------------------------------------------- task

#[test]
fn task_contracts_round_trip() {
    for kind in [
        TaskKind::LlmGeneration,
        TaskKind::ImageGeneration,
        TaskKind::Stt,
        TaskKind::Tts,
        TaskKind::Embedding,
        TaskKind::ModelDownload,
        TaskKind::ModelLoad,
        TaskKind::ModelUnload,
    ] {
        round_trip(&kind);
    }
    for state in [
        TaskState::Queued,
        TaskState::Running,
        TaskState::Succeeded,
        TaskState::Failed,
        TaskState::Cancelled,
    ] {
        round_trip(&state);
    }
    assert!(TaskState::Succeeded.is_terminal());
    assert!(!TaskState::Running.is_terminal());

    let status = TaskStatus {
        id: task_id(),
        kind: TaskKind::LlmGeneration,
        state: TaskState::Running,
        progress: Some(0.5),
        detail: Some("halfway".into()),
    };
    status.validate().expect("valid");
    round_trip(&status);
    round_trip(&CancelRequest { task_id: task_id() });
}

#[test]
fn task_status_rejects_out_of_range_progress() {
    let bad = TaskStatus {
        id: task_id(),
        kind: TaskKind::Stt,
        state: TaskState::Running,
        progress: Some(1.5),
        detail: None,
    };
    assert!(matches!(bad.validate(), Err(AppError::Validation(_))));

    let nan = TaskStatus {
        progress: Some(f32::NAN),
        ..bad
    };
    assert!(nan.validate().is_err());
}

#[test]
fn unknown_task_state_variant_is_rejected() {
    assert!(serde_json::from_str::<TaskState>("\"Paused\"").is_err());
}

// ---------------------------------------------------------------- model

#[test]
fn model_contracts_round_trip() {
    for kind in [
        ModelKind::Llm,
        ModelKind::Stt,
        ModelKind::Tts,
        ModelKind::Image,
        ModelKind::Embedder,
    ] {
        round_trip(&kind);
    }
    for state in [
        ModelState::Unloaded,
        ModelState::Loading,
        ModelState::Loaded,
        ModelState::Busy,
        ModelState::Failed,
        ModelState::Unloading,
    ] {
        round_trip(&state);
    }
    let meta = ModelMetadata {
        id: model_id(),
        display_name: "Some Instruct 8B".into(),
        kind: ModelKind::Llm,
        backend: ModelBackend("runtime-a".into()),
        quant: Some(Quant("Q4_K_M".into())),
        capabilities: ModelCapabilities {
            streaming: true,
            context_tokens: Some(8192),
        },
        estimated_vram_mb: Some(7000),
    };
    round_trip(&meta);

    let minimal = ModelMetadata {
        quant: None,
        estimated_vram_mb: None,
        capabilities: ModelCapabilities {
            streaming: false,
            context_tokens: None,
        },
        ..meta.clone()
    };
    round_trip(&minimal);

    for a in [RegistryAvailability::Ready, RegistryAvailability::Missing] {
        round_trip(&a);
    }
    for d in [Device::Cuda, Device::Cpu] {
        round_trip(&d);
    }
    round_trip(&RegisteredModel {
        metadata: meta,
        path: "C:/models/x.gguf".into(),
        availability: RegistryAvailability::Ready,
        devices: vec![Device::Cuda, Device::Cpu],
    });

    round_trip(&LifecycleStatus {
        id: model_id(),
        state: ModelState::Busy,
        vram_mb: Some(7_100),
        error: None,
    });
    round_trip(&LifecycleStatus {
        id: model_id(),
        state: ModelState::Failed,
        vram_mb: None,
        error: Some("backend exited".into()),
    });
}

#[test]
fn unknown_registry_availability_is_rejected() {
    assert!(serde_json::from_str::<RegistryAvailability>("\"Downloading\"").is_err());
}

// ---------------------------------------------------------------- generation

#[test]
fn generation_contracts_round_trip() {
    let params = SamplingParams {
        temperature: Some(0.5),
        top_p: Some(0.5),
        top_k: Some(40),
        max_tokens: Some(512),
        stop: vec!["</s>".into()],
        seed: Some(42u32),
    };
    params.validate().expect("valid");
    round_trip(&params);
    round_trip(&GenerationRequest {
        task_id: task_id(),
        model: model_id(),
        prompt: "hello".into(),
        params,
    });

    for ev in [
        GenerationEvent::TokenDelta {
            index: 0,
            text: "Hel".into(),
        },
        GenerationEvent::Done {
            stop_reason: StopReason::EndOfText,
            tokens: 3,
        },
        GenerationEvent::Error {
            error: AppError::BackendUnavailable("server down".into()),
        },
        GenerationEvent::Cancelled,
    ] {
        round_trip(&ev);
    }
}

#[test]
fn generation_event_is_adjacently_tagged() {
    let v = serde_json::to_value(GenerationEvent::TokenDelta {
        index: 2,
        text: "x".into(),
    })
    .unwrap();
    assert_eq!(v["type"], "TokenDelta");
    assert_eq!(v["data"]["index"], 2);
}

#[test]
fn sampling_params_validate_rejects_bad_ranges() {
    let neg_temp = SamplingParams {
        temperature: Some(-0.1),
        ..SamplingParams::default_for_test()
    };
    assert!(neg_temp.validate().is_err());

    let big_top_p = SamplingParams {
        top_p: Some(1.5),
        ..SamplingParams::default_for_test()
    };
    assert!(big_top_p.validate().is_err());

    let zero_max = SamplingParams {
        max_tokens: Some(0),
        ..SamplingParams::default_for_test()
    };
    assert!(zero_max.validate().is_err());
}

impl SamplingParams {
    fn default_for_test() -> Self {
        Self {
            temperature: None,
            top_p: None,
            top_k: None,
            max_tokens: None,
            stop: Vec::new(),
            seed: None,
        }
    }
}

#[test]
fn unknown_generation_event_tag_is_rejected() {
    assert!(serde_json::from_str::<GenerationEvent>(r#"{"type":"Boom","data":null}"#).is_err());
}

// ---------------------------------------------------------------- conversation

#[test]
fn conversation_contracts_round_trip() {
    for role in [Role::System, Role::User, Role::Assistant] {
        round_trip(&role);
    }
    for content in [
        MessageContent::Text { text: "hi".into() },
        MessageContent::Audio {
            asset: AssetId::from_trusted("sha-audio"),
            transcript: Some("hello there".into()),
        },
        MessageContent::Image {
            asset: AssetId::from_trusted("sha-image"),
            caption: None,
        },
    ] {
        round_trip(&content);
    }

    let msg = Message {
        id: MessageId::from_trusted("m-1"),
        conversation_id: conversation_id(),
        role: Role::Assistant,
        content: MessageContent::Text {
            text: "generated".into(),
        },
        created_at: "2026-09-06T00:00:00Z".into(),
        generation: Some(GenerationMeta {
            model: model_id(),
            stop_reason: StopReason::MaxTokens,
            tokens: 128,
            duration_ms: 900,
        }),
    };
    round_trip(&msg);

    round_trip(&Conversation {
        id: conversation_id(),
        kind: ConversationKind::Character,
        title: Some("First chat".into()),
        persona_id: None,
        created_at: "2026-09-06T00:00:00Z".into(),
        updated_at: "2026-09-06T00:01:00Z".into(),
    });

    round_trip(&GenerationState { generating: None });
    round_trip(&GenerationState {
        generating: Some(GenerationHandle {
            task_id: task_id(),
            conversation_id: conversation_id(),
        }),
    });
}

#[test]
fn memory_contracts_round_trip() {
    use super::ids::MemoryId;
    use super::memory::{Memory, MemoryKind};

    for kind in [
        MemoryKind::Fact,
        MemoryKind::Preference,
        MemoryKind::Event,
        MemoryKind::Trait,
    ] {
        round_trip(&kind);
    }
    round_trip(&Memory {
        id: MemoryId::from_trusted("mem-1"),
        kind: MemoryKind::Fact,
        content: "keeps bees on the roof".into(),
        importance: 4,
        source_conversation_id: Some(conversation_id()),
        created_at: "2026-09-06T00:00:00Z".into(),
    });
    round_trip(&Memory {
        id: MemoryId::from_trusted("mem-2"),
        kind: MemoryKind::Preference,
        content: "no oat milk".into(),
        importance: 3,
        source_conversation_id: None,
        created_at: "2026-09-06T00:00:00Z".into(),
    });
}

#[test]
fn memory_unknown_kind_is_rejected() {
    use super::memory::MemoryKind;
    assert!(serde_json::from_str::<MemoryKind>("\"Grudge\"").is_err());
}

#[test]
fn message_missing_role_is_rejected() {
    let json = r#"{
        "id":"m-1","conversation_id":"c-1",
        "content":{"type":"Text","data":{"text":"x"}},
        "created_at":"2026-09-06T00:00:00Z"
    }"#;
    assert!(serde_json::from_str::<Message>(json).is_err());
}

// ---------------------------------------------------------------- resource

#[test]
fn resource_contracts_round_trip() {
    for kind in [ResourceKind::Gpu, ResourceKind::SystemRam] {
        round_trip(&kind);
    }
    for state in [
        ReservationState::Requested,
        ReservationState::Held,
        ReservationState::Released,
        ReservationState::Denied,
    ] {
        round_trip(&state);
    }
    round_trip(&Reservation {
        id: ReservationId::from_trusted("r-1"),
        kind: ResourceKind::Gpu,
        amount_mb: 8000,
        task_id: Some(task_id()),
        state: ReservationState::Held,
    });

    round_trip(&ResourceSnapshot {
        gpu: Some(GpuMemory {
            total_mb: 16_384,
            used_mb: 2_048,
            free_mb: 14_336,
        }),
        ram: Some(RamInfo {
            total_mb: 32_768,
            available_mb: 20_000,
        }),
        reserved_gpu_mb: 7_000,
        reserved_ram_mb: 0,
    });
    round_trip(&ResourceSnapshot {
        gpu: None,
        ram: None,
        reserved_gpu_mb: 0,
        reserved_ram_mb: 0,
    });
}

// ---------------------------------------------------------------- worker

#[test]
fn worker_contracts_round_trip() {
    for kind in [WorkerKind::Stt, WorkerKind::Tts, WorkerKind::Embed] {
        round_trip(&kind);
    }
    round_trip(&WorkerHello {
        protocol_version: WORKER_PROTOCOL_VERSION,
        worker: WorkerKind::Stt,
    });
    round_trip(&WorkerRequest {
        id: WorkerJobId::from_trusted("w-1"),
        kind: WorkerKind::Tts,
        payload: serde_json::json!({ "text": "speak this" }),
    });
    for result in [
        WorkerResult::Ok {
            data: serde_json::json!({ "transcript": "hi" }),
        },
        WorkerResult::Err {
            error: AppError::WorkerCrashed("segfault".into()),
        },
        WorkerResult::Progress {
            progress: 0.25,
            detail: Some("decoding".into()),
        },
    ] {
        round_trip(&WorkerResponse {
            id: WorkerJobId::from_trusted("w-1"),
            result,
        });
    }
}

#[test]
fn worker_hello_version_mismatch_is_detectable() {
    let hello: WorkerHello =
        serde_json::from_str(r#"{"protocol_version":99,"worker":"Stt"}"#).unwrap();
    assert_ne!(hello.protocol_version, WORKER_PROTOCOL_VERSION);
}

#[test]
fn unknown_worker_result_status_is_rejected() {
    assert!(serde_json::from_str::<WorkerResult>(r#"{"status":"Exploded","body":null}"#).is_err());
}

// ---------------------------------------------------------------- image (P22)

fn valid_image_request() -> ImageRequest {
    ImageRequest {
        prompt: "a photo of a lighthouse at dawn".to_owned(),
        negative: None,
        width: 1024,
        height: 1024,
        steps: None,
        guidance: None,
        seed: Some(42),
        batch_count: 1,
        loras: vec![],
    }
}

#[test]
fn image_contracts_round_trip() {
    round_trip(&ImageLoraId::from_trusted("lora-1"));
    round_trip(&ImagePresetId::from_trusted("preset-1"));
    round_trip(&GeneratedImageId::from_trusted("img-1"));
    round_trip(&valid_image_request());
    round_trip(&ImageRequest {
        negative: Some("blurry".to_owned()),
        loras: vec![LoraSelection {
            id: ImageLoraId::from_trusted("lora-1"),
            weight: 0.85,
        }],
        ..valid_image_request()
    });
    let progress = ImageProgress {
        phase: ImagePhase::Generating,
        step: 3,
        total_steps: 8,
        image_index: 0,
        batch_count: 2,
    };
    round_trip(&progress);
    let row = GeneratedImageRow {
        id: GeneratedImageId::from_trusted("img-1"),
        asset: AssetId::from_trusted("a".repeat(64)),
        prompt: "a lighthouse".to_owned(),
        width: 1024,
        height: 1024,
        seed: 42,
        lora: Some("Realism".to_owned()),
        created_at: "2026-09-06T00:00:00Z".to_owned(),
    };
    round_trip(&row);
    for ev in [
        ImageEvent::Progress(progress),
        ImageEvent::Done { images: vec![row] },
        ImageEvent::Error {
            error: AppError::WorkerCrashed("sidecar exited".to_owned()),
        },
        ImageEvent::Cancelled,
    ] {
        round_trip(&ev);
    }
    round_trip(&ImageLora {
        id: ImageLoraId::from_trusted("lora-1"),
        display_name: "Realism".to_owned(),
        base_compat: "krea2".to_owned(),
        default_weight: 0.9,
        tags: vec!["realism".to_owned()],
    });
    round_trip(&ImagePreset {
        id: ImagePresetId::from_trusted("preset-1"),
        name: "Portrait".to_owned(),
        params: ImagePresetParams {
            width: 928,
            height: 1232,
            steps: 8,
            guidance: 0.0,
        },
    });
}

#[test]
fn image_event_is_adjacently_tagged() {
    let json = serde_json::to_string(&ImageEvent::Cancelled).unwrap();
    assert_eq!(json, r#"{"type":"Cancelled"}"#);
}

#[test]
fn image_request_validate_accepts_a_good_request() {
    valid_image_request().validate().expect("valid");
}

#[test]
fn image_request_validate_rejects_bad_input() {
    let lora = |weight: f32| LoraSelection {
        id: ImageLoraId::from_trusted("a"),
        weight,
    };
    let cases: Vec<(&str, ImageRequest)> = vec![
        (
            "empty prompt",
            ImageRequest {
                prompt: "   ".to_owned(),
                ..valid_image_request()
            },
        ),
        (
            "non-multiple-of-16 width",
            ImageRequest {
                width: 1020,
                ..valid_image_request()
            },
        ),
        (
            "width too small",
            ImageRequest {
                width: 256,
                ..valid_image_request()
            },
        ),
        (
            "height too large",
            ImageRequest {
                height: 2048,
                ..valid_image_request()
            },
        ),
        (
            "steps 0",
            ImageRequest {
                steps: Some(0),
                ..valid_image_request()
            },
        ),
        (
            "steps too high",
            ImageRequest {
                steps: Some(200),
                ..valid_image_request()
            },
        ),
        (
            "guidance negative",
            ImageRequest {
                guidance: Some(-1.0),
                ..valid_image_request()
            },
        ),
        (
            "batch 0",
            ImageRequest {
                batch_count: 0,
                ..valid_image_request()
            },
        ),
        (
            "batch too high",
            ImageRequest {
                batch_count: 99,
                ..valid_image_request()
            },
        ),
        (
            "two loras",
            ImageRequest {
                loras: vec![lora(0.5), lora(0.5)],
                ..valid_image_request()
            },
        ),
        (
            "lora weight out of range",
            ImageRequest {
                loras: vec![lora(1.5)],
                ..valid_image_request()
            },
        ),
    ];
    for (label, req) in cases {
        assert!(
            matches!(req.validate(), Err(AppError::Validation(_))),
            "expected {label:?} to be rejected"
        );
    }
}

// ---------------------------------------------------------------- perf probe

/// Gate item 7: `TokenDelta` round-trip must be far cheaper than per-token LLM
/// latency. Prints ns/op; asserts a generous ceiling so it fails loudly if a
/// future change makes the hot streaming type expensive.
#[test]
fn token_delta_round_trip_perf() {
    let ev = GenerationEvent::TokenDelta {
        index: 12_345,
        text: " lorem".into(),
    };
    let iters = 10_000;
    let start = std::time::Instant::now();
    for _ in 0..iters {
        let json = serde_json::to_string(&ev).unwrap();
        let back: GenerationEvent = serde_json::from_str(&json).unwrap();
        std::hint::black_box(back);
    }
    let per_op = start.elapsed() / iters;
    println!(
        "TokenDelta serialize+deserialize: {} ns/op",
        per_op.as_nanos()
    );
    assert!(
        per_op < std::time::Duration::from_micros(50),
        "TokenDelta round-trip regressed to {per_op:?}/op"
    );
}
