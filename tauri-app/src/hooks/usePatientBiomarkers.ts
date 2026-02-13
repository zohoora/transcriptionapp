/**
 * usePatientBiomarkers — Patient-focused biomarker trending for continuous mode.
 *
 * Listens to `biomarker_update` events, stores the latest raw update for
 * PatientPulse, aggregates non-clinician speakers into one "patient" via
 * weighted average, and computes per-encounter baseline trends on the aggregate.
 *
 * Session mode doesn't need this hook — PatientPulse handles aggregation
 * internally from the raw `biomarkers` prop.
 */
import { useState, useEffect, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { BiomarkerUpdate, SpeakerBiomarkers } from '../types';

// ============================================================================
// Types
// ============================================================================

export type TrendDirection = 'improving' | 'stable' | 'declining' | 'insufficient';

// Legacy types — kept for backward compatibility (PatientVoiceMonitor)
export interface PatientMetrics {
  speakerId: string;
  vitality: number | null;
  stability: number | null;
  talkSharePct: number;
  utteranceCount: number;
  vitalityTrend: TrendDirection;
  stabilityTrend: TrendDirection;
}

export interface PatientBiomarkerData {
  patients: PatientMetrics[];
  coughCount: number;
  coughRatePerMin: number;
  engagementScore: number | null;
  insight: string | null;
  hasData: boolean;
}

export interface PatientTrends {
  vitalityTrend: TrendDirection;
  stabilityTrend: TrendDirection;
}

// ============================================================================
// Constants
// ============================================================================

/** Minimum total utterances before establishing baseline */
const BASELINE_MIN_UTTERANCES = 3;

/** Percent change threshold for trend detection (15%) */
const TREND_THRESHOLD = 0.15;

// ============================================================================
// Aggregation utility
// ============================================================================

interface AggregatedBaseline {
  vitality: number | null;
  stability: number | null;
}

/**
 * Pool all non-clinician speakers into one aggregate via weighted average
 * by talk_time_ms. Returns aggregated vitality, stability, and total utterances.
 */
function aggregatePatientSpeakers(speakers: SpeakerBiomarkers[]): {
  vitality: number | null;
  stability: number | null;
  totalUtterances: number;
} {
  const hasClinicians = speakers.some(s => s.is_clinician);
  const patients = hasClinicians ? speakers.filter(s => !s.is_clinician) : speakers;

  if (patients.length === 0) {
    return { vitality: null, stability: null, totalUtterances: 0 };
  }

  const totalTalkTime = patients.reduce((sum, s) => sum + s.talk_time_ms, 0);
  const totalUtterances = patients.reduce((sum, s) => sum + s.utterance_count, 0);

  let vitality: number | null = null;
  let stability: number | null = null;

  if (totalTalkTime > 0) {
    const vSpeakers = patients.filter(s => s.vitality_mean !== null);
    if (vSpeakers.length > 0) {
      const vTalkTime = vSpeakers.reduce((sum, s) => sum + s.talk_time_ms, 0);
      if (vTalkTime > 0) {
        vitality = vSpeakers.reduce(
          (sum, s) => sum + (s.vitality_mean! * s.talk_time_ms), 0,
        ) / vTalkTime;
      }
    }

    const sSpeakers = patients.filter(s => s.stability_mean !== null);
    if (sSpeakers.length > 0) {
      const sTalkTime = sSpeakers.reduce((sum, s) => sum + s.talk_time_ms, 0);
      if (sTalkTime > 0) {
        stability = sSpeakers.reduce(
          (sum, s) => sum + (s.stability_mean! * s.talk_time_ms), 0,
        ) / sTalkTime;
      }
    }
  }

  return { vitality, stability, totalUtterances };
}

// ============================================================================
// Trend computation
// ============================================================================

function computeTrend(current: number | null, baseline: number | null): TrendDirection {
  if (current === null || baseline === null || baseline === 0) return 'insufficient';

  const change = (current - baseline) / Math.abs(baseline);
  if (change > TREND_THRESHOLD) return 'improving';
  if (change < -TREND_THRESHOLD) return 'declining';
  return 'stable';
}

// ============================================================================
// Hook
// ============================================================================

const EMPTY_TRENDS: PatientTrends = {
  vitalityTrend: 'insufficient',
  stabilityTrend: 'insufficient',
};

export interface UsePatientBiomarkersResult {
  /** Latest raw biomarker update (passed to PatientPulse) */
  biomarkers: BiomarkerUpdate | null;
  /** Aggregated trends from baseline (only meaningful after baseline captured) */
  trends: PatientTrends;
  /** Immediately reset all metrics (call on manual "New Patient" click) */
  reset: () => void;
}

export function usePatientBiomarkers(isActive: boolean): UsePatientBiomarkersResult {
  const [biomarkers, setBiomarkers] = useState<BiomarkerUpdate | null>(null);
  const [trends, setTrends] = useState<PatientTrends>(EMPTY_TRENDS);

  // Baseline snapshot — stored as ref to avoid re-renders on capture
  const baselineRef = useRef<AggregatedBaseline | null>(null);
  const baselineCapturedRef = useRef(false);

  // Reset baseline on encounter boundary
  const reset = useCallback(() => {
    baselineRef.current = null;
    baselineCapturedRef.current = false;
    setBiomarkers(null);
    setTrends(EMPTY_TRENDS);
  }, []);

  useEffect(() => {
    if (!isActive) {
      reset();
      return;
    }

    let mounted = true;

    // Listen for biomarker_update events
    const unlistenBiomarker = listen<BiomarkerUpdate>('biomarker_update', (event) => {
      if (!mounted) return;

      const update = event.payload;

      // Store raw update for PatientPulse
      setBiomarkers(update);

      // Aggregate patient speakers for trend computation
      const agg = aggregatePatientSpeakers(update.speaker_metrics);

      // Capture baseline on first adequate data
      if (!baselineCapturedRef.current && agg.totalUtterances >= BASELINE_MIN_UTTERANCES) {
        baselineRef.current = {
          vitality: agg.vitality,
          stability: agg.stability,
        };
        baselineCapturedRef.current = true;
      }

      // Compute trends against baseline
      if (baselineCapturedRef.current && baselineRef.current) {
        setTrends({
          vitalityTrend: computeTrend(agg.vitality, baselineRef.current.vitality),
          stabilityTrend: computeTrend(agg.stability, baselineRef.current.stability),
        });
      }
    });

    // Listen for encounter_detected → reset baseline for next encounter
    const unlistenEncounter = listen('continuous_mode_event', (event) => {
      if (!mounted) return;
      const payload = event.payload as { type: string };
      if (payload.type === 'encounter_detected') {
        reset();
      }
    });

    return () => {
      mounted = false;
      unlistenBiomarker.then(fn => fn());
      unlistenEncounter.then(fn => fn());
    };
  }, [isActive, reset]);

  return { biomarkers, trends, reset };
}
