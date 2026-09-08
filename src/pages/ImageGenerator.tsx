import { useCallback, useEffect, useRef, useState } from 'react';

import {
  imageCancel,
  imageGenerate,
  imageHistory,
  imageLoras,
  imageObjectUrl,
  imageOpenOutputDir,
  imageOutputDir,
  imagePresets,
  toAppError,
} from '../lib/ipc';
import type {
  GeneratedImageRow,
  ImageLora,
  ImagePhase,
  ImagePreset,
  ImageRequest,
} from '../lib/contracts';
import { log } from '../lib/log';
import { PromptWizard } from '../components/wizard/prompt-wizard';
import { Icon } from '../components/Icon';
import './ImageGenerator.css';

type Size = { label: string; width: number; height: number };
const DEFAULT_SIZE: Size = { label: 'Square', width: 1024, height: 1024 };
const SIZES: Size[] = [
  DEFAULT_SIZE,
  { label: 'Portrait', width: 928, height: 1232 },
  { label: 'Landscape', width: 1232, height: 928 },
];

/** Client-side prompt flavour chips — appended to the prompt on click. */
const STYLES: { label: string; phrase: string }[] = [
  { label: 'Photoreal', phrase: 'photorealistic, natural light, sharp focus' },
  { label: 'Cinematic', phrase: 'cinematic lighting, shallow depth of field, film grain' },
  { label: 'Studio portrait', phrase: 'studio portrait, softbox lighting, 85mm' },
  { label: 'Golden hour', phrase: 'golden hour, warm backlight, hazy' },
  { label: 'B&W', phrase: 'black and white, high contrast, analog film' },
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
    /* jsdom / older webviews lack it */
  }
}

/** A monotonically-growing asset-id → object-URL cache for the session. */
function useAssetUrls(): [Record<string, string>, (assets: string[]) => void] {
  const [urls, setUrls] = useState<Record<string, string>>({});
  const ref = useRef<Record<string, string>>({});
  useEffect(
    () => () => {
      Object.values(ref.current).forEach(revoke);
    },
    [],
  );
  const resolve = useCallback((assets: string[]) => {
    const need = assets.filter((a) => a && !ref.current[a]);
    if (need.length === 0) return;
    Promise.all(need.map(async (a) => [a, await imageObjectUrl(a)] as const))
      .then((pairs) => {
        pairs.forEach(([a, u]) => {
          ref.current[a] = u;
        });
        setUrls({ ...ref.current });
      })
      .catch((e) => log.warn('image', `resolve urls: ${toAppError(e).kind}`));
  }, []);
  return [urls, resolve];
}

