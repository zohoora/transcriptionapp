/**
 * useContinuousModeOrchestrator — Composite hook that groups all continuous-mode-related
 * state from App.tsx into a single hook.
 *
 * Internally composes:
 * - useContinuousMode() — core continuous mode state
 * - usePatientBiomarkers(isActive) — patient-focused biomarker trending
 * - usePredictiveHint({...}) — predictive hints for continuous mode
 * - useMiisImages({...}) — MIIS image suggestions for continuous mode
 * - continuousMiisSessionId state + useEffect (stable session ID)
 * - handleNewPatient callback (resets biomarkers before triggerNewPatient)
 */
import { useState, useEffect, useCallback } from 'react';
import type { Settings, ContinuousModeStats, AudioQualitySnapshot, BiomarkerUpdate } from '../types';
import type { PatientTrends } from './usePatientBiomarkers';
import type { MiisSuggestion } from './useMiisImages';
import type { AiImage } from './useAiImages';
import { useContinuousMode } from './useContinuousMode';
import { usePatientBiomarkers } from './usePatientBiomarkers';
import { usePredictiveHint } from './usePredictiveHint';
import { useMiisImages } from './useMiisImages';
import { useAiImages } from './useAiImages';

// ============================================================================
// Types
// ============================================================================

export interface ContinuousModeOrchestratorConfig {
  settings: Settings | null;
}

export interface ContinuousModeOrchestratorResult {
  // Needed by App.tsx directly
  isActive: boolean;

  // All ContinuousMode component props (except onViewHistory, which comes from App.tsx)
  isStopping: boolean;
  stats: ContinuousModeStats;
  liveTranscript: string;
  error: string | null;
  predictiveHint: string;
  predictiveHintLoading: boolean;
  audioQuality: AudioQualitySnapshot | null;
  biomarkers: BiomarkerUpdate | null;
  biomarkerTrends: PatientTrends;
  encounterNotes: string;
  onEncounterNotesChange: (notes: string) => void;
  miisSuggestions: MiisSuggestion[];
  miisLoading: boolean;
  miisError: string | null;
  miisEnabled: boolean;
  onMiisImpression: (imageId: number) => void;
  onMiisClick: (imageId: number) => void;
  onMiisDismiss: (imageId: number) => void;
  miisGetImageUrl: (path: string) => string;
  // AI-generated images
  aiImages: AiImage[];
  aiLoading: boolean;
  aiError: string | null;
  onAiDismiss: (index: number) => void;
  imageSource: 'miis' | 'ai' | 'off';
  onStart: () => void;
  onStop: () => void;
  onNewPatient: () => void;
}

// ============================================================================
// Hook
// ============================================================================

export function useContinuousModeOrchestrator({
  settings,
}: ContinuousModeOrchestratorConfig): ContinuousModeOrchestratorResult {
  // Core continuous mode state
  const {
    isActive,
    isStopping,
    stats,
    liveTranscript,
    audioQuality,
    encounterNotes,
    setEncounterNotes,
    start,
    stop,
    triggerNewPatient,
    error,
  } = useContinuousMode();

  // Patient biomarker trending (filters clinician voices, tracks trends)
  const {
    biomarkers: continuousBiomarkers,
    trends: continuousTrends,
    reset: resetPatientBiomarkers,
  } = usePatientBiomarkers(isActive);

  // Predictive hints for continuous mode (uses live transcript preview)
  const {
    hint: predictiveHint,
    concepts: continuousImageConcepts,
    imagePrompt: continuousImagePrompt,
    isLoading: predictiveHintLoading,
  } = usePredictiveHint({
    transcript: liveTranscript,
    isRecording: isActive && !isStopping,
  });

  // Stable session ID for continuous mode MIIS (set on start, cleared on stop)
  const [continuousMiisSessionId, setContinuousMiisSessionId] = useState<string | null>(null);
  useEffect(() => {
    if (isActive && !continuousMiisSessionId) {
      setContinuousMiisSessionId(`continuous-${Date.now()}`);
    } else if (!isActive && continuousMiisSessionId) {
      setContinuousMiisSessionId(null);
    }
  }, [isActive, continuousMiisSessionId]);

  // MIIS image suggestions for continuous mode
  const {
    suggestions: miisSuggestions,
    isLoading: miisLoading,
    error: miisError,
    recordImpression: miisRecordImpression,
    recordClick: miisRecordClick,
    recordDismiss: miisRecordDismiss,
    getImageUrl: miisGetImageUrl,
  } = useMiisImages({
    sessionId: continuousMiisSessionId,
    concepts: continuousImageConcepts,
    enabled: (settings?.image_source ?? 'off') === 'miis',
    serverUrl: settings?.miis_server_url ?? '',
  });

  const continuousImageSource = (settings?.image_source ?? 'off') as 'off' | 'miis' | 'ai';

  // AI-generated image suggestions for continuous mode
  const {
    images: aiImages,
    isLoading: aiLoading,
    error: aiError,
    dismissImage: aiDismiss,
  } = useAiImages({
    imagePrompt: continuousImagePrompt,
    enabled: continuousImageSource === 'ai',
    sessionId: continuousMiisSessionId,
  });

  // Wrap "New Patient" to immediately reset frontend state before backend processes
  const handleNewPatient = useCallback(async () => {
    resetPatientBiomarkers();
    await triggerNewPatient();
  }, [resetPatientBiomarkers, triggerNewPatient]);

  return {
    // Needed by App.tsx directly
    isActive,

    // ContinuousMode component props
    isStopping,
    stats,
    liveTranscript,
    error,
    predictiveHint,
    predictiveHintLoading,
    audioQuality,
    biomarkers: continuousBiomarkers,
    biomarkerTrends: continuousTrends,
    encounterNotes,
    onEncounterNotesChange: setEncounterNotes,
    miisSuggestions,
    miisLoading,
    miisError,
    miisEnabled: continuousImageSource !== 'off',
    onMiisImpression: miisRecordImpression,
    onMiisClick: miisRecordClick,
    onMiisDismiss: miisRecordDismiss,
    miisGetImageUrl,
    aiImages,
    aiLoading,
    aiError,
    onAiDismiss: aiDismiss,
    imageSource: continuousImageSource,
    onStart: start,
    onStop: stop,
    onNewPatient: handleNewPatient,
  };
}
