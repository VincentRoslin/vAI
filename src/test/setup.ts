import '@testing-library/react';

// The Tauri IPC bridge isn't present under jsdom. Stub the core module so
// component tests can render without a real backend.
import { vi } from 'vitest';

class FakeChannel {
  onmessage: ((m: unknown) => void) | null = null;
}

// jsdom doesn't implement Blob/File.prototype.arrayBuffer; the app uses it to
// read a picked file before an IPC upload. Shim it via FileReader (which jsdom
// does implement) so component tests can exercise that path.
if (typeof Blob !== 'undefined' && !Blob.prototype.arrayBuffer) {
  Blob.prototype.arrayBuffer = function arrayBuffer(): Promise<ArrayBuffer> {
    return new Promise((resolve, reject) => {
      const fr = new FileReader();
      fr.onload = () => resolve(fr.result as ArrayBuffer);
      fr.onerror = () => reject(fr.error);
      fr.readAsArrayBuffer(this as unknown as Blob);
    });
  };
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
  'voice_input_devices',
  'voice_output_devices',
  'voice_list',
  'persona_list',
  'memory_list',
]);

vi.mock('@tauri-apps/api/core', () => ({
  Channel: FakeChannel,
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'app_ready') return { version: '0.1.0' };
    if (cmd === 'app_ping') return { nonce: 'test', version: '0.1.0' };
    if (cmd === 'config_get') {
      return {
        version: 7,
        models: { dir: '/tmp/models', budget_gb: 100, min_free_gb: 20 },
        logging: { level: 'info' },
        resources: { vram_safety_margin_mb: 1500 },
        runtimes: { dir: '/tmp/runtimes' },
        workers: { dir: '/tmp/workers', python: '/tmp/py/python' },
        voice: { input_device: null, output_device: null, end_of_speech_ms: 900 },
      };
    }
    if (cmd === 'config_set') return undefined;
    if (cmd === 'voice_import') return { id: 'v-new', name: 'x', active: false, created_at: 't' };
    if (cmd === 'voice_delete' || cmd === 'voice_set_active') return undefined;
    if (cmd === 'conversation_create') {
      return {
        id: 'conv-test',
        kind: 'Persona',
        title: null,
        persona_id: null,
        created_at: '2026-01-01T00:00:00Z',
        updated_at: '2026-01-01T00:00:00Z',
      };
    }
    if (cmd === 'chat_send' || cmd === 'chat_generate') return 'task-test';
    if (cmd === 'persona_create') return 'persona-test';
    if (cmd === 'models_rescan') return 0;
    if (cmd === 'model_register_local') return 'model-test';
    if (cmd === 'chat_prompt_preview') {
      return {
        prompt:
          '<|im_start|>system\nInstructions below are your Persona, strictly follow them:<|im_end|>\n<|im_start|>assistant\n',
        provenance: {
          total_tokens: 12,
          budget_tokens: 2816,
          system_tokens: 12,
          persona: 'Absent',
          memory_items: 0,
          memory_tokens: 0,
          history_turns_included: 0,
          history_turns_dropped: 0,
        },
      };
    }
    if (cmd === 'chat_state') return { generating: null };
    if (cmd === 'voice_start' || cmd === 'voice_stop') return undefined;
    if (cmd === 'voice_state') return { kind: 'Idle' };
    if (cmd === 'diag_export') return 'C:\\path\\diagnostics\\diag-test.json';
    if (cmd === 'diag_snapshot') {
      return {
        taken_at: '2026-01-01T00:00:00Z',
        build: { version: '0.1.0', git_sha: 'abc1234', profile: 'debug' },
        config: {},
        models: [],
        resources: {},
        lifecycle: [],
        conversations: [],
        recent_logs: [],
        host: { os: 'test', nvidia_smi: null, log_dir: null },
      };
    }
    if (arrayCommands.has(cmd)) return [];
    return undefined;
  }),
}));
