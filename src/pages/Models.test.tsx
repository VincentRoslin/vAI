import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { Models } from './Models';

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'models_list') {
      return [
        {
          metadata: {
            id: 'm1',
            display_name: 'Test 8B',
            kind: 'Llm',
            backend: 'llama.cpp',
            quant: 'Q4_K_M',
            capabilities: { streaming: true, context_tokens: 8192 },
            estimated_vram_mb: 7000,
          },
          path: '/tmp/models/x.gguf',
          availability: 'Ready',
          devices: ['Cuda', 'Cpu'],
        },
      ];
    }
    if (cmd === 'downloads_list') return [];
    if (cmd === 'hf_search') throw { kind: 'BackendUnavailable', message: 'offline' };
    return undefined;
  });
});

describe('Models page', () => {
  it('lists installed models', async () => {
    render(<Models />);
    expect(await screen.findByText('Test 8B')).not.toBeNull();
    expect(screen.getByText('Q4_K_M')).not.toBeNull();
  });

  it('shows an offline state when search fails', async () => {
    render(<Models />);
    fireEvent.change(screen.getByLabelText('Search HuggingFace'), { target: { value: 'qwen' } });
    fireEvent.click(screen.getByRole('button', { name: 'Search' }));
    await waitFor(() => expect(screen.getByText(/offline/i)).not.toBeNull());
  });
});
