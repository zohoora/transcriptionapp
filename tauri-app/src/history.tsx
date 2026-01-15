import React from 'react';
import ReactDOM from 'react-dom/client';
import { ErrorBoundary, AuthProvider } from './components';
import HistoryWindow from './components/HistoryWindow';
import './styles.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <AuthProvider>
        <HistoryWindow />
      </AuthProvider>
    </ErrorBoundary>
  </React.StrictMode>
);
