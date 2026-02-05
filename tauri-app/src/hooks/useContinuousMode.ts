import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type { ContinuousModeStats, ContinuousModeEvent, TranscriptUpdate } from '../types';

export interface UseContinuousModeResult {
  /** Whether continuous mode is actively running */
  isActive: boolean;
  /** Current stats from the backend */
  stats: ContinuousModeStats;
  /** Live transcript preview text (last ~500 chars) */
  liveTranscript: string;
  /** Start continuous mode */
  start: () => Promise<void>;
  /** Stop continuous mode */
  stop: () => Promise<void>;
  /** Error message if any */
  error: string | null;
}

const IDLE_STATS: ContinuousModeStats = {
  state: 'idle',
  recording_since: '',
  encounters_detected: 0,
  last_encounter_at: null,
  last_encounter_words: null,
  last_error: null,
  buffer_word_count: 0,
};

/**
 * Hook for managing continuous charting mode.
 *
 * Listens to `continuous_mode_event` from the Rust backend and provides
 * stats, live transcript preview, and start/stop controls.
 */
export function useContinuousMode(): UseContinuousModeResult {
  const [isActive, setIsActive] = useState(false);
  const [stats, setStats] = useState<ContinuousModeStats>(IDLE_STATS);
  const [liveTranscript, setLiveTranscript] = useState('');
  const [error, setError] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Listen to continuous mode events from backend
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    listen<ContinuousModeEvent>('continuous_mode_event', (event) => {
      const payload = event.payload;

      switch (payload.type) {
        case 'started':
          setIsActive(true);
          setError(null);
          break;
        case 'stopped':
          setIsActive(false);
          setStats(IDLE_STATS);
          setLiveTranscript('');
          break;
        case 'encounter_detected':
          // Stats will be refreshed by polling
          break;
        case 'soap_generated':
          break;
        case 'soap_failed':
          // SOAP failure for a specific encounter — don't set global error
          break;
        case 'checking':
          break;
        case 'error':
          setError(payload.error || 'Unknown error');
          break;
      }
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  // Listen to transcript updates for live preview
  useEffect(() => {
    if (!isActive) return;

    let unlisten: UnlistenFn | null = null;

    listen<TranscriptUpdate>('transcript_update', (event) => {
      setLiveTranscript(event.payload.finalized_text || '');
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, [isActive]);

  // Poll for stats while active
  useEffect(() => {
    if (!isActive) {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
      return;
    }

    const fetchStats = async () => {
      try {
        const result = await invoke<ContinuousModeStats>('get_continuous_mode_status');
        setStats(result);
      } catch (e) {
        // Ignore poll errors
      }
    };

    // Fetch immediately, then every 5 seconds
    fetchStats();
    pollRef.current = setInterval(fetchStats, 5000);

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [isActive]);

  const start = useCallback(async () => {
    try {
      setError(null);
      await invoke('start_continuous_mode');
      setIsActive(true);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const stop = useCallback(async () => {
    try {
      await invoke('stop_continuous_mode');
      // isActive will be set to false when we receive the 'stopped' event
    } catch (e) {
      setError(String(e));
      // Force reset if stop failed
      setIsActive(false);
    }
  }, []);

  return {
    isActive,
    stats,
    liveTranscript,
    start,
    stop,
    error,
  };
}
