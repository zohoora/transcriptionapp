import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { LLMStatus, Settings } from '../types';
import { formatErrorMessage } from '../utils';

export interface UseOllamaConnectionResult {
  status: LLMStatus | null;
  isChecking: boolean;
  isPrewarming: boolean;
  error: string | null;
  checkConnection: () => Promise<void>;
  prewarmModel: () => Promise<void>;
  testConnection: (routerUrl: string, apiKey: string, clientId: string, soapModel: string, fastModel: string, currentSettings: Settings) => Promise<boolean>;
}

/**
 * Hook for managing LLM Router connection status.
 * Checks connection on mount and provides test functionality.
 *
 * Note: Named useOllamaConnection for backward compatibility,
 * but now uses OpenAI-compatible LLM router API.
 */
export function useOllamaConnection(): UseOllamaConnectionResult {
  const [status, setStatus] = useState<LLMStatus | null>(null);
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
      const result = await invoke<LLMStatus>('check_ollama_status');
      setStatus(result);
      if (!result.connected && result.error) {
        setError(result.error);
      }
    } catch (e) {
      console.error('Failed to check LLM router status:', e);
      const errorMsg = formatErrorMessage(e);
      setError(errorMsg);
      setStatus({ connected: false, available_models: [], error: errorMsg });
    } finally {
      setIsChecking(false);
    }
  }, []);

  // Pre-warm the LLM model to reduce latency on first request
  const prewarmModel = useCallback(async () => {
    // Skip if already prewarming or if prewarm was already attempted
    if (isPrewarming || prewarmAttemptedRef.current) return;
    prewarmAttemptedRef.current = true;
    setIsPrewarming(true);
    try {
      console.log('Pre-warming LLM model...');
      await invoke('prewarm_ollama_model');
      console.log('LLM model pre-warmed successfully');
    } catch (e) {
      // Pre-warming is a best-effort operation, don't set error state
      console.warn('Failed to pre-warm LLM model:', e);
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
      // Run pre-warming in background (prewarmModel will set the ref)
      prewarmModel();
    }
  }, [status?.connected, prewarmModel]);

  // Test connection with specific settings (saves temporarily to test)
  const testConnection = useCallback(
    async (
      routerUrl: string,
      apiKey: string,
      clientId: string,
      soapModel: string,
      fastModel: string,
      currentSettings: Settings
    ): Promise<boolean> => {
      setIsChecking(true);
      setError(null);

      try {
        // Temporarily save settings to test the connection
        const testSettings: Settings = {
          ...currentSettings,
          llm_router_url: routerUrl,
          llm_api_key: apiKey,
          llm_client_id: clientId,
          soap_model: soapModel,
          fast_model: fastModel,
        };
        await invoke('set_settings', { settings: testSettings });

        // Check status with new settings
        const result = await invoke<LLMStatus>('check_ollama_status');
        setStatus(result);

        if (!result.connected && result.error) {
          setError(result.error);
        }

        return result.connected;
      } catch (e) {
        console.error('Failed to test LLM router connection:', e);
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
