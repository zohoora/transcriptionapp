import React from 'react';
import type { LocalArchiveSummary } from '../../types';
import { formatLocalTime } from '../../utils';

interface DeleteConfirmDialogProps {
  sessions: LocalArchiveSummary[];
  onConfirm: () => void;
  onCancel: () => void;
}

const DeleteConfirmDialog: React.FC<DeleteConfirmDialogProps> = ({
  sessions,
  onConfirm,
  onCancel,
}) => {
  return (
    <div className="history-dialog-overlay" onClick={onCancel}>
      <div className="history-dialog" onClick={(e) => e.stopPropagation()}>
        <h3>Delete {sessions.length === 1 ? 'Session' : `${sessions.length} Sessions`}?</h3>
        <p className="history-dialog-warning">
          This cannot be undone. The following sessions will be permanently deleted:
        </p>
        <ul className="history-dialog-list">
          {sessions.map((s) => (
            <li key={s.session_id}>
              <span className="dialog-session-time">{formatLocalTime(s.date)}</span>
              <span className="dialog-session-info">
                {s.charting_mode === 'continuous' && s.encounter_number != null
                  ? `Encounter #${s.encounter_number}`
                  : `${s.word_count} words`}
                {s.patient_name ? ` \u2014 ${s.patient_name}` : ''}
              </span>
            </li>
          ))}
        </ul>
        <div className="history-dialog-actions">
          <button className="history-dialog-btn cancel" onClick={onCancel}>Cancel</button>
          <button className="history-dialog-btn confirm-delete" onClick={onConfirm}>
            Delete {sessions.length === 1 ? 'Session' : 'Sessions'}
          </button>
        </div>
      </div>
    </div>
  );
};

export default DeleteConfirmDialog;
