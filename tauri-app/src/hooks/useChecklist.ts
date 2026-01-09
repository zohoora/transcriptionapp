import { useState, useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ChecklistResult, ModelStatus } from '../types';

/** Microphone permission status from backend */
export interface MicrophonePermissionStatus {
  status: string;
  authorized: boolean;
  message: string;
}

export interface UseChecklistResult {
  checklistResult: ChecklistResult | null;
  checklistRunning: boolean;
  downloadingModel: string | null;
  modelStatus: ModelStatus | null;
  runChecklist: () => Promise<void>;
  handleDownloadModel: (modelName: string) => Promise<void>;
  setModelStatus: (status: ModelStatus | null) => void;
  /** Check microphone permission and optionally open settings if denied */
  checkMicrophonePermission: () => Promise<MicrophonePermissionStatus>;
  /** Open system settings to microphone privacy section */
  openMicrophoneSettings: () => Promise<void>;
}

export function useChecklist(): UseChecklistResult {
  const [checklistResult, setChecklistResult] = useState<ChecklistResult | null>(null);
  const [checklistRunning, setChecklistRunning] = useState(true);
  const [downloadingModel, setDownloadingModel] = useState<string | null>(null);
  const [modelStatus, setModelStatus] = useState<ModelStatus | null>(null);
  const initialRunRef = useRef(false);

  // Run checklist function
  const runChecklist = useCallback(async () => {
    setChecklistRunning(true);
    try {
      const result = await invoke<ChecklistResult>('run_checklist');
      setChecklistResult(result);
    } catch (e) {
      console.error('Failed to run checklist:', e);
      setChecklistResult({
        checks: [],
        all_passed: false,
        can_start: false,
        summary: 'Failed to run checklist',
      });
    } finally {
      setChecklistRunning(false);
    }
  }, []);

  // Run checklist and check model status on mount (only once)
  useEffect(() => {
    if (initialRunRef.current) return;
    initialRunRef.current = true;

    async function init() {
      await runChecklist();
      try {
        const status = await invoke<ModelStatus>('check_model_status');
        setModelStatus(status);
      } catch (e) {
        console.error('Failed to check model status:', e);
      }
    }
    init();
  }, [runChecklist]);

  // Handle model download
  const handleDownloadModel = useCallback(
    async (modelName: string) => {
      setDownloadingModel(modelName);
      try {
        let command = '';
        if (modelName === 'speaker_embedding') {
          command = 'download_speaker_model';
        } else if (modelName === 'gtcrn_simple') {
          command = 'download_enhancement_model';
        } else if (modelName === 'yamnet') {
          command = 'download_yamnet_model';
        } else {
          command = 'download_whisper_model';
        }
        // Pass model name for Whisper downloads
        if (command === 'download_whisper_model') {
          await invoke(command, { modelName });
        } else {
          await invoke(command);
        }
        await runChecklist();
        const modelResult = await invoke<ModelStatus>('check_model_status');
        setModelStatus(modelResult);
      } catch (e) {
        console.error('Failed to download model:', e);
      } finally {
        setDownloadingModel(null);
      }
    },
    [runChecklist]
  );

  // Check microphone permission
  const checkMicrophonePermission = useCallback(async (): Promise<MicrophonePermissionStatus> => {
    try {
      const result = await invoke<MicrophonePermissionStatus>('check_microphone_permission');
      return result;
    } catch (e) {
      console.error('Failed to check microphone permission:', e);
      return {
        status: 'unknown',
        authorized: false,
        message: `Failed to check permission: ${e}`,
      };
    }
  }, []);

  // Open microphone settings
  const openMicrophoneSettings = useCallback(async (): Promise<void> => {
    try {
      await invoke('open_microphone_settings');
    } catch (e) {
      console.error('Failed to open microphone settings:', e);
    }
  }, []);

  return {
    checklistResult,
    checklistRunning,
    downloadingModel,
    modelStatus,
    runChecklist,
    handleDownloadModel,
    setModelStatus,
    checkMicrophonePermission,
    openMicrophoneSettings,
  };
}
