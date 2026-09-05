import { NavLink, Outlet } from 'react-router-dom';

const primary = [
  { to: '/chat', label: 'Chat / Voice' },
  { to: '/images', label: 'Image Generator' },
  { to: '/discovery', label: 'Discovery' },
];
const utility = [
  { to: '/models', label: 'Models' },
  { to: '/settings', label: 'Settings' },
];

function tabStyle({ isActive }: { isActive: boolean }): React.CSSProperties {
  return {
    padding: '6px 12px',
    borderRadius: 'var(--radius)',
    textDecoration: 'none',
    color: isActive ? 'var(--accent-text)' : 'var(--text)',
    background: isActive ? 'var(--accent)' : 'transparent',
  };
}

/** Frame: a top nav (3 primary tabs + Models + Settings) and the routed page. */
export function AppShell(): React.JSX.Element {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      <header
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 'var(--gap)',
          padding: '8px 12px',
          borderBottom: '1px solid var(--border)',
          background: 'var(--bg-elevated)',
        }}
      >
        <strong style={{ marginRight: 8 }}>LocalAI</strong>
        <nav style={{ display: 'flex', gap: 4 }}>
          {primary.map((t) => (
            <NavLink key={t.to} to={t.to} style={tabStyle}>
              {t.label}
            </NavLink>
          ))}
        </nav>
        <div style={{ flex: 1 }} />
        <nav style={{ display: 'flex', gap: 4 }}>
          {utility.map((t) => (
            <NavLink key={t.to} to={t.to} style={tabStyle}>
              {t.label}
            </NavLink>
          ))}
        </nav>
      </header>
      <main style={{ flex: 1, overflow: 'auto', padding: 24 }}>
        <Outlet />
      </main>
    </div>
  );
}
