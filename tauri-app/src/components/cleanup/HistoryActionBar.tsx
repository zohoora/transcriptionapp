import React from 'react';

interface HistoryActionBarProps {
  selectedCount: number;
  onMerge: () => void;
  onDelete: () => void;
  onEditName: () => void;
  onConfirmPatient: () => void;
  onSplit: () => void;
  onRegenSoap: () => void;
}

/** Contextual action bar for the History Window. Renders only when `selectedCount > 0`. */
const HistoryActionBar: React.FC<HistoryActionBarProps> = ({
  selectedCount,
  onMerge,
  onDelete,
  onEditName,
  onConfirmPatient,
  onSplit,
  onRegenSoap,
}) => {
  if (selectedCount <= 0) return null;
  const isSingle = selectedCount === 1;
  return (
    <div className="history-action-bar">
      <div className="history-actions">
        <span className="history-action-count">{selectedCount} selected</span>
        {!isSingle && (
          <button className="history-action-btn merge" onClick={onMerge}>Merge</button>
        )}
        <button className="history-action-btn delete" onClick={onDelete}>Delete</button>
        {isSingle && (
          <button className="history-action-btn" onClick={onEditName}>Edit Name</button>
        )}
        <button
          className="history-action-btn"
          onClick={onConfirmPatient}
          title="Confirm patient name + DOB. Syncs to Medplum EMR and profile service."
        >
          Confirm Patient
        </button>
        {isSingle && (
          <button className="history-action-btn" onClick={onSplit}>Split</button>
        )}
        <button className="history-action-btn" onClick={onRegenSoap}>Regen SOAP</button>
      </div>
    </div>
  );
};

export default HistoryActionBar;
