import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { WhisperModelInfo } from '../types';

export interface DownloadProgress {
  modelId: string;
  status: 'downloading' | 'testing' | 'completed' | 'failed';
  error?: string;
}

export interface UseWhisperModelsResult {
  models: WhisperModelInfo[];
  modelsByCategory: Record<string, WhisperModelInfo[]>;
  isLoading: boolean;
  error: string | null;
  refreshModels: () => Promise<void>;
  downloadModel: (modelId: string) => Promise<boolean>;
  testModel: (modelId: string) => Promise<boolean>;
  downloadProgress: DownloadProgress | null;
  getModelById: (id: string) => WhisperModelInfo | undefined;
  isModelDownloaded: (id: string) => boolean;
}

/**
 * Hook for managing Whisper model listing, downloading, and testing.
 * Loads models with their download status on mount.
 */
export function useWhisperModels(): UseWhisperModelsResult {
  const [models, setModels] = useState<WhisperModelInfo[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<DownloadProgress | null>(null);
  const initialLoadRef = useRef(false);

  // Load models
  const loadModels = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<WhisperModelInfo[]>('get_whisper_models');
      setModels(result);
    } catch (e) {
      console.error('Failed to load whisper models:', e);
      setError(String(e));
      setModels([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Load models on mount (only once)
  useEffect(() => {
    if (initialLoadRef.current) return;
    initialLoadRef.current = true;
    loadModels();
  }, [loadModels]);

  // Group models by category
  const modelsByCategory = models.reduce(
    (acc, model) => {
      if (!acc[model.category]) {
        acc[model.category] = [];
      }
      acc[model.category].push(model);
      return acc;
    },
    {} as Record<string, WhisperModelInfo[]>
  );

  // Get model by ID
  const getModelById = useCallback(
    (id: string): WhisperModelInfo | undefined => {
      return models.find((m) => m.id === id);
    },
    [models]
  );

  // Check if model is downloaded
  const isModelDownloaded = useCallback(
    (id: string): boolean => {
      const model = models.find((m) => m.id === id);
      return model?.downloaded ?? false;
    },
    [models]
  );

  // Download a model
  const downloadModel = useCallback(
    async (modelId: string): Promise<boolean> => {
      setDownloadProgress({ modelId, status: 'downloading' });
      try {
        await invoke<string>('download_whisper_model_by_id', { modelId });

        // Test the model after download
        setDownloadProgress({ modelId, status: 'testing' });
        const testResult = await invoke<boolean>('test_whisper_model', { modelId });

        if (testResult) {
          setDownloadProgress({ modelId, status: 'completed' });
          // Refresh model list to update downloaded status
          await loadModels();
          setTimeout(() => setDownloadProgress(null), 2000);
          return true;
        } else {
          setDownloadProgress({
            modelId,
            status: 'failed',
            error: 'Model test failed - file may be corrupted',
          });
          return false;
        }
      } catch (e) {
        console.error('Failed to download model:', e);
        setDownloadProgress({
          modelId,
          status: 'failed',
          error: String(e),
        });
        return false;
      }
    },
    [loadModels]
  );

  // Test a model
  const testModel = useCallback(async (modelId: string): Promise<boolean> => {
    try {
      const result = await invoke<boolean>('test_whisper_model', { modelId });
      return result;
    } catch (e) {
      console.error('Failed to test model:', e);
      return false;
    }
  }, []);

  return {
    models,
    modelsByCategory,
    isLoading,
    error,
    refreshModels: loadModels,
    downloadModel,
    testModel,
    downloadProgress,
    getModelById,
    isModelDownloaded,
  };
}
