import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Settings } from '../types';

/**
 * Pending settings that the user is editing before saving.
 * A subset of Settings fields that are commonly changed in the UI.
 */
export interface PendingSettings {
  model: string;
  language: string;
  device: string;
  diarization_enabled: boolean;
  max_speakers: number;
  ollama_server_url: string;
  ollama_model: string;
  medplum_server_url: string;
  medplum_client_id: string;
  medplum_auto_sync: boolean;
  // Whisper server settings (remote only - local mode removed)
  whisper_mode: 'remote';  // Always 'remote'
  whisper_server_url: string;
  whisper_server_model: string;
}

export interface UseSettingsResult {
  // Settings state
  settings: Settings | null;
  pendingSettings: PendingSettings | null;
  isLoading: boolean;
  isSaving: boolean;
  error: string | null;

  // Actions
  setPendingSettings: (settings: PendingSettings | null) => void;
  saveSettings: () => Promise<boolean>;
  reloadSettings: () => Promise<void>;

  // Derived helpers
  hasUnsavedChanges: boolean;
}

/**
 * Hook for managing application settings with pending changes.
 * Loads settings on mount and provides save functionality.
 */
export function useSettings(): UseSettingsResult {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [pendingSettings, setPendingSettings] = useState<PendingSettings | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const initialLoadRef = useRef(false);

  // Create pending settings from full settings
  const createPendingFromSettings = useCallback((s: Settings): PendingSettings => ({
    model: s.whisper_model,
    language: s.language,
    device: s.input_device_id || 'default',
    diarization_enabled: true, // Always enabled - speaker detection is always on
    max_speakers: s.max_speakers,
    ollama_server_url: s.ollama_server_url,
    ollama_model: s.ollama_model,
    medplum_server_url: s.medplum_server_url,
    medplum_client_id: s.medplum_client_id,
    medplum_auto_sync: s.medplum_auto_sync,
    whisper_mode: 'remote',  // Always remote - local mode removed
    whisper_server_url: s.whisper_server_url,
    whisper_server_model: s.whisper_server_model,
  }), []);

  // Load settings
  const loadSettings = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<Settings>('get_settings');
      setSettings(result);
      setPendingSettings(createPendingFromSettings(result));
    } catch (e) {
      console.error('Failed to load settings:', e);
      setError(String(e));
    } finally {
      setIsLoading(false);
    }
  }, [createPendingFromSettings]);

  // Load settings on mount (only once)
  useEffect(() => {
    if (initialLoadRef.current) return;
    initialLoadRef.current = true;
    loadSettings();
  }, [loadSettings]);

  // Save settings
  const saveSettings = useCallback(async (): Promise<boolean> => {
    if (!settings || !pendingSettings) return false;

    setIsSaving(true);
    setError(null);

    try {
      const newSettings: Settings = {
        ...settings,
        whisper_model: pendingSettings.model,
        language: pendingSettings.language,
        input_device_id: pendingSettings.device === 'default' ? null : pendingSettings.device,
        diarization_enabled: true, // Always enabled - speaker detection is always on
        max_speakers: pendingSettings.max_speakers,
        ollama_server_url: pendingSettings.ollama_server_url,
        ollama_model: pendingSettings.ollama_model,
        medplum_server_url: pendingSettings.medplum_server_url,
        medplum_client_id: pendingSettings.medplum_client_id,
        medplum_auto_sync: pendingSettings.medplum_auto_sync,
        whisper_mode: 'remote',  // Always remote - local mode removed
        whisper_server_url: pendingSettings.whisper_server_url,
        whisper_server_model: pendingSettings.whisper_server_model,
      };

      await invoke('set_settings', { settings: newSettings });
      setSettings(newSettings);
      return true;
    } catch (e) {
      console.error('Failed to save settings:', e);
      setError(String(e));
      return false;
    } finally {
      setIsSaving(false);
    }
  }, [settings, pendingSettings]);

  // Check if there are unsaved changes
  // Note: whisper_mode removed from comparison - always 'remote'
  const hasUnsavedChanges = settings !== null && pendingSettings !== null && (
    settings.whisper_model !== pendingSettings.model ||
    settings.language !== pendingSettings.language ||
    (settings.input_device_id || 'default') !== pendingSettings.device ||
    settings.max_speakers !== pendingSettings.max_speakers ||
    settings.ollama_server_url !== pendingSettings.ollama_server_url ||
    settings.ollama_model !== pendingSettings.ollama_model ||
    settings.medplum_server_url !== pendingSettings.medplum_server_url ||
    settings.medplum_client_id !== pendingSettings.medplum_client_id ||
    settings.medplum_auto_sync !== pendingSettings.medplum_auto_sync ||
    settings.whisper_server_url !== pendingSettings.whisper_server_url ||
    settings.whisper_server_model !== pendingSettings.whisper_server_model
  );

  return {
    settings,
    pendingSettings,
    isLoading,
    isSaving,
    error,
    setPendingSettings,
    saveSettings,
    reloadSettings: loadSettings,
    hasUnsavedChanges,
  };
}
