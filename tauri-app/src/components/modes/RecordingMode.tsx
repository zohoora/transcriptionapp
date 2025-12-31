import { memo, useState, useCallback } from 'react';
import type { AudioQualitySnapshot, BiomarkerUpdate } from '../../types';

interface RecordingModeProps {
  // Timer
  elapsedMs: number;

  // Audio quality for status bars
  audioQuality: AudioQualitySnapshot | null;

  // Optional: biomarkers for tap-to-reveal
  biomarkers: BiomarkerUpdate | null;

  // Transcript preview (optional toggle)
  transcriptText: string;
  draftText: string | null;

  // State
  isStopping: boolean;

  // Actions
  onStop: () => void;
}

// Format elapsed time as HH:MM:SS
const formatTime = (ms: number): string => {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
  }
  return `${minutes}:${seconds.toString().padStart(2, '0')}`;
};

// Get audio quality status (good/fair/poor) for status bars
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
  elapsedMs,
  audioQuality,
  biomarkers,
  transcriptText,
  draftText,
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
      {/* Recording indicator in header area */}
      <div className="recording-indicator">
        <span className="rec-dot" />
        <span className="rec-text">REC</span>
        <div className="spacer" />
        {/* Status bars - tap to reveal details */}
        <button
          className={`status-bars ${qualityLevel}`}
          onClick={handleDetailsClick}
          aria-label="Audio quality status - tap for details"
        >
          <span className="bar" />
          <span className="bar" />
          <span className="bar" />
        </button>
      </div>

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
          {biomarkers && biomarkers.cough_count > 0 && (
            <div className="detail-row">
              <span className="detail-label">Coughs</span>
              <span className="detail-value">{biomarkers.cough_count}</span>
            </div>
          )}
        </div>
      )}

      {/* Large Timer */}
      <div className="timer-large">
        {formatTime(elapsedMs)}
      </div>

      {/* Stop Button */}
      <button
        className={`stop-button ${isStopping ? 'stopping' : ''}`}
        onClick={onStop}
        disabled={isStopping}
        aria-label={isStopping ? 'Stopping...' : 'Stop recording'}
      >
        <span className="stop-icon" />
        <span className="stop-label">{isStopping ? 'Stopping...' : 'STOP'}</span>
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
    </div>
  );
});

export default RecordingMode;
