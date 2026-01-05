import { memo } from 'react';

interface ReadyModeProps {
  // Audio level for mic preview
  audioLevel?: number; // 0-100 percent

  // Error state
  errorMessage?: string | null;

  // Start action
  onStart: () => void;
}

/**
 * Ready mode UI - shown when idle and ready to start recording.
 * Features a large start button and mic level preview.
 */
export const ReadyMode = memo(function ReadyMode({
  audioLevel = 0,
  errorMessage,
  onStart,
}: ReadyModeProps) {
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
        className="start-button-large ready"
        onClick={onStart}
        aria-label="Start recording"
      >
        <span className="start-icon" />
        <span className="start-label">START</span>
      </button>

      {/* Status Text */}
      <div className="ready-status">
        {errorMessage ? (
          <span className="status-error">{errorMessage}</span>
        ) : (
          <span className="status-ready">Ready to record</span>
        )}
      </div>
    </div>
  );
});

export default ReadyMode;
