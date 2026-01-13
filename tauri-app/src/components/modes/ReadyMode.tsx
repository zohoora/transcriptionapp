import { memo } from 'react';
import type { ListeningStatus } from '../../types';

interface ReadyModeProps {
  // Audio level for mic preview
  audioLevel?: number; // 0-100 percent

  // Error state
  errorMessage?: string | null;
  // Whether the error is specifically a permission error
  isPermissionError?: boolean;

  // Auto-detection state
  autoStartEnabled?: boolean;
  isListening?: boolean;
  listeningStatus?: ListeningStatus | null;

  // Start action
  onStart: () => void;
  // Open system settings (for permission errors)
  onOpenSettings?: () => void;
  // Toggle auto-detection on/off
  onAutoStartToggle?: (enabled: boolean) => void;
}

/**
 * Helper to get listening state label
 */
function getListeningLabel(status: ListeningStatus | null | undefined): string {
  if (!status) return 'Listening...';
  if (status.analyzing) return 'Analyzing...';
  if (status.speech_detected) {
    const seconds = Math.floor(status.speech_duration_ms / 1000);
    return `Speech detected (${seconds}s)...`;
  }
  return 'Listening...';
}

/**
 * Ready mode UI - shown when idle and ready to start recording.
 * Features a large start button and mic level preview.
 * When auto-start is enabled, shows listening indicator.
 */
export const ReadyMode = memo(function ReadyMode({
  audioLevel = 0,
  errorMessage,
  isPermissionError = false,
  autoStartEnabled = false,
  isListening = false,
  listeningStatus,
  onStart,
  onOpenSettings,
  onAutoStartToggle,
}: ReadyModeProps) {
  // Determine if we're in listening mode with visual feedback
  const showListeningIndicator = autoStartEnabled && isListening;

  return (
    <div className="ready-mode">
      {/* Auto-Detection Toggle */}
      {onAutoStartToggle && (
        <div className="auto-detect-toggle">
          <label className="toggle-label">
            <input
              type="checkbox"
              checked={autoStartEnabled}
              onChange={(e) => onAutoStartToggle(e.target.checked)}
              className="toggle-checkbox"
            />
            <span className="toggle-switch"></span>
            <span className="toggle-text">Auto-detect</span>
          </label>
        </div>
      )}

      {/* Listening Mode Indicator */}
      {showListeningIndicator && (
        <div className="listening-indicator">
          <div
            className={`listening-pulse ${listeningStatus?.analyzing ? 'analyzing' : ''} ${listeningStatus?.speech_detected ? 'speech' : ''}`}
          />
          <span className="listening-label">
            {getListeningLabel(listeningStatus)}
          </span>
        </div>
      )}

      {/* Mic Level Preview - always shown */}
      <div className="mic-preview">
        <div className="mic-level-bar">
          <div
            className="mic-level-fill"
            style={{ width: `${Math.min(100, Math.max(0, audioLevel))}%` }}
          />
        </div>
      </div>

      {/* Start Button - smaller when listening */}
      <button
        className={`start-button-large ready ${showListeningIndicator ? 'start-button-small' : ''}`}
        onClick={onStart}
        aria-label="Start new session"
      >
        <span className="start-label">
          {showListeningIndicator ? 'Start Manually' : 'Start New Session'}
        </span>
      </button>

      {/* Error Messages Only */}
      {errorMessage && (
        <div className="ready-status">
          <div className="permission-error-container">
            <span className="status-error">{errorMessage}</span>
            {isPermissionError && onOpenSettings && (
              <button
                className="open-settings-btn"
                onClick={onOpenSettings}
                aria-label="Open microphone settings"
              >
                Open Settings
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
});

export default ReadyMode;
