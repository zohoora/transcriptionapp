import { BiomarkerUpdate, BIOMARKER_THRESHOLDS } from '../types';

interface BiomarkersSectionProps {
  biomarkers: BiomarkerUpdate | null;
  expanded: boolean;
  onToggle: () => void;
  isPreparing: boolean;
}

const {
  VITALITY_GOOD, VITALITY_WARNING, VITALITY_MAX_DISPLAY,
  STABILITY_GOOD, STABILITY_WARNING, STABILITY_MAX_DISPLAY
} = BIOMARKER_THRESHOLDS;

function getVitalityPercent(value: number | null): number {
  if (value === null) return 0;
  return Math.min(100, (value / VITALITY_MAX_DISPLAY) * 100);
}

function getVitalityClass(value: number | null): string {
  if (value === null) return '';
  if (value >= VITALITY_GOOD) return 'metric-good';
  if (value >= VITALITY_WARNING) return 'metric-warning';
  return 'metric-low';
}

function getStabilityPercent(value: number | null): number {
  if (value === null) return 0;
  return Math.min(100, (value / STABILITY_MAX_DISPLAY) * 100);
}

function getStabilityClass(value: number | null): string {
  if (value === null) return '';
  if (value >= STABILITY_GOOD) return 'metric-good';
  if (value >= STABILITY_WARNING) return 'metric-warning';
  return 'metric-low';
}

export default function BiomarkersSection({ biomarkers, expanded, onToggle, isPreparing }: BiomarkersSectionProps) {
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onToggle();
    }
  };

  return (
    <section className="biomarkers-section">
      <div
        className="biomarkers-header"
        onClick={onToggle}
        onKeyDown={handleKeyDown}
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
      >
        <div className="biomarkers-header-left">
          <span className={`chevron ${expanded ? '' : 'collapsed'}`} aria-hidden="true">
            &#9660;
          </span>
          <span className="biomarkers-title">Biomarkers</span>
        </div>
      </div>

      {expanded && (
        <div className="biomarkers-content">
          {biomarkers ? (
            <>
              {/* Per-Speaker Biomarkers (when diarization enabled) */}
              {biomarkers.speaker_metrics.length > 0 ? (
                <div className="speaker-biomarkers">
                  {biomarkers.speaker_metrics.map((speaker) => (
                    <div key={speaker.speaker_id} className="speaker-metrics-group">
                      <div className="speaker-label">{speaker.speaker_id}</div>
                      <div className="metric-row">
                        <span className="metric-label">Vitality</span>
                        <div className="metric-bar-container">
                          <div
                            className={`metric-bar ${getVitalityClass(speaker.vitality_mean)}`}
                            style={{ width: `${getVitalityPercent(speaker.vitality_mean)}%` }}
                          />
                        </div>
                        <span className="metric-value">
                          {speaker.vitality_mean?.toFixed(1) ?? '--'} Hz
                        </span>
                      </div>
                      <div className="metric-row">
                        <span className="metric-label">Stability</span>
                        <div className="metric-bar-container">
                          <div
                            className={`metric-bar ${getStabilityClass(speaker.stability_mean)}`}
                            style={{ width: `${getStabilityPercent(speaker.stability_mean)}%` }}
                          />
                        </div>
                        <span className="metric-value">
                          {speaker.stability_mean?.toFixed(1) ?? '--'} dB
                        </span>
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <>
                  {/* Combined Session Metrics (fallback when no per-speaker data) */}
                  <div className="metric-row">
                    <span className="metric-label">Vitality</span>
                    <div className="metric-bar-container">
                      <div
                        className={`metric-bar ${getVitalityClass(biomarkers.vitality_session_mean)}`}
                        style={{ width: `${getVitalityPercent(biomarkers.vitality_session_mean)}%` }}
                      />
                    </div>
                    <span className="metric-value">
                      {biomarkers.vitality_session_mean?.toFixed(1) ?? '--'} Hz
                    </span>
                  </div>
                  <div className="metric-row">
                    <span className="metric-label">Stability</span>
                    <div className="metric-bar-container">
                      <div
                        className={`metric-bar ${getStabilityClass(biomarkers.stability_session_mean)}`}
                        style={{ width: `${getStabilityPercent(biomarkers.stability_session_mean)}%` }}
                      />
                    </div>
                    <span className="metric-value">
                      {biomarkers.stability_session_mean?.toFixed(1) ?? '--'} dB
                    </span>
                  </div>
                </>
              )}

              {/* Session Metrics (if diarization enabled with multiple speakers) */}
              {biomarkers.turn_count > 1 && (
                <div className="session-metrics">
                  <div className="metric-row">
                    <span className="metric-label">Turns</span>
                    <span className="metric-value-wide">{biomarkers.turn_count}</span>
                  </div>
                  {biomarkers.talk_time_ratio !== null && (
                    <div className="metric-row">
                      <span className="metric-label">Balance</span>
                      <span className="metric-value-wide">
                        {(biomarkers.talk_time_ratio * 100).toFixed(0)}%
                      </span>
                    </div>
                  )}
                </div>
              )}
            </>
          ) : (
            <div className="biomarkers-placeholder">
              {isPreparing ? 'Initializing...' : 'Listening...'}
            </div>
          )}
        </div>
      )}
    </section>
  );
}
