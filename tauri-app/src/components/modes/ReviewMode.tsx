import { memo, useState, useCallback } from 'react';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import type {
  AudioQualitySnapshot,
  BiomarkerUpdate,
  SoapNote,
  AuthState,
} from '../../types';
import { formatLocalDateTime } from '../../utils';

interface ReviewModeProps {
  // Session info
  elapsedMs: number;
  audioQuality: AudioQualitySnapshot | null;

  // Transcript
  originalTranscript: string;
  editedTranscript: string;
  onTranscriptEdit: (text: string) => void;

  // SOAP note
  soapNote: SoapNote | null;
  isGeneratingSoap: boolean;
  soapError: string | null;
  ollamaConnected: boolean;
  onGenerateSoap: () => void;

  // Biomarkers / Insights
  biomarkers: BiomarkerUpdate | null;

  // Transcription mode info
  whisperMode: 'local' | 'remote';
  whisperModel: string;

  // Sync
  authState: AuthState;
  isSyncing: boolean;
  syncSuccess: boolean;
  syncError: string | null;
  onSync: () => void;
  onClearSyncError: () => void;

  // Actions
  onNewSession: () => void;
  onLogin: () => void;
  onCancelLogin: () => void;
  authLoading: boolean;
  autoSyncEnabled: boolean;
}

// Format duration as mm:ss or h:mm:ss
const formatDuration = (ms: number): string => {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
  }
  return `${minutes}:${seconds.toString().padStart(2, '0')}`;
};

// Get quality badge from audio quality
const getQualityBadge = (quality: AudioQualitySnapshot | null): { label: string; className: string } => {
  if (!quality) return { label: 'Unknown', className: 'quality-unknown' };

  const rmsOk = quality.rms_db >= -40 && quality.rms_db <= -6;
  const snrOk = quality.snr_db >= 15;
  const clippingOk = quality.clipped_ratio < 0.001;

  if (rmsOk && snrOk && clippingOk) return { label: 'Good', className: 'quality-good' };
  if (quality.snr_db < 10 || quality.clipped_ratio >= 0.01) return { label: 'Poor', className: 'quality-poor' };
  return { label: 'Fair', className: 'quality-fair' };
};

/**
 * Review mode UI - shown after recording is complete.
 * Features editable transcript, SOAP note generation, and insights panel.
 */
