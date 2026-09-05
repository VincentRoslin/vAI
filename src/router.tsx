import { createHashRouter, Navigate } from 'react-router-dom';

import { AppShell } from './components/AppShell';
import { ChatVoice } from './pages/ChatVoice';
import { ImageGenerator } from './pages/ImageGenerator';
import { Discovery } from './pages/Discovery';
import { Models } from './pages/Models';
import { Settings } from './pages/Settings';
import { NotFound } from './pages/NotFound';

/**
 * Hash routing (desktop app, no server). Three primary tabs + Models + Settings
 * per `UI_GUIDELINES.md` §3. The shell renders the nav; each route is a page.
 */
export const router = createHashRouter([
  {
    path: '/',
    element: <AppShell />,
    children: [
      { index: true, element: <Navigate to="/chat" replace /> },
      { path: 'chat', element: <ChatVoice /> },
      { path: 'images', element: <ImageGenerator /> },
      { path: 'discovery', element: <Discovery /> },
      { path: 'models', element: <Models /> },
      { path: 'settings', element: <Settings /> },
      { path: '*', element: <NotFound /> },
    ],
  },
]);
