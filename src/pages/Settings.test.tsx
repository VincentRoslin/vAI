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
});
