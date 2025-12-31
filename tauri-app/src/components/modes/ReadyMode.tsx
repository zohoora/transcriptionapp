import { memo } from 'react';
import type { ModelStatus, ChecklistResult, CheckStatus } from '../../types';

interface ReadyModeProps {
  // Model status
  modelStatus: ModelStatus | null;
  modelName: string;

  // Checklist
  checklistRunning: boolean;
  checklistResult: ChecklistResult | null;
  onRunChecklist: () => void;
  onDownloadModel: (modelName: string) => void;
  downloadingModel: string | null;

  // Audio level for mic preview (optional)
  audioLevel?: number; // 0-100 percent

  // Error state
  errorMessage?: string | null;

  // Start action
  canStart: boolean;
  onStart: () => void;
}

// Status icon helpers
const getCheckIcon = (status: CheckStatus): string => {
  switch (status) {
    case 'pass': return '\u2713';  // checkmark
    case 'fail': return '\u2717';  // x
    case 'warning': return '\u26A0'; // warning
    case 'pending': return '\u25CB'; // circle
    case 'skipped': return '\u2212'; // minus
  }
};

const getCheckClass = (status: CheckStatus): string => {
  switch (status) {
    case 'pass': return 'check-pass';
    case 'fail': return 'check-fail';
    case 'warning': return 'check-warning';
    case 'pending': return 'check-pending';
    case 'skipped': return 'check-skipped';
  }
};

/**
 * Ready mode UI - shown when idle and ready to start recording.
 * Features a large start button, mic level preview, and inline checklist status.
 */
export const ReadyMode = memo(function ReadyMode({
  modelStatus,
  modelName,
  checklistRunning,
  checklistResult,
  onRunChecklist,
  onDownloadModel,
  downloadingModel,
  audioLevel = 0,
  errorMessage,
  canStart,
  onStart,
}: ReadyModeProps) {
  const hasChecklist = checklistRunning || checklistResult;
  const checklistHasFailures = checklistResult && !checklistResult.can_start;
  const showInlineChecklist = hasChecklist && (checklistRunning || checklistHasFailures);

  return (
    <div className="ready-mode">
      {/* Mic Level Preview */}
      <div className="mic-preview">
        <div className="mic-level-bar">
          <div
            className="mic-level-fill"
            style={{ width: `${Math.min(100, Math.max(0, audioLevel))}%` }}
          />
        </div>
      </div>

      {/* Large Start Button */}
      <button
        className={`start-button-large ${canStart ? 'ready' : 'disabled'}`}
        onClick={onStart}
        disabled={!canStart}
        aria-label="Start recording"
      >
        <span className="start-icon" />
        <span className="start-label">START</span>
      </button>

      {/* Status Text */}
      <div className="ready-status">
        {errorMessage ? (
          <span className="status-error">{errorMessage}</span>
        ) : checklistRunning ? (
          <span className="status-checking">Running checks...</span>
        ) : !modelStatus?.available ? (
          <span className="status-error">Model not found</span>
        ) : checklistHasFailures ? (
          <span className="status-error">{checklistResult?.summary}</span>
        ) : (
          <span className="status-ready">
            Ready to record
            <span className="model-info">{modelName}</span>
          </span>
        )}
      </div>

      {/* Inline Checklist (only show when running or has failures) */}
      {showInlineChecklist && (
        <div className="ready-checklist">
          {checklistRunning ? (
            <div className="checklist-loading-inline">
              <div className="spinner-small" />
              <span>Running checks...</span>
            </div>
          ) : (
            <>
              <div className="checklist-items-inline">
                {checklistResult?.checks.map((check) => (
                  <div key={check.id} className={`checklist-item-inline ${getCheckClass(check.status)}`}>
                    <span className="check-icon-inline">{getCheckIcon(check.status)}</span>
                    <div className="check-content-inline">
                      <span className="check-name-inline">{check.name}</span>
                      {check.message && (
                        <span className="check-message-inline">{check.message}</span>
                      )}
                    </div>
                    {check.status === 'fail' && check.action?.download_model && (
                      <button
                        className="download-btn-small"
                        onClick={() => onDownloadModel(check.action!.download_model!.model_name)}
                        disabled={downloadingModel !== null}
                      >
                        {downloadingModel === check.action.download_model.model_name ? '...' : 'Get'}
                      </button>
                    )}
                  </div>
                ))}
              </div>
              <button className="btn-retry-small" onClick={onRunChecklist} disabled={checklistRunning || downloadingModel !== null}>
                Re-check
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
});

export default ReadyMode;
