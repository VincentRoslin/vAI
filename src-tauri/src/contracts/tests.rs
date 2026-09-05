//! Round-trip + rejection sweep for every contract type (Phase 7 gate items
//! 2 & 3), plus the `TokenDelta` serialization perf probe (gate item 7).

use std::fmt::Debug;

use serde::de::DeserializeOwned;
use serde::Serialize;

use super::conversation::{
    Conversation, ConversationKind, GenerationMeta, Message, MessageContent, Role,
};
use super::generation::{GenerationEvent, GenerationRequest, SamplingParams, StopReason};
use super::ids::{
    AssetId, ConversationId, DownloadId, MessageId, ModelId, ReservationId, TaskId, WorkerJobId,
};
use super::model::{
    Device, ModelBackend, ModelCapabilities, ModelKind, ModelMetadata, ModelState, Quant,
    RegisteredModel, RegistryAvailability,
};
use super::resource::{Reservation, ReservationState, ResourceKind};
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
        created_at: "2026-09-06T00:00:00Z".into(),
        updated_at: "2026-09-06T00:01:00Z".into(),
    });
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
