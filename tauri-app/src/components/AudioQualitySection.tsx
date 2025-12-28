import { AudioQualitySnapshot, AUDIO_QUALITY_THRESHOLDS } from '../types';

interface AudioQualitySectionProps {
  audioQuality: AudioQualitySnapshot;
  expanded: boolean;
  onToggle: () => void;
}

// Level: -40 to -6 dBFS is good range
function getLevelPercent(value: number): number {
  const { LEVEL_MIN_DISPLAY, LEVEL_MAX_DISPLAY } = AUDIO_QUALITY_THRESHOLDS;
  return Math.min(100, Math.max(0, ((value - LEVEL_MIN_DISPLAY) / (LEVEL_MAX_DISPLAY - LEVEL_MIN_DISPLAY)) * 100));
}

function getLevelClass(value: number): string {
  const { LEVEL_TOO_QUIET, LEVEL_TOO_HOT } = AUDIO_QUALITY_THRESHOLDS;
  if (value < LEVEL_TOO_QUIET) return 'metric-low';
  if (value > LEVEL_TOO_HOT) return 'metric-warning';
  return 'metric-good';
}

// SNR: >10 dB is good
function getSnrPercent(value: number): number {
  const { SNR_MAX_DISPLAY } = AUDIO_QUALITY_THRESHOLDS;
  return Math.min(100, Math.max(0, (value / SNR_MAX_DISPLAY) * 100));
}

function getSnrClass(value: number): string {
  const { SNR_GOOD, SNR_WARNING } = AUDIO_QUALITY_THRESHOLDS;
  if (value >= SNR_GOOD) return 'metric-good';
  if (value >= SNR_WARNING) return 'metric-warning';
  return 'metric-low';
}

function getQualityStatus(quality: AudioQualitySnapshot): { label: string; class: string } {
  const { LEVEL_TOO_QUIET, LEVEL_TOO_HOT, SNR_WARNING, CLIPPING_OK } = AUDIO_QUALITY_THRESHOLDS;
  const levelOk = quality.rms_db >= LEVEL_TOO_QUIET && quality.rms_db <= LEVEL_TOO_HOT;
  const snrOk = quality.snr_db >= SNR_WARNING;
  const clippingOk = quality.clipped_ratio < CLIPPING_OK;
  const dropoutOk = quality.dropout_count === 0;

  if (levelOk && snrOk && clippingOk && dropoutOk) {
    return { label: 'Good', class: 'quality-good' };
  }
  if (!clippingOk || !dropoutOk) {
    return { label: 'Poor', class: 'quality-poor' };
  }
  return { label: 'Fair', class: 'quality-fair' };
}

function getQualitySuggestion(quality: AudioQualitySnapshot): string | null {
  const { LEVEL_TOO_QUIET, LEVEL_TOO_HOT, SNR_WARNING, CLIPPING_OK } = AUDIO_QUALITY_THRESHOLDS;

  if (quality.clipped_ratio >= CLIPPING_OK) {
    return 'Speak softer or move mic further away';
  }
  if (quality.dropout_count > 0) {
    return 'Audio gaps detected - check connection';
  }
  if (quality.rms_db < LEVEL_TOO_QUIET) {
    return 'Move microphone closer';
  }
  if (quality.rms_db > LEVEL_TOO_HOT) {
    return 'Move microphone further away';
  }
  if (quality.snr_db < SNR_WARNING) {
    return 'Reduce background noise';
  }
  return null;
}

export default function AudioQualitySection({ audioQuality, expanded, onToggle }: AudioQualitySectionProps) {
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onToggle();
    }
  };

  return (
    <section className="audio-quality-section">
      <div
        className="audio-quality-header"
        onClick={onToggle}
        onKeyDown={handleKeyDown}
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
      >
        <div className="audio-quality-header-left">
          <span className={`chevron ${expanded ? '' : 'collapsed'}`} aria-hidden="true">
            &#9660;
          </span>
          <span className="audio-quality-title">Audio Quality</span>
        </div>
        <span className={`quality-badge ${getQualityStatus(audioQuality).class}`}>
          {getQualityStatus(audioQuality).label}
        </span>
      </div>

      {expanded && (
        <div className="audio-quality-content">
          {getQualitySuggestion(audioQuality) && (
            <div className="quality-suggestion">
              {getQualitySuggestion(audioQuality)}
            </div>
          )}
          <div className="metric-row">
            <span className="metric-label">Level</span>
            <div className="metric-bar-container">
              <div
                className={`metric-bar ${getLevelClass(audioQuality.rms_db)}`}
                style={{ width: `${getLevelPercent(audioQuality.rms_db)}%` }}
              />
            </div>
            <span className="metric-value">
              {audioQuality.rms_db.toFixed(0)} dB
            </span>
          </div>
          <div className="metric-row">
            <span className="metric-label">SNR</span>
            <div className="metric-bar-container">
              <div
                className={`metric-bar ${getSnrClass(audioQuality.snr_db)}`}
                style={{ width: `${getSnrPercent(audioQuality.snr_db)}%` }}
              />
            </div>
            <span className="metric-value">
              {audioQuality.snr_db.toFixed(0)} dB
            </span>
          </div>
          {audioQuality.total_clipped > 0 && (
            <div className="metric-row quality-warning">
              <span className="metric-label">Clips</span>
              <span className="metric-value-wide">{audioQuality.total_clipped}</span>
            </div>
          )}
          {audioQuality.dropout_count > 0 && (
            <div className="metric-row quality-warning">
              <span className="metric-label">Drops</span>
              <span className="metric-value-wide">{audioQuality.dropout_count}</span>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
