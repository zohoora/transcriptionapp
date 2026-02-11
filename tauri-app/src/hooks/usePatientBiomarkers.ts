/**
 * usePatientBiomarkers — Patient-focused biomarker trending for continuous mode.
 *
 * Listens to `biomarker_update` events, filters to patient voices only
 * (enrolled clinical staff excluded), computes per-encounter baselines
 * and trends, and generates clinical insight text.
 */
import { useState, useEffect, useRef, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { BiomarkerUpdate, SpeakerBiomarkers } from '../types';

// ============================================================================
// Types
// ============================================================================

export type TrendDirection = 'improving' | 'stable' | 'declining' | 'insufficient';

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

// ============================================================================
// Constants
// ============================================================================

/** Minimum utterances before establishing baseline */
const BASELINE_MIN_UTTERANCES = 3;

/** Percent change threshold for trend detection (15%) */
const TREND_THRESHOLD = 0.15;

/** Cough rate threshold for insight (coughs per minute) */
const COUGH_RATE_INSIGHT_THRESHOLD = 2.0;

/** Talk share threshold for "patient speaking less" insight */
const TALK_SHARE_LOW_THRESHOLD = 10; // percent

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
// Insight generation (threshold-based, not LLM)
// ============================================================================

function generateInsight(patients: PatientMetrics[], coughRatePerMin: number): string | null {
  // Priority order: stability declining > vitality declining > talk share dropping > coughing
  for (const p of patients) {
    if (p.stabilityTrend === 'declining') {
      return 'Vocal strain increasing';
    }
  }

  for (const p of patients) {
    if (p.vitalityTrend === 'declining' && p.stabilityTrend !== 'declining') {
      return 'Voice becoming more monotone';
    }
  }

  for (const p of patients) {
    if (p.talkSharePct < TALK_SHARE_LOW_THRESHOLD && p.utteranceCount >= BASELINE_MIN_UTTERANCES) {
      return 'Patient speaking less';
    }
  }

  if (coughRatePerMin > COUGH_RATE_INSIGHT_THRESHOLD) {
    return 'Frequent coughing detected';
  }

  return null;
}

// ============================================================================
// Hook
// ============================================================================

const EMPTY_DATA: PatientBiomarkerData = {
  patients: [],
  coughCount: 0,
  coughRatePerMin: 0,
  engagementScore: null,
  insight: null,
  hasData: false,
};

interface Baseline {
  speakerId: string;
  vitality: number | null;
  stability: number | null;
  talkSharePct: number;
}

export function usePatientBiomarkers(isActive: boolean): PatientBiomarkerData {
  const [data, setData] = useState<PatientBiomarkerData>(EMPTY_DATA);

  // Baseline snapshot — stored as ref to avoid re-renders on capture
  const baselineRef = useRef<Baseline[]>([]);
  const baselineCapturedRef = useRef(false);

  // Track previous talk share for "speaking less" insight
  const prevTalkShareRef = useRef<Map<string, number>>(new Map());

  // Reset baseline on encounter boundary
  const resetBaseline = useCallback(() => {
    baselineRef.current = [];
    baselineCapturedRef.current = false;
    prevTalkShareRef.current.clear();
    setData(EMPTY_DATA);
  }, []);

  useEffect(() => {
    if (!isActive) {
      resetBaseline();
      return;
    }

    let mounted = true;

    // Listen for biomarker_update events
    const unlistenBiomarker = listen<BiomarkerUpdate>('biomarker_update', (event) => {
      if (!mounted) return;

      const update = event.payload;

      // Filter to patient speakers only (exclude enrolled clinicians)
      // If no speakers are marked as clinicians, show all (no enrollment = no filtering)
      const hasClinicians = update.speaker_metrics.some(s => s.is_clinician);
      const patientSpeakers: SpeakerBiomarkers[] = hasClinicians
        ? update.speaker_metrics.filter(s => !s.is_clinician)
        : update.speaker_metrics;

      // Calculate total talk time across all speakers for talk share %
      const totalTalkTimeMs = update.speaker_metrics.reduce((sum, s) => sum + s.talk_time_ms, 0);

      // Build patient metrics
      const patients: PatientMetrics[] = patientSpeakers.map(s => {
        const talkSharePct = totalTalkTimeMs > 0
          ? (s.talk_time_ms / totalTalkTimeMs) * 100
          : 0;

        // Capture baseline if not yet captured and this speaker has enough data
        if (!baselineCapturedRef.current && s.utterance_count >= BASELINE_MIN_UTTERANCES) {
          // Capture baseline for ALL patient speakers at this point
          baselineRef.current = patientSpeakers.map(ps => ({
            speakerId: ps.speaker_id,
            vitality: ps.vitality_mean,
            stability: ps.stability_mean,
            talkSharePct: totalTalkTimeMs > 0
              ? (ps.talk_time_ms / totalTalkTimeMs) * 100
              : 0,
          }));
          baselineCapturedRef.current = true;
        }

        // Find this speaker's baseline
        const baseline = baselineRef.current.find(b => b.speakerId === s.speaker_id);

        const vitalityTrend = baselineCapturedRef.current
          ? computeTrend(s.vitality_mean, baseline?.vitality ?? null)
          : 'insufficient' as TrendDirection;

        const stabilityTrend = baselineCapturedRef.current
          ? computeTrend(s.stability_mean, baseline?.stability ?? null)
          : 'insufficient' as TrendDirection;

        // Track talk share history for "speaking less" insight
        prevTalkShareRef.current.set(s.speaker_id, talkSharePct);

        return {
          speakerId: s.speaker_id,
          vitality: s.vitality_mean,
          stability: s.stability_mean,
          talkSharePct,
          utteranceCount: s.utterance_count,
          vitalityTrend,
          stabilityTrend,
        };
      });

      // Extract engagement score from conversation dynamics
      const engagementScore = update.conversation_dynamics?.engagement_score ?? null;

      // Generate clinical insight
      const insight = generateInsight(patients, update.cough_rate_per_min);

      // Determine if we have enough data to render
      const hasData = patients.length > 0 && patients.some(p => p.utteranceCount >= 1);

      setData({
        patients,
        coughCount: update.cough_count,
        coughRatePerMin: update.cough_rate_per_min,
        engagementScore,
        insight,
        hasData,
      });
    });

    // Listen for encounter_detected → reset baseline for next encounter
    const unlistenEncounter = listen('continuous_mode_event', (event) => {
      if (!mounted) return;
      const payload = event.payload as { type: string };
      if (payload.type === 'encounter_detected') {
        resetBaseline();
      }
    });

    return () => {
      mounted = false;
      unlistenBiomarker.then(fn => fn());
      unlistenEncounter.then(fn => fn());
    };
  }, [isActive, resetBaseline]);

  return data;
}
