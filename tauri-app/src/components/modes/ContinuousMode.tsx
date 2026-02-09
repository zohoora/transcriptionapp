/**
 * ContinuousMode - Dashboard for continuous charting mode (end-of-day workflow)
 *
 * Displays:
 * - Idle state: Icon, description, "Start Recording" button
 * - Active state: Pulsing status dot, elapsed timer, stats grid (encounters/buffer),
 *   last encounter summary, live transcript preview, error display, stop button
 *
 * The continuous mode records all day without manual session start/stop.
 * An LLM-based encounter detector automatically segments the transcript
 * and generates SOAP notes for each detected patient encounter.
 */
import { memo, useState, useEffect } from 'react';
import type { ContinuousModeStats } from '../../types';

interface ContinuousModeProps {
  /** Whether continuous mode is active */
  isActive: boolean;
  /** Stats from the backend */
  stats: ContinuousModeStats;
  /** Live transcript preview (last ~500 chars) */
  liveTranscript: string;
  /** Error message */
  error: string | null;
  /** Start continuous mode */
  onStart: () => void;
  /** Stop continuous mode */
  onStop: () => void;
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
 * Shows recording status, encounter count, live transcript preview,
 * and controls for the end-of-day charting workflow.
 */
export const ContinuousMode = memo(function ContinuousMode({
  isActive,
  stats,
  liveTranscript,
  error,
  onStart,
  onStop,
  onViewHistory,
}: ContinuousModeProps) {
  const elapsedTime = useElapsedTime(isActive ? stats.recording_since : undefined);

  // Not active — show start button
  if (!isActive) {
    return (
      <div className="mode-content continuous-mode">
        <div className="continuous-idle">
          <div className="continuous-icon">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
              <circle cx="12" cy="12" r="10" />
              <circle cx="12" cy="12" r="3" />
              <line x1="12" y1="2" x2="12" y2="4" />
              <line x1="12" y1="20" x2="12" y2="22" />
              <line x1="2" y1="12" x2="4" y2="12" />
              <line x1="20" y1="12" x2="22" y2="12" />
            </svg>
          </div>
          <h3>End-of-Day Charting</h3>
          <p className="continuous-description">
            Records continuously throughout the day. Patient encounters are detected automatically and SOAP notes generated in the background.
          </p>

          {error && (
            <div className="continuous-error">
              {error}
            </div>
          )}

          <button className="btn-primary continuous-start-btn" onClick={onStart}>
            Start Recording
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
        <span className={`continuous-dot ${stats.state === 'checking' ? 'checking' : 'recording'}`} />
        <span className="continuous-status-text">
          {stats.state === 'checking' ? 'Checking for encounters...' : 'Recording continuously'}
        </span>
      </div>

      {/* Running timer */}
      {elapsedTime && (
        <div className="continuous-timer">
          Recording for {elapsedTime}
        </div>
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

      {/* Live transcript preview */}
      <div className="continuous-transcript-section">
        <span className="continuous-section-label">Live transcript</span>
        <div className="continuous-transcript-preview">
          {liveTranscript ? (
            <p>{liveTranscript}</p>
          ) : (
            <p className="continuous-transcript-empty">Waiting for speech...</p>
          )}
        </div>
      </div>

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
        <button className="btn-danger" onClick={onStop}>
          Stop Recording
        </button>
      </div>
    </div>
  );
});
