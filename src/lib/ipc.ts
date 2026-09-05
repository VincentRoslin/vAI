/**
 * Typed wrappers over Tauri IPC (`docs/decisions/0002-ipc-design.md`).
 *
 * The frontend never calls `invoke` directly — every command has a thin typed
 * wrapper here, using the DTO/error types generated from Rust by `ts-rs` into
 * `src/bindings/`. Rejected commands are normalized to a typed `AppError`.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { AppError } from '../bindings/AppError';
import type { AppReady } from '../bindings/AppReady';
import type { FrontendLog } from '../bindings/FrontendLog';
import type { Pong } from '../bindings/Pong';

export type { AppError, AppReady, FrontendLog, Pong };

/** Narrow an unknown `invoke` rejection to our `AppError` shape. */
export function toAppError(err: unknown): AppError {
  if (err && typeof err === 'object' && 'kind' in err) {
    return err as AppError;
  }
  return { kind: 'Internal' };
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

/** Subscribe to the one-shot `app://ready` event. */
export function listenAppReady(cb: (ready: AppReady) => void): Promise<UnlistenFn> {
  return listen<AppReady>('app://ready', (e) => cb(e.payload));
}
