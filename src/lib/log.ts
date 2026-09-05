/**
 * Frontend logger. Forwards to the Rust structured log stream via the
 * `frontend_log` command (`AI_PIPELINES.md` / observability). The frontend never
 * writes its own log files. Console output is kept for dev only.
 */
import { frontendLog } from './ipc';
import type { FrontendLog } from '../bindings/FrontendLog';

type Level = FrontendLog['level'];

function emit(level: Level, target: string, message: string): void {
  if (import.meta.env.DEV) {
    const line = `[${target}] ${message}`;
    // eslint-disable-next-line no-console
    if (level === 'error') console.error(line);
    // eslint-disable-next-line no-console
    else if (level === 'warn') console.warn(line);
    // eslint-disable-next-line no-console
    else console.log(line);
  }
  void frontendLog({ level, target, message }).catch(() => {
    // If the core isn't up yet, drop it — the console line above is enough in dev.
  });
}

export const log = {
  error: (target: string, message: string) => emit('error', target, message),
  warn: (target: string, message: string) => emit('warn', target, message),
  info: (target: string, message: string) => emit('info', target, message),
  debug: (target: string, message: string) => emit('debug', target, message),
};
