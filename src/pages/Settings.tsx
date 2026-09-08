import { useEffect, useState } from 'react';

import type { AppConfig, InputDevice, Memory, OutputDevice, Persona, PersonaDraft, Voice } from '../lib/contracts';
import {
  configGet,
  configSet,
  diagExport,
  memoryDelete,
  memoryList,
  personaCreate,
  personaDelete,
  personaList,
  personaUpdate,
  toAppError,
  voiceDelete,
  voiceImport,
  voiceInputDevices,
  voiceList,
  voiceOutputDevices,
  voiceSetActive,
} from '../lib/ipc';
import { log } from '../lib/log';
import './Settings.css';

/** Settings — personas, memories, voice tuning, a diagnostics export, and a
 * read-only dump of the effective config. Most config keys are still file-only;
 * edit `config.json` for those. */
export function Settings(): React.JSX.Element {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [diagPath, setDiagPath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    configGet()
      .then(setConfig)
      .catch((e) => setError(`Could not load config: ${toAppError(e).kind}`));
  }, []);

  async function exportDiag(): Promise<void> {
    setBusy(true);
    setError(null);
    try {
      const path = await diagExport();
      setDiagPath(path);
      log.info('ui', 'diagnostics exported');
    } catch (e) {
      setError(`Export failed: ${toAppError(e).kind}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="settings">
      <h1>Settings</h1>

      <div className="settings__section">
        <h2>Diagnostics</h2>
        <p className="settings__hint">
          Writes a local JSON snapshot (build, config, models, resources, recent logs, host info —
          no conversation content) you can share with Claude for debugging.
        </p>
        <div className="settings__form-actions">
          <button type="button" onClick={() => void exportDiag()} disabled={busy}>
            {busy ? 'Exporting…' : 'Export diagnostics'}
          </button>
        </div>
        {diagPath && (
          <p className="settings__saved">
            Saved to <code>{diagPath}</code>
          </p>
        )}
      </div>

      <Voice config={config} />

      <Voices />

      <Personas />

      <Memories />

      <div className="settings__section">
        <h2>Effective configuration</h2>
        <p className="settings__hint">
          Read-only. Edit <code>config.json</code> in the app data folder to change it.
        </p>
        <pre className="settings__config">
          {config ? JSON.stringify(config, null, 2) : 'Loading…'}
        </pre>
      </div>

      {error && <p className="settings__error">{error}</p>}
    </section>
  );
}

/** Voice tuning: devices + end-of-speech hang. Applies at the next call. */
function Voice({ config }: { config: AppConfig | null }): React.JSX.Element {
  const [ms, setMs] = useState('');
  const [input, setInput] = useState('');
  const [output, setOutput] = useState('');
  const [inputs, setInputs] = useState<InputDevice[]>([]);
  const [outputs, setOutputs] = useState<OutputDevice[]>([]);
  const [status, setStatus] = useState('');

  useEffect(() => {
    if (config?.voice) {
      setMs(String(config.voice.end_of_speech_ms));
      setInput(config.voice.input_device ?? '');
      setOutput(config.voice.output_device ?? '');
    }
  }, [config]);

  function loadDevices(): void {
    void voiceInputDevices()
      .then(setInputs)
      .catch(() => setInputs([]));
    void voiceOutputDevices()
      .then(setOutputs)
      .catch(() => setOutputs([]));
  }

  useEffect(() => {
    loadDevices();
  }, []);

  async function save(): Promise<void> {
    const n = Number(ms);
    if (!Number.isFinite(n) || n < 300 || n > 5000) {
      setStatus('Enter a pause between 300 and 5000 ms.');
      return;
    }
    try {
      await configSet({ key: 'VoiceEndOfSpeechMs', value: String(Math.round(n)), persist: true });
      await configSet({ key: 'VoiceInputDevice', value: input, persist: true });
      await configSet({ key: 'VoiceOutputDevice', value: output, persist: true });
      setStatus('Saved — applies the next time you start a voice call.');
    } catch (e) {
      setStatus(`Save failed: ${toAppError(e).kind}`);
    }
  }

  return (
    <div className="settings__section">
      <h2>Voice</h2>
      <p className="settings__hint">
        Gaming headsets often appear twice (Chat / Headset vs Game / 7.1). For a
        call, pick the <strong>Headset / Chat</strong> input and output — not Speakers
        and not the Game endpoint. Headphones, not speakers, until echo cancellation
        lands (the mic will otherwise hear the AI and interrupt itself).
      </p>
      <div className="settings__form-actions" style={{ flexWrap: 'wrap' }}>
        <label className="settings__picker">
          Microphone
          <select value={input} onChange={(e) => setInput(e.target.value)}>
            <option value="">System default</option>
            {inputs.map((d) => (
              <option key={d.name} value={d.name}>
                {d.name}
                {d.is_default ? ' (default)' : ''}
              </option>
            ))}
          </select>
        </label>
        <label className="settings__picker">
          Speakers / headset
          <select value={output} onChange={(e) => setOutput(e.target.value)}>
            <option value="">System default</option>
            {outputs.map((d) => (
              <option key={d.name} value={d.name}>
                {d.name}
                {d.is_default ? ' (default)' : ''}
              </option>
            ))}
          </select>
        </label>
        <label className="settings__picker">
          Pause (ms)
          <input
            type="number"
            min={300}
            max={5000}
            step={100}
            value={ms}
            onChange={(e) => setMs(e.target.value)}
            style={{ width: '7ch' }}
          />
        </label>
        <button type="button" onClick={() => void save()}>
          Apply
        </button>
        <button type="button" onClick={loadDevices}>
          Refresh devices
        </button>
      </div>
      <p className="settings__hint">
        End-of-speech pause — how long to wait after you stop talking before the turn
        is sent. Raise it if you get cut off; lower it for snappier replies. Default
        1500&nbsp;ms.
      </p>
      {status && <p className="settings__saved">{status}</p>}
    </div>
  );
}

/** Cloned voices (Phase 19 follow-up). Import a reference WAV, pick which voice
 * the assistant speaks in. One global active voice; applied at the next voice
 * session. */
function Voices(): React.JSX.Element {
  const [list, setList] = useState<Voice[]>([]);
  const [name, setName] = useState('');
  const [file, setFile] = useState<File | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = (): Promise<void> =>
    voiceList()
      .then(setList)
      .catch((e) => setError(`Could not load voices: ${toAppError(e).kind}`));

  useEffect(() => {
    void refresh();
  }, []);

  function fail(prefix: string, e: unknown): void {
    const a = toAppError(e);
    setError(`${prefix}: ${'message' in a ? a.message : a.kind}`);
  }

  async function add(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    if (!name.trim() || !file) return;
    setBusy(true);
    setError(null);
    try {
      const buf = await file.arrayBuffer();
      await voiceImport(name.trim(), new Uint8Array(buf));
      setName('');
      setFile(null);
      await refresh();
    } catch (err) {
      fail('Import failed', err);
    } finally {
      setBusy(false);
    }
  }

  async function pick(id: string | null): Promise<void> {
    setError(null);
    try {
      await voiceSetActive(id);
      await refresh();
    } catch (err) {
      fail('Could not switch voice', err);
    }
  }

  async function remove(id: string): Promise<void> {
    setError(null);
    try {
      await voiceDelete(id);
      await refresh();
    } catch (err) {
      fail('Delete failed', err);
    }
  }

  const anyActive = list.some((v) => v.active);

  return (
    <div className="settings__section">
      <h2>Voices</h2>
      <p className="settings__hint">
        Clone a speaker for the assistant&apos;s voice from a reference recording — at least
        6&nbsp;seconds of clean speech, as a WAV file. The choice applies at the next voice session.
      </p>

      <ul className="settings__list">
        <li className="settings__row">
          <label className="settings__row-main">
            <input
              type="radio"
              name="active-voice"
              checked={!anyActive}
              onChange={() => void pick(null)}
            />{' '}
            Default (built-in)
          </label>
        </li>
        {list.map((v) => (
          <li key={v.id} className="settings__row">
            <label className="settings__row-main">
              <input
                type="radio"
                name="active-voice"
                checked={v.active}
                onChange={() => void pick(v.id)}
              />{' '}
              {v.name}
            </label>
            <span className="settings__row-actions">
              <button type="button" onClick={() => void remove(v.id)}>
                Delete
              </button>
            </span>
          </li>
        ))}
      </ul>

      <form className="settings__form" onSubmit={add}>
        <label className="settings__field">
          voice name
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. Narrator"
          />
        </label>
        <label className="settings__field">
          reference WAV
          <input
            type="file"
            accept=".wav,audio/wav,audio/x-wav"
            onChange={(e) => setFile(e.target.files?.[0] ?? null)}
          />
        </label>
        <div className="settings__form-actions">
          <button type="submit" disabled={busy || !name.trim() || !file}>
            {busy ? 'Importing…' : 'Add voice'}
          </button>
        </div>
      </form>

      {error && <p className="settings__error">{error}</p>}
    </div>
  );
}

const EMPTY_DRAFT: PersonaDraft = {
  name: '',
  summary: '',
  personality: '',
  tone: '',
  style: '',
  guidance: [],
};

/** Persona list + create/edit form (Phase 20, FR-30..35). Personas are
 * structured behaviour data the context builder renders into the system block. */
function Personas(): React.JSX.Element {
  const [list, setList] = useState<Persona[]>([]);
  const [editing, setEditing] = useState<{
    id: string | null;
    draft: PersonaDraft;
    guidanceText: string;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh(): Promise<void> {
    try {
      setList(await personaList());
    } catch (e) {
      setError(`Could not load personas: ${toAppError(e).kind}`);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function save(): Promise<void> {
    if (!editing) return;
    setBusy(true);
    setError(null);
    // `guidance` is edited as free text (one rule per line) and only normalised
    // here — trimming per keystroke made spaces and new lines impossible to type.
    const draft: PersonaDraft = {
      ...editing.draft,
      guidance: editing.guidanceText
        .split('\n')
        .map((s) => s.trim())
        .filter(Boolean),
    };
    try {
      if (editing.id) await personaUpdate(editing.id, draft);
      else await personaCreate(draft);
      log.info('ui', `persona ${editing.id ? 'updated' : 'created'}`);
      setEditing(null);
      await refresh();
    } catch (e) {
      setError(`Save failed: ${toAppError(e).kind}`);
    } finally {
      setBusy(false);
    }
  }

  async function remove(id: string): Promise<void> {
    setError(null);
    try {
      await personaDelete(id);
      await refresh();
    } catch (e) {
      setError(`Delete failed: ${toAppError(e).kind}`);
    }
  }

  return (
    <div className="settings__section">
      <h2>Personas</h2>
      <p className="settings__hint">
        Structured behaviour for Tab&nbsp;1 chats. A conversation&apos;s persona is fixed once it
        has a message.
      </p>

      <ul className="settings__list">
        {list.map((p) => (
          <li key={p.id} className="settings__row">
            <span className="settings__row-main">
              <strong>{p.name}</strong>
              {p.summary ? <span className="settings__muted"> — {p.summary}</span> : null}
            </span>
            <span className="settings__row-actions">
              <button
                type="button"
                onClick={() =>
                  setEditing({
                    id: p.id,
                    draft: toDraft(p),
                    guidanceText: p.guidance.join('\n'),
                  })
                }
              >
                Edit
              </button>
              <button type="button" onClick={() => void remove(p.id)}>
                Delete
              </button>
            </span>
          </li>
        ))}
        {list.length === 0 && <li className="settings__empty">No personas yet.</li>}
      </ul>

      {editing ? (
        <form
          className="settings__form"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          {(['name', 'summary', 'personality', 'tone', 'style'] as const).map((field) => (
            <label key={field} className="settings__field">
              {field}
              <input
                value={editing.draft[field]}
                onChange={(e) =>
                  setEditing({ ...editing, draft: { ...editing.draft, [field]: e.target.value } })
                }
              />
            </label>
          ))}
          <label className="settings__field">
            guidance (one rule per line)
            <textarea
              rows={3}
              value={editing.guidanceText}
              onChange={(e) => setEditing({ ...editing, guidanceText: e.target.value })}
            />
          </label>
          <div className="settings__form-actions">
            <button type="submit" disabled={busy || !editing.draft.name.trim()}>
              {busy ? 'Saving…' : 'Save'}
            </button>
            <button type="button" onClick={() => setEditing(null)}>
              Cancel
            </button>
          </div>
        </form>
      ) : (
        <div className="settings__form-actions">
          <button
            type="button"
            onClick={() => setEditing({ id: null, draft: { ...EMPTY_DRAFT }, guidanceText: '' })}
          >
            New persona
          </button>
        </div>
      )}

      {error && <p className="settings__error">{error}</p>}
    </div>
  );
}

/** What each Persona remembers (Phase 21, FR-52/53). View + delete; edit is a
 * later phase. */
function Memories(): React.JSX.Element {
  const [personas, setPersonas] = useState<Persona[]>([]);
  const [selected, setSelected] = useState('');
  const [rows, setRows] = useState<Memory[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    personaList()
      .then(setPersonas)
      .catch((e) => setError(`Could not load personas: ${toAppError(e).kind}`));
  }, []);

  async function refresh(personaId: string): Promise<void> {
    setError(null);
    if (!personaId) {
      setRows([]);
      return;
    }
    try {
      setRows(await memoryList(personaId));
    } catch (e) {
      setError(`Could not load memories: ${toAppError(e).kind}`);
    }
  }

  async function remove(id: string): Promise<void> {
    try {
      await memoryDelete(id);
      await refresh(selected);
    } catch (e) {
      setError(`Delete failed: ${toAppError(e).kind}`);
    }
  }

  return (
    <div className="settings__section">
      <h2>Memories</h2>
      <p className="settings__hint">
        What each Persona has remembered across conversations. Extracted automatically after a turn.
      </p>
      <label className="settings__picker">
        Persona
        <select
          value={selected}
          onChange={(e) => {
            setSelected(e.target.value);
            void refresh(e.target.value);
          }}
        >
          <option value="">Choose a persona…</option>
          {personas.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </label>

      {selected && (
        <ul className="settings__list">
          {rows.map((m) => (
            <li key={m.id} className="settings__row">
              <span className="settings__row-main">
                <span className="settings__muted">
                  {m.kind} · {m.importance}/5 ·{' '}
                </span>
                {m.content}
              </span>
              <span className="settings__row-actions">
                <button type="button" onClick={() => void remove(m.id)}>
                  Delete
                </button>
              </span>
            </li>
          ))}
          {rows.length === 0 && <li className="settings__empty">Nothing remembered yet.</li>}
        </ul>
      )}

      {error && <p className="settings__error">{error}</p>}
    </div>
  );
}

function toDraft(p: Persona): PersonaDraft {
  return {
    name: p.name,
    summary: p.summary,
    personality: p.personality,
    tone: p.tone,
    style: p.style,
    guidance: p.guidance,
  };
}
