import { useCallback, useEffect, useRef, useState } from 'react';

import {
  imageCancel,
  imageGenerate,
  imageHistory,
  imageLoras,
  imageObjectUrl,
  toAppError,
} from '../lib/ipc';
import type { GeneratedImageRow, ImageLora, ImagePhase, ImageRequest } from '../lib/contracts';
import { log } from '../lib/log';
import './ImageGenerator.css';

type Size = { label: string; width: number; height: number };
const DEFAULT_SIZE: Size = { label: 'Square', width: 1024, height: 1024 };
const SIZES: Size[] = [
  DEFAULT_SIZE,
  { label: 'Portrait', width: 928, height: 1232 },
  { label: 'Landscape', width: 1232, height: 928 },
];

const PHASE_TEXT: Record<ImagePhase, string> = {
  Evicting: 'Freeing VRAM (unloading the chat model)…',
  Loading: 'Loading Krea 2 Turbo…',
  Generating: 'Generating…',
  Restoring: 'Restoring the chat model…',
};

type RunState =
  | { kind: 'idle' }
  | {
      kind: 'running';
      phase: ImagePhase;
      step: number;
      total: number;
      index: number;
      batch: number;
    }
  | { kind: 'error'; message: string };

function errText(err: unknown): string {
  const a = toAppError(err);
  return 'message' in a ? a.message : a.kind;
}

function revoke(u: string): void {
  try {
    URL.revokeObjectURL(u);
  } catch {
    /* jsdom / older webviews lack it — the URL just lives until reload */
  }
}

/** Resolve a list of asset ids to object URLs, revoking the previous set. */
function useAssetUrls(): [Record<string, string>, (assets: string[]) => void] {
  const [urls, setUrls] = useState<Record<string, string>>({});
  const live = useRef<string[]>([]);
  useEffect(
    () => () => {
      live.current.forEach(revoke);
    },
    [],
  );
  const resolve = useCallback((assets: string[]) => {
    Promise.all(assets.map(async (a) => [a, await imageObjectUrl(a)] as const)).then((pairs) => {
      live.current.forEach(revoke);
      live.current = pairs.map(([, u]) => u);
      setUrls(Object.fromEntries(pairs));
    });
  }, []);
  return [urls, resolve];
}

