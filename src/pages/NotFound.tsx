import { Link } from 'react-router-dom';

export function NotFound(): React.JSX.Element {
  return (
    <section role="alert" style={{ padding: 'var(--space-6)' }}>
      <h1 style={{ marginTop: 0 }}>Page not found</h1>
      <p style={{ color: 'var(--text-muted)' }}>That route doesn&apos;t exist.</p>
      <Link to="/chat">Go to Chat</Link>
    </section>
  );
}
