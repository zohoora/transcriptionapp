import React from 'react';
import ReactDOM from 'react-dom/client';
import { ErrorBoundary } from './components';
import ClinicalAssistantWindow from './components/ClinicalAssistantWindow';
import './styles.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <ClinicalAssistantWindow />
    </ErrorBoundary>
  </React.StrictMode>
);
