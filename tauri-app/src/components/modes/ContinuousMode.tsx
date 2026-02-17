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
import type { ContinuousModeStats, AudioQualitySnapshot, BiomarkerUpdate } from '../../types';
import type { PatientTrends } from '../../hooks/usePatientBiomarkers';
import type { MiisSuggestion } from '../../hooks/useMiisImages';
import { getAudioQualityLevel } from '../../utils';
import { MarkdownContent } from '../ClinicalChat';
import { ImageSuggestions } from '../ImageSuggestions';
import { PatientPulse } from '../PatientPulse';

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
  /** Start continuous mode */
  onStart: () => void;
  /** Stop continuous mode */
  onStop: () => void;
  /** Trigger manual new patient encounter split */
  onNewPatient: () => void;
  /** Open history window to view today's sessions */
  onViewHistory: () => void;
}

/**
 * Format elapsed time since a given ISO timestamp
 */
function useElapsedTime(since: string | undefined): string {
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
    const interval = setInterval(update, 30000); // Update every 30s
    return () => clearInterval(interval);
  }, [since]);

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
  onStart,
  onStop,
  onNewPatient,
  onViewHistory,
}: ContinuousModeProps) {
  const elapsedTime = useElapsedTime(isActive ? stats.recording_since : undefined);
  const encounterElapsed = useElapsedTime(stats.buffer_started_at ?? undefined);

  // Local UI toggle states
  const [showTranscript, setShowTranscript] = useState(false);
  const [showDetails, setShowDetails] = useState(false);
  const [showNotes, setShowNotes] = useState(false);

  // 2-second cooldown guard to prevent double-clicks on "New Patient"
  const newPatientCooldownRef = useRef(false);
  const handleNewPatient = useCallback(() => {
    if (newPatientCooldownRef.current) return;
    newPatientCooldownRef.current = true;
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

      {/* Session length timer */}
      {elapsedTime && (
        <div className="continuous-timer">
          Session: {elapsedTime}
        </div>
      )}

      {/* Audio quality indicator */}
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

      {/* New Patient button */}
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
        New Patient
      </button>

      {/* Patient voice pulse indicator */}
      <PatientPulse biomarkers={biomarkers} trends={biomarkerTrends} />

      {/* Current encounter info */}
      <div className="continuous-encounter-info">
        {stats.buffer_started_at && stats.buffer_word_count > 0 ? (
          <span>Current encounter: {encounterElapsed || '<1m'} &middot; {stats.buffer_word_count} words</span>
        ) : (
          <span className="continuous-encounter-waiting">Waiting for next patient...</span>
        )}
      </div>

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

      {/* Stats grid */}
      <div className="continuous-stats">
        <div className="continuous-stat">
          <span className="continuous-stat-value">{stats.encounters_detected}</span>
          <span className="continuous-stat-label">
            encounter{stats.encounters_detected !== 1 ? 's' : ''} charted
          </span>
        </div>
        <div className="continuous-stat">
          <span className="continuous-stat-value">{stats.buffer_word_count}</span>
          <span className="continuous-stat-label">words in buffer</span>
        </div>
      </div>

      {/* Last encounter summary */}
      {stats.last_encounter_at && (
        <div className="continuous-last-encounter">
          <span className="continuous-section-label">Last encounter</span>
          <div className="continuous-last-encounter-info">
            <span>{formatTime(stats.last_encounter_at)}</span>
            {stats.last_encounter_words && (
              <span> &middot; {stats.last_encounter_words} words</span>
            )}
            {stats.last_encounter_patient_name && (
              <span> &mdash; {stats.last_encounter_patient_name}</span>
            )}
          </div>
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

      {/* Actions */}
      <div className="continuous-actions">
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
    </div>
  );
});
