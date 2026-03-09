/**
 * useConnectionTests — Composite hook that groups all connection testing state
 * and callbacks for the SettingsDrawer.
 *
 * Internally composes:
 * - useOllamaConnection() — LLM Router connection checking
 * - Sync effect: ollamaConnectionStatus -> setOllamaStatus/setOllamaModels
 * - Medplum init effect (restore session + check connection)
 * - Whisper server status state
 * - handleTestLLM, handleTestMedplum, handleTestWhisperServer callbacks
 *
 * Note: Ollama status/models state lives in useSoapNote; Medplum connected/error
 * state lives in useMedplumSync. This hook bridges them via setters passed in.
 */
import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Settings, WhisperServerStatus, OllamaStatus } from '../types';
import type { PendingSettings } from './useSettings';
import { useOllamaConnection } from './useOllamaConnection';

// ============================================================================
// Types
// ============================================================================

export interface ConnectionTestsConfig {
  settings: Settings | null;
  pendingSettings: PendingSettings | null;
  /** Setter from useSoapNote — updates LLM status used for auto-SOAP generation */
  setOllamaStatus: (status: OllamaStatus | null) => void;
  /** Setter from useSoapNote — updates available LLM models list */
  setOllamaModels: (models: string[]) => void;
  /** Setter from useMedplumSync — updates Medplum connection status */
  setMedplumConnected: (connected: boolean) => void;
  /** Setter from useMedplumSync — updates Medplum error */
  setMedplumError: (error: string | null) => void;
}

export interface ConnectionTestsResult {
  // Whisper server state (owned by this hook)
  whisperServerStatus: WhisperServerStatus | null;
  whisperServerModels: string[];
  // Test handlers (used by SettingsDrawer)
  handleTestLLM: () => Promise<void>;
  handleTestMedplum: () => Promise<void>;
  handleTestWhisperServer: () => Promise<void>;
}

// ============================================================================
// Hook
// ============================================================================

export function useConnectionTests({
  settings: _settings,
  pendingSettings,
  setOllamaStatus,
  setOllamaModels,
  setMedplumConnected,
  setMedplumError,
}: ConnectionTestsConfig): ConnectionTestsResult {
  // Ollama connection from hook (initial status check on mount)
  const { status: ollamaConnectionStatus } = useOllamaConnection();

  // Whisper server state (owned by this hook)
  const [whisperServerStatus, setWhisperServerStatus] = useState<WhisperServerStatus | null>(null);
  const [whisperServerModels, setWhisperServerModels] = useState<string[]>([]);

  // Sync Ollama status from connection hook to SOAP hook
  useEffect(() => {
    if (ollamaConnectionStatus) {
      setOllamaStatus(ollamaConnectionStatus);
      if (ollamaConnectionStatus.connected) {
        setOllamaModels(ollamaConnectionStatus.available_models);
      }
    }
  }, [ollamaConnectionStatus, setOllamaStatus, setOllamaModels]);

  // Check Medplum connection and restore session on mount
  useEffect(() => {
    let mounted = true;

    async function initMedplum() {
      try {
        // Try restore session
        await invoke('medplum_try_restore_session');
        if (!mounted) return;

        // Check Medplum connection
        const connected = await invoke<boolean>('medplum_check_connection');
        if (mounted) {
          setMedplumConnected(connected);
        }
      } catch (e) {
        if (mounted) {
          setMedplumConnected(false);
          setMedplumError(String(e));
        }
      }
    }
    initMedplum();

    return () => {
      mounted = false;
    };
  }, [setMedplumConnected, setMedplumError]);

  // Test LLM Router connection (passes pending URL directly, does not persist)
  const handleTestLLM = useCallback(async () => {
    if (!pendingSettings) return;
    try {
      const status = await invoke<OllamaStatus>('check_ollama_status', {
        url: pendingSettings.llm_router_url,
        apiKey: pendingSettings.llm_api_key,
        clientId: pendingSettings.llm_client_id,
      });
      setOllamaStatus(status);
      if (status.connected) {
        setOllamaModels(status.available_models);
      }
    } catch (e) {
      console.error('Failed to test LLM router:', e);
      setOllamaStatus({ connected: false, available_models: [], error: String(e) });
    }
  }, [pendingSettings, setOllamaStatus, setOllamaModels]);

  // Test Medplum connection (passes pending URL directly, does not persist)
  const handleTestMedplum = useCallback(async () => {
    if (!pendingSettings) return;
    setMedplumError(null);
    try {
      const result = await invoke<boolean>('medplum_check_connection', {
        url: pendingSettings.medplum_server_url,
      });
      setMedplumConnected(result);
      if (!result) {
        setMedplumError('Could not connect to server');
      }
    } catch (e) {
      console.error('Failed to test Medplum:', e);
      setMedplumConnected(false);
      setMedplumError(String(e));
    }
  }, [pendingSettings, setMedplumConnected, setMedplumError]);

  // Test Whisper Server connection (passes pending URL directly, does not persist)
  const handleTestWhisperServer = useCallback(async () => {
    if (!pendingSettings) return;
    try {
      const status = await invoke<WhisperServerStatus>('check_whisper_server_status', {
        url: pendingSettings.whisper_server_url,
      });
      setWhisperServerStatus(status);
      if (status.connected) {
        setWhisperServerModels(status.available_models);
      }
    } catch (e) {
      console.error('Failed to test Whisper server:', e);
      setWhisperServerStatus({ connected: false, available_models: [], error: String(e) });
    }
  }, [pendingSettings]);

  return {
    whisperServerStatus,
    whisperServerModels,
    handleTestLLM,
    handleTestMedplum,
    handleTestWhisperServer,
  };
}
