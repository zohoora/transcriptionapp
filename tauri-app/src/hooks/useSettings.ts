import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Settings, SpeakerRole, ChartingMode, EncounterDetectionMode } from '../types';

/**
 * Pending settings that the user is editing before saving.
 * A subset of Settings fields that are commonly changed in the UI.
 */
export interface PendingSettings {
  language: string;
  device: string;
  // LLM Router settings (OpenAI-compatible API)
  llm_router_url: string;
  llm_api_key: string;
  llm_client_id: string;
  soap_model: string;
  fast_model: string;
  // Medplum EMR settings
  medplum_server_url: string;
  medplum_client_id: string;
  medplum_auto_sync: boolean;
  // STT Router settings
  whisper_server_url: string;
  // Auto-session detection settings
  auto_start_enabled: boolean;
  auto_start_require_enrolled: boolean;
  auto_start_required_role: SpeakerRole | null;
  // Auto-end settings
  auto_end_enabled: boolean;
  // AI image generation
  image_source: string;
  gemini_api_key: string;
  // Screen capture
  screen_capture_enabled: boolean;
  // Continuous charting mode
  charting_mode: ChartingMode;
  // Presence sensor settings (hybrid mode)
  encounter_detection_mode: EncounterDetectionMode;
  presence_sensor_port: string;
  presence_sensor_url: string;
  presence_absence_threshold_secs: number;
  // Encounter merge
  encounter_merge_enabled: boolean;
  // SOAP personal instructions
  soap_custom_instructions: string;
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
    language: s.language,
    device: s.input_device_id || 'default',
    llm_router_url: s.llm_router_url,
    llm_api_key: s.llm_api_key,
    llm_client_id: s.llm_client_id,
    soap_model: s.soap_model,
    fast_model: s.fast_model,
    medplum_server_url: s.medplum_server_url,
    medplum_client_id: s.medplum_client_id,
    medplum_auto_sync: s.medplum_auto_sync,
    whisper_server_url: s.whisper_server_url,
    auto_start_enabled: s.auto_start_enabled,
    auto_start_require_enrolled: s.auto_start_require_enrolled,
    auto_start_required_role: s.auto_start_required_role,
    auto_end_enabled: s.auto_end_enabled,
    image_source: s.image_source,
    gemini_api_key: s.gemini_api_key,
    screen_capture_enabled: s.screen_capture_enabled,
    charting_mode: s.charting_mode,
    encounter_detection_mode: s.encounter_detection_mode,
    presence_sensor_port: s.presence_sensor_port,
    presence_sensor_url: s.presence_sensor_url,
    presence_absence_threshold_secs: s.presence_absence_threshold_secs,
    encounter_merge_enabled: s.encounter_merge_enabled,
    soap_custom_instructions: s.soap_custom_instructions,
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
        language: pendingSettings.language,
        input_device_id: pendingSettings.device === 'default' ? null : pendingSettings.device,
        diarization_enabled: true, // Always enabled - speaker detection is always on
        llm_router_url: pendingSettings.llm_router_url,
        llm_api_key: pendingSettings.llm_api_key,
        llm_client_id: pendingSettings.llm_client_id,
        soap_model: pendingSettings.soap_model,
        fast_model: pendingSettings.fast_model,
        medplum_server_url: pendingSettings.medplum_server_url,
        medplum_client_id: pendingSettings.medplum_client_id,
        medplum_auto_sync: pendingSettings.medplum_auto_sync,
        whisper_mode: 'remote',  // Always remote - local mode removed
        whisper_server_url: pendingSettings.whisper_server_url,
        auto_start_enabled: pendingSettings.auto_start_enabled,
        auto_start_require_enrolled: pendingSettings.auto_start_require_enrolled,
        auto_start_required_role: pendingSettings.auto_start_required_role,
        auto_end_enabled: pendingSettings.auto_end_enabled,
        miis_enabled: pendingSettings.image_source === 'miis', // Derive from image_source
        image_source: pendingSettings.image_source,
        gemini_api_key: pendingSettings.gemini_api_key,
        screen_capture_enabled: pendingSettings.screen_capture_enabled,
        charting_mode: pendingSettings.charting_mode,
        encounter_merge_enabled: pendingSettings.encounter_merge_enabled,
        encounter_detection_mode: pendingSettings.encounter_detection_mode,
        presence_sensor_port: pendingSettings.presence_sensor_port,
        presence_sensor_url: pendingSettings.presence_sensor_url,
        presence_absence_threshold_secs: pendingSettings.presence_absence_threshold_secs,
        soap_custom_instructions: pendingSettings.soap_custom_instructions,
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

  // Check if there are unsaved changes by comparing settings to pending values
  const hasUnsavedChanges = useMemo(() => {
    if (!settings || !pendingSettings) return false;

    const comparisons: [unknown, unknown][] = [
      [settings.language, pendingSettings.language],
      [settings.input_device_id || 'default', pendingSettings.device],
      [settings.llm_router_url, pendingSettings.llm_router_url],
      [settings.llm_api_key, pendingSettings.llm_api_key],
      [settings.llm_client_id, pendingSettings.llm_client_id],
      [settings.soap_model, pendingSettings.soap_model],
      [settings.fast_model, pendingSettings.fast_model],
      [settings.medplum_server_url, pendingSettings.medplum_server_url],
      [settings.medplum_client_id, pendingSettings.medplum_client_id],
      [settings.medplum_auto_sync, pendingSettings.medplum_auto_sync],
      [settings.whisper_server_url, pendingSettings.whisper_server_url],
      [settings.auto_start_enabled, pendingSettings.auto_start_enabled],
      [settings.auto_start_require_enrolled, pendingSettings.auto_start_require_enrolled],
      [settings.auto_start_required_role, pendingSettings.auto_start_required_role],
      [settings.auto_end_enabled, pendingSettings.auto_end_enabled],
      [settings.image_source, pendingSettings.image_source],
      [settings.gemini_api_key, pendingSettings.gemini_api_key],
      [settings.screen_capture_enabled, pendingSettings.screen_capture_enabled],
      [settings.charting_mode, pendingSettings.charting_mode],
      [settings.encounter_merge_enabled, pendingSettings.encounter_merge_enabled],
      [settings.encounter_detection_mode, pendingSettings.encounter_detection_mode],
      [settings.presence_sensor_port, pendingSettings.presence_sensor_port],
      [settings.presence_sensor_url, pendingSettings.presence_sensor_url],
      [settings.presence_absence_threshold_secs, pendingSettings.presence_absence_threshold_secs],
      [settings.soap_custom_instructions, pendingSettings.soap_custom_instructions],
    ];

    return comparisons.some(([saved, pending]) => saved !== pending);
  }, [settings, pendingSettings]);

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
