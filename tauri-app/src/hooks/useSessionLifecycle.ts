/**
 * useSessionLifecycle Hook
 *
 * Centralizes session start/reset coordination across all hooks.
 *
 * ## Problem Solved
 *
 * Previously, session lifecycle resets were scattered across 3 callbacks in App.tsx,
 * each manually calling resetSyncState(), setSessionNotes(''), etc. Adding new state
 * that needed session cleanup required editing every callback. Clinical chat was never
 * cleared between sessions, potentially leaking patient data.
 *
 * ## Design
 *
 * Each hook that has session-scoped state provides a reset callback. This hook calls
 * them all from a single `resetAllSessionState()` function, ensuring nothing is missed.
 *
 * State managed here (previously scattered in App.tsx):
 * - sessionNotes (clinician observations during recording)
 * - autoStartPending ref (auto-detection greeting confirmation tracking)
 *
 * Reset callbacks coordinated:
 * - useSessionState.handleStart / handleReset (transcript, biomarkers, SOAP result)
 * - useMedplumSync.resetSyncState (sync progress, encounter tracking)
 * - useClinicalChat.clearChat (chat messages from previous session)
 * - useSoapNote.setSoapError (stale error from previous session)
 */
import { useState, useCallback, useRef } from 'react';

export interface SessionLifecycleHandlers {
  /** Start recording — resets transcript, biomarkers, etc. (from useSessionState) */
  sessionStart: (deviceId: string | null) => Promise<void>;
  /** Reset to idle — clears session state on backend + frontend (from useSessionState) */
  sessionReset: () => Promise<void>;
  /** Reset sync state — clears encounter, sync progress (from useMedplumSync) */
  resetSyncState: () => void;
  /** Clear chat messages — prevents patient data leak across sessions (from useClinicalChat) */
  clearChat: () => void;
  /** Clear stale SOAP error from previous session (from useSoapNote) */
  clearSoapError: () => void;
}

export interface UseSessionLifecycleResult {
  /** Clinician session notes (observations during recording) */
  sessionNotes: string;
  setSessionNotes: (notes: string) => void;

  /**
   * Start a manual session — resets all cross-hook state, then starts recording.
   * Called when the user clicks the record button.
   */
  startSession: (deviceId: string | null) => Promise<void>;

  /**
   * Start an auto-detected session — same resets, but marks as pending greeting confirmation.
   * Called by useAutoDetection when speech is detected.
   */
  startSessionAutoDetect: (deviceId: string | null) => Promise<void>;

  /**
   * Mark auto-start as confirmed (greeting accepted).
   * Called when the backend confirms the greeting check passed.
   */
  confirmAutoStart: () => void;

  /**
   * Handle greeting rejection — resets session only if still pending confirmation.
   * Returns true if the session was reset (was still pending), false if it was kept
   * (session already confirmed or user-initiated).
   */
  handleGreetingRejected: () => Promise<boolean>;

  /**
   * Reset all cross-hook state and return to idle.
   * Called when user clicks "New Session" or session is discarded.
   */
  resetSession: () => Promise<void>;
}

export function useSessionLifecycle({
  sessionStart,
  sessionReset,
  resetSyncState,
  clearChat,
  clearSoapError,
}: SessionLifecycleHandlers): UseSessionLifecycleResult {
  const [sessionNotes, setSessionNotes] = useState('');
  const autoStartPendingRef = useRef(false);

  // Single source of truth for all cross-hook resets.
  // When adding new session-scoped state to any hook, add its reset here.
  const resetAllSessionState = useCallback(() => {
    setSessionNotes('');
    resetSyncState();
    clearChat();
    clearSoapError();
  }, [resetSyncState, clearChat, clearSoapError]);

  const startSession = useCallback(async (deviceId: string | null) => {
    autoStartPendingRef.current = false;
    resetAllSessionState();
    await sessionStart(deviceId);
  }, [resetAllSessionState, sessionStart]);

  const startSessionAutoDetect = useCallback(async (deviceId: string | null) => {
    autoStartPendingRef.current = true;
    resetAllSessionState();
    await sessionStart(deviceId);
  }, [resetAllSessionState, sessionStart]);

  const confirmAutoStart = useCallback(() => {
    autoStartPendingRef.current = false;
  }, []);

  const handleGreetingRejected = useCallback(async (): Promise<boolean> => {
    if (autoStartPendingRef.current) {
      autoStartPendingRef.current = false;
      await sessionReset();
      return true;
    }
    return false;
  }, [sessionReset]);

  const resetSession = useCallback(async () => {
    autoStartPendingRef.current = false;
    resetAllSessionState();
    await sessionReset();
  }, [resetAllSessionState, sessionReset]);

  return {
    sessionNotes,
    setSessionNotes,
    startSession,
    startSessionAutoDetect,
    confirmAutoStart,
    handleGreetingRejected,
    resetSession,
  };
}
