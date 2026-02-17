import { memo, useState, useCallback } from 'react';
import type { AudioQualitySnapshot, BiomarkerUpdate, SilenceWarningPayload } from '../../types';
import type { ChatMessage } from '../../hooks/useClinicalChat';
import type { MiisSuggestion } from '../../hooks/useMiisImages';
import { getAudioQualityLevel } from '../../utils';
import { ClinicalChat, MarkdownContent } from '../ClinicalChat';
import { ImageSuggestions } from '../ImageSuggestions';
import { PatientPulse } from '../PatientPulse';

interface RecordingModeProps {
  // Timer (kept for potential future use, not displayed)
  elapsedMs: number;

  // Audio quality for status indicator
  audioQuality: AudioQualitySnapshot | null;

  // Optional: biomarkers for tap-to-reveal
  biomarkers: BiomarkerUpdate | null;

  // Transcript preview (optional toggle)
  transcriptText: string;
  draftText: string | null;

  // Transcription mode info
  whisperMode: 'local' | 'remote';
  whisperModel: string;

  // Session notes (clinician observations during recording)
  sessionNotes: string;
  onSessionNotesChange: (notes: string) => void;

  // State
  isStopping: boolean;

  // Silence warning for auto-end countdown
  silenceWarning: SilenceWarningPayload | null;

  // Clinical chat props
  chatMessages: ChatMessage[];
  chatIsLoading: boolean;
  chatError: string | null;
  onChatSendMessage: (content: string) => void;
  onChatClear: () => void;

  // Predictive hint
  predictiveHint: string;
  predictiveHintLoading: boolean;

  // MIIS (Medical Illustration Image Server) suggestions
  miisSuggestions: MiisSuggestion[];
  miisLoading: boolean;
  miisError: string | null;
  miisEnabled: boolean;
  onMiisImpression: (imageId: number) => void;
  onMiisClick: (imageId: number) => void;
  onMiisDismiss: (imageId: number) => void;
  miisGetImageUrl: (path: string) => string;

  // Auto-end toggle
  autoEndEnabled: boolean;
  onAutoEndToggle: (enabled: boolean) => void;

  // Actions
  onStop: () => void;
  onCancelAutoEnd?: () => void;
}

/**
 * Recording mode UI - minimal distraction while clinician focuses on patient.
 * Shows timer, stop button, and optional transcript preview.
 */
