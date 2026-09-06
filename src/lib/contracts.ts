/**
 * Single import surface for the application contracts generated from Rust by
 * `ts-rs` (`src-tauri/src/contracts/`, ADR-0002). Import contract types from
 * here rather than reaching into `../bindings/` file by file.
 *
 * These are the shapes that cross the typed IPC boundary (and, on the Rust
 * side, the worker boundary). Evolution rules: `docs/contracts.md`.
 */

// Identifiers (all `string` at runtime; distinct types for intent).
export type { TaskId } from '../bindings/TaskId';
export type { ModelId } from '../bindings/ModelId';
export type { ConversationId } from '../bindings/ConversationId';
export type { MessageId } from '../bindings/MessageId';
export type { ReservationId } from '../bindings/ReservationId';
export type { WorkerJobId } from '../bindings/WorkerJobId';
export type { DownloadId } from '../bindings/DownloadId';
export type { AssetId } from '../bindings/AssetId';

// Errors.
export type { AppError } from '../bindings/AppError';
export type { ErrorEnvelope } from '../bindings/ErrorEnvelope';

// Configuration (ADR-0016; not a wire contract, but the effective view crosses IPC).
export type { AppConfig } from '../bindings/AppConfig';
export type { ModelsConfig } from '../bindings/ModelsConfig';
export type { LoggingConfig } from '../bindings/LoggingConfig';
export type { ConfigKey } from '../bindings/ConfigKey';
export type { ConfigSet } from '../bindings/ConfigSet';
export type { ConfigKeyInfo } from '../bindings/ConfigKeyInfo';

// Tasks.
export type { TaskKind } from '../bindings/TaskKind';
export type { TaskState } from '../bindings/TaskState';
export type { TaskStatus } from '../bindings/TaskStatus';
export type { CancelRequest } from '../bindings/CancelRequest';

// Models.
export type { ModelKind } from '../bindings/ModelKind';
export type { ModelBackend } from '../bindings/ModelBackend';
export type { Quant } from '../bindings/Quant';
export type { ModelCapabilities } from '../bindings/ModelCapabilities';
export type { ModelMetadata } from '../bindings/ModelMetadata';
export type { ModelState } from '../bindings/ModelState';
export type { RegisteredModel } from '../bindings/RegisteredModel';
export type { RegistryAvailability } from '../bindings/RegistryAvailability';
export type { Device } from '../bindings/Device';

// Acquisition (HF picker + downloads, Phase 12).
export type { HfModelSummary } from '../bindings/HfModelSummary';
export type { HfGgufFile } from '../bindings/HfGgufFile';
export type { DownloadState } from '../bindings/DownloadState';
export type { DownloadInfo } from '../bindings/DownloadInfo';
export type { DownloadProgress } from '../bindings/DownloadProgress';
export type { DownloadRequest } from '../bindings/DownloadRequest';
export type { FixedModelKind } from '../bindings/FixedModelKind';

// Generation + streaming.
export type { SamplingParams } from '../bindings/SamplingParams';
export type { GenerationRequest } from '../bindings/GenerationRequest';
export type { StopReason } from '../bindings/StopReason';
export type { GenerationEvent } from '../bindings/GenerationEvent';

// Conversations + messages.
export type { Role } from '../bindings/Role';
export type { MessageContent } from '../bindings/MessageContent';
export type { GenerationMeta } from '../bindings/GenerationMeta';
export type { Message } from '../bindings/Message';
export type { ConversationKind } from '../bindings/ConversationKind';
export type { Conversation } from '../bindings/Conversation';

// Resource reservations.
export type { ResourceKind } from '../bindings/ResourceKind';
export type { ReservationState } from '../bindings/ReservationState';
export type { Reservation } from '../bindings/Reservation';

// Worker protocol (Rust ↔ Python; surfaced here for completeness).
export type { WorkerKind } from '../bindings/WorkerKind';
export type { WorkerHello } from '../bindings/WorkerHello';
export type { WorkerRequest } from '../bindings/WorkerRequest';
export type { WorkerResult } from '../bindings/WorkerResult';
export type { WorkerResponse } from '../bindings/WorkerResponse';
