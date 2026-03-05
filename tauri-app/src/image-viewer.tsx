import React from 'react';
import ReactDOM from 'react-dom/client';
import { ErrorBoundary } from './components';
import ImageViewerWindow from './components/ImageViewerWindow';
import './styles.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <ImageViewerWindow />
    </ErrorBoundary>
  </React.StrictMode>
);
