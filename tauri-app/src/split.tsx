import React from 'react';
import ReactDOM from 'react-dom/client';
import { ErrorBoundary } from './components';
import SplitWindow from './components/SplitWindow';
import './styles.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <SplitWindow />
    </ErrorBoundary>
  </React.StrictMode>
);