export const ReviewMode = memo(function ReviewMode({
  elapsedMs,
  audioQuality,
  originalTranscript,
  editedTranscript,
  onTranscriptEdit,
  soapNote,
  isGeneratingSoap,
  soapError,
  ollamaConnected,
  onGenerateSoap,
  biomarkers,
  whisperMode,
  whisperModel,
  authState,
  isSyncing,
  syncSuccess,
  syncError,
  onSync,
  onClearSyncError,
  onNewSession,
  onLogin,
  onCancelLogin,
  authLoading,
  autoSyncEnabled,
}: ReviewModeProps) {
  const [transcriptExpanded, setTranscriptExpanded] = useState(true);
  const [soapExpanded, setSoapExpanded] = useState(true);
  const [insightsExpanded, setInsightsExpanded] = useState(false);
  const [debugExpanded, setDebugExpanded] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [copySuccess, setCopySuccess] = useState(false);

  const qualityBadge = getQualityBadge(audioQuality);
  const hasTranscript = editedTranscript.trim().length > 0;
  const isModified = editedTranscript !== originalTranscript;

  const handleCopyTranscript = useCallback(async () => {
    if (!editedTranscript) return;
    await writeText(editedTranscript);
    setCopySuccess(true);
    setTimeout(() => setCopySuccess(false), 2000);
  }, [editedTranscript]);

  const handleCopySoap = useCallback(async () => {
    if (!soapNote) return;
    const fullNote = `SUBJECTIVE:\n${soapNote.subjective}\n\nOBJECTIVE:\n${soapNote.objective}\n\nASSESSMENT:\n${soapNote.assessment}\n\nPLAN:\n${soapNote.plan}`;
    await writeText(fullNote);
  }, [soapNote]);

  return (
    <div className="review-mode">
      {/* Session Summary Bar */}
      <div className="session-summary">
        <span className="summary-check">&#10003;</span>
        <span className="summary-label">Complete</span>
        <span className="summary-duration">{formatDuration(elapsedMs)}</span>
        <span className={`summary-quality ${qualityBadge.className}`}>{qualityBadge.label}</span>
        <span className="summary-model" title={`Transcription: ${whisperMode === 'remote' ? 'Remote' : 'Local'}`}>
          {whisperMode === 'remote' ? '🌐' : '💻'} {whisperModel}
        </span>
      </div>

      {/* Transcript Section */}
      <section className="review-transcript-section">
        <div className="review-section-header">
          <button
            className="review-section-header-left"
            onClick={() => setTranscriptExpanded(!transcriptExpanded)}
            aria-expanded={transcriptExpanded}
            aria-label={`${transcriptExpanded ? 'Collapse' : 'Expand'} Transcript section`}
          >
            <span className={`chevron ${transcriptExpanded ? '' : 'collapsed'}`}>&#9660;</span>
            <span className="review-section-title">Transcript</span>
            {isModified && <span className="modified-badge">edited</span>}
          </button>
          <div className="review-section-actions">
            {transcriptExpanded && hasTranscript && (
              <>
                <button
                  className={`btn-small ${isEditing ? 'active' : ''}`}
                  onClick={() => setIsEditing(!isEditing)}
                >
                  {isEditing ? 'Done' : 'Edit'}
                </button>
                <button
                  className={`btn-small copy-btn ${copySuccess ? 'success' : ''}`}
                  onClick={handleCopyTranscript}
                >
                  {copySuccess ? 'Copied!' : 'Copy'}
                </button>
              </>
            )}
          </div>
        </div>

        {transcriptExpanded && (
          <div className="review-transcript-content">
            {hasTranscript ? (
              isEditing ? (
                <textarea
                  className="transcript-editor"
                  value={editedTranscript}
                  onChange={(e) => onTranscriptEdit(e.target.value)}
                  placeholder="Edit transcript..."
                />
              ) : (
                <div className="transcript-display">
                  {editedTranscript.split('\n\n').map((paragraph, i) => (
                    <p key={i}>{paragraph}</p>
                  ))}
                </div>
              )
            ) : (
              <div className="transcript-empty">No transcript recorded</div>
            )}
          </div>
        )}
      </section>

      {/* SOAP Note Section */}
      {hasTranscript && (
        <section className="review-soap-section">
          <div className="review-section-header">
            <button
              className="review-section-header-left"
              onClick={() => setSoapExpanded(!soapExpanded)}
              aria-expanded={soapExpanded}
              aria-label={`${soapExpanded ? 'Collapse' : 'Expand'} SOAP Note section`}
            >
              <span className={`chevron ${soapExpanded ? '' : 'collapsed'}`}>&#9660;</span>
              <span className="review-section-title">SOAP Note</span>
            </button>
            {soapNote && soapExpanded && (
              <button
                className="btn-small copy-btn"
                onClick={handleCopySoap}
              >
                Copy
              </button>
            )}
          </div>

          {soapExpanded && (
            <div className="review-soap-content">
              {!soapNote && !isGeneratingSoap && !soapError && (
                <button
                  className="btn-generate"
                  onClick={onGenerateSoap}
                  disabled={!ollamaConnected}
                >
                  {ollamaConnected ? 'Generate SOAP Note' : 'Ollama not connected'}
                </button>
              )}

              {isGeneratingSoap && (
                <div className="soap-loading">
                  <div className="spinner-small" />
                  <span>Generating SOAP note...</span>
                </div>
              )}

              {soapError && (
                <div className="soap-error">
                  <span>{soapError}</span>
                  <button className="btn-retry-small" onClick={onGenerateSoap}>
                    Retry
                  </button>
                </div>
              )}

              {soapNote && (
                <div className="soap-display">
                  <div className="soap-item">
                    <div className="soap-label">SUBJECTIVE</div>
                    <div className="soap-text">{soapNote.subjective}</div>
                  </div>
                  <div className="soap-item">
                    <div className="soap-label">OBJECTIVE</div>
                    <div className="soap-text">{soapNote.objective}</div>
                  </div>
                  <div className="soap-item">
                    <div className="soap-label">ASSESSMENT</div>
                    <div className="soap-text">{soapNote.assessment}</div>
                  </div>
                  <div className="soap-item">
                    <div className="soap-label">PLAN</div>
                    <div className="soap-text">{soapNote.plan}</div>
                  </div>
                  <div className="soap-footer">
                    Generated {formatLocalDateTime(soapNote.generated_at)} ({soapNote.model_used})
                  </div>

                  {/* Debug: Raw model response (collapsible) */}
                  {soapNote.raw_response && (
                    <div className="soap-debug">
                      <button
                        className="soap-debug-toggle"
                        onClick={() => setDebugExpanded(!debugExpanded)}
                      >
                        <span className={`chevron-small ${debugExpanded ? '' : 'collapsed'}`}>&#9660;</span>
                        Raw Response
                      </button>
                      {debugExpanded && (
                        <pre className="soap-debug-content">
                          {soapNote.raw_response}
                        </pre>
                      )}
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {/* Session Insights (collapsed by default) */}
      {biomarkers && (
        <section className="review-insights-section">
          <div className="review-section-header">
            <button
              className="review-section-header-left"
              onClick={() => setInsightsExpanded(!insightsExpanded)}
              aria-expanded={insightsExpanded}
              aria-label={`${insightsExpanded ? 'Collapse' : 'Expand'} Session Insights section`}
            >
              <span className={`chevron ${insightsExpanded ? '' : 'collapsed'}`}>&#9660;</span>
              <span className="review-section-title">Session Insights</span>
            </button>
            <div className="insights-summary">
              <span className={`summary-quality ${qualityBadge.className}`}>
                {qualityBadge.label}
              </span>
              {biomarkers.turn_count > 1 && (
                <span className="summary-speakers">{biomarkers.speaker_metrics.length || 2} speakers</span>
              )}
            </div>
          </div>

          {insightsExpanded && (
            <div className="review-insights-content">
              {/* Audio Quality */}
              {audioQuality && (
                <div className="insight-group">
                  <div className="insight-label">Audio Quality</div>
                  <div className="insight-row">
                    <span>Level: {audioQuality.rms_db.toFixed(0)} dB</span>
                    <span>SNR: {audioQuality.snr_db.toFixed(0)} dB</span>
                  </div>
                  {audioQuality.total_clipped > 0 && (
                    <div className="insight-warning">Clipped samples: {audioQuality.total_clipped}</div>
                  )}
                </div>
              )}

              {/* Speaker Metrics */}
              {biomarkers.speaker_metrics.length > 0 && (
                <div className="insight-group">
                  <div className="insight-label">Speakers</div>
                  {biomarkers.speaker_metrics.map((speaker) => (
                    <div key={speaker.speaker_id} className="insight-row">
                      <span>{speaker.speaker_id}</span>
                      <span>{speaker.turn_count} turns</span>
                      <span>{Math.round(speaker.talk_time_ms / 1000)}s</span>
                    </div>
                  ))}
                </div>
              )}

              {/* Conversation Dynamics */}
              {biomarkers.conversation_dynamics && (
                <div className="insight-group">
                  <div className="insight-label">Conversation</div>
                  <div className="insight-row">
                    <span>Response: {Math.round(biomarkers.conversation_dynamics.mean_response_latency_ms)}ms</span>
                    {biomarkers.conversation_dynamics.engagement_score !== null && (
                      <span>Engagement: {Math.round(biomarkers.conversation_dynamics.engagement_score)}</span>
                    )}
                  </div>
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {/* Sync Login Banner (if auto-sync enabled but not authenticated) */}
      {autoSyncEnabled && !authState.is_authenticated && (
        <div className="sync-login-banner">
          <span className="sync-login-message">Sign in to sync this session to Medplum</span>
          <div className="sync-login-actions">
            <button
              className="btn-signin-small"
              onClick={onLogin}
              disabled={authLoading}
            >
              {authLoading ? 'Signing in...' : 'Sign In'}
            </button>
            {authLoading && (
              <button
                className="btn-cancel-small"
                onClick={onCancelLogin}
              >
                Cancel
              </button>
            )}
          </div>
        </div>
      )}

      {/* Sync Error Toast */}
      {syncError && (
        <div className="sync-error-toast">
          <span>{syncError}</span>
          <button onClick={onClearSyncError}>&times;</button>
        </div>
      )}

      {/* Action Bar */}
      <div className="action-bar">
        {authState.is_authenticated && hasTranscript && (
          <button
            className="btn-secondary"
            onClick={onSync}
            disabled={isSyncing || syncSuccess}
          >
            {isSyncing ? 'Syncing...' : syncSuccess ? '\u2713 Synced' : 'Sync to Medplum'}
          </button>
        )}
        <button className="btn-primary" onClick={onNewSession}>
          New Session
        </button>
      </div>
    </div>
  );
});

export default ReviewMode;
