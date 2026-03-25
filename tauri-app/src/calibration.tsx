import React from 'react';
import ReactDOM from 'react-dom/client';
import { ErrorBoundary } from './components';
import CalibrationWindow from './components/CalibrationWindow';
import './styles.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <CalibrationWindow />
    </ErrorBoundary>
  </React.StrictMode>
);
