import { memo, useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { SyncStatusBar } from '../SyncStatusBar';
import type {
  AudioQualitySnapshot,
  BiomarkerUpdate,
  AuthState,
  SoapOptions,
  SoapFormat,
  SyncedEncounter,
  MultiPatientSoapResult,
} from '../../types';
import {
  DETAIL_LEVEL_LABELS,
  getVitalityStatus,
  getStabilityStatus,
  getEngagementStatus,
  getResponseTimeStatus,
} from '../../types';
import { formatLocalDateTime, getAudioQualityLevel, formatDurationClock } from '../../utils';

type ReviewTab = 'transcript' | 'soap' | 'insights';

interface ReviewModeProps {
  // Session info
  elapsedMs: number;
  audioQuality: AudioQualitySnapshot | null;

  // Transcript
  originalTranscript: string;
  editedTranscript: string;
  onTranscriptEdit: (text: string) => void;

  // SOAP note (multi-patient result)
  soapResult: MultiPatientSoapResult | null;
  isGeneratingSoap: boolean;
  soapError: string | null;
  llmConnected: boolean;
  onGenerateSoap: () => void;

  // SOAP options
  soapOptions: SoapOptions;
  onSoapDetailLevelChange: (level: number) => void;
  onSoapFormatChange: (format: SoapFormat) => void;
  onSoapCustomInstructionsChange: (instructions: string) => void;

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
  syncedEncounter: SyncedEncounter | null;
  isAddingSoap: boolean;
  onClearSyncError: () => void;

  // Vision SOAP (experimental)
  onGenerateVisionSoap?: (imagePath: string) => void;
  screenshotCount?: number;

  // Actions
  onNewSession: () => void;
  onLogin: () => void;
  onCancelLogin: () => void;
  authLoading: boolean;
  autoSyncEnabled: boolean;
}

// Format duration as mm:ss or h:mm:ss (uses shared utility)
const formatDuration = formatDurationClock;

// Get quality badge from audio quality (thin wrapper over shared utility)
const getQualityBadge = (quality: AudioQualitySnapshot | null): { label: string; className: string } => {
  if (!quality) return { label: 'Unknown', className: 'quality-unknown' };

  const level = getAudioQualityLevel(quality);
  const labelMap = { good: 'Good', fair: 'Fair', poor: 'Poor' } as const;
  return { label: labelMap[level], className: `quality-${level}` };
};

/**
 * Review mode UI - shown after recording is complete.
 * Tab-based layout: Transcript | SOAP | Insights
 */
export const ReviewMode = memo(function ReviewMode({
  elapsedMs,
  audioQuality,
  originalTranscript,
  editedTranscript,
  onTranscriptEdit,
  soapResult,
  isGeneratingSoap,
  soapError,
  llmConnected,
  onGenerateSoap,
  soapOptions,
  onSoapDetailLevelChange,
  onSoapFormatChange,
  onSoapCustomInstructionsChange,
  biomarkers,
  whisperMode,
  whisperModel,
  authState,
  isSyncing,
  syncSuccess,
  syncError,
  syncedEncounter,
  isAddingSoap,
  onGenerateVisionSoap,
  screenshotCount = 0,
  onClearSyncError,
  onNewSession,
  onLogin,
  onCancelLogin,
  authLoading,
  autoSyncEnabled,
}: ReviewModeProps) {
  // Default to SOAP tab since we auto-generate
  const [activeTab, setActiveTab] = useState<ReviewTab>('soap');
  const [isEditing, setIsEditing] = useState(false);
  const [copySuccess, setCopySuccess] = useState(false);
  const [soapCopySuccess, setSoapCopySuccess] = useState(false);
  const [customInstructionsExpanded, setCustomInstructionsExpanded] = useState(false);
  // Active patient tab for multi-patient SOAP notes
  const [activePatient, setActivePatient] = useState(0);

  // Vision thumbnail picker state
  const [showVisionPicker, setShowVisionPicker] = useState(false);
  const [visionThumbnails, setVisionThumbnails] = useState<Array<{ path: string; data_url: string; label: string }>>([]);
  const [loadingThumbnails, setLoadingThumbnails] = useState(false);

  // Auto-switch to SOAP tab when generation starts
  useEffect(() => {
    if (isGeneratingSoap) {
      setActiveTab('soap');
    }
  }, [isGeneratingSoap]);

  // Reset activePatient when soapResult changes and index is out of bounds
  useEffect(() => {
    if (soapResult && activePatient >= soapResult.notes.length) {
      setActivePatient(0);
    }
  }, [soapResult, activePatient]);

  const qualityBadge = getQualityBadge(audioQuality);
  const hasTranscript = editedTranscript.trim().length > 0;
  const isModified = editedTranscript !== originalTranscript;

  const handleCopyTranscript = useCallback(async () => {
    if (!editedTranscript) return;
    try {
      await writeText(editedTranscript);
      setCopySuccess(true);
      setTimeout(() => setCopySuccess(false), 2000);
    } catch (e) {
      console.error('Failed to copy transcript:', e);
    }
  }, [editedTranscript]);

  // Get active patient's SOAP note content (safe bounds check)
  const safeActivePatient = soapResult && activePatient < soapResult.notes.length ? activePatient : 0;
  const activeSoapContent = soapResult?.notes[safeActivePatient]?.content ?? null;
  const isMultiPatient = (soapResult?.notes.length ?? 0) > 1;

  const handleCopySoap = useCallback(async () => {
    if (!activeSoapContent) return;
    try {
      await writeText(activeSoapContent);
      setSoapCopySuccess(true);
      setTimeout(() => setSoapCopySuccess(false), 2000);
    } catch (e) {
      console.error('Failed to copy SOAP note:', e);
    }
  }, [activeSoapContent]);

  // Open vision picker: fetch thumbnails and show grid
  const handleOpenVisionPicker = useCallback(async () => {
    setLoadingThumbnails(true);
    setShowVisionPicker(true);
    try {
      const thumbs = await invoke<Array<{ path: string; data_url: string; label: string }>>('get_screenshot_thumbnails');
      setVisionThumbnails(thumbs);
    } catch (e) {
      console.error('Failed to fetch thumbnails:', e);
      setVisionThumbnails([]);
    } finally {
      setLoadingThumbnails(false);
    }
  }, []);

  // Select a thumbnail and trigger vision SOAP generation
  const handleThumbnailSelect = useCallback((path: string) => {
    setShowVisionPicker(false);
    onGenerateVisionSoap?.(path);
  }, [onGenerateVisionSoap]);

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

      {/* Tab Navigation */}
      <div className="review-tabs">
        <button
          className={`review-tab ${activeTab === 'transcript' ? 'active' : ''}`}
          onClick={() => setActiveTab('transcript')}
        >
          Transcript
          {isModified && <span className="tab-badge">edited</span>}
        </button>
        <button
          className={`review-tab ${activeTab === 'soap' ? 'active' : ''}`}
          onClick={() => setActiveTab('soap')}
          disabled={!hasTranscript}
        >
          SOAP
          {soapResult && <span className="tab-badge done">✓</span>}
        </button>
        <button
          className={`review-tab ${activeTab === 'insights' ? 'active' : ''}`}
          onClick={() => setActiveTab('insights')}
        >
          Insights
        </button>
      </div>

      {/* Tab Content */}
      <div className="review-tab-content">
        {/* Transcript Tab */}
        {activeTab === 'transcript' && (
          <div className="tab-panel transcript-panel">
            <div className="panel-header">
              <div className="panel-actions">
                {hasTranscript && (
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

            <div className="panel-body">
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
                <div className="panel-empty">No transcript recorded</div>
              )}
            </div>
          </div>
        )}

        {/* SOAP Tab */}
        {activeTab === 'soap' && (
          <div className="tab-panel soap-panel">
            {/* SOAP Options */}
            {!isGeneratingSoap && (
              <div className="soap-options">
                {/* Detail Level Slider */}
                <div className="soap-option-row">
                  <label className="soap-option-label">
                    Detail: {DETAIL_LEVEL_LABELS[soapOptions.detail_level]?.name || 'Standard'}
                  </label>
                  <div className="soap-detail-slider">
                    <input
                      type="range"
                      min="1"
                      max="10"
                      value={soapOptions.detail_level}
                      onChange={(e) => onSoapDetailLevelChange(Number(e.target.value) || 5)}
                      className="detail-slider"
                      aria-label="SOAP note detail level"
                    />
                    <span className="detail-value">{soapOptions.detail_level}</span>
                  </div>
                </div>

                {/* Format Toggle */}
                <div className="soap-option-row">
                  <label className="soap-option-label">Format</label>
                  <div className="soap-format-toggle">
                    <button
                      className={`format-btn ${soapOptions.format === 'problem_based' ? 'active' : ''}`}
                      onClick={() => onSoapFormatChange('problem_based')}
                    >
                      Problem
                    </button>
                    <button
                      className={`format-btn ${soapOptions.format === 'comprehensive' ? 'active' : ''}`}
                      onClick={() => onSoapFormatChange('comprehensive')}
                    >
                      Comprehensive
                    </button>
                  </div>
                </div>

                {/* Custom Instructions */}
                <div className="soap-option-row custom-instructions">
                  <button
                    className="custom-instructions-toggle"
                    onClick={() => setCustomInstructionsExpanded(!customInstructionsExpanded)}
                  >
                    <span className={`chevron-small ${customInstructionsExpanded ? '' : 'collapsed'}`}>&#9660;</span>
                    Custom Instructions
                    {soapOptions.custom_instructions.trim() && (
                      <span className="custom-badge">Active</span>
                    )}
                  </button>
                  {customInstructionsExpanded && (
                    <textarea
                      className="custom-instructions-input"
                      value={soapOptions.custom_instructions}
                      onChange={(e) => onSoapCustomInstructionsChange(e.target.value)}
                      placeholder="Add specific instructions..."
                      rows={3}
                    />
                  )}
                </div>
              </div>
            )}

            {/* Generate Buttons */}
            {!soapResult && !isGeneratingSoap && !soapError && (
              <div className="soap-generate-actions">
                <button
                  className="btn-generate"
                  onClick={onGenerateSoap}
                  disabled={!llmConnected}
                >
                  {llmConnected ? 'Generate SOAP Note' : 'LLM not connected'}
                </button>
                {onGenerateVisionSoap && (
                  <button
                    className="btn-generate btn-vision"
                    onClick={handleOpenVisionPicker}
                    disabled={!llmConnected || screenshotCount === 0}
                    title={screenshotCount === 0 ? 'No screenshots captured' : `${screenshotCount} screenshots available`}
                  >
                    Vision SOAP
                  </button>
                )}
              </div>
            )}

            {/* Vision Thumbnail Picker */}
            {showVisionPicker && (
              <div className="vision-picker">
                <div className="vision-picker-header">
                  <span className="vision-picker-title">Select a screenshot</span>
                  <button className="btn-link" onClick={() => setShowVisionPicker(false)}>Cancel</button>
                </div>
                {loadingThumbnails ? (
                  <div className="soap-loading">
                    <div className="spinner-small" />
                    <span>Loading thumbnails...</span>
                  </div>
                ) : visionThumbnails.length === 0 ? (
                  <div className="panel-empty">No thumbnails available</div>
                ) : (
                  <div className="vision-picker-grid">
                    {visionThumbnails.map((thumb, i) => (
                      <button
                        key={i}
                        className="vision-thumb-btn"
                        onClick={() => handleThumbnailSelect(thumb.path)}
                        title={thumb.label}
                      >
                        <img src={thumb.data_url} alt={thumb.label} className="vision-thumb-img" />
                        <span className="vision-thumb-label">{thumb.label}</span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}

            {/* Loading State */}
            {isGeneratingSoap && (
              <div className="soap-loading">
                <div className="spinner-small" />
                <span>Generating SOAP note...</span>
              </div>
            )}

            {/* Error State */}
            {soapError && (
              <div className="soap-error">
                <span>{soapError}</span>
                <button className="btn-retry-small" onClick={onGenerateSoap}>
                  Retry
                </button>
              </div>
            )}

            {/* SOAP Display */}
            {soapResult && activeSoapContent && (
              <div className="soap-display">
                <div className="soap-header">
                  <span className="soap-timestamp">
                    Generated {formatLocalDateTime(soapResult.generated_at)}
                  </span>
                  <div className="soap-actions">
                    <button
                      className={`btn-small copy-btn ${soapCopySuccess ? 'success' : ''}`}
                      onClick={handleCopySoap}
                    >
                      {soapCopySuccess ? 'Copied!' : 'Copy'}
                    </button>
                    <button
                      className="btn-small"
                      onClick={onGenerateSoap}
                      disabled={isGeneratingSoap}
                    >
                      Regenerate
                    </button>
                    {onGenerateVisionSoap && screenshotCount > 0 && (
                      <button
                        className="btn-small btn-vision"
                        onClick={handleOpenVisionPicker}
                        disabled={isGeneratingSoap || !llmConnected}
                        title={`${screenshotCount} screenshots available`}
                      >
                        Vision
                      </button>
                    )}
                  </div>
                </div>

                {/* Multi-patient info and tabs */}
                {isMultiPatient && (
                  <div className="multi-patient-soap">
                    <div className="patient-info">
                      <span className="physician-label">
                        Physician: {soapResult.physician_speaker || 'Not identified'}
                      </span>
                      <span className="patient-count">
                        {soapResult.notes.length} patients detected
                      </span>
                    </div>
                    <div className="patient-tabs">
                      {soapResult.notes.map((note, i) => (
                        <button
                          key={i}
                          className={`patient-tab ${activePatient === i ? 'active' : ''}`}
                          onClick={() => setActivePatient(i)}
                        >
                          {note.patient_label}
                          <span className="speaker-id">({note.speaker_id})</span>
                        </button>
                      ))}
                    </div>
                  </div>
                )}

                <div className="soap-content">
                  <pre className="soap-text-content">{activeSoapContent}</pre>
                </div>

                <div className="soap-meta">
                  <span className="soap-model">Model: {soapResult.model_used}</span>
                </div>
              </div>
            )}
          </div>
        )}

        {/* Insights Tab */}
        {activeTab === 'insights' && (
          <div className="tab-panel insights-panel">
            {/* Audio Quality */}
            {audioQuality != null && (
              <div className="insight-card">
                <div className="insight-card-header">Audio Quality</div>
                <div className="insight-card-body">
                  <div className="insight-metric">
                    <span className="metric-label">Level</span>
                    <span className="metric-value">{audioQuality?.rms_db?.toFixed(0) ?? '—'} dB</span>
                  </div>
                  <div className="insight-metric">
                    <span className="metric-label">SNR</span>
                    <span className="metric-value">{audioQuality?.snr_db?.toFixed(0) ?? '—'} dB</span>
                  </div>
                  {(audioQuality?.total_clipped ?? 0) > 0 && (
                    <div className="insight-metric warning">
                      <span className="metric-label">Clipped</span>
                      <span className="metric-value">{audioQuality?.total_clipped}</span>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* Speaker Metrics */}
            {biomarkers && biomarkers.speaker_metrics.length > 0 && (
              <div className="insight-card">
                <div className="insight-card-header">Speakers</div>
                <div className="insight-card-body">
                  {biomarkers.speaker_metrics.map((speaker) => (
                    <div key={speaker.speaker_id} className="speaker-row">
                      <span className="speaker-name">{speaker.speaker_id}</span>
                      <span className="speaker-stat">{speaker.turn_count} turns</span>
                      <span className="speaker-stat">{Math.round(speaker.talk_time_ms / 1000)}s</span>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* Conversation Dynamics */}
            {biomarkers?.conversation_dynamics && (
              <div className="insight-card">
                <div className="insight-card-header">Conversation</div>
                <div className="insight-card-body">
                  <div className="insight-metric">
                    <span className="metric-label">Response Time</span>
                    <span className="metric-value-group">
                      <span className="metric-value">
                        {Math.round(biomarkers.conversation_dynamics.mean_response_latency_ms)}ms
                      </span>
                      <span className={`metric-status status-${getResponseTimeStatus(biomarkers.conversation_dynamics.mean_response_latency_ms).level}`}>
                        {getResponseTimeStatus(biomarkers.conversation_dynamics.mean_response_latency_ms).label}
                      </span>
                    </span>
                  </div>
                  {biomarkers.conversation_dynamics.engagement_score !== null && (
                    <div className="insight-metric">
                      <span className="metric-label">Engagement</span>
                      <span className="metric-value-group">
                        <span className="metric-value">
                          {Math.round(biomarkers.conversation_dynamics.engagement_score)}
                        </span>
                        <span className={`metric-status status-${getEngagementStatus(biomarkers.conversation_dynamics.engagement_score).level}`}>
                          {getEngagementStatus(biomarkers.conversation_dynamics.engagement_score).label}
                        </span>
                      </span>
                    </div>
                  )}
                  {biomarkers.conversation_dynamics.total_interruption_count > 0 && (
                    <div className="insight-metric">
                      <span className="metric-label">Interruptions</span>
                      <span className="metric-value">
                        {biomarkers.conversation_dynamics.total_interruption_count}
                      </span>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* Vocal Biomarkers */}
            {biomarkers && (biomarkers.vitality_session_mean !== null || biomarkers.stability_session_mean !== null) && (
              <div className="insight-card">
                <div className="insight-card-header">Vocal Biomarkers</div>
                <div className="insight-card-body">
                  {biomarkers.vitality_session_mean !== null && (
                    <div className="insight-metric">
                      <span className="metric-label">Vitality</span>
                      <span className="metric-value-group">
                        <span className="metric-value">{biomarkers.vitality_session_mean.toFixed(1)} Hz</span>
                        <span className={`metric-status status-${getVitalityStatus(biomarkers.vitality_session_mean).level}`}>
                          {getVitalityStatus(biomarkers.vitality_session_mean).label}
                        </span>
                      </span>
                    </div>
                  )}
                  {biomarkers.stability_session_mean !== null && (
                    <div className="insight-metric">
                      <span className="metric-label">Stability</span>
                      <span className="metric-value-group">
                        <span className="metric-value">{biomarkers.stability_session_mean.toFixed(1)} dB</span>
                        <span className={`metric-status status-${getStabilityStatus(biomarkers.stability_session_mean).level}`}>
                          {getStabilityStatus(biomarkers.stability_session_mean).label}
                        </span>
                      </span>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* Empty state */}
            {!audioQuality && !biomarkers && (
              <div className="panel-empty">No insights available</div>
            )}
          </div>
        )}
      </div>

      {/* Sync Status Bar */}
      <SyncStatusBar
        authState={authState}
        authLoading={authLoading}
        onLogin={onLogin}
        onCancelLogin={onCancelLogin}
        isSyncing={isSyncing}
        syncSuccess={syncSuccess}
        syncError={syncError}
        syncedEncounter={syncedEncounter}
        isAddingSoap={isAddingSoap}
        onClearSyncError={onClearSyncError}
        autoSyncEnabled={autoSyncEnabled}
      />

      {/* Action Bar */}
      <div className="action-bar">
        <button className="btn-primary" onClick={onNewSession}>
          New Session
        </button>
      </div>
    </div>
  );
});

export default ReviewMode;
