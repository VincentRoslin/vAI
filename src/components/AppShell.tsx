import { useState } from 'react';
import { NavLink, Outlet } from 'react-router-dom';

import { Icon } from './Icon';
import './AppShell.css';

const primary = [
  { to: '/chat', label: 'Chat', icon: 'chat' as const },
  { to: '/images', label: 'Image', icon: 'image' as const },
  { to: '/discovery', label: 'Discovery', icon: 'compass' as const },
];

// Placeholder until conversation history exists (Phase 17/24).
const recent = ['Welcome Assistant', 'Ideas for UI', 'Explain React useEffect'];

const COLLAPSE_KEY = 'localai.nav.collapsed';

function readCollapsed(): boolean {
  try {
    return localStorage.getItem(COLLAPSE_KEY) === '1';
  } catch {
    return false;
  }
}

/**
 * Three-zone-capable shell (`docs/design/visual-language.md` §1). Left nav +
 * center workspace. The right panel is per-surface and not rendered here yet.
 */
export function AppShell(): React.JSX.Element {
  const [collapsed, setCollapsed] = useState(readCollapsed);

  function toggle(): void {
    setCollapsed((c) => {
      const next = !c;
      try {
        localStorage.setItem(COLLAPSE_KEY, next ? '1' : '0');
      } catch {
        /* per-viewer convenience only */
      }
      return next;
    });
  }

  return (
    <div className="shell">
      <nav className="nav" data-collapsed={collapsed} aria-label="Primary">
        <div className="nav__head">
          <Icon name="compass" size={22} />
          <span className="nav__brand">LocalAI</span>
          <button
            className="nav__collapse"
            onClick={toggle}
            aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            aria-pressed={collapsed}
          >
            <Icon name="chevron-left" size={18} />
          </button>
        </div>

        <div className="nav__group">
          {primary.map((t) => (
            <NavLink
              key={t.to}
              to={t.to}
              className="nav__item"
              title={collapsed ? t.label : undefined}
            >
              <Icon name={t.icon} />
              <span className="nav__item-label">{t.label}</span>
            </NavLink>
          ))}
        </div>

        <div className="nav__section-label">Recent</div>
        <div className="nav__recent">
          {recent.map((title) => (
            <a key={title} className="nav__recent-item" href="#" aria-disabled="true">
              {title}
            </a>
          ))}
        </div>

        <div className="nav__foot">
          <NavLink to="/settings" className="nav__item" title={collapsed ? 'Settings' : undefined}>
            <Icon name="settings" />
            <span className="nav__item-label">Settings</span>
          </NavLink>
        </div>
      </nav>

      <main className="workspace">
        <Outlet />
      </main>
    </div>
  );
}
