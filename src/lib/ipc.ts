/**
 * Typed wrappers over Tauri IPC (`docs/decisions/0002-ipc-design.md`).
 *
 * The frontend never calls `invoke` directly — every command has a thin typed
 * wrapper here, using the DTO/error types generated from Rust by `ts-rs` into
 * `src/bindings/`. Rejected commands are normalized to a typed `AppError`.
 */
import { invoke } from '@tauri-apps/api/core';

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
