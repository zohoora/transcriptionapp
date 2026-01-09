import { memo, useState, useCallback } from 'react';
import type { AudioQualitySnapshot, BiomarkerUpdate } from '../../types';

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

  // State
  isStopping: boolean;

  // Actions
  onStop: () => void;
}

// Get audio quality status (good/fair/poor) for indicator
const getQualityLevel = (quality: AudioQualitySnapshot | null): 'good' | 'fair' | 'poor' => {
  if (!quality) return 'good';

  const rmsOk = quality.rms_db >= -40 && quality.rms_db <= -6;
  const snrOk = quality.snr_db >= 15;
  const clippingOk = quality.clipped_ratio < 0.001;

  if (rmsOk && snrOk && clippingOk) return 'good';
  if (quality.snr_db < 10 || quality.clipped_ratio >= 0.01) return 'poor';
  return 'fair';
};

/**
 * Recording mode UI - minimal distraction while clinician focuses on patient.
 * Shows timer, stop button, and optional transcript preview.
 */
export const RecordingMode = memo(function RecordingMode({
  elapsedMs: _elapsedMs, // Kept for potential future use, not displayed
  audioQuality,
  biomarkers: _biomarkers, // Kept for potential future use (audio events now sent to LLM)
  transcriptText,
  draftText,
  whisperMode,
  whisperModel,
  isStopping,
  onStop,
}: RecordingModeProps) {
  const [showTranscript, setShowTranscript] = useState(false);
  const [showDetails, setShowDetails] = useState(false);

  const qualityLevel = getQualityLevel(audioQuality);

  const handleDetailsClick = useCallback(() => {
    setShowDetails(!showDetails);
  }, [showDetails]);

  return (
    <div className="recording-mode">
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

      {/* Model indicator */}
      <div className="model-indicator">
        {whisperMode === 'remote' ? '🌐' : '💻'} {whisperModel}
      </div>
    </div>
  );
});

export default RecordingMode;