export function ImageGenerator(): React.JSX.Element {
  const [prompt, setPrompt] = useState('');
  const [negative, setNegative] = useState('');
  const [size, setSize] = useState<Size>(DEFAULT_SIZE);
  const [batch, setBatch] = useState(1);
  const [seed, setSeed] = useState('');
  const [loras, setLoras] = useState<ImageLora[]>([]);
  const [loraId, setLoraId] = useState<string>('');
  const [loraWeight, setLoraWeight] = useState(0.9);

  const [run, setRun] = useState<RunState>({ kind: 'idle' });
  const [taskId, setTaskId] = useState<string | null>(null);
  const [results, setResults] = useState<GeneratedImageRow[]>([]);
  const [history, setHistory] = useState<GeneratedImageRow[]>([]);
  const [urls, resolveUrls] = useAssetUrls();

  const refreshHistory = useCallback(() => {
    imageHistory(12)
      .then(setHistory)
      .catch((e) => log.warn('image', `history: ${toAppError(e).kind}`));
  }, []);

  useEffect(() => {
    imageLoras()
      .then(setLoras)
      .catch((e) => log.warn('image', `loras: ${toAppError(e).kind}`));
    refreshHistory();
  }, [refreshHistory]);

  useEffect(() => {
    const l = loras.find((x) => x.id === loraId);
    if (l) setLoraWeight(l.default_weight);
  }, [loraId, loras]);

  const busy = run.kind === 'running';

  async function generate(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    if (busy || !prompt.trim()) return;
    setResults([]);
    const req: ImageRequest = {
      prompt: prompt.trim(),
      negative: negative.trim() || null,
      width: size.width,
      height: size.height,
      steps: null,
      guidance: null,
      seed: seed.trim() ? BigInt(seed.trim()) : null,
      batch_count: batch,
      loras: loraId ? [{ id: loraId, weight: loraWeight }] : [],
    };
    setRun({ kind: 'running', phase: 'Evicting', step: 0, total: 0, index: 0, batch });
    try {
      const id = await imageGenerate(req, (ev) => {
        if (ev.type === 'Progress') {
          const p = ev.data;
          setRun({
            kind: 'running',
            phase: p.phase,
            step: p.step,
            total: p.total_steps,
            index: p.image_index,
            batch: p.batch_count,
          });
        } else if (ev.type === 'Done') {
          setResults(ev.data.images);
          resolveUrls(ev.data.images.map((i) => i.asset));
          setRun({ kind: 'idle' });
          setTaskId(null);
          refreshHistory();
        } else if (ev.type === 'Cancelled') {
          setRun({ kind: 'idle' });
          setTaskId(null);
        } else {
          setRun({
            kind: 'error',
            message: errText(ev.data.error),
          });
          setTaskId(null);
        }
      });
      setTaskId(id);
    } catch (err) {
      setRun({ kind: 'error', message: errText(err) });
    }
  }

  const progressPct =
    run.kind === 'running' && run.phase === 'Generating' && run.total > 0
      ? Math.round((run.step / run.total) * 100)
      : null;

  return (
    <div className="imagegen">
      <h1>Image Generator</h1>

      <form className="imagegen__form" onSubmit={generate}>
        <label className="imagegen__field">
          <span>Prompt</span>
          <textarea
            rows={3}
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder="a weathered lighthouse at dawn, soft fog, photographic"
          />
        </label>

        <label className="imagegen__field">
          <span>Negative prompt (optional)</span>
          <input
            type="text"
            value={negative}
            onChange={(e) => setNegative(e.target.value)}
            placeholder="blurry, low quality"
          />
        </label>

        <div className="imagegen__row">
          <div className="imagegen__seg" role="group" aria-label="Image size">
            {SIZES.map((s) => (
              <button
                key={s.label}
                type="button"
                className={s.label === size.label ? 'is-active' : ''}
                onClick={() => setSize(s)}
              >
                {s.label}
                <em>
                  {s.width}×{s.height}
                </em>
              </button>
            ))}
          </div>
        </div>

        <div className="imagegen__row">
          <label className="imagegen__inline">
            <span>Images</span>
            <input
              type="number"
              min={1}
              max={8}
              value={batch}
              onChange={(e) => setBatch(Math.min(8, Math.max(1, Number(e.target.value) || 1)))}
            />
          </label>
          <label className="imagegen__inline">
            <span>Seed</span>
            <input
              type="text"
              inputMode="numeric"
              value={seed}
              placeholder="random"
              onChange={(e) => setSeed(e.target.value.replace(/[^0-9]/g, ''))}
            />
          </label>
        </div>

        {loras.length > 0 && (
          <div className="imagegen__row imagegen__lora">
            <label className="imagegen__inline">
              <span>Realism LoRA</span>
              <select value={loraId} onChange={(e) => setLoraId(e.target.value)}>
                <option value="">None (base model)</option>
                {loras.map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.display_name}
                  </option>
                ))}
              </select>
            </label>
            {loraId && (
              <label className="imagegen__inline imagegen__weight">
                <span>Weight {loraWeight.toFixed(2)}</span>
                <input
                  type="range"
                  min={0}
                  max={1}
                  step={0.05}
                  value={loraWeight}
                  onChange={(e) => setLoraWeight(Number(e.target.value))}
                />
              </label>
            )}
          </div>
        )}

        <div className="imagegen__actions">
          {busy ? (
            <button
              type="button"
              className="imagegen__cancel"
              onClick={() => taskId && imageCancel(taskId).catch(() => {})}
            >
              Cancel
            </button>
          ) : (
            <button type="submit" className="imagegen__go" disabled={!prompt.trim()}>
              Generate
            </button>
          )}
        </div>
      </form>

      {run.kind === 'running' && (
        <div className="imagegen__status">
          <p>
            {PHASE_TEXT[run.phase]}
            {run.phase === 'Generating' && run.batch > 1
              ? ` (image ${run.index + 1}/${run.batch})`
              : ''}
          </p>
          <div className="imagegen__bar">
            <div
              className="imagegen__bar-fill"
              style={{ width: progressPct === null ? '35%' : `${progressPct}%` }}
              data-indeterminate={progressPct === null}
            />
          </div>
        </div>
      )}
      {run.kind === 'error' && <p className="imagegen__error">{run.message}</p>}

      {results.length > 0 && (
        <div className="imagegen__grid">
          {results.map((r) => (
            <figure key={r.id}>
              {urls[r.asset] ? (
                <img src={urls[r.asset]} alt={r.prompt} />
              ) : (
                <div className="imagegen__ph" />
              )}
              <figcaption>seed {r.seed}</figcaption>
            </figure>
          ))}
        </div>
      )}

      {history.length > 0 && (
        <section className="imagegen__history">
          <h2>Recent</h2>
          <ul>
            {history.map((h) => (
              <li key={h.id}>
                <span className="imagegen__hist-prompt">{h.prompt}</span>
                <span className="imagegen__hist-meta">
                  {h.width}×{h.height} · seed {h.seed}
                  {h.lora ? ` · ${h.lora}` : ''}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
