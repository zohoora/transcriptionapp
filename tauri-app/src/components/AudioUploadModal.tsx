import { memo, useCallback } from 'react';
import type { AudioUploadProgress, AudioUploadResult } from '../types';

interface AudioUploadModalProps {
  filePath: string | null;
  fileName: string | null;
  recordingDate: string;
  isProcessing: boolean;
  progress: AudioUploadProgress | null;
  result: AudioUploadResult | null;
  error: string | null;
  onSelectFile: () => void;
  onSetDate: (date: string) => void;
  onStartProcessing: () => void;
  onReset: () => void;
  onClose: () => void;
  onViewHistory?: (date: string) => void;
}

const STEP_LABELS: Record<string, string> = {
  transcoding: 'Converting audio...',
  transcribing: 'Transcribing speech...',
  detecting: 'Detecting encounters...',
  generating_soap: 'Generating SOAP notes...',
  complete: 'Processing complete',
  failed: 'Processing failed',
};

export const AudioUploadModal = memo(function AudioUploadModal({
  filePath,
  fileName,
  recordingDate,
  isProcessing,
  progress,
  result,
  error,
  onSelectFile,
  onSetDate,
  onStartProcessing,
  onReset,
  onClose,
  onViewHistory,
}: AudioUploadModalProps) {
  const handleOverlayClick = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === e.currentTarget && !isProcessing) {
        onClose();
      }
    },
    [isProcessing, onClose]
  );

  const handleViewHistory = useCallback(() => {
    onViewHistory?.(recordingDate);
    onClose();
  }, [onViewHistory, recordingDate, onClose]);

  const handleTryAgain = useCallback(() => {
    onReset();
  }, [onReset]);

  // Determine which view to show
  const showResult = result && !error;
  const showError = !!error && !isProcessing;
  const showProcessing = isProcessing;
  const showSelect = !showResult && !showError && !showProcessing;

  return (
    <div className="audio-upload-overlay" onClick={handleOverlayClick}>
      <div className="audio-upload-modal" onClick={(e) => e.stopPropagation()}>
        <div className="audio-upload-header">
          <h3>Upload Recording</h3>
          {!isProcessing && (
            <button
              className="audio-upload-close"
              onClick={onClose}
              aria-label="Close"
            >
              &times;
            </button>
          )}
        </div>

        {/* ── Select state ─────────────────────────────────────── */}
        {showSelect && (
          <div className="audio-upload-body">
            <div className="audio-upload-field">
              <label className="audio-upload-label">Audio file</label>
              <button className="audio-upload-file-btn" onClick={onSelectFile}>
                {fileName || 'Select file...'}
              </button>
              {fileName && (
                <span className="audio-upload-file-info">{fileName}</span>
              )}
            </div>

            <div className="audio-upload-field">
              <label className="audio-upload-label">Recording date</label>
              <input
                type="date"
                className="audio-upload-date"
                value={recordingDate}
                onChange={(e) => onSetDate(e.target.value)}
                max={new Date().toISOString().split('T')[0]}
              />
            </div>

            <div className="audio-upload-hint">
              Supported: MP3, WAV, M4A, AAC, FLAC, OGG, WMA, WebM
            </div>

            <button
              className="audio-upload-process-btn"
              onClick={onStartProcessing}
              disabled={!filePath || isProcessing}
            >
              Process Recording
            </button>
          </div>
        )}

        {/* ── Processing state ─────────────────────────────────── */}
        {showProcessing && progress && (
          <div className="audio-upload-body">
            <div className="audio-upload-progress">
              <div className="audio-upload-spinner" />
              <span className="audio-upload-step">
                {STEP_LABELS[progress.step] || progress.step}
              </span>
              {progress.step === 'generating_soap' &&
                progress.encounter != null &&
                progress.total != null && (
                  <span className="audio-upload-step-detail">
                    Encounter {progress.encounter} of {progress.total}
                  </span>
                )}
            </div>
          </div>
        )}

        {/* ── Complete state ───────────────────────────────────── */}
        {showResult && result && (
          <div className="audio-upload-body">
            <div className="audio-upload-result">
              <div className="audio-upload-result-icon">&#10003;</div>
              <div className="audio-upload-result-summary">
                {result.sessions.length} encounter{result.sessions.length !== 1 ? 's' : ''} created
                <span className="audio-upload-result-words">
                  {result.totalWordCount.toLocaleString()} words total
                </span>
              </div>
              {result.sessions.map((s) => (
                <div key={s.sessionId} className="audio-upload-session-item">
                  <span>Encounter #{s.encounterNumber}</span>
                  <span className="audio-upload-session-words">
                    {s.wordCount.toLocaleString()} words
                  </span>
                  <span className={`audio-upload-soap-badge ${s.hasSoap ? 'has-soap' : 'no-soap'}`}>
                    {s.hasSoap ? 'SOAP' : 'No SOAP'}
                  </span>
                </div>
              ))}
            </div>

            <div className="audio-upload-actions">
              {onViewHistory && (
                <button
                  className="audio-upload-history-btn"
                  onClick={handleViewHistory}
                >
                  View in History
                </button>
              )}
              <button className="audio-upload-done-btn" onClick={onClose}>
                Done
              </button>
            </div>
          </div>
        )}

        {/* ── Error state ──────────────────────────────────────── */}
        {showError && (
          <div className="audio-upload-body">
            <div className="audio-upload-error">
              <div className="audio-upload-error-icon">!</div>
              <div className="audio-upload-error-message">{error}</div>
            </div>
            <button className="audio-upload-retry-btn" onClick={handleTryAgain}>
              Try Again
            </button>
          </div>
        )}
      </div>
    </div>
  );
});

export default AudioUploadModal;
