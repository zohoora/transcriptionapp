/**
 * PatientPulse — Glanceable "check engine light" for patient voice metrics.
 *
 * Replaces the busy BiomarkersSection and PatientVoiceMonitor with a single
 * minimal display: invisible when normal, clear when something is noteworthy.
 *
 * Three states:
 *   Hidden (0px)  — Not enough data yet (<3 utterances)
 *   Normal (32px) — Green dot, "Patient voice normal"
 *   Alert  (60px) — Amber/red card with only the metric(s) that crossed thresholds
 *
 * All non-clinician speakers are pooled into one "patient" via weighted average
 * by talk_time_ms, eliminating noisy per-speaker breakdowns from VAD splits.
 */
import { memo, useMemo } from 'react';
import type { BiomarkerUpdate } from '../types';
import { BIOMARKER_THRESHOLDS } from '../types';
import type { TrendDirection } from '../hooks/usePatientBiomarkers';
import { aggregatePatientSpeakers } from '../utils';

// ============================================================================
// Types
// ============================================================================

type PulseState = 'hidden' | 'normal' | 'attention' | 'alert';

interface PulseAlert {
  text: string;
  metricLabel: string;
  value: number;
  max: number;
  unit: string;
  severity: 'attention' | 'alert';
}

interface AggregatedPatient {
  vitality: number | null;
  stability: number | null;
  engagement: number | null;
  totalUtterances: number;
}

export interface PatientPulseProps {
  biomarkers: BiomarkerUpdate | null;
  trends?: {
    vitalityTrend: TrendDirection;
    stabilityTrend: TrendDirection;
  };
}

// ============================================================================
// Constants
// ============================================================================

/** Minimum utterances across all patient speakers before showing anything */
const MIN_UTTERANCES = 3;

// ============================================================================
// Aggregation
// ============================================================================

/**
 * Pool all non-clinician speakers into one aggregate patient via weighted
 * average by talk_time_ms, adding engagement from conversation dynamics.
 * Delegates core aggregation to shared utility.
 */
function aggregatePatientMetrics(
  speakers: import('../types').SpeakerBiomarkers[],
  engagementScore: number | null,
): AggregatedPatient {
  const { vitality, stability, totalUtterances } = aggregatePatientSpeakers(speakers);
  return { vitality, stability, engagement: engagementScore, totalUtterances };
}

// ============================================================================
// Alert logic
// ============================================================================

/**
 * Determine pulse state and any alerts based on aggregated metrics and trends.
 */
function determinePulseState(
  patient: AggregatedPatient,
  trends?: { vitalityTrend: TrendDirection; stabilityTrend: TrendDirection },
): { state: PulseState; alerts: PulseAlert[] } {
  if (patient.totalUtterances < MIN_UTTERANCES) {
    return { state: 'hidden', alerts: [] };
  }

  const alerts: PulseAlert[] = [];

  // Check vitality (Voice Energy)
  if (patient.vitality !== null) {
    if (patient.vitality < BIOMARKER_THRESHOLDS.VITALITY_WARNING) {
      alerts.push({
        text: 'Flat affect detected',
        metricLabel: 'Voice Energy',
        value: patient.vitality,
        max: BIOMARKER_THRESHOLDS.VITALITY_MAX_DISPLAY,
        unit: 'Hz',
        severity: 'alert',
      });
    } else if (patient.vitality < BIOMARKER_THRESHOLDS.VITALITY_GOOD) {
      alerts.push({
        text: 'Reduced expression',
        metricLabel: 'Voice Energy',
        value: patient.vitality,
        max: BIOMARKER_THRESHOLDS.VITALITY_MAX_DISPLAY,
        unit: 'Hz',
        severity: 'attention',
      });
    }
  }

  // Check stability (Vocal Control)
  if (patient.stability !== null) {
    if (patient.stability < BIOMARKER_THRESHOLDS.STABILITY_WARNING) {
      alerts.push({
        text: 'Vocal strain',
        metricLabel: 'Vocal Control',
        value: patient.stability,
        max: BIOMARKER_THRESHOLDS.STABILITY_MAX_DISPLAY,
        unit: 'dB',
        severity: 'alert',
      });
    } else if (patient.stability < BIOMARKER_THRESHOLDS.STABILITY_GOOD) {
      alerts.push({
        text: 'Mild vocal irregularity',
        metricLabel: 'Vocal Control',
        value: patient.stability,
        max: BIOMARKER_THRESHOLDS.STABILITY_MAX_DISPLAY,
        unit: 'dB',
        severity: 'attention',
      });
    }
  }

  // Check engagement
  if (patient.engagement !== null) {
    if (patient.engagement < BIOMARKER_THRESHOLDS.ENGAGEMENT_WARNING) {
      alerts.push({
        text: 'Patient disengaging',
        metricLabel: 'Engagement',
        value: patient.engagement,
        max: 100,
        unit: '',
        severity: 'alert',
      });
    } else if (patient.engagement < BIOMARKER_THRESHOLDS.ENGAGEMENT_GOOD) {
      alerts.push({
        text: 'Low engagement',
        metricLabel: 'Engagement',
        value: patient.engagement,
        max: 100,
        unit: '',
        severity: 'attention',
      });
    }
  }

  // Trend-based alerts (continuous mode) — surface declining trends even if
  // absolute values haven't crossed thresholds yet
  if (trends) {
    if (trends.vitalityTrend === 'declining' && !alerts.some(a => a.metricLabel === 'Voice Energy')) {
      if (patient.vitality !== null) {
        alerts.push({
          text: 'Voice energy declining',
          metricLabel: 'Voice Energy',
          value: patient.vitality,
          max: BIOMARKER_THRESHOLDS.VITALITY_MAX_DISPLAY,
          unit: 'Hz',
          severity: 'attention',
        });
      }
    }
    if (trends.stabilityTrend === 'declining' && !alerts.some(a => a.metricLabel === 'Vocal Control')) {
      if (patient.stability !== null) {
        alerts.push({
          text: 'Vocal control declining',
          metricLabel: 'Vocal Control',
          value: patient.stability,
          max: BIOMARKER_THRESHOLDS.STABILITY_MAX_DISPLAY,
          unit: 'dB',
          severity: 'attention',
        });
      }
    }
  }

  if (alerts.length === 0) {
    return { state: 'normal', alerts: [] };
  }

  const hasAlertSeverity = alerts.some(a => a.severity === 'alert');
  return { state: hasAlertSeverity ? 'alert' : 'attention', alerts };
}

