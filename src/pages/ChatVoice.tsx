import { useEffect, useState } from 'react';

import { Placeholder } from '../components/Placeholder';
import { appPing, toAppError } from '../lib/ipc';
import { log } from '../lib/log';

/**
 * Tab 1 — Chat / Voice. Phase 16 builds the real vertical slice.
 * For now it also hosts the Phase 6 IPC round-trip check.
 */
export function ChatVoice(): React.JSX.Element {
  const [status, setStatus] = useState<string>('checking IPC…');

  useEffect(() => {
    const nonce = Math.random().toString(36).slice(2);
    appPing(nonce)
      .then((pong) => {
        const ok = pong.nonce === nonce;
        setStatus(ok ? `IPC OK — core v${pong.version}` : 'IPC returned an unexpected nonce');
        log.info('chat', `app_ping round-trip ${ok ? 'ok' : 'MISMATCH'}`);
      })
      .catch((err) => {
        setStatus(`IPC failed: ${toAppError(err).kind}`);
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
