import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { AuthState, SoapNote, SyncResult } from '../types';

export interface UseMedplumSyncResult {
  medplumConnected: boolean;
  medplumError: string | null;
  isSyncing: boolean;
  syncError: string | null;
  syncSuccess: boolean;
  syncToMedplum: (params: {
    authState: AuthState;
    transcript: string;
    soapNote: SoapNote | null;
    elapsedMs: number;
  }) => Promise<void>;
  setMedplumConnected: (connected: boolean) => void;
  setMedplumError: (error: string | null) => void;
  setSyncError: (error: string | null) => void;
  setSyncSuccess: (success: boolean) => void;
  resetSyncState: () => void;
}

export function useMedplumSync(): UseMedplumSyncResult {
  const [medplumConnected, setMedplumConnected] = useState(false);
  const [medplumError, setMedplumError] = useState<string | null>(null);
  const [isSyncing, setIsSyncing] = useState(false);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [syncSuccess, setSyncSuccess] = useState(false);

  // Sync to Medplum
  const syncToMedplum = useCallback(
    async (params: {
      authState: AuthState;
      transcript: string;
      soapNote: SoapNote | null;
      elapsedMs: number;
    }) => {
      const { authState, transcript, soapNote, elapsedMs } = params;
      if (!authState.is_authenticated) return;

      setIsSyncing(true);
      setSyncError(null);
      setSyncSuccess(false);

      try {
        const audioFilePath = await invoke<string | null>('get_audio_file_path');

        const soapText = soapNote
          ? `SUBJECTIVE:\n${soapNote.subjective}\n\nOBJECTIVE:\n${soapNote.objective}\n\nASSESSMENT:\n${soapNote.assessment}\n\nPLAN:\n${soapNote.plan}`
          : null;

        const result = await invoke<SyncResult>('medplum_quick_sync', {
          transcript,
          soapNote: soapText,
          audioFilePath,
          sessionDurationMs: elapsedMs,
        });

        if (result.success) {
          setSyncSuccess(true);
        } else {
          setSyncError(result.error || 'Sync failed');
        }
      } catch (e) {
        console.error('Failed to sync to Medplum:', e);
        setSyncError(String(e));
      } finally {
        setIsSyncing(false);
      }
    },
    []
  );

  // Reset sync state for new session
  const resetSyncState = useCallback(() => {
    setSyncSuccess(false);
    setSyncError(null);
  }, []);

  return {
    medplumConnected,
    medplumError,
    isSyncing,
    syncError,
    syncSuccess,
    syncToMedplum,
    setMedplumConnected,
    setMedplumError,
    setSyncError,
    setSyncSuccess,
    resetSyncState,
  };
}
