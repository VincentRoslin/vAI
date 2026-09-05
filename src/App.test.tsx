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
  it('renders the three primary tabs', () => {
    renderAt('/chat');
    expect(screen.getByRole('link', { name: 'Chat / Voice' })).not.toBeNull();
    expect(screen.getByRole('link', { name: 'Image Generator' })).not.toBeNull();
    expect(screen.getByRole('link', { name: 'Discovery' })).not.toBeNull();
  });

  it('shows a not-found view for an unknown route', () => {
    renderAt('/nope');
    expect(screen.getByRole('alert').textContent).toContain('Page not found');
  });
});
