import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ChecklistResult, ModelStatus } from '../types';

export interface UseChecklistResult {
  checklistResult: ChecklistResult | null;
  checklistRunning: boolean;
  downloadingModel: string | null;
  modelStatus: ModelStatus | null;
  runChecklist: () => Promise<void>;
  handleDownloadModel: (modelName: string) => Promise<void>;
  setModelStatus: (status: ModelStatus | null) => void;
}

export function useChecklist(): UseChecklistResult {
  const [checklistResult, setChecklistResult] = useState<ChecklistResult | null>(null);
  const [checklistRunning, setChecklistRunning] = useState(true);
  const [downloadingModel, setDownloadingModel] = useState<string | null>(null);
  const [modelStatus, setModelStatus] = useState<ModelStatus | null>(null);

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
        } else if (modelName === 'wav2small') {
          command = 'download_emotion_model';
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

  return {
    checklistResult,
    checklistRunning,
    downloadingModel,
    modelStatus,
    runChecklist,
    handleDownloadModel,
    setModelStatus,
  };
}
