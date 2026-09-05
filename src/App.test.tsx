import { render, screen } from '@testing-library/react';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import { AppShell } from './components/AppShell';
import { NotFound } from './pages/NotFound';

function renderAt(path: string) {
  const router = createMemoryRouter(
    [
      {
        path: '/',
        element: <AppShell />,
        children: [
          { path: 'chat', element: <div>chat page</div> },
          { path: '*', element: <NotFound /> },
        ],
      },
    ],
    { initialEntries: [path] },
  );
  return render(<RouterProvider router={router} />);
}

describe('AppShell', () => {
  it('renders the primary nav + Settings', () => {
    renderAt('/chat');
    expect(screen.getByRole('link', { name: 'Chat' })).not.toBeNull();
    expect(screen.getByRole('link', { name: 'Image' })).not.toBeNull();
    expect(screen.getByRole('link', { name: 'Discovery' })).not.toBeNull();
    expect(screen.getByRole('link', { name: 'Settings' })).not.toBeNull();
    // Settings is a plain nav item, not a collapsible — no such control.
    expect(screen.queryByRole('button', { name: /collapse settings/i })).toBeNull();
  });

  it('has a working sidebar collapse toggle', () => {
    renderAt('/chat');
    expect(screen.getByRole('button', { name: /collapse sidebar/i })).not.toBeNull();
  });

  it('shows a not-found view for an unknown route', () => {
    renderAt('/nope');
    expect(screen.getByRole('alert').textContent).toContain('Page not found');
  });
});
