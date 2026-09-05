import '@testing-library/react';

// The Tauri IPC bridge isn't present under jsdom. Stub the core module so
// component tests can render without a real backend.
import { vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'app_ready') return { version: '0.1.0' };
    if (cmd === 'app_ping') return { nonce: 'test', version: '0.1.0' };
    if (cmd === 'config_get') {
      return {
        version: 2,
        models: { dir: '/tmp/models', budget_gb: 100 },
        logging: { level: 'info' },
      };
    }
    if (cmd === 'config_keys') return [];
    if (cmd === 'config_set') return undefined;
    return undefined;
  }),
}));
