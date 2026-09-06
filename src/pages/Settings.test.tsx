import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { Settings } from './Settings';
import * as ipc from '../lib/ipc';

vi.mock('../lib/ipc', async (importOriginal) => {
  const real = await importOriginal<typeof import('../lib/ipc')>();
  return {
    ...real,
    configGet: vi.fn(async () => ({ version: 6, models: { dir: '/m' } })),
    diagExport: vi.fn(async () => 'C:\\data\\diagnostics\\diag-x.json'),
    personaList: vi.fn(async () => [
      {
        id: 'p1',
        name: 'Ada',
        summary: '',
        personality: '',
        tone: '',
        style: '',
        guidance: [],
        created_at: 't',
        updated_at: 't',
      },
    ]),
    personaCreate: vi.fn(async () => 'p-new'),
    personaUpdate: vi.fn(async () => undefined),
    personaDelete: vi.fn(async () => undefined),
    memoryList: vi.fn(async () => [
      {
        id: 'mem1',
        kind: 'Fact' as const,
        content: 'keeps honeybees on a rooftop',
        importance: 4,
        source_conversation_id: null,
        created_at: 't',
      },
    ]),
    memoryDelete: vi.fn(async () => undefined),
  };
});

describe('Settings', () => {
  it('shows the config and exports diagnostics', async () => {
    render(<Settings />);

    await waitFor(() => expect(screen.getByText(/"version": 6/)).toBeTruthy());

    fireEvent.click(screen.getByRole('button', { name: /export diagnostics/i }));
    await waitFor(() => expect(ipc.diagExport).toHaveBeenCalled());
    expect(await screen.findByText(/diag-x\.json/)).toBeTruthy();
  });

  it('creates a persona through the form', async () => {
    render(<Settings />);

    fireEvent.click(await screen.findByRole('button', { name: /new persona/i }));
    fireEvent.change(await screen.findByLabelText('name'), { target: { value: 'Ada' } });
    fireEvent.change(screen.getByLabelText('summary'), { target: { value: 'an analyst' } });
    fireEvent.change(screen.getByLabelText(/guidance/i), {
      target: { value: 'be precise\ncite sources' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() =>
      expect(ipc.personaCreate).toHaveBeenCalledWith({
        name: 'Ada',
        summary: 'an analyst',
        personality: '',
        tone: '',
        style: '',
        guidance: ['be precise', 'cite sources'],
      }),
    );
  });

  it('lists and deletes a persona memory', async () => {
    render(<Settings />);

    // The Memories section: pick a persona → its memories load.
    const picker = await screen.findByRole('combobox');
    fireEvent.change(picker, { target: { value: 'p1' } });

    const row = await screen.findByText(/keeps honeybees on a rooftop/);
    await waitFor(() => expect(ipc.memoryList).toHaveBeenCalledWith('p1'));

    const li = row.closest('li') as HTMLElement;
    fireEvent.click(within(li).getByRole('button', { name: 'Delete' }));
    await waitFor(() => expect(ipc.memoryDelete).toHaveBeenCalledWith('mem1'));
  });
});
