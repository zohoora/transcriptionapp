import { ConversationDynamics, BIOMARKER_THRESHOLDS } from '../types';

interface ConversationDynamicsSectionProps {
  dynamics: ConversationDynamics;
  expanded: boolean;
  onToggle: () => void;
}

const { RESPONSE_LATENCY_GOOD, RESPONSE_LATENCY_WARNING, ENGAGEMENT_GOOD, ENGAGEMENT_WARNING } = BIOMARKER_THRESHOLDS;

function getResponseLatencyClass(value: number): string {
  if (value < RESPONSE_LATENCY_GOOD) return 'metric-good';
  if (value < RESPONSE_LATENCY_WARNING) return 'metric-warning';
  return 'metric-low';
}

function getEngagementPercent(value: number | null): number {
  if (value === null) return 0;
  return Math.min(100, Math.max(0, value));
}

function getEngagementClass(value: number | null): string {
  if (value === null) return '';
  if (value >= ENGAGEMENT_GOOD) return 'metric-good';
  if (value >= ENGAGEMENT_WARNING) return 'metric-warning';
  return 'metric-low';
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export default function ConversationDynamicsSection({ dynamics, expanded, onToggle }: ConversationDynamicsSectionProps) {
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onToggle();
    }
  };

  return (
    <section className="dynamics-section">
      <div
        className="dynamics-header"
        onClick={onToggle}
        onKeyDown={handleKeyDown}
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
      >
        <div className="dynamics-header-left">
          <span className={`chevron ${expanded ? '' : 'collapsed'}`} aria-hidden="true">
            &#9660;
          </span>
          <span className="dynamics-title">Conversation</span>
        </div>
        {dynamics.engagement_score !== null && (
          <span className={`engagement-badge ${getEngagementClass(dynamics.engagement_score)}`}>
            {Math.round(dynamics.engagement_score)}
          </span>
        )}
      </div>

      {expanded && (
        <div className="dynamics-content">
          {/* Response Latency */}
          <div className="metric-row">
            <span className="metric-label">Response</span>
            <span className={`metric-value-wide ${getResponseLatencyClass(dynamics.mean_response_latency_ms)}`}>
              {formatDuration(dynamics.mean_response_latency_ms)} avg
            </span>
          </div>

          {/* Overlaps & Interruptions */}
          {(dynamics.total_overlap_count > 0 || dynamics.total_interruption_count > 0) && (
            <div className="metric-row">
              <span className="metric-label">Overlaps</span>
              <span className="metric-value-wide">
                {dynamics.total_overlap_count}
                {dynamics.total_interruption_count > 0 && (
                  <span className="interruption-count"> ({dynamics.total_interruption_count} interr.)</span>
                )}
              </span>
            </div>
          )}

          {/* Long Pauses */}
          {dynamics.silence.long_pause_count > 0 && (
            <div className="metric-row">
              <span className="metric-label">Long Pauses</span>
              <span className="metric-value-wide">{dynamics.silence.long_pause_count}</span>
            </div>
          )}

          {/* Engagement Score with bar */}
          {dynamics.engagement_score !== null && (
            <div className="metric-row">
              <span className="metric-label">Engagement</span>
              <div className="metric-bar-container">
                <div
                  className={`metric-bar ${getEngagementClass(dynamics.engagement_score)}`}
                  style={{ width: `${getEngagementPercent(dynamics.engagement_score)}%` }}
                />
              </div>
              <span className="metric-value">
                {Math.round(dynamics.engagement_score)}
              </span>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
