import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ChatVoice } from './ChatVoice';
import * as ipc from '../lib/ipc';

vi.mock('../lib/ipc', async (importOriginal) => {
  const real = await importOriginal<typeof import('../lib/ipc')>();
  return {
    ...real,
    lifecycleStatus: vi.fn(),
    modelsList: vi.fn(async () => []),
    conversationList: vi.fn(async () => []),
    conversationCreate: vi.fn(async () => ({
      id: 'c1',
      kind: 'Persona' as const,
      title: null,
      created_at: 't',
      updated_at: 't',
    })),
    conversationMessages: vi.fn(async () => []),
    chatSend: vi.fn(),
    chatCancel: vi.fn(async () => undefined),
    modelLoad: vi.fn(async () => undefined),
    modelUnload: vi.fn(async () => undefined),
    modelRegisterLocal: vi.fn(async () => 'm1'),
    voiceStart: vi.fn(async () => undefined),
    voiceStop: vi.fn(async () => undefined),
  };
});

const loadedModel = [{ id: 'm1', state: 'Loaded' as const, vram_mb: 1024, error: null }];

describe('ChatVoice', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(ipc.lifecycleStatus).mockResolvedValue([]);
  });

  it('shows the no-model state and a Load button', async () => {
    render(<ChatVoice />);
    expect(await screen.findByText(/no model loaded/i)).toBeTruthy();
    expect(screen.getByRole('button', { name: /load model/i })).toBeTruthy();
  });

  it('streams deltas into the transcript and Stop cancels', async () => {
    vi.mocked(ipc.lifecycleStatus).mockResolvedValue(loadedModel);
    vi.mocked(ipc.modelsList).mockResolvedValue([
      { metadata: { id: 'm1', display_name: 'Qwen', kind: 'Llm' } },
    ] as never);

    let emit: (e: unknown) => void = () => undefined;
    vi.mocked(ipc.chatSend).mockImplementation(async (_input, onEvent) => {
      emit = onEvent as (e: unknown) => void;
      return 'task-1';
    });

    render(<ChatVoice />);

    const box = await screen.findByPlaceholderText(/message/i);
    fireEvent.change(box, { target: { value: 'hello' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => expect(ipc.chatSend).toHaveBeenCalled());
    emit({ type: 'TokenDelta', data: { index: 0, text: 'Hi ' } });
    emit({ type: 'TokenDelta', data: { index: 1, text: 'there' } });
    await waitFor(() => expect(screen.getByText(/Hi there/)).toBeTruthy());

    fireEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(ipc.chatCancel).toHaveBeenCalledWith('task-1');
  });

  it('the mic toggles a voice session', async () => {
    let pushState: (s: unknown) => void = () => undefined;
    vi.mocked(ipc.voiceStart).mockImplementation(async (_id, _model, onState) => {
      pushState = onState as (s: unknown) => void;
    });

    render(<ChatVoice />);
    const mic = await screen.findByRole('button', { name: /start voice/i });

    fireEvent.click(mic);
    await waitFor(() =>
      expect(ipc.voiceStart).toHaveBeenCalledWith('c1', null, expect.any(Function)),
    );
    pushState({ kind: 'Listening' });
    await waitFor(() => expect(mic.className).toMatch(/chat__mic--live/));

    fireEvent.click(screen.getByRole('button', { name: /stop voice/i }));
    await waitFor(() => expect(ipc.voiceStop).toHaveBeenCalled());
  });
});
