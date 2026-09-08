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
import type { Persona } from '../bindings/Persona';
import type { PersonaDraft } from '../bindings/PersonaDraft';
import type { PromptPreview } from '../bindings/PromptPreview';
import type { Memory } from '../bindings/Memory';
import type { GenerationEvent } from '../bindings/GenerationEvent';
import type { GenerationState } from '../bindings/GenerationState';
import type { LifecycleStatus } from '../bindings/LifecycleStatus';
import type { Message } from '../bindings/Message';
import type { RegisteredModel } from '../bindings/RegisteredModel';
import type { ResourceSnapshot } from '../bindings/ResourceSnapshot';
import type { InputDevice } from '../bindings/InputDevice';
import type { OutputDevice } from '../bindings/OutputDevice';
import type { Voice } from '../bindings/Voice';
import type { VoiceState } from '../bindings/VoiceState';
import type { DiagSnapshot } from '../bindings/DiagSnapshot';
import type { ImageRequest } from '../bindings/ImageRequest';
import type { ImageEvent } from '../bindings/ImageEvent';
import type { ImageLora } from '../bindings/ImageLora';
import type { ImagePreset } from '../bindings/ImagePreset';
import type { GeneratedImageRow } from '../bindings/GeneratedImageRow';

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

/** Register a GGUF that already sits in `<models>/llm/` (no download). */
export const modelRegisterLocal = (filename: string): Promise<string> =>
  call('model_register_local', { filename });

/** Scan `<models>/llm/` for GGUFs not yet registered. Returns how many were added. */
export const modelsRescan = (): Promise<number> => call('models_rescan');

/** Load a registered model into memory. */
export const modelLoad = (id: string): Promise<void> => call('model_load', { id });

/** Unload a model. */
export const modelUnload = (id: string): Promise<void> => call('model_unload', { id });

// ---------------------------------------------------------------- chat (Phase 16)

/** Start a new conversation, optionally bound to a Persona (Phase 20). */
export const conversationCreate = (personaId: string | null = null): Promise<Conversation> =>
  call('conversation_create', { personaId });

/** Bind (or clear, with `null`) the Persona for a conversation. Rejected once
 * the conversation has a turn (FR-17). */
export const conversationSetPersona = (
  conversationId: string,
  personaId: string | null,
): Promise<void> => call('conversation_set_persona', { conversationId, personaId });

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

/** The exact prompt the next generation on `conversationId` with `modelId`
 * would receive, plus its provenance (FR-34 "show prompt" surface). */
export const chatPromptPreview = (
  conversationId: string,
  modelId: string,
): Promise<PromptPreview> => call('chat_prompt_preview', { conversationId, modelId });

// ---------------------------------------------------------------- personas (Phase 20)

/** Every persona, newest first. */
export const personaList = (): Promise<Persona[]> => call('persona_list');

/** One persona. */
export const personaGet = (id: string): Promise<Persona> => call('persona_get', { id });

/** Create a persona; returns its id. */
export const personaCreate = (draft: PersonaDraft): Promise<string> =>
  call('persona_create', { draft });

/** Update a persona in place. */
export const personaUpdate = (id: string, draft: PersonaDraft): Promise<void> =>
  call('persona_update', { id, draft });

/** Delete a persona. Bound conversations fall back to the default assistant. */
export const personaDelete = (id: string): Promise<void> => call('persona_delete', { id });

// ---------------------------------------------------------------- memory (Phase 21)

/** Every memory a Persona holds, newest first (FR-52). */
export const memoryList = (personaId: string): Promise<Memory[]> =>
  call('memory_list', { personaId });

/** Delete one memory (FR-53). */
export const memoryDelete = (id: string): Promise<void> => call('memory_delete', { id });

// --- Voice (Phase 18 in, Phase 19 out) ---

/** The machine's audio input devices. */
export const voiceInputDevices = (): Promise<InputDevice[]> => call('voice_input_devices');

