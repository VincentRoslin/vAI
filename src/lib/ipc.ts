/**
 * Typed wrappers over Tauri IPC (`docs/decisions/0002-ipc-design.md`).
 *
 * The frontend never calls `invoke` directly — every command has a thin typed
 * wrapper here, using the DTO/error types generated from Rust by `ts-rs` into
 * `src/bindings/`. Rejected commands are normalized to a typed `AppError`.
 */
import { Channel, invoke } from '@tauri-apps/api/core';

import type { AppConfig } from '../bindings/AppConfig';
import type { AppError } from '../bindings/AppError';
import type { AppReady } from '../bindings/AppReady';
import type { ConfigKeyInfo } from '../bindings/ConfigKeyInfo';
import type { ConfigSet } from '../bindings/ConfigSet';
import type { DownloadInfo } from '../bindings/DownloadInfo';
import type { DownloadProgress } from '../bindings/DownloadProgress';
import type { DownloadRequest } from '../bindings/DownloadRequest';
import type { FixedModelKind } from '../bindings/FixedModelKind';
import type { FrontendLog } from '../bindings/FrontendLog';
import type { HfGgufFile } from '../bindings/HfGgufFile';
import type { HfModelSummary } from '../bindings/HfModelSummary';
import type { Pong } from '../bindings/Pong';
import type { Conversation } from '../bindings/Conversation';
import type { GenerationEvent } from '../bindings/GenerationEvent';
import type { GenerationState } from '../bindings/GenerationState';
import type { LifecycleStatus } from '../bindings/LifecycleStatus';
import type { Message } from '../bindings/Message';
import type { RegisteredModel } from '../bindings/RegisteredModel';
import type { ResourceSnapshot } from '../bindings/ResourceSnapshot';
import type { InputDevice } from '../bindings/InputDevice';
import type { VoiceState } from '../bindings/VoiceState';

export type { AppConfig, AppError, AppReady, ConfigKeyInfo, ConfigSet, FrontendLog, Pong };

/** Narrow an unknown `invoke` rejection to our `AppError` shape. */
export function toAppError(err: unknown): AppError {
  if (err && typeof err === 'object' && 'kind' in err) {
    return err as AppError;
  }
  return { kind: 'Internal' };
}

/** Startup handshake — confirms the core is up and returns its version. */
export async function appReady(): Promise<AppReady> {
  try {
    return await invoke<AppReady>('app_ready');
  } catch (err) {
    throw toAppError(err);
  }
}

/** Round-trip no-op used to verify the IPC path. */
export async function appPing(nonce: string): Promise<Pong> {
  try {
    return await invoke<Pong>('app_ping', { nonce });
  } catch (err) {
    throw toAppError(err);
  }
}

/** Forward a frontend log line to the Rust structured log stream. */
export async function frontendLog(entry: FrontendLog): Promise<void> {
  await invoke('frontend_log', { entry });
}

/** The effective configuration (defaults ← file ← session overrides). */
export async function configGet(): Promise<AppConfig> {
  try {
    return await invoke<AppConfig>('config_get');
  } catch (err) {
    throw toAppError(err);
  }
}

/** Change one config value — `persist` writes the file, otherwise session-only. */
export async function configSet(req: ConfigSet): Promise<void> {
  try {
    await invoke('config_set', { req });
  } catch (err) {
    throw toAppError(err);
  }
}

/** List every overridable config key with its current effective value. */
export async function configKeys(): Promise<ConfigKeyInfo[]> {
  try {
    return await invoke<ConfigKeyInfo[]>('config_keys');
  } catch (err) {
    throw toAppError(err);
  }
}

// ---------------------------------------------------------------- models + acquisition

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    throw toAppError(err);
  }
}

/** Every registered model. */
export const modelsList = (): Promise<RegisteredModel[]> => call('models_list');

/** Delete a model (file + registry row + any download row). */
export const modelDelete = (id: string): Promise<void> => call('model_delete', { id });

/** Search HuggingFace for GGUF models. */
export const hfSearch = (query: string, limit = 20): Promise<HfModelSummary[]> =>
  call('hf_search', { query, limit });

/** List a repo's `.gguf` files with quant / context from the header. */
export const hfListFiles = (repo: string): Promise<HfGgufFile[]> => call('hf_list_files', { repo });