export function ImageGenerator(): React.JSX.Element {
  const [prompt, setPrompt] = useState('');
  const [negative, setNegative] = useState('');
  const [size, setSize] = useState<Size>(DEFAULT_SIZE);
  const [steps, setSteps] = useState<number | null>(null);
  const [guidance, setGuidance] = useState<number | null>(null);
  const [batch, setBatch] = useState(1);
  const [seed, setSeed] = useState('');
  const [loras, setLoras] = useState<ImageLora[]>([]);
  const [loraId, setLoraId] = useState('');
  const [loraWeight, setLoraWeight] = useState(0.9);
  const [presets, setPresets] = useState<ImagePreset[]>([]);
  const [outDir, setOutDir] = useState('');
  const [wizardOpen, setWizardOpen] = useState(false);

  const [run, setRun] = useState<RunState>({ kind: 'idle' });
  const [taskId, setTaskId] = useState<string | null>(null);
  const [batchResults, setBatchResults] = useState<GeneratedImageRow[]>([]);
  const [history, setHistory] = useState<GeneratedImageRow[]>([]);
  const [selected, setSelected] = useState<GeneratedImageRow | null>(null);
  const [urls, resolveUrls] = useAssetUrls();

  const refreshHistory = useCallback(() => {
    imageHistory(60)
      .then((h) => {
        setHistory(h);
        resolveUrls(h.map((r) => r.asset));
      })
      .catch((e) => log.warn('image', `history: ${toAppError(e).kind}`));
  }, [resolveUrls]);

  useEffect(() => {
    imageLoras()
      .then(setLoras)
      .catch((e) => log.warn('image', `loras: ${toAppError(e).kind}`));
    imagePresets()
      .then(setPresets)
      .catch((e) => log.warn('image', `presets: ${toAppError(e).kind}`));
    imageOutputDir()
      .then(setOutDir)
      .catch(() => {});
    refreshHistory();
  }, [refreshHistory]);

  useEffect(() => {
    const l = loras.find((x) => x.id === loraId);
    if (l) setLoraWeight(l.default_weight);
  }, [loraId, loras]);

  const busy = run.kind === 'running';

  function applyPreset(p: ImagePreset): void {
    const match = SIZES.find((s) => s.width === p.params.width && s.height === p.params.height);
    setSize(match ?? { label: 'Custom', width: p.params.width, height: p.params.height });
    setSteps(p.params.steps || null);
    setGuidance(p.params.guidance || null);
  }

  function addStyle(phrase: string): void {
    setPrompt((cur) =>
      cur.includes(phrase) ? cur : `${cur.trim()}${cur.trim() ? ', ' : ''}${phrase}`,
    );
  }

  async function generate(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    if (busy || !prompt.trim()) return;
    setBatchResults([]);
    const req: ImageRequest = {
      prompt: prompt.trim(),
      negative: negative.trim() || null,
      width: size.width,
      height: size.height,
      steps,
      guidance,
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
          setBatchResults(ev.data.images);
          resolveUrls(ev.data.images.map((i) => i.asset));
          if (ev.data.images[0]) setSelected(ev.data.images[0]);
          setRun({ kind: 'idle' });
          setTaskId(null);
          refreshHistory();
        } else if (ev.type === 'Cancelled') {
          setRun({ kind: 'idle' });
          setTaskId(null);
        } else {
          setRun({ kind: 'error', message: errText(ev.data.error) });
          setTaskId(null);
        }
      });
      setTaskId(id);
    } catch (err) {
      setRun({ kind: 'error', message: errText(err) });
    }
  }

  const pct =
    run.kind === 'running' && run.phase === 'Generating' && run.total > 0
      ? Math.round((run.step / run.total) * 100)
      : null;

  const filmstrip = batchResults.length > 1 ? batchResults : [];

  return (
    <>
      <div className="imagegen">
        <form className="imagegen__panel" onSubmit={generate}>
          <h1>Image</h1>
          <p className="imagegen__hint">Write a prompt, or build one with the wizard.</p>
          <button type="button" className="imagegen__wizard" onClick={() => setWizardOpen(true)}>
            <Icon name="wand" size={16} />
            Prompt wizard
          </button>

          <label className="imagegen__field">
            <span>Prompt</span>
            <textarea
              rows={5}
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder="a weathered lighthouse at dawn, soft fog, photographic"
            />
          </label>

          <div className="imagegen__chips" aria-label="Style">
            {STYLES.map((s) => (
              <button key={s.label} type="button" onClick={() => addStyle(s.phrase)}>
                {s.label}
              </button>
            ))}
          </div>

          <label className="imagegen__field">
            <span>Negative prompt</span>
            <input
              type="text"
              value={negative}
              onChange={(e) => setNegative(e.target.value)}
              placeholder="blurry, low quality"
            />
          </label>

          {presets.length > 0 && (
            <div className="imagegen__section">
              <span className="imagegen__label">Presets</span>
              <div className="imagegen__chips">
                {presets.map((p) => (
                  <button key={p.id} type="button" onClick={() => applyPreset(p)}>
                    {p.name}
                  </button>
                ))}
              </div>
            </div>
          )}

          <div className="imagegen__section">
            <span className="imagegen__label">Size</span>
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
            <div className="imagegen__row">
              <label className="imagegen__inline">
                <span>LoRA</span>
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
                  style={{ width: pct === null ? '35%' : `${pct}%` }}
                  data-indeterminate={pct === null}
                />
              </div>
            </div>
          )}
          {run.kind === 'error' && <p className="imagegen__error">{run.message}</p>}
        </form>

        <main className="imagegen__main">
          <section className="imagegen__viewer">
            {selected && urls[selected.asset] ? (
              <>
                <div className="imagegen__canvas">
                  <img src={urls[selected.asset]} alt={selected.prompt} />
                </div>
                <div className="imagegen__meta">
                  <p className="imagegen__meta-prompt">{selected.prompt}</p>
                  <p className="imagegen__meta-sub">
                    {selected.width}×{selected.height} · seed {String(selected.seed)}
                    {selected.lora ? ` · ${selected.lora}` : ''}
                  </p>
                </div>
                {filmstrip.length > 0 && (
                  <div className="imagegen__filmstrip">
                    {filmstrip.map((r) => (
                      <button
                        key={r.id}
                        type="button"
                        className={selected.id === r.id ? 'is-active' : ''}
                        onClick={() => setSelected(r)}
                      >
                        {urls[r.asset] ? <img src={urls[r.asset]} alt="" /> : <span />}
                      </button>
                    ))}
                  </div>
                )}
              </>
            ) : (
              <div className="imagegen__empty">
                <p>
                  {busy
                    ? PHASE_TEXT[run.kind === 'running' ? run.phase : 'Generating']
                    : 'No image yet'}
                </p>
                <span>
                  Write a prompt and hit Generate. Results also land in the images folder.
                </span>
              </div>
            )}
          </section>

          <section className="imagegen__library">
            <header>
              <h2>Library</h2>
              <div className="imagegen__lib-actions">
                {outDir && <span title={outDir}>{outDir}</span>}
                <button type="button" onClick={() => imageOpenOutputDir().catch(() => {})}>
                  Open folder
                </button>
              </div>
            </header>
            {history.length === 0 ? (
              <p className="imagegen__lib-empty">Generated images show up here.</p>
            ) : (
              <div className="imagegen__lib-grid">
                {history.map((h) => (
                  <button
                    key={h.id}
                    type="button"
                    className={selected?.id === h.id ? 'is-active' : ''}
                    onClick={() => setSelected(h)}
                    title={`${h.prompt}\n${h.width}×${h.height} · seed ${String(h.seed)}`}
                  >
                    {urls[h.asset] ? <img src={urls[h.asset]} alt={h.prompt} /> : <span />}
                  </button>
                ))}
              </div>
            )}
          </section>
        </main>
      </div>
      <PromptWizard open={wizardOpen} onOpenChange={setWizardOpen} onUse={setPrompt} />
    </>
  );
}
