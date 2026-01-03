import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type {
  SessionStatus,
  TranscriptUpdate,
  BiomarkerUpdate,
  AudioQualitySnapshot,
  SoapNote,
} from '../types';

export interface UseSessionStateResult {
  // Session state
  status: SessionStatus;
  transcript: TranscriptUpdate;
  editedTranscript: string;
  setEditedTranscript: (text: string) => void;
  biomarkers: BiomarkerUpdate | null;
  audioQuality: AudioQualitySnapshot | null;
  soapNote: SoapNote | null;
  setSoapNote: (note: SoapNote | null) => void;
  localElapsedMs: number;

  // Actions
  handleStart: (deviceId: string | null) => Promise<void>;
  handleStop: () => Promise<void>;
  handleReset: () => Promise<void>;

  // Derived state
  isRecording: boolean;
  isStopping: boolean;
  isIdle: boolean;
  isCompleted: boolean;
}

export function useSessionState(): UseSessionStateResult {
  // Session state
  const [status, setStatus] = useState<SessionStatus>({
    state: 'idle',
    provider: null,
    elapsed_ms: 0,
    is_processing_behind: false,
  });
  const [transcript, setTranscript] = useState<TranscriptUpdate>({
    finalized_text: '',
    draft_text: null,
    segment_count: 0,
  });
  const [editedTranscript, setEditedTranscript] = useState('');
  const [biomarkers, setBiomarkers] = useState<BiomarkerUpdate | null>(null);
  const [audioQuality, setAudioQuality] = useState<AudioQualitySnapshot | null>(null);
  const [soapNote, setSoapNote] = useState<SoapNote | null>(null);

  // Timer state
  const [localElapsedMs, setLocalElapsedMs] = useState(0);
  const recordingStartRef = useRef<number | null>(null);

  // Sync edited transcript with original when recording completes
  useEffect(() => {
    if (status.state === 'completed' && transcript.finalized_text && !editedTranscript) {
      setEditedTranscript(transcript.finalized_text);
    }
  }, [status.state, transcript.finalized_text, editedTranscript]);

  // Local timer that runs during recording/preparing
  useEffect(() => {
    if (status.state === 'preparing' || status.state === 'recording') {
      if (recordingStartRef.current === null) {
        recordingStartRef.current = Date.now();
      }

      const interval = setInterval(() => {
        if (recordingStartRef.current) {
          setLocalElapsedMs(Date.now() - recordingStartRef.current);
        }
      }, 100);

      return () => clearInterval(interval);
    } else {
      recordingStartRef.current = null;
      if (status.state === 'idle') {
        setLocalElapsedMs(0);
      }
    }
  }, [status.state]);

  // Subscribe to events
  useEffect(() => {
    let mounted = true;
    const unlistenPromises: Promise<UnlistenFn>[] = [];

    // Set up listeners and collect their unsubscribe promises
    unlistenPromises.push(
      listen<SessionStatus>('session_status', (event) => {
        if (mounted) setStatus(event.payload);
      })
    );

    unlistenPromises.push(
      listen<TranscriptUpdate>('transcript_update', (event) => {
        if (mounted) setTranscript(event.payload);
      })
    );

    unlistenPromises.push(
      listen<BiomarkerUpdate>('biomarker_update', (event) => {
        if (mounted) setBiomarkers(event.payload);
      })
    );

    unlistenPromises.push(
      listen<AudioQualitySnapshot>('audio_quality', (event) => {
        if (mounted) setAudioQuality(event.payload);
      })
    );

    return () => {
      mounted = false;
      // Await all promises and unsubscribe
      Promise.all(unlistenPromises).then((unlisteners) => {
        unlisteners.forEach((fn) => fn());
      });
    };
  }, []);

  // Handle start recording
  const handleStart = useCallback(async (deviceId: string | null) => {
    // Reset state for new session
    setTranscript({ finalized_text: '', draft_text: null, segment_count: 0 });
    setEditedTranscript('');
    setBiomarkers(null);
    setAudioQuality(null);
    setSoapNote(null);

    await invoke('start_session', { deviceId });
  }, []);

  // Handle stop recording
  const handleStop = useCallback(async () => {
    await invoke('stop_session');
  }, []);

  // Handle reset/new session
  const handleReset = useCallback(async () => {
    await invoke('reset_session');
    setTranscript({ finalized_text: '', draft_text: null, segment_count: 0 });
    setEditedTranscript('');
    setBiomarkers(null);
    setAudioQuality(null);
    setSoapNote(null);
  }, []);

  // Derived state
  const isRecording = status.state === 'recording';
  const isStopping = status.state === 'stopping';
  const isIdle = status.state === 'idle';
  const isCompleted = status.state === 'completed';

  return {
    status,
    transcript,
    editedTranscript,
    setEditedTranscript,
    biomarkers,
    audioQuality,
    soapNote,
    setSoapNote,
    localElapsedMs,
    handleStart,
    handleStop,
    handleReset,
    isRecording,
    isStopping,
    isIdle,
    isCompleted,
  };
}
