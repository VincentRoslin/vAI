import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { Settings } from './Settings';
import * as ipc from '../lib/ipc';

vi.mock('../lib/ipc', async (importOriginal) => {
  const real = await importOriginal<typeof import('../lib/ipc')>();
  return {
    ...real,
    configGet: vi.fn(async () => ({ version: 6, models: { dir: '/m' } })),
    diagExport: vi.fn(async () => 'C:\\data\\diagnostics\\diag-x.json'),
    personaList: vi.fn(async () => []),
    personaCreate: vi.fn(async () => 'p-new'),
    personaUpdate: vi.fn(async () => undefined),
    personaDelete: vi.fn(async () => undefined),
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
});
