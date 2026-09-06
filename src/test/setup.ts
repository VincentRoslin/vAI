import '@testing-library/react';

// The Tauri IPC bridge isn't present under jsdom. Stub the core module so
// component tests can render without a real backend.
import { vi } from 'vitest';

class FakeChannel {
  onmessage: ((m: unknown) => void) | null = null;
}

const arrayCommands = new Set([
  'config_keys',
  'models_list',
  'downloads_list',
  'hf_search',
  'hf_list_files',
  'lifecycle_status',
  'conversation_list',
  'conversation_messages',
]);

vi.mock('@tauri-apps/api/core', () => ({
  Channel: FakeChannel,
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'app_ready') return { version: '0.1.0' };
    if (cmd === 'app_ping') return { nonce: 'test', version: '0.1.0' };
    if (cmd === 'config_get') {
      return {
        version: 5,
        models: { dir: '/tmp/models', budget_gb: 100, min_free_gb: 20 },
        logging: { level: 'info' },
        resources: { vram_safety_margin_mb: 1500 },
        runtimes: { dir: '/tmp/runtimes' },
      };
    }
    if (cmd === 'conversation_create') {
      return {
        id: 'conv-test',
        kind: 'Persona',
        title: null,
        created_at: '2026-01-01T00:00:00Z',
        updated_at: '2026-01-01T00:00:00Z',
      };
    }
    if (cmd === 'chat_send') return 'task-test';
    if (arrayCommands.has(cmd)) return [];
    return undefined;
  }),
}));
