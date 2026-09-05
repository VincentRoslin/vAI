import { Component, type ErrorInfo, type ReactNode } from 'react';

import { log } from '../lib/log';

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

/**
 * Top-level error boundary (`UI_GUIDELINES.md` §2). A render error shows a
 * recovery panel, never a white screen, and is forwarded to the Rust log.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    log.error('ui', `render error: ${error.message} ${info.componentStack ?? ''}`);
  }

  render(): ReactNode {
    if (this.state.error) {
      return (
        <div role="alert" style={{ padding: 24, maxWidth: 560 }}>
          <h2>Something went wrong</h2>
          <p style={{ color: 'var(--text-muted)' }}>
            The interface hit an unexpected error. Your data is safe. Reload to continue.
          </p>
          <button onClick={() => window.location.reload()}>Reload</button>
          <details style={{ marginTop: 16 }}>
            <summary>Details</summary>
            <pre style={{ whiteSpace: 'pre-wrap', color: 'var(--text-muted)' }}>
              {this.state.error.message}
            </pre>
          </details>
        </div>
      );
    }
    return this.props.children;
  }
}
