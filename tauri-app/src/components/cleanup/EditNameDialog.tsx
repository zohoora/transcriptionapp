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
    <div className="cleanup-dialog-overlay" onClick={onCancel}>
      <div className="cleanup-dialog" onClick={(e) => e.stopPropagation()}>
        <h3>Edit Patient Name</h3>
        <form onSubmit={handleSubmit}>
          <input
            type="text"
            className="cleanup-dialog-input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Patient name (leave empty to clear)"
            autoFocus
          />
          <div className="cleanup-dialog-actions">
            <button type="button" className="cleanup-dialog-btn cancel" onClick={onCancel}>
              Cancel
            </button>
            <button type="submit" className="cleanup-dialog-btn confirm">
              Save
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};

export default EditNameDialog;
