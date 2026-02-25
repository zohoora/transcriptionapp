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
  settings,
  pendingSettings,
  setOllamaStatus,
  setOllamaModels,
  setMedplumConnected,
  setMedplumError,
}: ConnectionTestsConfig): ConnectionTestsResult {
  // Ollama connection from hook
  const { status: ollamaConnectionStatus, checkConnection: checkOllamaConnection } = useOllamaConnection();

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

  // Test LLM Router connection
  const handleTestLLM = useCallback(async () => {
    if (!pendingSettings || !settings) return;
    try {
      await invoke('set_settings', {
        settings: {
          ...settings,
          llm_router_url: pendingSettings.llm_router_url,
          llm_api_key: pendingSettings.llm_api_key,
          llm_client_id: pendingSettings.llm_client_id,
          soap_model: pendingSettings.soap_model,
          fast_model: pendingSettings.fast_model,
        },
      });
      await checkOllamaConnection();
    } catch (e) {
      console.error('Failed to test LLM router:', e);
      setOllamaStatus({ connected: false, available_models: [], error: String(e) });
    }
  }, [settings, pendingSettings, checkOllamaConnection, setOllamaStatus]);

  // Test Medplum connection
  const handleTestMedplum = useCallback(async () => {
    if (!pendingSettings || !settings) return;
    setMedplumError(null);
    try {
      const testSettings: Settings = {
        ...settings,
        medplum_server_url: pendingSettings.medplum_server_url,
        medplum_client_id: pendingSettings.medplum_client_id,
        medplum_auto_sync: pendingSettings.medplum_auto_sync,
      };
      await invoke('set_settings', { settings: testSettings });

      const result = await invoke<boolean>('medplum_check_connection');
      setMedplumConnected(result);
      if (!result) {
        setMedplumError('Could not connect to server');
      }
    } catch (e) {
      console.error('Failed to test Medplum:', e);
      setMedplumConnected(false);
      setMedplumError(String(e));
    }
  }, [settings, pendingSettings, setMedplumConnected, setMedplumError]);

  // Test Whisper Server connection
  const handleTestWhisperServer = useCallback(async () => {
    if (!pendingSettings || !settings) return;
    try {
      await invoke('set_settings', {
        settings: {
          ...settings,
          whisper_server_url: pendingSettings.whisper_server_url,
          whisper_server_model: pendingSettings.whisper_server_model,
        },
      });

      const status = await invoke<WhisperServerStatus>('check_whisper_server_status');
      setWhisperServerStatus(status);
      if (status.connected) {
        setWhisperServerModels(status.available_models);
      }
    } catch (e) {
      console.error('Failed to test Whisper server:', e);
      setWhisperServerStatus({ connected: false, available_models: [], error: String(e) });
    }
  }, [settings, pendingSettings]);

  return {
    whisperServerStatus,
    whisperServerModels,
    handleTestLLM,
    handleTestMedplum,
    handleTestWhisperServer,
  };
}
