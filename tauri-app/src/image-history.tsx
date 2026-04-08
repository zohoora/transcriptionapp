import React from 'react';
import ReactDOM from 'react-dom/client';
import { ErrorBoundary } from './components';
import ImageHistoryWindow from './components/ImageHistoryWindow';
import './styles.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <ImageHistoryWindow />
    </ErrorBoundary>
  </React.StrictMode>
);
