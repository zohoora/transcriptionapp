import React from 'react';
import type { LocalArchiveSummary } from '../../types';
import { formatLocalTime } from '../../utils';

interface MergeConfirmDialogProps {
  sessions: LocalArchiveSummary[];
  onConfirm: () => void;
  onCancel: () => void;
}

const MergeConfirmDialog: React.FC<MergeConfirmDialogProps> = ({
  sessions,
  onConfirm,
  onCancel,
}) => {
  // Sort chronologically for display
  const sorted = [...sessions].sort(
    (a, b) => new Date(a.date).getTime() - new Date(b.date).getTime()
  );
  const totalWords = sorted.reduce((sum, s) => sum + s.word_count, 0);

  return (
    <div className="cleanup-dialog-overlay" onClick={onCancel}>
      <div className="cleanup-dialog" onClick={(e) => e.stopPropagation()}>
        <h3>Merge {sorted.length} Sessions</h3>
        <p className="cleanup-dialog-subtitle">
          Sessions will be merged in chronological order. The earliest session survives.
        </p>
        <ul className="cleanup-dialog-list">
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
        <div className="cleanup-dialog-meta">
          Combined: ~{totalWords} words
        </div>
        {sorted.some(s => s.has_soap_note) && (
          <p className="cleanup-dialog-warning">
            Existing SOAP notes will be invalidated and need regeneration.
          </p>
        )}
        <div className="cleanup-dialog-actions">
          <button className="cleanup-dialog-btn cancel" onClick={onCancel}>Cancel</button>
          <button className="cleanup-dialog-btn confirm" onClick={onConfirm}>
            Merge Sessions
          </button>
        </div>
      </div>
    </div>
  );
};

export default MergeConfirmDialog;
