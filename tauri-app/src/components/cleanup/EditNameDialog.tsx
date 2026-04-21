import React, { useState } from 'react';

interface EditNameDialogProps {
  currentName: string | null;
  onConfirm: (name: string) => void;
  onCancel: () => void;
}

const EditNameDialog: React.FC<EditNameDialogProps> = ({
  currentName,
  onConfirm,
  onCancel,
}) => {
  const [name, setName] = useState(currentName ?? '');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onConfirm(name);
  };

  return (
    <div className="history-dialog-overlay" onClick={onCancel}>
      <div className="history-dialog" onClick={(e) => e.stopPropagation()}>
        <h3>Edit Patient Name</h3>
        <form onSubmit={handleSubmit}>
          <input
            type="text"
            className="history-dialog-input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Patient name (leave empty to clear)"
            autoFocus
          />
          <div className="history-dialog-actions">
            <button type="button" className="history-dialog-btn cancel" onClick={onCancel}>
              Cancel
            </button>
            <button type="submit" className="history-dialog-btn confirm">
              Save
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};

export default EditNameDialog;
