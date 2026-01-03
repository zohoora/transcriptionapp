import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { OllamaStatus, Settings } from '../types';
import { formatErrorMessage } from '../utils';

export interface UseOllamaConnectionResult {
  status: OllamaStatus | null;
  isChecking: boolean;
  error: string | null;
  checkConnection: () => Promise<void>;
  testConnection: (serverUrl: string, model: string, currentSettings: Settings) => Promise<boolean>;
}

/**
 * Hook for managing Ollama LLM server connection status.
 * Checks connection on mount and provides test functionality.
 */
export function useOllamaConnection(): UseOllamaConnectionResult {
  const [status, setStatus] = useState<OllamaStatus | null>(null);
  const [isChecking, setIsChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const initialCheckRef = useRef(false);

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

  // Check on mount (only once)
  useEffect(() => {
    if (initialCheckRef.current) return;
    initialCheckRef.current = true;
    checkConnection();
  }, [checkConnection]);

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
    error,
    checkConnection,
    testConnection,
  };
}
