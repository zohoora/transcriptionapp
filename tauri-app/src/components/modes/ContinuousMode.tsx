/**
 * ContinuousMode - Dashboard for continuous charting mode (end-of-day workflow)
 *
 * Displays:
 * - Idle state: Icon, description, "Start Session" button
 * - Active state: Pulsing status dot, elapsed timer, audio quality indicator,
 *   stats grid (encounters/buffer), last encounter summary, encounter notes,
 *   toggleable transcript preview, MIIS image suggestions, error display, end button
 *
 * Runs as an ambient companion throughout the day. An LLM-based encounter
 * detector automatically segments the transcript and generates SOAP notes
 * for each detected patient encounter.
 */
import { memo, useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import type { ContinuousModeStats, AudioQualitySnapshot, BiomarkerUpdate } from '../../types';
import type { PatientTrends } from '../../hooks/usePatientBiomarkers';
import type { MiisSuggestion } from '../../hooks/useMiisImages';
import type { AiImage } from '../../hooks/useAiImages';
import type { DifferentialDiagnosis } from '../../hooks/usePredictiveHint';
import { DDX_LIKELIHOOD_LABELS } from '../../hooks/usePredictiveHint';
import { getAudioQualityLevel } from '../../utils';
import { MarkdownContent } from '../ClinicalChat';
import { ImageSuggestions } from '../ImageSuggestions';
import { PatientPulse } from '../PatientPulse';

const ALL_LANGUAGES = [
  { iso: 'auto', name: 'Auto-detect' },
  { iso: 'en', name: 'English' }, { iso: 'fr', name: 'French' },
  { iso: 'fa', name: 'Persian' }, { iso: 'es', name: 'Spanish' },
  { iso: 'de', name: 'German' }, { iso: 'zh', name: 'Chinese' },
  { iso: 'ar', name: 'Arabic' }, { iso: 'hi', name: 'Hindi' },
  { iso: 'pt', name: 'Portuguese' }, { iso: 'it', name: 'Italian' },
  { iso: 'ja', name: 'Japanese' }, { iso: 'ko', name: 'Korean' },
  { iso: 'ru', name: 'Russian' }, { iso: 'nl', name: 'Dutch' },
  { iso: 'pl', name: 'Polish' }, { iso: 'tr', name: 'Turkish' },
  { iso: 'sv', name: 'Swedish' }, { iso: 'da', name: 'Danish' },
  { iso: 'fi', name: 'Finnish' }, { iso: 'el', name: 'Greek' },
  { iso: 'cs', name: 'Czech' }, { iso: 'ro', name: 'Romanian' },
  { iso: 'hu', name: 'Hungarian' }, { iso: 'th', name: 'Thai' },
  { iso: 'vi', name: 'Vietnamese' }, { iso: 'id', name: 'Indonesian' },
  { iso: 'ms', name: 'Malay' }, { iso: 'tl', name: 'Filipino' },
  { iso: 'mk', name: 'Macedonian' }, { iso: 'yue', name: 'Cantonese' },
];

interface ContinuousModeProps {
  /** Whether continuous mode is active */
  isActive: boolean;
  /** Whether a stop is in progress (flushing buffer, generating final SOAP) */
  isStopping: boolean;
  /** Stats from the backend */
  stats: ContinuousModeStats;
  /** Live transcript preview (last ~500 chars) */
  liveTranscript: string;
  /** Error message */
  error: string | null;
  /** Predictive hint text for the physician */
  predictiveHint: string;
  /** Whether a predictive hint is being generated */
  predictiveHintLoading: boolean;
  /** Top differential diagnoses */
  differentialDiagnoses: DifferentialDiagnosis[];
  /** Audio quality snapshot from the pipeline */
  audioQuality: AudioQualitySnapshot | null;
  /** Raw biomarker update for PatientPulse */
  biomarkers: BiomarkerUpdate | null;
  /** Aggregated patient trends from baseline tracking */
  biomarkerTrends: PatientTrends;
  /** Per-encounter notes text */
  encounterNotes: string;
  /** Callback when encounter notes change */
  onEncounterNotesChange: (notes: string) => void;
  /** MIIS image suggestions */
  miisSuggestions: MiisSuggestion[];
  miisLoading: boolean;
  miisError: string | null;
  miisEnabled: boolean;
  onMiisImpression: (imageId: number) => void;
  onMiisClick: (imageId: number) => void;
  onMiisDismiss: (imageId: number) => void;
  miisGetImageUrl: (path: string) => string;
  // AI-generated images
  aiImages?: AiImage[];
  aiLoading?: boolean;
  aiError?: string | null;
  onAiGenerate?: (description: string) => void;
  onAiDismiss?: (index: number) => void;
  imageSource?: 'miis' | 'ai' | 'off';
  /** Current STT language (ISO code) */
  sttLanguage: string;
  /** Physician's preferred languages (ISO codes, ordered) */
  preferredLanguages: string[];
  /** Language change callback */
  onLanguageChange: (iso: string) => void;
  /** Start continuous mode */
  onStart: () => void;
  /** Stop continuous mode */
  onStop: () => void;
  /** Trigger manual new patient encounter split */
  onNewPatient: () => void;
  /** Generate a patient-friendly visit summary handout */
  onGenerateHandout: () => void;
  /** Whether handout generation is in progress */
  isGeneratingHandout: boolean;
  /** Open history window to view today's sessions */
  onViewHistory: () => void;
  /** Speech detected but no transcription being produced */
  transcriptionStalled?: boolean;
  /** Whether the pipeline is currently in overnight sleep mode */
  isSleeping?: boolean;
  /** ISO timestamp when sleep mode will resume (null when not sleeping) */
  sleepResumeAt?: string | null;
}

/**
 * Format elapsed time since a given ISO timestamp
 */
function useElapsedTime(since: string | undefined, intervalMs = 30000): string {
  const [elapsed, setElapsed] = useState('');

  useEffect(() => {
    if (!since) {
      setElapsed('');
      return;
    }

    const update = () => {
      const start = new Date(since).getTime();
      const now = Date.now();
      const diffMs = Math.max(0, now - start);

      const hours = Math.floor(diffMs / 3600000);
      const minutes = Math.floor((diffMs % 3600000) / 60000);

      if (hours > 0) {
        setElapsed(`${hours}h ${minutes}m`);
      } else {
        setElapsed(`${minutes}m`);
      }
    };

    update();
    const interval = setInterval(update, intervalMs);
    return () => clearInterval(interval);
  }, [since, intervalMs]);

  return elapsed;
}

/**
 * Format a timestamp to local time (e.g., "2:34 PM")
 */
function formatTime(isoString: string | null): string {
  if (!isoString) return '';
  try {
    return new Date(isoString).toLocaleTimeString([], {
      hour: 'numeric',
      minute: '2-digit',
    });
  } catch {
    return '';
  }
}

/**
 * Continuous Mode monitoring dashboard.
 *
 * Shows session status, encounter count, live transcript preview,
 * and controls for the end-of-day charting workflow.
 */
export const ContinuousMode = memo(function ContinuousMode({
  isActive,
  isStopping,
  stats,
  liveTranscript,
  error,
  predictiveHint,
  predictiveHintLoading,
  differentialDiagnoses,
  audioQuality,
  biomarkers,
  biomarkerTrends,
  encounterNotes,
  onEncounterNotesChange,
  miisSuggestions,
  miisLoading,
  miisError,
  miisEnabled,
  onMiisImpression,
  onMiisClick,
  onMiisDismiss,
  miisGetImageUrl,
  aiImages,
  aiLoading,
  aiError,
  onAiGenerate,
  onAiDismiss,
  imageSource = 'miis',
  sttLanguage,
  preferredLanguages,
  onLanguageChange,
  onStart,
  onStop,
  onNewPatient,
  onGenerateHandout,
  isGeneratingHandout,
  onViewHistory,
  transcriptionStalled,
  isSleeping,
  sleepResumeAt,
}: ContinuousModeProps) {
  const encounterElapsed = useElapsedTime(stats.buffer_started_at ?? undefined, 10000);

  // Local UI toggle states
  const [showTranscript, setShowTranscript] = useState(false);
  const [showDetails, setShowDetails] = useState(false);
  const [showNotes, setShowNotes] = useState(false);
  const [copiedEncounterId, setCopiedEncounterId] = useState<string | null>(null);

  // 2-second cooldown guard to prevent double-clicks on "New Patient"
  const newPatientCooldownRef = useRef(false);
  const handleNewPatient = useCallback(() => {
    if (newPatientCooldownRef.current) return;
    newPatientCooldownRef.current = true;
    // Reset all local UI state for fresh patient experience
    setShowTranscript(false);
    setShowDetails(false);
    setShowNotes(false);
    setCopiedEncounterId(null);
    onNewPatient();
    setTimeout(() => { newPatientCooldownRef.current = false; }, 2000);
  }, [onNewPatient]);

  const qualityLevel = getAudioQualityLevel(audioQuality);

  const handleDetailsClick = useCallback(() => {
    setShowDetails(prev => !prev);
  }, []);

  const handleNotesToggle = useCallback(() => {
    setShowNotes(prev => !prev);
  }, []);

  const handleNotesChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    onEncounterNotesChange(e.target.value);
  }, [onEncounterNotesChange]);

  // Not active — show start button
  if (!isActive) {
    return (
      <div className="mode-content continuous-mode">
        <div className="continuous-idle">
          <div className="continuous-icon">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
              {/* Stethoscope — clinical listening, not surveillance */}
              <path d="M4.5 12.5a5 5 0 0 0 5 5h1a3 3 0 0 0 3-3v-2" />
              <path d="M19.5 8.5a2.5 2.5 0 1 0-5 0v3a3 3 0 0 1-3 3" />
              <circle cx="19.5" cy="6" r="1.5" />
              <path d="M4.5 12.5V6" />
              <path d="M9.5 12.5V6" />
              <path d="M4.5 6a2.5 2.5 0 0 1 5 0" />
            </svg>
          </div>
          <h3>End-of-Day Charting</h3>
          <p className="continuous-description">
            Listens throughout the day. Patient encounters are detected automatically and SOAP notes generated in the background.
          </p>

          {error && (
            <div className="continuous-error">
              {error}
            </div>
          )}

          <LanguageSelector
            value={sttLanguage}
            preferred={preferredLanguages}
            onChange={onLanguageChange}
          />

          <button className="btn-primary continuous-start-btn" onClick={onStart}>
            Start Session
          </button>

          <button className="btn-link" onClick={onViewHistory}>
            View Past Sessions
          </button>
        </div>
      </div>
    );
  }

  // Active — show monitoring dashboard
  return (
    <div className="mode-content continuous-mode">
      {isSleeping && (
        <div className="continuous-sleep-mode">
          <div className="sleep-mode-icon">🌙</div>
          <div className="sleep-mode-title">Sleep Mode</div>
          <div className="sleep-mode-subtitle">
            Recording paused overnight.
            {sleepResumeAt && ` Resumes at ${new Date(sleepResumeAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}.`}
          </div>
          <div className="continuous-actions">
            <button className="continuous-stop-btn" onClick={onStop} disabled={isStopping}>
              Stop
            </button>
            <button className="continuous-action-btn" onClick={onViewHistory}>
              View Today&apos;s Sessions
            </button>
          </div>
        </div>
      )}

      {!isSleeping && (
      <>
      {/* Status header with pulsing indicator */}
      <div className="continuous-status-header">
        <span className={`continuous-dot ${isStopping ? 'stopping' : stats.state === 'checking' ? 'checking' : 'listening'}`} />
        <span className="continuous-status-text">
          {isStopping
            ? 'Ending... finalizing notes'
            : stats.state === 'checking'
              ? 'Checking for encounters...'
              : 'Continuous mode active'}
        </span>
      </div>

      {/* Sensor status indicator (only in sensor detection mode) */}
      {stats.sensor_connected !== undefined && (
        <div className="continuous-sensor-status" style={{ fontSize: 11, marginBottom: 4, display: 'flex', alignItems: 'center', gap: 4 }}>
          <span style={{
            display: 'inline-block',
            width: 8,
            height: 8,
            borderRadius: '50%',
            backgroundColor: !stats.sensor_connected
              ? '#ef4444'
              : stats.sensor_state === 'present'
                ? '#22c55e'
                : stats.sensor_state === 'absent'
                  ? '#94a3b8'
                  : '#a3a3a3',
          }} />
          <span style={{ opacity: 0.7 }}>
            Sensor: {!stats.sensor_connected
              ? 'Disconnected'
              : stats.sensor_state === 'present'
                ? 'Present'
                : stats.sensor_state === 'absent'
                  ? 'Absent'
                  : 'Unknown'}
          </span>
        </div>
      )}

      {/* Shadow mode indicator (dual detection comparison) */}
      {stats.shadow_mode_active && (
        <div style={{
          fontSize: 11,
          marginBottom: 4,
          display: 'flex',
          alignItems: 'center',
          gap: 4,
          padding: '2px 6px',
          borderRadius: 4,
          backgroundColor: 'rgba(147, 51, 234, 0.1)',
        }}>
          <span style={{
            display: 'inline-block',
            width: 8,
            height: 8,
            borderRadius: '50%',
            backgroundColor: stats.last_shadow_outcome === 'would_split' ? '#a855f7' : '#6b7280',
          }} />
          <span style={{ opacity: 0.7 }}>
            Shadow ({stats.shadow_method?.toUpperCase()}): {
              stats.last_shadow_outcome === 'would_split'
                ? 'Would split'
                : 'Observing...'
            }
          </span>
        </div>
      )}

      {/* Encounter timer + buffer word count */}
      <div className="continuous-timer">
        {stats.buffer_started_at && stats.buffer_word_count > 0 ? (
          <>
            Encounter: {encounterElapsed || '<1m'}
            <span className="continuous-timer-words">{stats.buffer_word_count} words</span>
          </>
        ) : (
          <span className="continuous-timer-waiting">Waiting for next patient...</span>
        )}
      </div>

      {/* Audio quality indicator */}
      <button
        className={`quality-indicator ${qualityLevel}`}
        onClick={handleDetailsClick}
        aria-label="Audio quality - tap for details"
      >
        <span className="quality-dot" />
        <span className="quality-label">
          {qualityLevel === 'good' ? 'Good audio' : qualityLevel === 'fair' ? 'Fair audio' : qualityLevel === 'no_data' ? 'No audio' : 'Poor audio'}
        </span>
      </button>

      {/* Audio quality details popover */}
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

      {/* Transcription stalled warning */}
      {transcriptionStalled && (
        <div className="transcription-stalled-warning">
          Speech detected but transcription not working. Check STT server connection.
        </div>
      )}

      {/* Patient Handout button (only when encounter has content) */}
      {isActive && stats.buffer_word_count > 0 && (
        <button
          className="handout-btn"
          onClick={onGenerateHandout}
          disabled={isGeneratingHandout}
        >
          {isGeneratingHandout ? 'Generating...' : 'Patient Handout'}
        </button>
      )}

      {/* Patient voice pulse indicator */}
      <PatientPulse biomarkers={biomarkers} trends={biomarkerTrends} />


      {/* Predictive hint — "Pssst..." */}
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

      {/* Image Suggestions (MIIS or AI) */}
      {miisEnabled && (
        <ImageSuggestions
          suggestions={miisSuggestions}
          isLoading={miisLoading}
          error={miisError}
          getImageUrl={miisGetImageUrl}
          onImpression={onMiisImpression}
          onClickImage={onMiisClick}
          onDismiss={onMiisDismiss}
          aiImages={aiImages}
          aiLoading={aiLoading}
          aiError={aiError}
          onAiGenerate={onAiGenerate}
          onAiDismiss={onAiDismiss}
          imageSource={imageSource}
        />
      )}


      {/* Differential Diagnosis */}
      {differentialDiagnoses.length > 0 && (
        <div className="ddx-section">
          <div className="ddx-title">Differential Diagnosis</div>
          {differentialDiagnoses.map((ddx, i) => (
            <div key={i} className="ddx-item" title={ddx.key_findings.join(' \u00B7 ')}>
              <span className="ddx-rank">{i + 1}.</span>
              <span className="ddx-name">{ddx.diagnosis}</span>
              <span className={`ddx-likelihood ddx-${ddx.likelihood}`}>
                {DDX_LIKELIHOOD_LABELS[ddx.likelihood] ?? ddx.likelihood}
              </span>
            </div>
          ))}
        </div>
      )}

      {/* Recent encounters list */}
      {stats.recent_encounters.length > 0 && (
        <div className="continuous-recent-encounters">
          <span className="continuous-section-label">Recent encounters</span>
          {stats.recent_encounters.map((enc) => (
            <button
              key={enc.sessionId}
              className="recent-encounter-item"
              onClick={async () => {
                try {
                  const soap = await invoke<string>('get_session_soap_note', {
                    sessionId: enc.sessionId,
                    date: new Date(enc.time).toISOString().split('T')[0],
                  });
                  await writeText(soap);
                  setCopiedEncounterId(enc.sessionId);
                  setTimeout(() => setCopiedEncounterId(null), 1500);
                } catch (e) {
                  console.warn('Failed to copy SOAP:', e);
                }
              }}
              title="Click to copy SOAP note"
            >
              <span className="recent-encounter-time">{formatTime(enc.time)}</span>
              {enc.patientName && (
                <span className="recent-encounter-name">{enc.patientName}</span>
              )}
              <span className="recent-encounter-copy">{copiedEncounterId === enc.sessionId ? '\u2705' : '\uD83D\uDCCB'}</span>
            </button>
          ))}
        </div>
      )}

      {/* Encounter Notes Toggle & Input */}
      <button
        className={`notes-toggle ${showNotes ? 'active' : ''} ${encounterNotes.trim() ? 'has-notes' : ''}`}
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
            placeholder="Enter observations for this encounter..."
            value={encounterNotes}
            onChange={handleNotesChange}
            rows={3}
            aria-label="Encounter notes"
          />
        </div>
      )}

      {/* Transcript Toggle */}
      <button
        className={`transcript-toggle ${showTranscript ? 'active' : ''}`}
        onClick={() => setShowTranscript(!showTranscript)}
      >
        {showTranscript ? 'Hide Transcript' : 'Show Transcript'}
      </button>

      {/* Live transcript preview (toggleable) */}
      {showTranscript && (
        <div className="transcript-preview">
          {liveTranscript ? (
            <div className="transcript-preview-text">{liveTranscript}</div>
          ) : (
            <div className="transcript-preview-placeholder">Waiting for speech...</div>
          )}
        </div>
      )}

      {/* Error display */}
      {(error || stats.last_error) && (
        <div className="continuous-error">
          {error || stats.last_error}
        </div>
      )}

      {/* Language + Actions */}
      <LanguageSelector value={sttLanguage} preferred={preferredLanguages} onChange={onLanguageChange} />
      <div className="continuous-actions">
        <button
          className="continuous-new-patient-btn"
          onClick={handleNewPatient}
          disabled={isStopping}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
            <circle cx="9" cy="7" r="4" />
            <line x1="19" y1="8" x2="19" y2="14" />
            <line x1="22" y1="11" x2="16" y2="11" />
          </svg>
          End Previous / Start New
        </button>
        <button className="btn-secondary" onClick={onViewHistory}>
          View Today&apos;s Sessions
        </button>
        <button
          className={`btn-end-session ${isStopping ? 'btn-stopping' : ''}`}
          onClick={onStop}
          disabled={isStopping}
        >
          {isStopping ? 'Ending...' : 'End Session'}
        </button>
      </div>
      </>
      )}
    </div>
  );
});

function LanguageSelector({ value, preferred, onChange }: {
  value: string;
  preferred: string[];
  onChange: (iso: string) => void;
}) {
  const preferredSet = new Set(preferred);
  const preferredLangs = ALL_LANGUAGES.filter(l => preferredSet.has(l.iso));
  const otherLangs = ALL_LANGUAGES.filter(l => !preferredSet.has(l.iso));

  return (
    <div className="language-selector">
      <select
        className="language-select"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        {preferredLangs.map(l => (
          <option key={l.iso} value={l.iso}>{l.name}</option>
        ))}
        {preferredLangs.length > 0 && otherLangs.length > 0 && (
          <option disabled>──────</option>
        )}
        {otherLangs.map(l => (
          <option key={l.iso} value={l.iso}>{l.name}</option>
        ))}
      </select>
    </div>
  );
}
