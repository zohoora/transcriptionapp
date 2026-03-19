import React from 'react';
import ReactDOM from 'react-dom/client';
import { ErrorBoundary } from './components';
import AdminPanel from './components/AdminPanel';
import './styles.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <AdminPanel />
    </ErrorBoundary>
  </React.StrictMode>
);
