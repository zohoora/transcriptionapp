import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type { ListeningStatus, ListeningEventPayload } from '../types';
import { formatErrorMessage } from '../utils';

export interface UseAutoDetectionResult {
  isListening: boolean;
  isPendingConfirmation: boolean;  // True while waiting for greeting check result
  listeningStatus: ListeningStatus | null;
  error: string | null;
  startListening: (deviceId: string | null) => Promise<void>;
  stopListening: () => Promise<void>;
}

export interface UseAutoDetectionCallbacks {
  /** Called when recording should start immediately (optimistic recording) */
  onStartRecording: () => void | Promise<void>;
  /** Called when greeting check passes - recording should continue */
  onGreetingConfirmed: (transcript: string, confidence: number) => void | Promise<void>;
  /** Called when greeting check fails - recording should be aborted */
  onGreetingRejected: (reason: string) => void | Promise<void>;
  /** Legacy: Called when a greeting is detected (for backward compatibility) */
  onGreetingDetected?: (transcript: string, confidence: number) => void | Promise<void>;
}

/**
 * Hook for managing auto-session detection via listening mode.
 * Uses VAD + Whisper + LLM to detect greeting speech and auto-start sessions.
 *
 * NEW FLOW (optimistic recording):
 * 1. Speech detected → StartRecording event → onStartRecording() → recording starts immediately
 * 2. Greeting check runs in background (~35s)
 * 3. If greeting confirmed → onGreetingConfirmed() → recording continues
 * 4. If greeting rejected → onGreetingRejected() → recording is discarded
 *
 * This prevents losing audio during the lengthy greeting check.
 *
 * @param autoStartEnabled Whether auto-start is enabled in settings
 * @param callbacks Callbacks for the different events
 */
