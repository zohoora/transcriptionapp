import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

const HINT_INTERVAL_MS = 30000; // 30 seconds
const INITIAL_DELAY_MS = 5000; // First hint after 5 seconds
const MIN_WORDS_FOR_HINT = 20;

/** Medical concept for MIIS image search */
export interface ImageConcept {
  /** The concept text (e.g., "knee anatomy", "rotator cuff") */
  text: string;
  /** Relevance weight 0.0-1.0 */
  weight: number;
}

/** Response from predictive hint generation */
interface PredictiveHintResponse {
  hint: string;
  concepts: ImageConcept[];
}

interface UsePredictiveHintOptions {
  transcript: string;
  isRecording: boolean;
}

interface UsePredictiveHintResult {
  /** Brief clinical hint for the physician */
  hint: string;
  /** Medical concepts for MIIS image search */
  concepts: ImageConcept[];
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
}: UsePredictiveHintOptions): UsePredictiveHintResult {
  const [hint, setHint] = useState('');
  const [concepts, setConcepts] = useState<ImageConcept[]>([]);
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
        if (result.concepts && result.concepts.length > 0) {
          console.log('Image concepts received:', result.concepts.map(c => c.text).join(', '));
          setConcepts(result.concepts);
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

  // Set up interval when recording starts
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
      // Clear state when not recording
      setHint('');
      setConcepts([]);
      setLastUpdated(null);
      lastGeneratedRef.current = '';
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
        timeoutRef.current = null;
      }
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    }
  }, [isRecording, generateHint]);

  return {
    hint,
    concepts,
    isLoading,
    lastUpdated,
  };
}
