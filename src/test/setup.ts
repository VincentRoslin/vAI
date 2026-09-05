import '@testing-library/react';

// The Tauri IPC bridge isn't present under jsdom. Stub the two modules the app
// touches at import time so component tests can render without a real backend.
import { vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'app_ping') return { nonce: 'test', version: '0.1.0' };
    return undefined;
  }),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));