export function useAutoDetection(
  _autoStartEnabled: boolean, // Used by parent to control when to start/stop listening
  callbacks: UseAutoDetectionCallbacks
): UseAutoDetectionResult {
  const [isListening, setIsListening] = useState(false);
  const [isPendingConfirmation, setIsPendingConfirmation] = useState(false);
  const [listeningStatus, setListeningStatus] = useState<ListeningStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);
  const isPendingConfirmationRef = useRef(false);

  // Keep ref in sync with state (ref used inside listener to avoid re-subscription)
  isPendingConfirmationRef.current = isPendingConfirmation;

  // Store callbacks in refs so the effect doesn't re-run when callback identities change.
  // This prevents tearing down and re-establishing the listener, which could miss events.
  const onStartRecordingRef = useRef(callbacks.onStartRecording);
  onStartRecordingRef.current = callbacks.onStartRecording;
  const onGreetingConfirmedRef = useRef(callbacks.onGreetingConfirmed);
  onGreetingConfirmedRef.current = callbacks.onGreetingConfirmed;
  const onGreetingRejectedRef = useRef(callbacks.onGreetingRejected);
  onGreetingRejectedRef.current = callbacks.onGreetingRejected;
  const onGreetingDetectedRef = useRef(callbacks.onGreetingDetected);
  onGreetingDetectedRef.current = callbacks.onGreetingDetected;

  // Start listening mode
  const startListening = useCallback(async (deviceId: string | null) => {
    if (isListening) return;

    setError(null);
    try {
      await invoke('start_listening', { deviceId });
      setIsListening(true);
      setListeningStatus({
        is_listening: true,
        speech_detected: false,
        speech_duration_ms: 0,
        analyzing: false,
      });
    } catch (e) {
      console.error('Failed to start listening:', e);
      setError(formatErrorMessage(e));
      setIsListening(false);
    }
  }, [isListening]);

  // Stop listening mode
  const stopListening = useCallback(async () => {
    if (!isListening) return;

    try {
      await invoke('stop_listening');
    } catch (e) {
      console.error('Failed to stop listening:', e);
    } finally {
      setIsListening(false);
      setListeningStatus(null);
    }
  }, [isListening]);

  // Listen for backend events
  useEffect(() => {
    let mounted = true;

    const setupListener = async () => {
      const unlisten = await listen<ListeningEventPayload>('listening_event', (event) => {
        if (!mounted) return;

        const payload = event.payload;

        // Handle different event types (Rust uses tag="type", rename_all="snake_case")
        const eventType = payload.type;

        if (eventType === 'started') {
          setListeningStatus({
            is_listening: true,
            speech_detected: false,
            speech_duration_ms: 0,
            analyzing: false,
          });
        } else if (eventType === 'speech_detected') {
          setListeningStatus((prev) => ({
            is_listening: true,
            speech_detected: true,
            speech_duration_ms: payload.duration_ms || 0,
            analyzing: prev?.analyzing ?? false,
          }));
        } else if (eventType === 'analyzing') {
          setListeningStatus((prev) => ({
            is_listening: true,
            speech_detected: prev?.speech_detected ?? false,
            speech_duration_ms: prev?.speech_duration_ms ?? 0,
            analyzing: true,
          }));
        } else if (eventType === 'start_recording') {
          // OPTIMISTIC RECORDING: Start recording immediately
          // The greeting check will run in the background
          setIsPendingConfirmation(true);
          setListeningStatus((prev) => ({
            is_listening: true,
            speech_detected: prev?.speech_detected ?? false,
            speech_duration_ms: prev?.speech_duration_ms ?? 0,
            analyzing: true,
          }));
          // Notify parent to start session NOW
          Promise.resolve(onStartRecordingRef.current()).catch(e => console.error('onStartRecording failed:', e));
        } else if (eventType === 'greeting_confirmed') {
          // Greeting check passed - recording should continue
          const transcript = payload.transcript || '';
          const confidence = payload.confidence || 0;
          setIsPendingConfirmation(false);
          setIsListening(false);
          setListeningStatus(null);
          Promise.resolve(onGreetingConfirmedRef.current(transcript, confidence)).catch(e => console.error('onGreetingConfirmed failed:', e));
        } else if (eventType === 'greeting_rejected') {
          // Greeting check failed - recording should be discarded
          const reason = payload.reason || 'Not a greeting';
          setIsPendingConfirmation(false);
          setListeningStatus((prev) => ({
            is_listening: prev?.is_listening ?? true,
            analyzing: false,
            speech_detected: false,
            speech_duration_ms: 0,
          }));
          Promise.resolve(onGreetingRejectedRef.current(reason)).catch(e => console.error('onGreetingRejected failed:', e));
        } else if (eventType === 'greeting_detected') {
          // Legacy event - for backward compatibility
          const transcript = payload.transcript || '';
          const confidence = payload.confidence || 0;
          // Only call if not already handled by start_recording flow
          if (!isPendingConfirmationRef.current) {
            setIsListening(false);
            setListeningStatus(null);
            onGreetingDetectedRef.current?.(transcript, confidence);
          }
        } else if (eventType === 'not_greeting') {
          // Legacy event
          setListeningStatus((prev) => ({
            is_listening: prev?.is_listening ?? true,
            speech_detected: false,
            speech_duration_ms: 0,
            analyzing: false,
          }));
        } else if (eventType === 'speaker_not_verified') {
          // Speaker verification failed - not an enrolled speaker or wrong role
          console.log('Speaker not verified:', payload.reason);
          setListeningStatus((prev) => ({
            is_listening: prev?.is_listening ?? true,
            speech_detected: false,
            speech_duration_ms: 0,
            analyzing: false,
          }));
        } else if (eventType === 'error') {
          setError(payload.message || 'Unknown error');
          setIsPendingConfirmation(false);
          setListeningStatus((prev) => ({
            is_listening: prev?.is_listening ?? true,
            speech_detected: prev?.speech_detected ?? false,
            speech_duration_ms: prev?.speech_duration_ms ?? 0,
            analyzing: false,
          }));
        } else if (eventType === 'stopped') {
          setIsListening(false);
          setIsPendingConfirmation(false);
          setListeningStatus(null);
        }
      });

      if (mounted) {
        unlistenRef.current = unlisten;
      } else {
        // Component unmounted before listener was set up — clean up immediately
        unlisten();
      }
    };

    setupListener();

    return () => {
      mounted = false;
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    };
  }, []); // Callbacks accessed via refs — no dependency-driven re-subscription

  return {
    isListening,
    isPendingConfirmation,
    listeningStatus,
    error,
    startListening,
    stopListening,
  };
}
