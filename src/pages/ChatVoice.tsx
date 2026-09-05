import { useEffect, useState } from 'react';

import { Placeholder } from '../components/Placeholder';
import { appPing, toAppError } from '../lib/ipc';
import { log } from '../lib/log';

// Phase 6 round-trip probe. Fires once per session and caches the result, so
// StrictMode's synchronous double-invoke, HMR remounts, and route revisits
// don't re-fire it. Phase 16 replaces this whole component with the real chat
// slice.
let probeStarted = false;
let probeResult: string | null = null;

/**
 * Tab 1 — Chat / Voice. Phase 16 builds the real vertical slice.
 * For now it also hosts the Phase 6 IPC round-trip check.
 */
export function ChatVoice(): React.JSX.Element {
  const [status, setStatus] = useState<string>(probeResult ?? 'checking IPC…');

  useEffect(() => {
    if (probeStarted) return;
    probeStarted = true;
    const nonce = Math.random().toString(36).slice(2);
    appPing(nonce)
      .then((pong) => {
        const ok = pong.nonce === nonce;
        probeResult = ok ? `IPC OK — core v${pong.version}` : 'IPC returned an unexpected nonce';
        setStatus(probeResult);
        log.info('chat', `app_ping round-trip ${ok ? 'ok' : 'MISMATCH'}`);
      })
      .catch((err) => {
        probeResult = `IPC failed: ${toAppError(err).kind}`;
        setStatus(probeResult);
      });
  }, []);

  return (
    <Placeholder title="Chat / Voice" phase="Phase 16 (text) / 18–19 (voice)">
      <p data-testid="ipc-status" style={{ color: 'var(--text-muted)' }}>
        {status}
      </p>
    </Placeholder>
  );
}