export const RecordingMode = memo(function RecordingMode({
  elapsedMs: _elapsedMs, // Kept for potential future use, not displayed
  audioQuality,
  biomarkers,
  transcriptText,
  draftText,
  whisperMode,
  whisperModel,
  sessionNotes,
  onSessionNotesChange,
  isStopping,
  silenceWarning,
  chatMessages,
  chatIsLoading,
  chatError,
  onChatSendMessage,
  onChatClear,
  predictiveHint,
  predictiveHintLoading,
  miisSuggestions,
  miisLoading,
  miisError,
  miisEnabled,
  onMiisImpression,
  onMiisClick,
  onMiisDismiss,
  miisGetImageUrl,
  autoEndEnabled,
  onAutoEndToggle,
  onStop,
  onCancelAutoEnd,
}: RecordingModeProps) {
  const [showTranscript, setShowTranscript] = useState(false);
  const [showDetails, setShowDetails] = useState(false);
  const [showNotes, setShowNotes] = useState(false);
  const [chatExpanded, setChatExpanded] = useState(false);

  const qualityLevel = getAudioQualityLevel(audioQuality);

  const handleDetailsClick = useCallback(() => {
    setShowDetails(prev => !prev);
  }, []);

  const handleNotesToggle = useCallback(() => {
    setShowNotes(prev => !prev);
  }, []);

  const handleNotesChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    onSessionNotesChange(e.target.value);
  }, [onSessionNotesChange]);

  // Format remaining time for display
  const formatRemainingTime = (ms: number): string => {
    const seconds = Math.max(0, Math.ceil(ms / 1000));
    return `${seconds}s`;
  };

  return (
    <div className="recording-mode">
      {/* Silence warning countdown overlay */}
      {silenceWarning && silenceWarning.remaining_ms > 0 && (
        <div className="silence-warning-overlay">
          <div className="silence-warning-content">
            <div className="silence-warning-icon">⏱️</div>
            <div className="silence-warning-text">
              No speech detected
            </div>
            <div className="silence-warning-countdown">
              Ending in {formatRemainingTime(silenceWarning.remaining_ms)}
            </div>
            {onCancelAutoEnd && (
              <button
                className="silence-warning-cancel"
                onClick={onCancelAutoEnd}
              >
                Keep Recording
              </button>
            )}
            <button
              className="silence-warning-end-now"
              onClick={onStop}
            >
              End Now
            </button>
            <div className="silence-warning-hint">
              Or speak to continue
            </div>
          </div>
        </div>
      )}

      {/* Auto-end toggle */}
      <div className="recording-auto-end-toggle">
        <label className="toggle-label compact">
          <input
            type="checkbox"
            checked={autoEndEnabled}
            onChange={(e) => onAutoEndToggle(e.target.checked)}
            className="toggle-checkbox"
          />
          <span className="toggle-switch"></span>
          <span className="toggle-text">Auto-end</span>
        </label>
      </div>

      {/* Predictive hint - "Pssst..." section */}
      {(predictiveHint || predictiveHintLoading) && (
        <div className="predictive-hint-container">
          <div className="predictive-hint-label">Pssst...</div>
          <div className="predictive-hint-content">
            {predictiveHintLoading ? (
              <span className="predictive-hint-loading">Thinking...</span>
            ) : (
              <MarkdownContent content={predictiveHint} className="predictive-hint-markdown" />
            )}
          </div>
        </div>
      )}

      {/* MIIS Image Suggestions */}
      {miisEnabled && (
        <ImageSuggestions
          suggestions={miisSuggestions}
          isLoading={miisLoading}
          error={miisError}
          getImageUrl={miisGetImageUrl}
          onImpression={onMiisImpression}
          onClickImage={onMiisClick}
          onDismiss={onMiisDismiss}
        />
      )}

      {/* Session in progress indicator */}
      <div className="session-progress">
        <div className="session-progress-indicator">
          <span className="pulse-dot" />
          <span className="pulse-dot" />
          <span className="pulse-dot" />
        </div>
        <div className="session-progress-text">Session in progress...</div>
      </div>

      {/* Audio quality indicator - subtle, tap for details */}
      <button
        className={`quality-indicator ${qualityLevel}`}
        onClick={handleDetailsClick}
        aria-label="Audio quality - tap for details"
      >
        <span className="quality-dot" />
        <span className="quality-label">
          {qualityLevel === 'good' ? 'Good audio' : qualityLevel === 'fair' ? 'Fair audio' : 'Poor audio'}
        </span>
      </button>

      {/* Details popover (tap to reveal) */}
      {showDetails && audioQuality && (
        <div className="recording-details-popover">
          <div className="detail-row">
            <span className="detail-label">Level</span>
            <span className="detail-value">{audioQuality.rms_db.toFixed(0)} dB</span>
          </div>
          <div className="detail-row">
            <span className="detail-label">SNR</span>
            <span className="detail-value">{audioQuality.snr_db.toFixed(0)} dB</span>
          </div>
          {audioQuality.total_clipped > 0 && (
            <div className="detail-row warning">
              <span className="detail-label">Clips</span>
              <span className="detail-value">{audioQuality.total_clipped}</span>
            </div>
          )}
        </div>
      )}

      {/* Patient voice pulse indicator */}
      <PatientPulse biomarkers={biomarkers} />

      {/* Session Notes Toggle & Input */}
      <button
        className={`notes-toggle ${showNotes ? 'active' : ''} ${sessionNotes.trim() ? 'has-notes' : ''}`}
        onClick={handleNotesToggle}
        aria-label={showNotes ? 'Hide notes' : 'Add notes'}
        aria-expanded={showNotes}
      >
        <span className="notes-icon">📝</span>
        <span className="notes-label">{showNotes ? 'Hide Notes' : 'Add Notes'}</span>
        <span className="notes-chevron">{showNotes ? '▲' : '▼'}</span>
      </button>

      {showNotes && (
        <div className="session-notes-container">
          <textarea
            className="session-notes-input"
            placeholder="Enter observations, procedures, or notes for this session..."
            value={sessionNotes}
            onChange={handleNotesChange}
            rows={3}
            aria-label="Session notes"
          />
        </div>
      )}

      {/* End Session Button */}
      <button
        className={`stop-button ${isStopping ? 'stopping' : ''}`}
        onClick={onStop}
        disabled={isStopping}
        aria-label={isStopping ? 'Ending session...' : 'End session'}
      >
        <span className="stop-icon" />
        <span className="stop-label">{isStopping ? 'Ending...' : 'End Session'}</span>
      </button>

      {/* Transcript Toggle */}
      <button
        className={`transcript-toggle ${showTranscript ? 'active' : ''}`}
        onClick={() => setShowTranscript(!showTranscript)}
      >
        {showTranscript ? 'Hide Transcript' : 'Show Transcript'}
      </button>

      {/* Floating Transcript Preview */}
      {showTranscript && (
        <div className="transcript-preview">
          {transcriptText ? (
            <>
              <div className="transcript-preview-text">{transcriptText}</div>
              {draftText && (
                <div className="transcript-preview-draft">{draftText}</div>
              )}
            </>
          ) : (
            <div className="transcript-preview-placeholder">Listening...</div>
          )}
        </div>
      )}

      {/* Clinical Assistant Chat */}
      <ClinicalChat
        messages={chatMessages}
        isLoading={chatIsLoading}
        error={chatError}
        onSendMessage={onChatSendMessage}
        onClear={onChatClear}
        isExpanded={chatExpanded}
        onToggleExpand={() => setChatExpanded(!chatExpanded)}
      />

      {/* Model indicator */}
      <div className="model-indicator">
        {whisperMode === 'remote' ? '🌐' : '💻'} {whisperModel}
      </div>
    </div>
  );
});

export default RecordingMode;
