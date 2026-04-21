import React, { useState } from 'react';
import type { LocalArchiveSummary } from '../../types';
import { formatLocalTime } from '../../utils';

interface MergeConfirmDialogProps {
  sessions: LocalArchiveSummary[];
  onConfirm: () => void;
  onCancel: () => void;
  /** True when all selected entries are patients from the same encounter */
  isSameSessionPatientMerge?: boolean;
  /** Patient names being merged (only for same-session patient merge) */
  selectedPatientNames?: string[];
  /** Callback for same-session patient merge (receives the merged label) */
  onPatientMergeConfirm?: (newLabel: string) => void;
}

const MergeConfirmDialog: React.FC<MergeConfirmDialogProps> = ({
  sessions,
  onConfirm,
  onCancel,
  isSameSessionPatientMerge,
  selectedPatientNames,
  onPatientMergeConfirm,
}) => {
  // For patient merge: editable merged name (default to first selected name)
  const [mergedLabel, setMergedLabel] = useState(
    selectedPatientNames?.[0] ?? 'Patient'
  );
  const [isMerging, setIsMerging] = useState(false);

  // Same-session patient merge UI
  if (isSameSessionPatientMerge && selectedPatientNames && onPatientMergeConfirm) {
    const handlePatientMerge = async () => {
      setIsMerging(true);
      try {
        await onPatientMergeConfirm(mergedLabel);
      } finally {
        setIsMerging(false);
      }
    };

    return (
      <div className="history-dialog-overlay" onClick={onCancel}>
        <div className="history-dialog" onClick={(e) => e.stopPropagation()}>
          <h3>Merge Patient Notes</h3>
          <p className="history-dialog-subtitle">
            These patients were detected in the same encounter. Merging will combine
            them into one patient note using AI to generate a unified SOAP note.
          </p>
          <ul className="history-dialog-list">
            {selectedPatientNames.map((name, i) => (
              <li key={i}>
                <span className="dialog-session-info">{name}</span>
              </li>
            ))}
          </ul>
          <div className="history-dialog-field">
            <label htmlFor="merged-label">Merged patient name:</label>
            <input
              id="merged-label"
              type="text"
              value={mergedLabel}
              onChange={(e) => setMergedLabel(e.target.value)}
              className="history-dialog-input"
              autoFocus
            />
          </div>
          <p className="history-dialog-warning">
            A new SOAP note will be generated combining clinical content from all
            selected patients. This corrects the AI's multi-patient detection.
          </p>
          <div className="history-dialog-actions">
            <button className="history-dialog-btn cancel" onClick={onCancel} disabled={isMerging}>
              Cancel
            </button>
            <button
              className="history-dialog-btn confirm"
              onClick={handlePatientMerge}
              disabled={isMerging || !mergedLabel.trim()}
            >
              {isMerging ? 'Merging...' : 'Merge Patient Notes'}
            </button>
          </div>
        </div>
      </div>
    );
  }

  // Cross-session merge UI (existing behavior)
  const sorted = [...sessions].sort(
    (a, b) => new Date(a.date).getTime() - new Date(b.date).getTime()
  );
  const totalWords = sorted.reduce((sum, s) => sum + s.word_count, 0);

  return (
    <div className="history-dialog-overlay" onClick={onCancel}>
      <div className="history-dialog" onClick={(e) => e.stopPropagation()}>
        <h3>Merge {sorted.length} Sessions</h3>
        <p className="history-dialog-subtitle">
          Sessions will be merged in chronological order. The earliest session survives.
        </p>
        <ul className="history-dialog-list">
          {sorted.map((s, i) => (
            <li key={s.session_id} className={i === 0 ? 'merge-target' : ''}>
              <span className="dialog-session-time">{formatLocalTime(s.date)}</span>
              <span className="dialog-session-info">
                {s.word_count} words
                {s.patient_name ? ` \u2014 ${s.patient_name}` : ''}
              </span>
              {i === 0 && <span className="merge-target-badge">keeps</span>}
            </li>
          ))}
        </ul>
        <div className="history-dialog-meta">
          Combined: ~{totalWords} words
        </div>
        {sorted.some(s => s.has_soap_note) && (
          <p className="history-dialog-warning">
            Existing SOAP notes will be invalidated and need regeneration.
          </p>
        )}
        <div className="history-dialog-actions">
          <button className="history-dialog-btn cancel" onClick={onCancel}>Cancel</button>
          <button className="history-dialog-btn confirm" onClick={onConfirm}>
            Merge Sessions
          </button>
        </div>
      </div>
    </div>
  );
};

export default MergeConfirmDialog;
