/**
 * PatientVoiceMonitor — Compact card showing patient voice metrics with trends.
 *
 * Displays voice energy (vitality), vocal control (stability), engagement,
 * talk share, clinical insights, and cough count — all filtered to patient
 * speakers only (enrolled clinicians excluded).
 */
import { memo } from 'react';
import type { PatientBiomarkerData, PatientMetrics, TrendDirection } from '../hooks/usePatientBiomarkers';
import { BIOMARKER_THRESHOLDS } from '../types';

interface PatientVoiceMonitorProps {
  data: PatientBiomarkerData;
}

// ============================================================================
// Sub-components
// ============================================================================

/** 10-segment dot bar with color based on thresholds */
function DotBar({ value, max, goodThreshold, warnThreshold }: {
  value: number;
  max: number;
  goodThreshold: number;
  warnThreshold: number;
}) {
  const filled = Math.round(Math.min(value / max, 1) * 10);
  const color = value >= goodThreshold ? 'good' : value >= warnThreshold ? 'moderate' : 'low';

  return (
    <span className="pvm-bar" aria-label={`${filled} of 10`}>
      {Array.from({ length: 10 }, (_, i) => (
        <span
          key={i}
          className={`pvm-dot ${i < filled ? `filled ${color}` : ''}`}
        />
      ))}
    </span>
  );
}

/** Trend arrow indicator */
function TrendArrow({ trend }: { trend: TrendDirection }) {
  if (trend === 'insufficient') return null;

  const arrow = trend === 'improving' ? '\u2197' : trend === 'declining' ? '\u2198' : '\u2192';
  return <span className={`pvm-trend-arrow ${trend}`}>{arrow}</span>;
}

/** Single patient's metrics */
function PatientRow({ patient, showLabel }: { patient: PatientMetrics; showLabel: boolean }) {
  return (
    <div className="pvm-patient-block">
      {showLabel && (
        <div className="pvm-speaker-label">{patient.speakerId}</div>
      )}

      {/* Voice Energy (vitality) */}
      {patient.vitality !== null && (
        <div className="pvm-metric-row">
          <span className="pvm-metric-label">Voice Energy</span>
          <DotBar
            value={patient.vitality}
            max={BIOMARKER_THRESHOLDS.VITALITY_MAX_DISPLAY}
            goodThreshold={BIOMARKER_THRESHOLDS.VITALITY_GOOD}
            warnThreshold={BIOMARKER_THRESHOLDS.VITALITY_WARNING}
          />
          <span className="pvm-metric-value">{patient.vitality.toFixed(0)} Hz</span>
          <TrendArrow trend={patient.vitalityTrend} />
        </div>
      )}

      {/* Vocal Control (stability) */}
      {patient.stability !== null && (
        <div className="pvm-metric-row">
          <span className="pvm-metric-label">Vocal Ctrl</span>
          <DotBar
            value={patient.stability}
            max={BIOMARKER_THRESHOLDS.STABILITY_MAX_DISPLAY}
            goodThreshold={BIOMARKER_THRESHOLDS.STABILITY_GOOD}
            warnThreshold={BIOMARKER_THRESHOLDS.STABILITY_WARNING}
          />
          <span className="pvm-metric-value">{patient.stability.toFixed(1)} dB</span>
          <TrendArrow trend={patient.stabilityTrend} />
        </div>
      )}

      {/* Talk Share */}
      <div className="pvm-metric-row">
        <span className="pvm-metric-label">Talk Share</span>
        <span className="pvm-metric-value pvm-talk-share">{patient.talkSharePct.toFixed(0)}%</span>
      </div>
    </div>
  );
}

// ============================================================================
// Main Component
// ============================================================================

export const PatientVoiceMonitor = memo(function PatientVoiceMonitor({ data }: PatientVoiceMonitorProps) {
  if (!data.hasData) return null;

  const showSpeakerLabels = data.patients.length > 1;

  return (
    <div className="patient-voice-monitor">
      <div className="pvm-header">Patient Voice</div>

      {/* Engagement score */}
      {data.engagementScore !== null && (
        <div className="pvm-metric-row">
          <span className="pvm-metric-label">Engagement</span>
          <DotBar
            value={data.engagementScore}
            max={100}
            goodThreshold={BIOMARKER_THRESHOLDS.ENGAGEMENT_GOOD}
            warnThreshold={BIOMARKER_THRESHOLDS.ENGAGEMENT_WARNING}
          />
          <span className="pvm-metric-value">{data.engagementScore.toFixed(0)}</span>
        </div>
      )}

      {/* Per-patient metrics */}
      {data.patients.map(patient => (
        <PatientRow
          key={patient.speakerId}
          patient={patient}
          showLabel={showSpeakerLabels}
        />
      ))}

      {/* Clinical insight banner */}
      {data.insight && (
        <div className="pvm-insight">
          <span className="pvm-insight-icon">i</span>
          {data.insight}
        </div>
      )}

      {/* Cough count */}
      {data.coughCount > 0 && (
        <div className="pvm-cough-row">
          Coughs: {data.coughCount} ({data.coughRatePerMin.toFixed(1)}/min)
        </div>
      )}
    </div>
  );
});