/** The machine's audio output devices (Phase 19). */
export const voiceOutputDevices = (): Promise<OutputDevice[]> => call('voice_output_devices');

/** Start a voice session on `conversationId`. Pass `modelId` to run the full
 * listen → think → speak loop with barge-in (Phase 19); omit it to transcribe
 * one utterance (Phase 18). `onState` receives every {@link VoiceState} change. */
export async function voiceStart(
  conversationId: string,
  modelId: string | null,
  onState: (s: VoiceState) => void,
): Promise<void> {
  const events = new Channel<VoiceState>();
  events.onmessage = onState;
  return call('voice_start', { conversationId, modelId, events });
}

/** Hang up / stop the session — transcribe an utterance in
 * progress, or barge-in on the assistant, then stop. */
export const voiceStop = (): Promise<void> => call('voice_stop');

/** Current voice state (poll fallback). */
export const voiceState = (): Promise<VoiceState> => call('voice_state');

// --- Cloned voices (Phase 19 follow-up) ---

/** Every imported cloned voice, newest first. */
export const voiceList = (): Promise<Voice[]> => call('voice_list');

/** Import a reference WAV under `name`; validated + stored by the core. */
export const voiceImport = (name: string, bytes: Uint8Array): Promise<Voice> =>
  call('voice_import', { name, bytes });

/** Delete a cloned voice (row + reference WAV). */
export const voiceDelete = (id: string): Promise<void> => call('voice_delete', { id });

/** Set the active voice (`null` = the built-in voice). Applies next session. */
export const voiceSetActive = (id: string | null): Promise<void> =>
  call('voice_set_active', { id });

// --- Diagnostics (Phase 18.5) ---

/** A live diagnostics snapshot (build / config / registry / resources /
 * lifecycle / conversation metadata / recent logs / host facts). Local only. */
export const diagSnapshot = (): Promise<DiagSnapshot> => call('diag_snapshot');

/** Write a diagnostics snapshot to `<app_data>/diagnostics/` and return its
 * path (for you to hand to Claude). */
export const diagExport = (): Promise<string> => call('diag_export');

// --- Image generation (Phase 22) ---

/** Generate one or more images with Krea 2 Turbo. `onEvent` receives progress
 * frames then one terminal ({@link ImageEvent} `Done` / `Error` / `Cancelled`).
 * Returns the task id (for {@link imageCancel}). */
export async function imageGenerate(
  req: ImageRequest,
  onEvent: (e: ImageEvent) => void,
): Promise<string> {
  const events = new Channel<ImageEvent>();
  events.onmessage = onEvent;
  return call('image_generate', { req, events });
}

/** Cancel the running image generation. */
export const imageCancel = (taskId: string): Promise<void> => call('image_cancel', { taskId });

/** The realism LoRAs available to Krea 2. */
export const imageLoras = (): Promise<ImageLora[]> => call('image_loras');

/** The saved image-generation presets. */
export const imagePresets = (): Promise<ImagePreset[]> => call('image_presets');

/** Recent generations, newest first. */
export const imageHistory = (limit = 24): Promise<GeneratedImageRow[]> =>
  call('image_history', { limit });

/** The PNG bytes of one stored image, as an object URL for `<img src>`. Caller
 * revokes the URL when done. */
export async function imageObjectUrl(asset: string): Promise<string> {
  // `image_bytes` returns a raw binary response → an ArrayBuffer here.
  const buf = await call<ArrayBuffer | number[]>('image_bytes', { asset });
  const bytes = buf instanceof ArrayBuffer ? new Uint8Array(buf) : new Uint8Array(buf);
  const blob = new Blob([bytes], { type: 'image/png' });
  return URL.createObjectURL(blob);
}

/** The browsable folder generated PNGs are written to (display path). */
export const imageOutputDir = (): Promise<string> => call('image_output_dir');

/** Open the image output folder in the OS file browser. */
export const imageOpenOutputDir = (): Promise<void> => call('image_open_output_dir');
