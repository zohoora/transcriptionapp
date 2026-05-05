import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type {
  ContinuousModeStats,
  ContinuousModeEvent,
  TranscriptUpdate,
  AudioQualitySnapshot,
  EncounterNote,
} from '../types';

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
  /** Chip-style submitted notes for the in-progress encounter (newest last) */
  encounterNotes: EncounterNote[];
  /** Submit a single note to the current encounter. Resolves after the backend stamps id + timestamp; rejects on empty input or if continuous mode isn't running. */
  submitEncounterNote: (text: string) => Promise<EncounterNote>;
  /** Remove a previously-submitted note by id. Idempotent — no-op if the id isn't present. */
  deleteEncounterNote: (id: string) => Promise<void>;
  /** Live patient name from vision tracker (null during buffering-before-vision or after DOB invalidation) */
  currentPatientName: string | null;
  /** Start continuous mode */
  start: () => Promise<void>;
  /** Stop continuous mode */
  stop: () => Promise<void>;
  /** Manually trigger a new patient encounter split */
  triggerNewPatient: () => Promise<void>;
  /** Error message if any */
  error: string | null;
  /** Event-driven session ID that changes synchronously on encounter_detected */
  encounterSessionId: string;
  /** True when speech is detected but no transcription is being produced */
  transcriptionStalled: boolean;
  /** Whether the pipeline is currently in overnight sleep mode */
  isSleeping: boolean;
  /** ISO timestamp when sleep mode will resume (null when not sleeping) */
  sleepResumeAt: string | null;
}

const IDLE_STATS: ContinuousModeStats = {
  state: 'idle',
  recording_since: '',
  encounters_detected: 0,
  recent_encounters: [],
  last_error: null,
  buffer_word_count: 0,
  buffer_started_at: null,
  is_sleeping: false,
  sleep_resume_at: null,
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
  const [encounterNotes, setEncounterNotes] = useState<EncounterNote[]>([]);
  const [currentPatientName, setCurrentPatientName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [encounterSessionId, setEncounterSessionId] = useState<string>(`continuous-${Date.now()}`);
  const [transcriptionStalled, setTranscriptionStalled] = useState(false);
  const [isSleeping, setIsSleeping] = useState(false);
  const [sleepResumeAt, setSleepResumeAt] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const isActiveRef = useRef(isActive);

  // Keep isActiveRef in sync with isActive state
  useEffect(() => {
    isActiveRef.current = isActive;
  }, [isActive]);

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
          setEncounterSessionId(`continuous-${Date.now()}`);
          setTranscriptionStalled(false);
          setIsSleeping(false);
          setSleepResumeAt(null);
          setEncounterNotes([]);
          setCurrentPatientName(null);
          break;
        case 'stopped':
          setIsActive(false);
          setIsStopping(false);
          setStats(IDLE_STATS);
          setLiveTranscript('');
          setAudioQuality(null);
          setEncounterNotes([]);
          setCurrentPatientName(null);
          setTranscriptionStalled(false);
          setIsSleeping(false);
          setSleepResumeAt(null);
          break;
        case 'encounter_detected':
          // Encounter split — archived notes are on disk. Reset the chip list
          // for the NEW encounter; reset the attachment label until the
          // vision tracker reacquires a name on the next screenshot cycle.
          setEncounterNotes([]);
          setCurrentPatientName(null);
          setEncounterSessionId(`continuous-${Date.now()}`);
          setTranscriptionStalled(false);
          break;
        case 'patient_name_updated':
          // `name` absent = tracker cleared (DOB invalidation). Store null so
          // the attachment label falls back to "current encounter".
          setCurrentPatientName(payload.name ?? null);
          break;
        case 'transcription_stalled':
          setTranscriptionStalled(true);
          break;
        case 'sleep_started':
          setIsSleeping(true);
          setSleepResumeAt(payload.resume_at ?? null);
          break;
        case 'sleep_ended':
          setIsSleeping(false);
          setSleepResumeAt(null);
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

  // Listen to transcript updates for live preview.
  // Registered once (not gated on isActive) so the listener is never torn down
  // and re-registered on active-state transitions, which would drop events.
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;
    let mounted = true;

    listen<TranscriptUpdate>('continuous_transcript_preview', (event) => {
      if (mounted && isActiveRef.current) {
        setLiveTranscript(event.payload.finalized_text || '');
      }
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
  }, []);

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

  // Submit a single note. The backend stamps id + timestamp and returns the
  // record; we append on success so React keys + timestamps come from one
  // source of truth (the backend), avoiding clock drift or id collisions
  // between optimistic UI and the eventual persisted list.
  const submitEncounterNote = useCallback(async (text: string): Promise<EncounterNote> => {
    const note = await invoke<EncounterNote>('submit_continuous_encounter_note', { text });
    setEncounterNotes((prev) => [...prev, note]);
    return note;
  }, []);

  // Remove a chip. Optimistic (filter locally first) so the UI responds
  // immediately; the backend call is idempotent so a failed network trip
  // doesn't corrupt state — the worst case is a stale note persisting and
  // being included in the SOAP prompt.
  const deleteEncounterNote = useCallback(async (id: string): Promise<void> => {
    setEncounterNotes((prev) => prev.filter((n) => n.id !== id));
    try {
      await invoke('delete_continuous_encounter_note', { id });
    } catch (e) {
      console.error('Failed to delete encounter note:', e);
    }
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
    submitEncounterNote,
    deleteEncounterNote,
    currentPatientName,
    start,
    stop,
    triggerNewPatient,
    error,
    encounterSessionId,
    transcriptionStalled,
    isSleeping,
    sleepResumeAt,
  };
}
