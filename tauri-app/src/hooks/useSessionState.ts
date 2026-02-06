import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type {
  SessionStatus,
  TranscriptUpdate,
  BiomarkerUpdate,
  AudioQualitySnapshot,
  MultiPatientSoapResult,
  AutoEndEventPayload,
  SilenceWarningPayload,
} from '../types';

export interface UseSessionStateResult {
  // Session state
  status: SessionStatus;
  transcript: TranscriptUpdate;
  editedTranscript: string;
  setEditedTranscript: (text: string) => void;
  biomarkers: BiomarkerUpdate | null;
  audioQuality: AudioQualitySnapshot | null;
  soapResult: MultiPatientSoapResult | null;
  setSoapResult: (result: MultiPatientSoapResult | null) => void;
  localElapsedMs: number;
  autoEndInfo: AutoEndEventPayload | null;
  /** Silence warning for auto-end countdown display (null = no warning active) */
  silenceWarning: SilenceWarningPayload | null;

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
  const [soapResult, setSoapResult] = useState<MultiPatientSoapResult | null>(null);
  const [autoEndInfo, setAutoEndInfo] = useState<AutoEndEventPayload | null>(null);
  const [silenceWarning, setSilenceWarning] = useState<SilenceWarningPayload | null>(null);

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
    const unlistenFns: UnlistenFn[] = [];

    const setupListener = <T>(event: string, handler: (payload: T) => void) => {
      listen<T>(event, (e) => {
        if (mounted) handler(e.payload);
      }).then((fn) => {
        if (mounted) {
          unlistenFns.push(fn);
        } else {
          fn(); // Component unmounted before listener resolved
        }
      });
    };

    setupListener<SessionStatus>('session_status', setStatus);
    setupListener<TranscriptUpdate>('transcript_update', setTranscript);
    setupListener<BiomarkerUpdate>('biomarker_update', setBiomarkers);
    setupListener<AudioQualitySnapshot>('audio_quality', setAudioQuality);

    setupListener<AutoEndEventPayload>('session_auto_end', (payload) => {
      console.log('Session auto-ended:', payload);
      setAutoEndInfo(payload);
      setSilenceWarning(null);
    });

    setupListener<SilenceWarningPayload>('silence_warning', (payload) => {
      if (payload.remaining_ms === 0) {
        setSilenceWarning(null);
      } else {
        setSilenceWarning(payload);
      }
    });

    return () => {
      mounted = false;
      unlistenFns.forEach((fn) => fn());
    };
  }, []);

  // Handle start recording
  const handleStart = useCallback(async (deviceId: string | null) => {
    // Reset state for new session
    setTranscript({ finalized_text: '', draft_text: null, segment_count: 0 });
    setEditedTranscript('');
    setBiomarkers(null);
    setAudioQuality(null);
    setSoapResult(null);
    setAutoEndInfo(null);
    setSilenceWarning(null);

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
    setSoapResult(null);
    setAutoEndInfo(null);
    setSilenceWarning(null);
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
    soapResult,
    setSoapResult,
    localElapsedMs,
    autoEndInfo,
    silenceWarning,
    handleStart,
    handleStop,
    handleReset,
    isRecording,
    isStopping,
    isIdle,
    isCompleted,
  };
}
