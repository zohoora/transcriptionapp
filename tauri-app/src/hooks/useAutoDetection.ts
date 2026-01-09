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
  onStartRecording: () => void;
  /** Called when greeting check passes - recording should continue */
  onGreetingConfirmed: (transcript: string, confidence: number) => void;
  /** Called when greeting check fails - recording should be aborted */
  onGreetingRejected: (reason: string) => void;
  /** Legacy: Called when a greeting is detected (for backward compatibility) */
  onGreetingDetected?: (transcript: string, confidence: number) => void;
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

  // Extract callbacks for use in effect
  const { onStartRecording, onGreetingConfirmed, onGreetingRejected, onGreetingDetected } = callbacks;

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
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const unlisten = await listen<any>('listening_event', (event) => {
        if (!mounted) return;

        const payload = event.payload as ListeningEventPayload;

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
            ...prev!,
            speech_detected: true,
            speech_duration_ms: payload.duration_ms || 0,
          }));
        } else if (eventType === 'analyzing') {
          setListeningStatus((prev) => ({
            ...prev!,
            analyzing: true,
          }));
        } else if (eventType === 'start_recording') {
          // OPTIMISTIC RECORDING: Start recording immediately
          // The greeting check will run in the background
          setIsPendingConfirmation(true);
          setListeningStatus((prev) => ({
            ...prev!,
            analyzing: true,
          }));
          // Notify parent to start session NOW
          onStartRecording();
        } else if (eventType === 'greeting_confirmed') {
          // Greeting check passed - recording should continue
          const transcript = payload.transcript || '';
          const confidence = payload.confidence || 0;
          setIsPendingConfirmation(false);
          setIsListening(false);
          setListeningStatus(null);
          onGreetingConfirmed(transcript, confidence);
        } else if (eventType === 'greeting_rejected') {
          // Greeting check failed - recording should be discarded
          const reason = payload.reason || 'Not a greeting';
          setIsPendingConfirmation(false);
          setListeningStatus((prev) => ({
            ...prev!,
            analyzing: false,
            speech_detected: false,
            speech_duration_ms: 0,
          }));
          onGreetingRejected(reason);
        } else if (eventType === 'greeting_detected') {
          // Legacy event - for backward compatibility
          const transcript = payload.transcript || '';
          const confidence = payload.confidence || 0;
          // Only call if not already handled by start_recording flow
          if (!isPendingConfirmation) {
            setIsListening(false);
            setListeningStatus(null);
            onGreetingDetected?.(transcript, confidence);
          }
        } else if (eventType === 'not_greeting') {
          // Legacy event
          setListeningStatus((prev) => ({
            ...prev!,
            speech_detected: false,
            speech_duration_ms: 0,
            analyzing: false,
          }));
        } else if (eventType === 'error') {
          setError(payload.message || 'Unknown error');
          setIsPendingConfirmation(false);
          setListeningStatus((prev) => ({
            ...prev!,
            analyzing: false,
          }));
        } else if (eventType === 'stopped') {
          setIsListening(false);
          setIsPendingConfirmation(false);
          setListeningStatus(null);
        }
      });

      unlistenRef.current = unlisten;
    };

    setupListener();

    return () => {
      mounted = false;
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    };
  }, [onStartRecording, onGreetingConfirmed, onGreetingRejected, onGreetingDetected, isPendingConfirmation]);

  return {
    isListening,
    isPendingConfirmation,
    listeningStatus,
    error,
    startListening,
    stopListening,
  };
}
