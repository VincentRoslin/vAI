/**
 * Typed wrappers over Tauri IPC (`docs/decisions/0002-ipc-design.md`).
 *
 * The frontend never calls `invoke` directly — every command has a thin typed
 * wrapper here, using the DTO/error types generated from Rust by `ts-rs` into
 * `src/bindings/`. Rejected commands are normalized to a typed `AppError`.
 */
import { invoke } from '@tauri-apps/api/core';

import type { AppConfig } from '../bindings/AppConfig';
import type { AppError } from '../bindings/AppError';
import type { AppReady } from '../bindings/AppReady';
import type { ConfigKeyInfo } from '../bindings/ConfigKeyInfo';
import type { ConfigSet } from '../bindings/ConfigSet';
import type { FrontendLog } from '../bindings/FrontendLog';
import type { Pong } from '../bindings/Pong';

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
