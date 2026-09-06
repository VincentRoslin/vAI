import { useEffect, useState } from 'react';

import type { AppConfig } from '../lib/contracts';
import { configGet, diagExport, toAppError } from '../lib/ipc';
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
