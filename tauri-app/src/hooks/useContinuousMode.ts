import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type { ContinuousModeStats, ContinuousModeEvent, TranscriptUpdate, AudioQualitySnapshot } from '../types';

export interface UseContinuousModeResult {
  /** Whether continuous mode is actively running */
  isActive: boolean;
  /** Whether a stop has been requested and we're waiting for cleanup (buffer flush + SOAP) */
  isStopping: boolean;
  /** Current stats from the backend */
  stats: ContinuousModeStats;
  /** Live transcript preview text (last ~500 chars) */
  liveTranscript: string;
  /** Audio quality snapshot from the pipeline */
  audioQuality: AudioQualitySnapshot | null;
  /** Per-encounter notes (passed to SOAP generation) */
  encounterNotes: string;
  /** Update encounter notes (debounced backend sync) */
  setEncounterNotes: (notes: string) => void;
  /** Start continuous mode */
  start: () => Promise<void>;
  /** Stop continuous mode */
  stop: () => Promise<void>;
  /** Manually trigger a new patient encounter split */
  triggerNewPatient: () => Promise<void>;
  /** Error message if any */
  error: string | null;
}

const IDLE_STATS: ContinuousModeStats = {
  state: 'idle',
  recording_since: '',
  encounters_detected: 0,
  last_encounter_at: null,
  last_encounter_words: null,
  last_encounter_patient_name: null,
  last_error: null,
  buffer_word_count: 0,
  buffer_started_at: null,
};

/**
 * Hook for managing continuous charting mode.
 *
 * Listens to `continuous_mode_event` from the Rust backend and provides
 * stats, live transcript preview, and start/stop controls.
 */
export function useContinuousMode(): UseContinuousModeResult {
  const [isActive, setIsActive] = useState(false);
  const [isStopping, setIsStopping] = useState(false);
  const [stats, setStats] = useState<ContinuousModeStats>(IDLE_STATS);
  const [liveTranscript, setLiveTranscript] = useState('');
  const [audioQuality, setAudioQuality] = useState<AudioQualitySnapshot | null>(null);
  const [encounterNotes, setEncounterNotesState] = useState('');
  const [error, setError] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const notesDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Listen to continuous mode events from backend
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let mounted = true;

    listen<ContinuousModeEvent>('continuous_mode_event', (event) => {
      if (!mounted) return;
      const payload = event.payload;

      switch (payload.type) {
        case 'started':
          setIsActive(true);
          setError(null);
          break;
        case 'stopped':
          setIsActive(false);
          setIsStopping(false);
          setStats(IDLE_STATS);
          setLiveTranscript('');
          setAudioQuality(null);
          setEncounterNotesState('');
          break;
        case 'encounter_detected':
          setEncounterNotesState('');
          break;
        case 'error':
          setError(payload.error || 'Unknown error');
          setIsActive(false);
          setIsStopping(false);
          break;
      }
    }).then((fn) => {
      if (mounted) {
        unlisten = fn;
      } else {
        fn(); // Component unmounted before listener resolved
      }
    });

    return () => {
      mounted = false;
      if (unlisten) unlisten();
    };
  }, []);

  // Listen to transcript updates for live preview
  useEffect(() => {
    if (!isActive) return;

    let unlisten: UnlistenFn | null = null;
    let mounted = true;

    listen<TranscriptUpdate>('continuous_transcript_preview', (event) => {
      if (mounted) setLiveTranscript(event.payload.finalized_text || '');
    }).then((fn) => {
      if (mounted) {
        unlisten = fn;
      } else {
        fn();
      }
    });

    return () => {
      mounted = false;
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

  // Listen to audio quality events from pipeline
  useEffect(() => {
    if (!isActive) return;

    let unlisten: UnlistenFn | null = null;
    let mounted = true;

    listen<AudioQualitySnapshot>('audio_quality', (event) => {
      if (mounted) setAudioQuality(event.payload);
    }).then((fn) => {
      if (mounted) {
        unlisten = fn;
      } else {
        fn();
      }
    });

    return () => {
      mounted = false;
      if (unlisten) unlisten();
    };
  }, [isActive]);

  // Debounced encounter notes setter — syncs to backend after 500ms idle
  const setEncounterNotes = useCallback((notes: string) => {
    setEncounterNotesState(notes);

    if (notesDebounceRef.current) {
      clearTimeout(notesDebounceRef.current);
    }
    notesDebounceRef.current = setTimeout(() => {
      invoke('set_continuous_encounter_notes', { notes }).catch((e) => {
        console.error('Failed to sync encounter notes:', e);
      });
    }, 500);
  }, []);

  // Cleanup debounce timer on unmount
  useEffect(() => {
    return () => {
      if (notesDebounceRef.current) {
        clearTimeout(notesDebounceRef.current);
      }
    };
  }, []);

  const start = useCallback(async () => {
    try {
      setError(null);
      await invoke('start_continuous_mode');
      // isActive will be set to true when we receive the 'started' event
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const stop = useCallback(async () => {
    try {
      setIsStopping(true);
      await invoke('stop_continuous_mode');
      // isActive will be set to false when we receive the 'stopped' event
    } catch (e) {
      setError(String(e));
      // Force reset if stop failed
      setIsActive(false);
      setIsStopping(false);
    }
  }, []);

  const triggerNewPatient = useCallback(async () => {
    try {
      await invoke('trigger_new_patient');
    } catch (e) {
      setError(String(e));
    }
  }, []);

  return {
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
  };
}
