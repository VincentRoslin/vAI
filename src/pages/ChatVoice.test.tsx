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
      persona_id: null,
      created_at: 't',
      updated_at: 't',
    })),
    conversationMessages: vi.fn(async () => []),
    conversationSetPersona: vi.fn(async () => undefined),
    personaList: vi.fn(async () => [
      {
        id: 'p1',
        name: 'Ada',
        summary: 's',
        personality: '',
        tone: '',
        style: '',
        guidance: [],
        created_at: 't',
        updated_at: 't',
      },
    ]),
    chatPromptPreview: vi.fn(async () => ({
      prompt: '<|im_start|>system\nYou are Ada.<|im_end|>\n<|im_start|>assistant\n',
      provenance: {
        total_tokens: 10,
        budget_tokens: 2816,
        system_tokens: 10,
        persona: 'Full' as const,
        memory_items: 0,
        memory_tokens: 0,
        history_turns_included: 0,
        history_turns_dropped: 0,
      },
    })),
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

  it('binds a persona on a fresh conversation and locks it once messages exist', async () => {
    const { unmount } = render(<ChatVoice />);
    const picker = (await screen.findByLabelText(/persona/i)) as HTMLSelectElement;
    expect(picker.disabled).toBe(false);

    fireEvent.change(picker, { target: { value: 'p1' } });
    await waitFor(() => expect(ipc.conversationSetPersona).toHaveBeenCalledWith('c1', 'p1'));
    unmount();

    // Same conversation, but now with a message → picker disabled.
    vi.mocked(ipc.conversationMessages).mockResolvedValue([
      {
        id: 'm1',
        conversation_id: 'c1',
        role: 'User',
        content: { type: 'Text', data: { text: 'hi' } },
        created_at: 't',
        generation: null,
      },
    ] as never);
    render(<ChatVoice />);
    const locked = (await screen.findByLabelText(/persona/i)) as HTMLSelectElement;
    await waitFor(() => expect(locked.disabled).toBe(true));
  });

  it('"New chat" starts a fresh conversation and unlocks the persona picker', async () => {
    vi.mocked(ipc.conversationMessages)
      .mockResolvedValueOnce([
        {
          id: 'm1',
          conversation_id: 'c1',
          role: 'User',
          content: { type: 'Text', data: { text: 'hi' } },
          created_at: 't',
          generation: null,
        },
      ] as never)
      .mockResolvedValue([] as never);

    render(<ChatVoice />);
    const picker = (await screen.findByLabelText(/persona/i)) as HTMLSelectElement;
    await waitFor(() => expect(picker.disabled).toBe(true));

    fireEvent.click(screen.getByRole('button', { name: 'New chat' }));
    await waitFor(() => expect(ipc.conversationCreate).toHaveBeenCalled());
    await waitFor(() => expect(picker.disabled).toBe(false));
  });

  it('"Show prompt" fetches the assembled prompt', async () => {
    vi.mocked(ipc.lifecycleStatus).mockResolvedValue(loadedModel);
    vi.mocked(ipc.modelsList).mockResolvedValue([
      { metadata: { id: 'm1', display_name: 'Qwen', kind: 'Llm' } },
    ] as never);

    render(<ChatVoice />);
    const details = await screen.findByText(/show prompt/i);
    fireEvent.click(details);
    await waitFor(() => expect(ipc.chatPromptPreview).toHaveBeenCalledWith('c1', 'm1'));
    expect(await screen.findByText(/You are Ada\./)).toBeTruthy();
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
