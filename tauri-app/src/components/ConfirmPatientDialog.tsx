import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface ConfirmPatientResult {
  medplumSynced: boolean;
  profileServiceSynced: boolean;
  patientId: string | null;
  medplumPatientId: string | null;
  confirmedAt: string;
  errors: string[];
}

interface ConfirmPatientDialogProps {
  sessionId: string;
  date: string;
  initialName: string | null;
  initialDob: string | null;
  sessionStartedAt: string;
  sessionDurationMs: number;
  soapNote: string | null;
  transcript: string | null;
  onConfirmed: () => void;
  onCancel: () => void;
}

const ConfirmPatientDialog: React.FC<ConfirmPatientDialogProps> = ({
  sessionId,
  date,
  initialName,
  initialDob,
  sessionStartedAt,
  sessionDurationMs,
  soapNote,
  transcript,
  onConfirmed,
  onCancel,
}) => {
  const [name, setName] = useState(initialName ?? '');
  const [dob, setDob] = useState(initialDob ?? '');
  const [status, setStatus] = useState<'idle' | 'syncing' | 'done'>('idle');
  const [result, setResult] = useState<ConfirmPatientResult | null>(null);
  const [validationError, setValidationError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmedName = name.trim();
    if (!trimmedName) {
      setValidationError('Patient name is required');
      return;
    }
    if (!/^\d{4}-\d{2}-\d{2}$/.test(dob)) {
      setValidationError('Date of birth must be YYYY-MM-DD');
      return;
    }
    setValidationError(null);
    setStatus('syncing');
    try {
      const res = await invoke<ConfirmPatientResult>('confirm_session_patient', {
        sessionId,
        date,
        patientName: trimmedName,
        patientDob: dob,
        soapNote,
        transcript,
        sessionStartedAt,
        sessionDurationMs,
      });
      setResult(res);
      setStatus('done');
    } catch (err) {
      setResult({
        medplumSynced: false,
        profileServiceSynced: false,
        patientId: null,
        medplumPatientId: null,
        confirmedAt: '',
        errors: [String(err)],
      });
      setStatus('done');
    }
  };

  const handleClose = () => {
    if (result && (result.medplumSynced || result.profileServiceSynced)) {
      onConfirmed();
    } else {
      onCancel();
    }
  };

  return (
    <div className="cleanup-dialog-overlay" onClick={status === 'done' ? handleClose : onCancel}>
      <div className="cleanup-dialog" onClick={(e) => e.stopPropagation()}>
        <h3>Confirm Patient</h3>
        {status === 'idle' && (
          <form onSubmit={handleSubmit}>
            <label className="cleanup-dialog-label">
              Name
              <input
                type="text"
                className="cleanup-dialog-input"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Full patient name"
                autoFocus
              />
            </label>
            <label className="cleanup-dialog-label">
              Date of birth
              <input
                type="date"
                className="cleanup-dialog-input"
                value={dob}
                onChange={(e) => setDob(e.target.value)}
                required
              />
            </label>
            {validationError && (
              <p className="cleanup-dialog-error" role="alert">{validationError}</p>
            )}
            <p className="cleanup-dialog-hint">
              Confirming syncs the patient record to both the Medplum EMR and the profile-service
              patient index. Used for longitudinal memory across visits.
            </p>
            <div className="cleanup-dialog-actions">
              <button type="button" className="cleanup-dialog-btn cancel" onClick={onCancel}>
                Cancel
              </button>
              <button type="submit" className="cleanup-dialog-btn confirm">
                Confirm &amp; sync to EMR
              </button>
            </div>
          </form>
        )}
        {status === 'syncing' && <p className="cleanup-dialog-syncing">Syncing…</p>}
        {status === 'done' && result && (
          <div className="cleanup-dialog-result">
            <ul className="cleanup-dialog-status-list">
              <li>
                <span className={result.medplumSynced ? 'status-ok' : 'status-fail'}>
                  {result.medplumSynced ? '✓' : '·'}
                </span>{' '}
                Medplum (EMR)
              </li>
              <li>
                <span className={result.profileServiceSynced ? 'status-ok' : 'status-fail'}>
                  {result.profileServiceSynced ? '✓' : '·'}
                </span>{' '}
                Profile service
              </li>
            </ul>
            {result.patientId && (
              <p className="cleanup-dialog-patient-id">
                Patient&nbsp;ID: {result.patientId}
                {!result.medplumPatientId && ' (local UUID — will reconcile on next Medplum sync)'}
              </p>
            )}
            {result.errors.length > 0 && (
              <details className="cleanup-dialog-errors">
                <summary>{result.errors.length} non-fatal error(s)</summary>
                <ul>
                  {result.errors.map((err, i) => (
                    <li key={i}>{err}</li>
                  ))}
                </ul>
              </details>
            )}
            <div className="cleanup-dialog-actions">
              <button type="button" className="cleanup-dialog-btn confirm" onClick={handleClose}>
                Close
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default ConfirmPatientDialog;
