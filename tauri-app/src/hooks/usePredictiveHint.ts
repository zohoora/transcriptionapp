import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

const HINT_INTERVAL_MS = 30000; // 30 seconds
const MIN_WORDS_FOR_HINT = 20;

interface UsePredictiveHintOptions {
  transcript: string;
  isRecording: boolean;
}

interface UsePredictiveHintResult {
  hint: string;
  isLoading: boolean;
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
  const [isLoading, setIsLoading] = useState(false);
  const [lastUpdated, setLastUpdated] = useState<number | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const lastTranscriptRef = useRef('');

  const generateHint = useCallback(async (text: string) => {
    // Skip if not enough content
    const wordCount = text.trim().split(/\s+/).length;
    if (wordCount < MIN_WORDS_FOR_HINT) {
      return;
    }

    // Skip if transcript hasn't changed significantly
    if (text === lastTranscriptRef.current) {
      return;
    }

    setIsLoading(true);
    try {
      const result = await invoke<string>('generate_predictive_hint', {
        transcript: text,
      });

      if (result && result.trim()) {
        setHint(result);
        setLastUpdated(Date.now());
        lastTranscriptRef.current = text;
      }
    } catch (error) {
      console.error('Failed to generate predictive hint:', error);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Set up interval when recording starts
  useEffect(() => {
    if (isRecording) {
      // Generate initial hint after a short delay
      const initialTimeout = setTimeout(() => {
        generateHint(transcript);
      }, 5000); // First hint after 5 seconds

      // Then every 30 seconds
      intervalRef.current = setInterval(() => {
        generateHint(transcript);
      }, HINT_INTERVAL_MS);

      return () => {
        clearTimeout(initialTimeout);
        if (intervalRef.current) {
          clearInterval(intervalRef.current);
          intervalRef.current = null;
        }
      };
    } else {
      // Clear state when not recording
      setHint('');
      setLastUpdated(null);
      lastTranscriptRef.current = '';
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    }
  }, [isRecording]); // Only depend on isRecording, not transcript

  // Update the transcript ref for the interval to use
  useEffect(() => {
    // The interval callback will read from the ref
  }, [transcript]);

  // Update closure's transcript reference
  useEffect(() => {
    if (intervalRef.current) {
      // Re-create interval with updated transcript
      clearInterval(intervalRef.current);
      intervalRef.current = setInterval(() => {
        generateHint(transcript);
      }, HINT_INTERVAL_MS);
    }
  }, [transcript, generateHint]);

  return {
    hint,
    isLoading,
    lastUpdated,
  };
}