// ============================================================================
// Sub-components
// ============================================================================

/** Compact 40px capsule progress bar (only shown on alert cards) */
function MiniBar({ value, max }: { value: number; max: number }) {
  const pct = Math.min(Math.max(value / max, 0), 1) * 100;
  return (
    <span className="pulse-mini-bar" aria-label={`${Math.round(pct)}%`}>
      <span className="pulse-mini-bar-fill" style={{ width: `${pct}%` }} />
    </span>
  );
}

// ============================================================================
// Main component
// ============================================================================

/**
 * PatientPulse — replaces BiomarkersSection and PatientVoiceMonitor.
 *
 * Shows nothing when data is insufficient, a muted "normal" row when
 * everything is fine, and a compact alert card only when metrics cross
 * clinical thresholds.
 */
export const PatientPulse = memo(function PatientPulse({ biomarkers, trends }: PatientPulseProps) {
  const { state, alerts } = useMemo(() => {
    if (!biomarkers) return { state: 'hidden' as PulseState, alerts: [] as PulseAlert[] };

    const patient = aggregatePatientMetrics(
      biomarkers.speaker_metrics,
      biomarkers.conversation_dynamics?.engagement_score ?? null,
    );
    return determinePulseState(patient, trends);
  }, [biomarkers, trends]);

  // Hidden — not enough data yet
  if (state === 'hidden') return null;

  // Normal — single muted row
  if (state === 'normal') {
    return (
      <div className="patient-pulse normal">
        <span className="pulse-status-dot normal" />
        <span className="pulse-status-text">Patient voice normal</span>
      </div>
    );
  }

  // Attention or alert — compact card with left accent border
  const displayAlerts = alerts.slice(0, 3);

  return (
    <div className={`patient-pulse ${state}`}>
      <div className="pulse-alert-header">
        <span className="pulse-alert-icon">{state === 'alert' ? '\u26A0' : '\u26A0'}</span>
        <span className="pulse-alert-text">{displayAlerts[0].text}</span>
      </div>
      {displayAlerts.map((alert, i) => (
        <div key={i} className="pulse-metric-row">
          <span className="pulse-metric-label">{alert.metricLabel}</span>
          <MiniBar value={alert.value} max={alert.max} />
          <span className="pulse-metric-value">
            {alert.unit
              ? `${alert.value.toFixed(alert.unit === 'Hz' ? 0 : 1)} ${alert.unit}`
              : alert.value.toFixed(0)}
          </span>
        </div>
      ))}
    </div>
  );
});
