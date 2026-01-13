import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { AuthState, SoapNote, SyncResult, SyncedEncounter, MultiPatientSoapResult, MultiPatientSyncResult, PatientSyncInfo } from '../types';
import { formatErrorMessage } from '../utils';

/** Format a SoapNote to plain text for Medplum */
function formatSoapNote(soapNote: SoapNote): string {
  // SOAP note content is now a single text block from the LLM
  return soapNote.content;
}

export interface UseMedplumSyncResult {
  medplumConnected: boolean;
  medplumError: string | null;
  isSyncing: boolean;
  syncError: string | null;
  syncSuccess: boolean;
  /** The synced encounter for updates (null if not synced yet) - single patient */
  syncedEncounter: SyncedEncounter | null;
  /** Synced patients from multi-patient sync (empty if not multi-patient) */
  syncedPatients: PatientSyncInfo[];
  /** Whether SOAP is being added to an existing encounter */
  isAddingSoap: boolean;
  /** Single-patient sync */
  syncToMedplum: (params: {
    authState: AuthState;
    transcript: string;
    soapNote: SoapNote | null;
    elapsedMs: number;
  }) => Promise<void>;
  /** Multi-patient sync with auto-detected SOAP notes */
  syncMultiPatientToMedplum: (params: {
    authState: AuthState;
    transcript: string;
    soapResult: MultiPatientSoapResult;
    elapsedMs: number;
  }) => Promise<MultiPatientSyncResult | null>;
  /** Add SOAP note to an already-synced encounter */
  addSoapToEncounter: (soapNote: SoapNote) => Promise<boolean>;
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
  const [syncedEncounter, setSyncedEncounter] = useState<SyncedEncounter | null>(null);
  const [syncedPatients, setSyncedPatients] = useState<PatientSyncInfo[]>([]);
  const [isAddingSoap, setIsAddingSoap] = useState(false);

  // Sync to Medplum (initial sync with transcript + audio, optionally SOAP)
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

        const soapText = soapNote ? formatSoapNote(soapNote) : null;

        const result = await invoke<SyncResult>('medplum_quick_sync', {
          transcript,
          soapNote: soapText,
          audioFilePath,
          sessionDurationMs: elapsedMs,
        });

        if (result.success) {
          setSyncSuccess(true);
          // Store the encounter IDs for subsequent updates
          if (result.encounterId && result.encounterFhirId) {
            setSyncedEncounter({
              encounterId: result.encounterId,
              encounterFhirId: result.encounterFhirId,
              syncedAt: new Date().toISOString(),
              hasSoap: soapNote !== null,
            });
          }
        } else {
          setSyncError(result.error || 'Sync failed');
        }
      } catch (e) {
        console.error('Failed to sync to Medplum:', e);
        setSyncError(formatErrorMessage(e));
      } finally {
        setIsSyncing(false);
      }
    },
    []
  );

  // Multi-patient sync to Medplum
  // Creates a patient and encounter for each patient in the SOAP result
  const syncMultiPatientToMedplum = useCallback(
    async (params: {
      authState: AuthState;
      transcript: string;
      soapResult: MultiPatientSoapResult;
      elapsedMs: number;
    }): Promise<MultiPatientSyncResult | null> => {
      const { authState, transcript, soapResult, elapsedMs } = params;
      if (!authState.is_authenticated) return null;

      setIsSyncing(true);
      setSyncError(null);
      setSyncSuccess(false);

      try {
        const audioFilePath = await invoke<string | null>('get_audio_file_path');

        const result = await invoke<MultiPatientSyncResult>('medplum_multi_patient_quick_sync', {
          transcript,
          soapResult,
          audioFilePath,
          sessionDurationMs: elapsedMs,
        });

        if (result.success) {
          setSyncSuccess(true);
          // Store synced patients for reference
          setSyncedPatients(result.patients);
          // Also set the first patient's encounter as the primary synced encounter
          // for backward compatibility
          if (result.patients.length > 0) {
            const first = result.patients[0];
            setSyncedEncounter({
              encounterId: first.encounterFhirId,
              encounterFhirId: first.encounterFhirId,
              syncedAt: new Date().toISOString(),
              hasSoap: first.hasSoap,
            });
          }
        } else {
          setSyncError(result.error || 'Multi-patient sync failed');
        }

        return result;
      } catch (e) {
        console.error('Failed to sync multi-patient to Medplum:', e);
        setSyncError(formatErrorMessage(e));
        return null;
      } finally {
        setIsSyncing(false);
      }
    },
    []
  );

  // Add SOAP note to an already-synced encounter
  const addSoapToEncounter = useCallback(
    async (soapNote: SoapNote): Promise<boolean> => {
      if (!syncedEncounter) {
        console.warn('Cannot add SOAP: no synced encounter');
        return false;
      }

      setIsAddingSoap(true);
      setSyncError(null);

      try {
        const soapText = formatSoapNote(soapNote);

        const success = await invoke<boolean>('medplum_add_soap_to_encounter', {
          encounterFhirId: syncedEncounter.encounterFhirId,
          soapNote: soapText,
        });

        if (success) {
          // Update the synced encounter to reflect SOAP was added
          setSyncedEncounter(prev => prev ? { ...prev, hasSoap: true } : null);
          return true;
        } else {
          setSyncError('Failed to add SOAP note to encounter');
          return false;
        }
      } catch (e) {
        console.error('Failed to add SOAP to encounter:', e);
        setSyncError(formatErrorMessage(e));
        return false;
      } finally {
        setIsAddingSoap(false);
      }
    },
    [syncedEncounter]
  );

  // Reset sync state for new session
  const resetSyncState = useCallback(() => {
    setSyncSuccess(false);
    setSyncError(null);
    setSyncedEncounter(null);
    setSyncedPatients([]);
    setIsAddingSoap(false);
  }, []);

  return {
    medplumConnected,
    medplumError,
    isSyncing,
    syncError,
    syncSuccess,
    syncedEncounter,
    syncedPatients,
    isAddingSoap,
    syncToMedplum,
    syncMultiPatientToMedplum,
    addSoapToEncounter,
    setMedplumConnected,
    setMedplumError,
    setSyncError,
    setSyncSuccess,
    resetSyncState,
  };
}
