import React, { useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ConfirmPatientResult, LocalArchiveDetails } from '../types';
import { formatLocalTime } from '../utils';

interface ConfirmPatientsBatchDialogProps {
  sessions: LocalArchiveDetails[];
  date: string;
  onConfirmed: () => void;
  onCancel: () => void;
}

type RowStatus = 'idle' | 'syncing' | 'done';

interface BatchRow {
  sessionId: string;
  startedAt: string;
  durationMs: number;
  soapNote: string | null;
  transcript: string | null;
  name: string;
  dob: string;
  status: RowStatus;
  result: ConfirmPatientResult | null;
  validationError: string | null;
}

const DOB_RE = /^\d{4}-\d{2}-\d{2}$/;

// `patient_count` lives on ArchiveSummary, not LocalArchiveMetadata — detect
// via the notes array instead.
function isMultiPatient(s: LocalArchiveDetails): boolean {
  return (s.patientNotes?.length ?? 0) > 1;
}

const ConfirmPatientsBatchDialog: React.FC<ConfirmPatientsBatchDialogProps> = ({
  sessions,
  date,
  onConfirmed,
  onCancel,
}) => {
  const { eligible, skippedCount } = useMemo(() => {
    const elig: LocalArchiveDetails[] = [];
    let skipped = 0;
    for (const s of sessions) {
      if (isMultiPatient(s)) {
        skipped += 1;
      } else {
        elig.push(s);
      }
    }
    return { eligible: elig, skippedCount: skipped };
  }, [sessions]);

  const [rows, setRows] = useState<BatchRow[]>(() =>
    eligible.map((s) => ({
      sessionId: s.session_id,
      startedAt: s.metadata.started_at,
      durationMs: s.metadata.duration_ms ?? 0,
      soapNote: s.soap_note ?? null,
      transcript: s.transcript ?? null,
      name: s.metadata.patient_name ?? '',
      dob: s.metadata.patient_dob ?? '',
      status: 'idle',
      result: null,
      validationError: null,
    })),
  );
  const [submitting, setSubmitting] = useState(false);

  const updateRow = (idx: number, patch: Partial<BatchRow>) => {
    setRows((prev) => prev.map((r, i) => (i === idx ? { ...r, ...patch } : r)));
  };

  const validateAll = (): boolean => {
    let ok = true;
    setRows((prev) =>
      prev.map((r) => {
        const trimmed = r.name.trim();
        if (!trimmed) {
          ok = false;
          return { ...r, validationError: 'Patient name is required' };
        }
        if (!DOB_RE.test(r.dob)) {
          ok = false;
          return { ...r, validationError: 'Date of birth must be YYYY-MM-DD' };
        }
        return { ...r, validationError: null };
      }),
    );
    return ok;
  };

  const handleConfirmAll = async () => {
    if (!validateAll()) return;
    setSubmitting(true);
    // Serial submission — concurrency=1 so we don't hammer the Medplum proxy.
    // Inputs are disabled during submit, so the closure snapshot is stable.
    const initial = rows;
    for (let i = 0; i < initial.length; i++) {
      const row = initial[i];
      updateRow(i, { status: 'syncing' });
      try {
        const res = await invoke<ConfirmPatientResult>('confirm_session_patient', {
          sessionId: row.sessionId,
          date,
          patientName: row.name.trim(),
          patientDob: row.dob,
          soapNote: row.soapNote,
          transcript: row.transcript,
          sessionStartedAt: row.startedAt,
          sessionDurationMs: row.durationMs,
        });
        updateRow(i, { status: 'done', result: res });
      } catch (err) {
        updateRow(i, {
          status: 'done',
          result: {
            medplumSynced: false,
            profileServiceSynced: false,
            patientId: null,
            medplumPatientId: null,
            confirmedAt: '',
            errors: [String(err)],
          },
        });
      }
    }
    setSubmitting(false);
  };

  const allDone = rows.length > 0 && rows.every((r) => r.status === 'done');
  const anySuccess = rows.some(
    (r) => r.result && (r.result.medplumSynced || r.result.profileServiceSynced),
  );

  const handleClose = () => {
    if (anySuccess) onConfirmed();
    else onCancel();
  };

  return (
    <div
      className="history-dialog-overlay"
      onClick={submitting ? undefined : allDone ? handleClose : onCancel}
    >
      <div
        className="history-dialog history-dialog-wide"
        onClick={(e) => e.stopPropagation()}
      >
        <h3>Confirm Patient{rows.length === 1 ? '' : 's'}</h3>
        {skippedCount > 0 && (
          <p className="history-dialog-hint" role="note">
            {skippedCount} multi-patient session{skippedCount > 1 ? 's' : ''} skipped (confirm
            individually from each patient's entry).
          </p>
        )}
        {rows.length === 0 ? (
          <>
            <p className="history-dialog-hint">
              No eligible sessions to confirm. Multi-patient sessions must be confirmed from each
              sub-patient's entry.
            </p>
            <div className="history-dialog-actions">
              <button type="button" className="history-dialog-btn cancel" onClick={onCancel}>
                Close
              </button>
            </div>
          </>
        ) : (
          <>
            <table className="history-batch-table" aria-label="Sessions to confirm">
              <thead>
                <tr>
                  <th>Time</th>
                  <th>Patient name</th>
                  <th>DOB</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r, i) => (
                  <tr key={r.sessionId}>
                    <td>{formatLocalTime(r.startedAt)}</td>
                    <td>
                      <input
                        type="text"
                        className="history-dialog-input"
                        value={r.name}
                        onChange={(e) => updateRow(i, { name: e.target.value })}
                        placeholder="Full name"
                        disabled={r.status !== 'idle' || submitting}
                        aria-label={`Patient name for session ${i + 1}`}
                      />
                    </td>
                    <td>
                      <input
                        type="date"
                        className="history-dialog-input"
                        value={r.dob}
                        onChange={(e) => updateRow(i, { dob: e.target.value })}
                        disabled={r.status !== 'idle' || submitting}
                        aria-label={`Date of birth for session ${i + 1}`}
                      />
                    </td>
                    <td>
                      {r.validationError && (
                        <span className="status-fail" role="alert">
                          {r.validationError}
                        </span>
                      )}
                      {!r.validationError && r.status === 'idle' && (
                        <span className="status-idle" aria-hidden>
                          ·
                        </span>
                      )}
                      {!r.validationError && r.status === 'syncing' && (
                        <span className="status-syncing" aria-label="syncing">
                          …
                        </span>
                      )}
                      {!r.validationError && r.status === 'done' && r.result && (
                        <BatchRowResult result={r.result} />
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="history-dialog-actions">
              {!allDone && (
                <>
                  <button
                    type="button"
                    className="history-dialog-btn cancel"
                    onClick={onCancel}
                    disabled={submitting}
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    className="history-dialog-btn confirm"
                    onClick={handleConfirmAll}
                    disabled={submitting}
                  >
                    {submitting
                      ? 'Syncing…'
                      : rows.length === 1
                        ? 'Confirm & sync to EMR'
                        : `Confirm all (${rows.length})`}
                  </button>
                </>
              )}
              {allDone && (
                <button
                  type="button"
                  className="history-dialog-btn confirm"
                  onClick={handleClose}
                >
                  Close
                </button>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
};

const BatchRowResult: React.FC<{ result: ConfirmPatientResult }> = ({ result }) => {
  const hasErrors = result.errors.length > 0;
  return (
    <span className="history-batch-row-status">
      <span
        className={result.medplumSynced ? 'status-ok' : 'status-fail'}
        title="Medplum (EMR)"
      >
        {result.medplumSynced ? '✓' : '·'}
      </span>
      {' '}EMR{' '}
      <span
        className={result.profileServiceSynced ? 'status-ok' : 'status-fail'}
        title="Profile service"
      >
        {result.profileServiceSynced ? '✓' : '·'}
      </span>
      {' '}PS
      {hasErrors && (
        <details className="history-batch-row-errors">
          <summary aria-label="errors">{result.errors.length}</summary>
          <ul>
            {result.errors.map((e, i) => (
              <li key={i}>{e}</li>
            ))}
          </ul>
        </details>
      )}
    </span>
  );
};

export default ConfirmPatientsBatchDialog;
