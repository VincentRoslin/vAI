import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import * as ipc from '../lib/ipc';
import { ImageGenerator } from './ImageGenerator';

vi.mock('../lib/ipc', async (orig) => {
  const actual = await orig<typeof import('../lib/ipc')>();
  return {
    ...actual,
    imageLoras: vi.fn(),
    imagePresets: vi.fn(),
    imageHistory: vi.fn(),
    imageGenerate: vi.fn(),
    imageCancel: vi.fn(),
    imageObjectUrl: vi.fn(),
    imageOutputDir: vi.fn(),
    imageOpenOutputDir: vi.fn(),
  };
});

beforeEach(() => {
  vi.mocked(ipc.imageLoras).mockResolvedValue([
    {
      id: 'lora-1',
      display_name: 'Realism',
      base_compat: 'krea2',
      default_weight: 0.9,
      tags: ['realism'],
    },
  ]);
  vi.mocked(ipc.imagePresets).mockResolvedValue([]);
  vi.mocked(ipc.imageHistory).mockResolvedValue([]);
  vi.mocked(ipc.imageObjectUrl).mockResolvedValue('blob:fake');
  vi.mocked(ipc.imageOutputDir).mockResolvedValue(
    'C:\\Users\\me\\AppData\\Roaming\\com.localai.app\\images',
  );
  vi.mocked(ipc.imageGenerate).mockReset();
});

const doneImage = {
  id: 'g1',
  asset: 'a'.repeat(64),
  prompt: 'a red door',
  width: 1024,
  height: 1024,
  seed: 3n,
  lora: null,
  created_at: '',
};

describe('Image Generator', () => {
  it('lists the realism LoRAs from the registry', async () => {
    render(<ImageGenerator />);
    expect(await screen.findByText('Realism')).not.toBeNull();
  });

  it('submits a valid ImageRequest and shows the result in the viewer', async () => {
    let sink: ((e: unknown) => void) | null = null;
    vi.mocked(ipc.imageGenerate).mockImplementation(async (_req, onEvent) => {
      sink = onEvent as (e: unknown) => void;
      return 'task-1';
    });

    render(<ImageGenerator />);
    fireEvent.change(screen.getByPlaceholderText(/lighthouse/i), {
      target: { value: 'a red door' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Generate' }));

    await waitFor(() => expect(ipc.imageGenerate).toHaveBeenCalled());
    const req = vi.mocked(ipc.imageGenerate).mock.calls[0]![0];
    expect(req.prompt).toBe('a red door');
    expect(req.width).toBe(1024);
    expect(req.batch_count).toBe(1);
    expect(req.loras).toEqual([]);

    sink!({
      type: 'Progress',
      data: { phase: 'Generating', step: 4, total_steps: 8, image_index: 0, batch_count: 1 },
    });
    expect((await screen.findAllByText(/Generating…/)).length).toBeGreaterThan(0);

    sink!({ type: 'Done', data: { images: [doneImage] } });
    expect(await screen.findByText(/seed 3/)).not.toBeNull();
    const img = await screen.findByAltText('a red door');
    expect(img.getAttribute('src')).toBe('blob:fake');
  });

  it('carries the selected LoRA in the request', async () => {
    vi.mocked(ipc.imageGenerate).mockResolvedValue('task-2');
    render(<ImageGenerator />);
    await screen.findByText('Realism');
    fireEvent.change(screen.getByPlaceholderText(/lighthouse/i), { target: { value: 'x' } });
    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'lora-1' } });
    fireEvent.click(screen.getByRole('button', { name: 'Generate' }));
    await waitFor(() => expect(ipc.imageGenerate).toHaveBeenCalled());
    const req = vi.mocked(ipc.imageGenerate).mock.calls[0]![0];
    expect(req.loras).toEqual([{ id: 'lora-1', weight: 0.9 }]);
  });

  it('a style chip appends to the prompt', async () => {
    render(<ImageGenerator />);
    const box = screen.getByPlaceholderText(/lighthouse/i) as HTMLTextAreaElement;
    fireEvent.change(box, { target: { value: 'a cat' } });
    fireEvent.click(screen.getByRole('button', { name: 'Cinematic' }));
    await waitFor(() => expect(box.value).toMatch(/a cat, cinematic lighting/));
  });

  it('opens the images folder', async () => {
    vi.mocked(ipc.imageOpenOutputDir).mockResolvedValue();
    render(<ImageGenerator />);
    fireEvent.click(await screen.findByRole('button', { name: 'Open folder' }));
    await waitFor(() => expect(ipc.imageOpenOutputDir).toHaveBeenCalled());
  });
});
