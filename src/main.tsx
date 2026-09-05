import React from 'react';
import ReactDOM from 'react-dom/client';
import { RouterProvider } from 'react-router-dom';

import { router } from './router';
import { ErrorBoundary } from './components/ErrorBoundary';
import { listenAppReady } from './lib/ipc';
import { log } from './lib/log';
import './styles/global.css';

void listenAppReady((ready) => {
  log.info('app', `core ready (v${ready.version})`);
});

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <RouterProvider router={router} />
    </ErrorBoundary>
  </React.StrictMode>,
);