/** Start a GGUF download; `onProgress` receives byte ticks over a Channel. */
export async function downloadStart(
  req: DownloadRequest,
  onProgress: (p: DownloadProgress) => void,
): Promise<string> {
  const progress = new Channel<DownloadProgress>();
  progress.onmessage = onProgress;
  return call('download_start', { req, progress });
}

export const downloadPause = (id: string): Promise<void> => call('download_pause', { id });
export const downloadResume = (id: string): Promise<void> => call('download_resume', { id });
export const downloadCancel = (id: string): Promise<void> => call('download_cancel', { id });

/** Every download row (queued / in-progress / done / failed). */
export const downloadsList = (): Promise<DownloadInfo[]> => call('downloads_list');

/** Acquire the pinned faster-whisper or Chatterbox model bundle. */
export const acquireFixed = (which: FixedModelKind): Promise<string[]> =>
  call('acquire_fixed', { which });

// ---------------------------------------------------------------- resources

/** The resource manager's current view: GPU / RAM measurement + reservations. */
export const resourcesSnapshot = (): Promise<ResourceSnapshot> => call('resources_snapshot');

// ---------------------------------------------------------------- lifecycle

/** Every model the lifecycle manager is tracking, with its runtime state. */
export const lifecycleStatus = (): Promise<LifecycleStatus[]> => call('lifecycle_status');

/** Register a GGUF that already sits in the model directory (no download). */
export const modelRegisterLocal = (filename: string): Promise<string> =>
  call('model_register_local', { filename });

/** Load a registered model into memory. */
export const modelLoad = (id: string): Promise<void> => call('model_load', { id });

/** Unload a model. */
export const modelUnload = (id: string): Promise<void> => call('model_unload', { id });

// ---------------------------------------------------------------- chat (Phase 16)

/** Start a new conversation. */
export const conversationCreate = (): Promise<Conversation> => call('conversation_create');

/** Every conversation, newest activity first. */
export const conversationList = (): Promise<Conversation[]> => call('conversation_list');

/** A conversation's messages, in order. */
export const conversationMessages = (id: string): Promise<Message[]> =>
  call('conversation_messages', { id });

/** Send a user message; `onEvent` receives streamed generation events. Returns
 * the generation's task id (for {@link chatCancel}). */
export async function chatSend(
  input: { conversationId: string; modelId: string; text: string },
  onEvent: (e: GenerationEvent) => void,
): Promise<string> {
  const events = new Channel<GenerationEvent>();
  events.onmessage = onEvent;
  return call('chat_send', {
    req: {
      conversation_id: input.conversationId,
      model_id: input.modelId,
      text: input.text,
    },
    events,
  });
}

/** Generate a reply over the conversation's existing history (no new user turn) —
 * used after a voice turn. `onEvent` receives streamed generation events. */
export async function chatGenerate(
  input: { conversationId: string; modelId: string },
  onEvent: (e: GenerationEvent) => void,
): Promise<string> {
  const events = new Channel<GenerationEvent>();
  events.onmessage = onEvent;
  return call('chat_generate', {
    conversationId: input.conversationId,
    modelId: input.modelId,
    events,
  });
}

/** The engine's streaming state (whether a generation is running). */
export const chatState = (): Promise<GenerationState> => call('chat_state');

/** Cancel an in-flight generation. */
export const chatCancel = (taskId: string): Promise<void> => call('chat_cancel', { taskId });

// --- Voice input (Phase 18) ---

/** The machine's audio input devices. */
export const voiceInputDevices = (): Promise<InputDevice[]> => call('voice_input_devices');

/** Start push-to-talk listening on `conversationId`; `onState` receives every
 * {@link VoiceState} change until the session ends. */
export async function voiceStart(
  conversationId: string,
  onState: (s: VoiceState) => void,
): Promise<void> {
  const events = new Channel<VoiceState>();
  events.onmessage = onState;
  return call('voice_start', { conversationId, events });
}

/** Release push-to-talk — transcribe an utterance in progress, then stop. */
export const voiceStop = (): Promise<void> => call('voice_stop');

/** Current voice state (poll fallback). */
export const voiceState = (): Promise<VoiceState> => call('voice_state');
