import { useEffect, useState } from 'react';
import { NavLink, Outlet, useNavigate } from 'react-router-dom';

import { appReady, toAppError } from '../lib/ipc';
import { log } from '../lib/log';
import { requestOpen, useSession } from '../lib/session';
import { Icon } from './Icon';
import './AppShell.css';

// Startup handshake — run once per session, not per mount (StrictMode double-
// invokes effects in dev and HMR remounts the tree).
let handshakeDone = false;

const primary = [
  { to: '/chat', label: 'Chat', icon: 'chat' as const },
  { to: '/images', label: 'Image', icon: 'image' as const },
  { to: '/discovery', label: 'Discovery', icon: 'compass' as const },
];

const utility = [
  { to: '/models', label: 'Models', icon: 'cube' as const },
  { to: '/settings', label: 'Settings', icon: 'settings' as const },
];

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
  const { list, currentId } = useSession();
  const navigate = useNavigate();

  useEffect(() => {
    if (handshakeDone) return;
    handshakeDone = true;
    appReady()
      .then((ready) => log.info('app', `core ready (v${ready.version})`))
      .catch((err) => log.warn('app', `app_ready failed: ${toAppError(err).kind}`));
  }, []);

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
          {list.length === 0 && (
            <span className="nav__recent-item" aria-disabled="true">
              No chats yet
            </span>
          )}
          {list.map((c) => (
            <button
              key={c.id}
              type="button"
              className="nav__recent-item"
              data-active={c.id === currentId ? 'true' : undefined}
              onClick={() => {
                requestOpen(c.id);
                void navigate('/chat');
              }}
            >
              {c.title?.trim() || 'Untitled chat'}
            </button>
          ))}
        </div>

        <div className="nav__foot">
          {utility.map((t) => (
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
      </nav>

      <main className="workspace">
        <Outlet />
      </main>
    </div>
  );
}
