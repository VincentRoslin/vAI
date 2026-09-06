import { useEffect, useState } from 'react';

import type { AppConfig, Persona, PersonaDraft } from '../lib/contracts';
import {
  configGet,
  diagExport,
  personaCreate,
  personaDelete,
  personaList,
  personaUpdate,
  toAppError,
} from '../lib/ipc';
import { log } from '../lib/log';

/** Settings — read-only config view + a diagnostics export (Phase 18.5).
 * Editing config lands in a later phase; for now the file is the source. */
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
    <section style={{ padding: '1.5rem', display: 'grid', gap: '1.5rem', maxWidth: 720 }}>
      <h1 style={{ margin: 0 }}>Settings</h1>

      <div>
        <h2 style={{ fontSize: '1rem' }}>Diagnostics</h2>
        <p style={{ opacity: 0.8, fontSize: '0.9rem' }}>
          Writes a local JSON snapshot (build, config, models, resources, recent logs, host info —
          no conversation content) you can share with Claude for debugging.
        </p>
        <button type="button" onClick={() => void exportDiag()} disabled={busy}>
          {busy ? 'Exporting…' : 'Export diagnostics'}
        </button>
        {diagPath && (
          <p style={{ fontSize: '0.85rem', wordBreak: 'break-all' }}>
            Saved to <code>{diagPath}</code>
          </p>
        )}
      </div>

      <Personas />

      <div>
        <h2 style={{ fontSize: '1rem' }}>Effective configuration</h2>
        <p style={{ opacity: 0.8, fontSize: '0.9rem' }}>
          Read-only. Edit <code>config.json</code> in the app data folder to change it.
        </p>
        <pre
          style={{
            fontSize: '0.8rem',
            overflowX: 'auto',
            padding: '0.75rem',
            border: '1px solid var(--border, #ccc)',
            borderRadius: 8,
          }}
        >
          {config ? JSON.stringify(config, null, 2) : 'Loading…'}
        </pre>
      </div>

      {error && <p style={{ color: 'var(--danger, #c33)' }}>{error}</p>}
    </section>
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
  const [editing, setEditing] = useState<{ id: string | null; draft: PersonaDraft } | null>(null);
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
    try {
      if (editing.id) await personaUpdate(editing.id, editing.draft);
      else await personaCreate(editing.draft);
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
    <div>
      <h2 style={{ fontSize: '1rem' }}>Personas</h2>
      <p style={{ opacity: 0.8, fontSize: '0.9rem' }}>
        Structured behaviour for Tab&nbsp;1 chats. A conversation&apos;s persona is fixed once it
        has a message.
      </p>

      <ul style={{ listStyle: 'none', padding: 0, display: 'grid', gap: '0.5rem' }}>
        {list.map((p) => (
          <li
            key={p.id}
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              gap: '0.75rem',
              border: '1px solid var(--border, #ccc)',
              borderRadius: 8,
              padding: '0.5rem 0.75rem',
            }}
          >
            <span>
              <strong>{p.name}</strong>
              {p.summary ? ` — ${p.summary}` : ''}
            </span>
            <span style={{ flexShrink: 0 }}>
              <button type="button" onClick={() => setEditing({ id: p.id, draft: toDraft(p) })}>
                Edit
              </button>{' '}
              <button type="button" onClick={() => void remove(p.id)}>
                Delete
              </button>
            </span>
          </li>
        ))}
        {list.length === 0 && <li style={{ opacity: 0.7 }}>No personas yet.</li>}
      </ul>

      {editing ? (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
          style={{ display: 'grid', gap: '0.5rem', marginTop: '0.75rem', maxWidth: 480 }}
        >
          {(['name', 'summary', 'personality', 'tone', 'style'] as const).map((field) => (
            <label key={field} style={{ display: 'grid', gap: '0.25rem', fontSize: '0.85rem' }}>
              {field}
              <input
                value={editing.draft[field]}
                onChange={(e) =>
                  setEditing({ ...editing, draft: { ...editing.draft, [field]: e.target.value } })
                }
              />
            </label>
          ))}
          <label style={{ display: 'grid', gap: '0.25rem', fontSize: '0.85rem' }}>
            guidance (one rule per line)
            <textarea
              rows={3}
              value={editing.draft.guidance.join('\n')}
              onChange={(e) =>
                setEditing({
                  ...editing,
                  draft: {
                    ...editing.draft,
                    guidance: e.target.value
                      .split('\n')
                      .map((s) => s.trim())
                      .filter(Boolean),
                  },
                })
              }
            />
          </label>
          <div>
            <button type="submit" disabled={busy || !editing.draft.name.trim()}>
              {busy ? 'Saving…' : 'Save'}
            </button>{' '}
            <button type="button" onClick={() => setEditing(null)}>
              Cancel
            </button>
          </div>
        </form>
      ) : (
        <button type="button" onClick={() => setEditing({ id: null, draft: { ...EMPTY_DRAFT } })}>
          New persona
        </button>
      )}

      {error && <p style={{ color: 'var(--danger, #c33)', fontSize: '0.85rem' }}>{error}</p>}
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
