import React from 'react';
import ReactDOM from 'react-dom/client';
import { ErrorBoundary } from './components';
import { PatientHandoutEditor } from './components/PatientHandoutEditor';
import './styles.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <PatientHandoutEditor />
    </ErrorBoundary>
  </React.StrictMode>
);
