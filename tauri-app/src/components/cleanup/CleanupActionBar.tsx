import React from 'react';

interface CleanupActionBarProps {
  selectedCount: number;
  onMerge: () => void;
  onDelete: () => void;
  onEditName: () => void;
  onSplit: () => void;
  onRegenSoap: () => void;
}

const CleanupActionBar: React.FC<CleanupActionBarProps> = ({
  selectedCount,
  onMerge,
  onDelete,
  onEditName,
  onSplit,
  onRegenSoap,
}) => {
  return (
    <div className="cleanup-action-bar">
      {selectedCount === 0 ? (
        <span className="cleanup-hint">Select sessions to manage</span>
      ) : selectedCount === 1 ? (
        <div className="cleanup-actions">
          <span className="cleanup-count">1 selected</span>
          <button className="cleanup-btn delete" onClick={onDelete}>Delete</button>
          <button className="cleanup-btn" onClick={onEditName}>Edit Name</button>
          <button className="cleanup-btn" onClick={onSplit}>Split</button>
          <button className="cleanup-btn" onClick={onRegenSoap}>Regen SOAP</button>
        </div>
      ) : (
        <div className="cleanup-actions">
          <span className="cleanup-count">{selectedCount} selected</span>
          <button className="cleanup-btn merge" onClick={onMerge}>Merge</button>
          <button className="cleanup-btn delete" onClick={onDelete}>Delete</button>
          <button className="cleanup-btn" onClick={onRegenSoap}>Regen SOAP</button>
        </div>
      )}
    </div>
  );
};

export default CleanupActionBar;
