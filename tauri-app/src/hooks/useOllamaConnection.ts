import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { OllamaStatus, Settings } from '../types';
import { formatErrorMessage } from '../utils';

export interface UseOllamaConnectionResult {
  status: OllamaStatus | null;
  isChecking: boolean;
  isPrewarming: boolean;
  error: string | null;
  checkConnection: () => Promise<void>;
  prewarmModel: () => Promise<void>;
  testConnection: (serverUrl: string, model: string, currentSettings: Settings) => Promise<boolean>;
}

/**
 * Hook for managing Ollama LLM server connection status.
 * Checks connection on mount and provides test functionality.
 */
export function useOllamaConnection(): UseOllamaConnectionResult {
  const [status, setStatus] = useState<OllamaStatus | null>(null);
  const [isChecking, setIsChecking] = useState(false);
  const [isPrewarming, setIsPrewarming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const initialCheckRef = useRef(false);
  const prewarmAttemptedRef = useRef(false);

  // Check connection status
  const checkConnection = useCallback(async () => {
    setIsChecking(true);
    setError(null);
    try {
      const result = await invoke<OllamaStatus>('check_ollama_status');
      setStatus(result);
      if (!result.connected && result.error) {
        setError(result.error);
      }
    } catch (e) {
      console.error('Failed to check Ollama status:', e);
      const errorMsg = formatErrorMessage(e);
      setError(errorMsg);
      setStatus({ connected: false, available_models: [], error: errorMsg });
    } finally {
      setIsChecking(false);
    }
  }, []);

  // Pre-warm the Ollama model to reduce latency on first request
  const prewarmModel = useCallback(async () => {
    if (isPrewarming) return;
    setIsPrewarming(true);
    try {
      console.log('Pre-warming Ollama model...');
      await invoke('prewarm_ollama_model');
      console.log('Ollama model pre-warmed successfully');
    } catch (e) {
      // Pre-warming is a best-effort operation, don't set error state
      console.warn('Failed to pre-warm Ollama model:', e);
    } finally {
      setIsPrewarming(false);
    }
  }, [isPrewarming]);

  // Check on mount (only once) and pre-warm if connected
  useEffect(() => {
    if (initialCheckRef.current) return;
    initialCheckRef.current = true;
    checkConnection();
  }, [checkConnection]);

  // Auto pre-warm when connection is established
  useEffect(() => {
    if (status?.connected && !prewarmAttemptedRef.current) {
      prewarmAttemptedRef.current = true;
      // Run pre-warming in background
      prewarmModel();
    }
  }, [status?.connected, prewarmModel]);

  // Test connection with specific settings (saves temporarily to test)
  const testConnection = useCallback(
    async (serverUrl: string, model: string, currentSettings: Settings): Promise<boolean> => {
      setIsChecking(true);
      setError(null);

      try {
        // Temporarily save settings to test the connection
        const testSettings: Settings = {
          ...currentSettings,
          ollama_server_url: serverUrl,
          ollama_model: model,
        };
        await invoke('set_settings', { settings: testSettings });

        // Check status with new settings
        const result = await invoke<OllamaStatus>('check_ollama_status');
        setStatus(result);

        if (!result.connected && result.error) {
          setError(result.error);
        }

        return result.connected;
      } catch (e) {
        console.error('Failed to test Ollama connection:', e);
        const errorMsg = formatErrorMessage(e);
        setError(errorMsg);
        setStatus({ connected: false, available_models: [], error: errorMsg });
        return false;
      } finally {
        setIsChecking(false);
      }
    },
    []
  );

  return {
    status,
    isChecking,
    isPrewarming,
    error,
    checkConnection,
    prewarmModel,
    testConnection,
  };
}
