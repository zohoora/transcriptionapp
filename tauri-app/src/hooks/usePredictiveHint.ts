import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

const HINT_INTERVAL_MS = 30000; // 30 seconds
const INITIAL_DELAY_MS = 5000; // First hint after 5 seconds
const MIN_WORDS_FOR_HINT = 20;

/** A differential diagnosis suggestion */
export interface DifferentialDiagnosis {
  diagnosis: string;
  likelihood: 'likely' | 'possible' | 'less_likely';
  key_findings: string[];
}

export const DDX_LIKELIHOOD_LABELS: Record<DifferentialDiagnosis['likelihood'], string> = {
  likely: 'Likely',
  possible: 'Possible',
  less_likely: 'Less likely',
};

/** Response from predictive hint generation */
interface PredictiveHintResponse {
  hint: string;
  image_prompt: string | null;
  differential_diagnoses: DifferentialDiagnosis[];
}

interface UsePredictiveHintOptions {
  transcript: string;
  isRecording: boolean;
  /**
   * Optional key that, when changed, clears all hint state and re-arms the
   * 5s INITIAL_DELAY_MS so the next encounter gets a fast first hint.
   * Continuous mode passes `encounterSessionId`; session-mode callers omit it.
   */
  resetKey?: string;
}

interface UsePredictiveHintResult {
  /** Brief clinical hint for the physician */
  hint: string;
  /** Image generation prompt from LLM (null if no image needed) */
  imagePrompt: string | null;
  /** Top 3 differential diagnoses */
  differentialDiagnoses: DifferentialDiagnosis[];
  /** Whether a hint is being generated */
  isLoading: boolean;
  /** Timestamp of last update */
  lastUpdated: number | null;
}

/**
 * Hook to generate predictive hints during recording sessions.
 * Every 30 seconds, sends the transcript to the LLM to predict
 * what information the physician might want to know.
 */
export function usePredictiveHint({
  transcript,
  isRecording,
  resetKey,
}: UsePredictiveHintOptions): UsePredictiveHintResult {
  const [hint, setHint] = useState('');
  const [imagePrompt, setImagePrompt] = useState<string | null>(null);
  const [differentialDiagnoses, setDifferentialDiagnoses] = useState<DifferentialDiagnosis[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [lastUpdated, setLastUpdated] = useState<number | null>(null);

  // Refs to avoid stale closures in intervals/timeouts
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const transcriptRef = useRef(transcript);
  const lastGeneratedRef = useRef('');
  const isLoadingRef = useRef(false);

  // Keep transcript ref current
  useEffect(() => {
    transcriptRef.current = transcript;
  }, [transcript]);

  const clearHintState = useCallback(() => {
    setHint('');
    setImagePrompt(null);
    setDifferentialDiagnoses([]);
    setLastUpdated(null);
    lastGeneratedRef.current = '';
  }, []);

  const generateHint = useCallback(async () => {
    // Read from ref to get current transcript value
    const text = transcriptRef.current;

    // Skip if already loading
    if (isLoadingRef.current) {
      return;
    }

    // Skip if not enough content
    const wordCount = text.trim().split(/\s+/).length;
    if (wordCount < MIN_WORDS_FOR_HINT) {
      console.log(`Predictive hint: Not enough words (${wordCount} < ${MIN_WORDS_FOR_HINT})`);
      return;
    }

    // Skip if transcript hasn't changed significantly
    if (text === lastGeneratedRef.current) {
      console.log('Predictive hint: Transcript unchanged, skipping');
      return;
    }

    console.log(`Predictive hint: Generating for ${wordCount} words`);
    setIsLoading(true);
    isLoadingRef.current = true;

    try {
      const result = await invoke<PredictiveHintResponse>('generate_predictive_hint', {
        transcript: text,
      });

      if (result) {
        if (result.hint && result.hint.trim()) {
          console.log('Predictive hint received:', result.hint.substring(0, 50) + '...');
          setHint(result.hint);
        }
        setImagePrompt(result.image_prompt ?? null);
        if (result.differential_diagnoses && result.differential_diagnoses.length > 0) {
          setDifferentialDiagnoses(result.differential_diagnoses);
        }
        setLastUpdated(Date.now());
        lastGeneratedRef.current = text;
      }
    } catch (error) {
      console.error('Failed to generate predictive hint:', error);
    } finally {
      setIsLoading(false);
      isLoadingRef.current = false;
    }
  }, []);

  // Set up interval when recording starts. resetKey is in the dep array so a
  // continuous-mode encounter split tears down the running 30s interval and
  // re-arms a fresh 5s INITIAL_DELAY_MS for the next encounter.
  useEffect(() => {
    if (isRecording) {
      console.log('Predictive hint: Starting hint generation timer');

      // Generate initial hint after a short delay
      timeoutRef.current = setTimeout(() => {
        generateHint();
      }, INITIAL_DELAY_MS);

      // Then every 30 seconds
      intervalRef.current = setInterval(() => {
        generateHint();
      }, HINT_INTERVAL_MS);

      return () => {
        console.log('Predictive hint: Cleaning up timers');
        if (timeoutRef.current) {
          clearTimeout(timeoutRef.current);
          timeoutRef.current = null;
        }
        if (intervalRef.current) {
          clearInterval(intervalRef.current);
          intervalRef.current = null;
        }
      };
    } else {
      clearHintState();
    }
  }, [isRecording, generateHint, resetKey, clearHintState]);

  // Reset hint state when a new encounter starts so the next patient sees a
  // clean panel instead of stale predictions from the previous encounter.
  useEffect(() => {
    if (resetKey === undefined) return;
    clearHintState();
  }, [resetKey, clearHintState]);

  return {
    hint,
    imagePrompt,
    differentialDiagnoses,
    isLoading,
    lastUpdated,
  };
}
