import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Settings, SpeakerRole } from '../types';

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
  // Whisper server settings (remote only - local mode removed)
  whisper_mode: 'remote';  // Always 'remote'
  whisper_server_url: string;
  whisper_server_model: string;
  // Auto-session detection settings
  auto_start_enabled: boolean;
  auto_start_require_enrolled: boolean;
  auto_start_required_role: SpeakerRole | null;
  // Auto-end settings
  auto_end_enabled: boolean;
  // MIIS (Medical Illustration Image Server) settings
  miis_enabled: boolean;
  miis_server_url: string;
  // Screen capture settings
  screen_capture_enabled: boolean;
  screen_capture_interval_secs: number;
  // Continuous charting mode
  charting_mode: string;
  encounter_check_interval_secs: number;
  encounter_silence_trigger_secs: number;
  // Presence sensor settings
  encounter_detection_mode: string;
  presence_sensor_port: string;
  presence_absence_threshold_secs: number;
  presence_debounce_secs: number;
  presence_csv_log_enabled: boolean;
  // Shadow mode settings
  shadow_active_method: string;
  shadow_csv_log_enabled: boolean;
  // Native STT shadow (Apple Speech)
  native_stt_shadow_enabled: boolean;
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
    llm_router_url: s.llm_router_url,
    llm_api_key: s.llm_api_key,
    llm_client_id: s.llm_client_id,
    soap_model: s.soap_model,
    fast_model: s.fast_model,
    medplum_server_url: s.medplum_server_url,
    medplum_client_id: s.medplum_client_id,
    medplum_auto_sync: s.medplum_auto_sync,
    whisper_mode: 'remote',  // Always remote - local mode removed
    whisper_server_url: s.whisper_server_url,
    whisper_server_model: s.whisper_server_model,
    auto_start_enabled: s.auto_start_enabled,
    auto_start_require_enrolled: s.auto_start_require_enrolled,
    auto_start_required_role: s.auto_start_required_role,
    auto_end_enabled: s.auto_end_enabled,
    miis_enabled: s.miis_enabled,
    miis_server_url: s.miis_server_url,
    screen_capture_enabled: s.screen_capture_enabled,
    screen_capture_interval_secs: s.screen_capture_interval_secs,
    charting_mode: s.charting_mode,
    encounter_check_interval_secs: s.encounter_check_interval_secs,
    encounter_silence_trigger_secs: s.encounter_silence_trigger_secs,
    encounter_detection_mode: s.encounter_detection_mode,
    presence_sensor_port: s.presence_sensor_port,
    presence_absence_threshold_secs: s.presence_absence_threshold_secs,
    presence_debounce_secs: s.presence_debounce_secs,
    presence_csv_log_enabled: s.presence_csv_log_enabled,
    shadow_active_method: s.shadow_active_method,
    shadow_csv_log_enabled: s.shadow_csv_log_enabled,
    native_stt_shadow_enabled: s.native_stt_shadow_enabled,
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
        whisper_server_model: pendingSettings.whisper_server_model,
        auto_start_enabled: pendingSettings.auto_start_enabled,
        auto_start_require_enrolled: pendingSettings.auto_start_require_enrolled,
        auto_start_required_role: pendingSettings.auto_start_required_role,
        auto_end_enabled: pendingSettings.auto_end_enabled,
        miis_enabled: pendingSettings.miis_enabled,
        miis_server_url: pendingSettings.miis_server_url,
        screen_capture_enabled: pendingSettings.screen_capture_enabled,
        screen_capture_interval_secs: pendingSettings.screen_capture_interval_secs,
        charting_mode: pendingSettings.charting_mode as Settings['charting_mode'],
        continuous_auto_copy_soap: settings.continuous_auto_copy_soap,
        encounter_check_interval_secs: pendingSettings.encounter_check_interval_secs,
        encounter_silence_trigger_secs: pendingSettings.encounter_silence_trigger_secs,
        encounter_detection_mode: pendingSettings.encounter_detection_mode,
        presence_sensor_port: pendingSettings.presence_sensor_port,
        presence_absence_threshold_secs: pendingSettings.presence_absence_threshold_secs,
        presence_debounce_secs: pendingSettings.presence_debounce_secs,
        presence_csv_log_enabled: pendingSettings.presence_csv_log_enabled,
        shadow_active_method: pendingSettings.shadow_active_method,
        shadow_csv_log_enabled: pendingSettings.shadow_csv_log_enabled,
        native_stt_shadow_enabled: pendingSettings.native_stt_shadow_enabled,
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
      [settings.whisper_model, pendingSettings.model],
      [settings.language, pendingSettings.language],
      [settings.input_device_id || 'default', pendingSettings.device],
      [settings.max_speakers, pendingSettings.max_speakers],
      [settings.llm_router_url, pendingSettings.llm_router_url],
      [settings.llm_api_key, pendingSettings.llm_api_key],
      [settings.llm_client_id, pendingSettings.llm_client_id],
      [settings.soap_model, pendingSettings.soap_model],
      [settings.fast_model, pendingSettings.fast_model],
      [settings.medplum_server_url, pendingSettings.medplum_server_url],
      [settings.medplum_client_id, pendingSettings.medplum_client_id],
      [settings.medplum_auto_sync, pendingSettings.medplum_auto_sync],
      [settings.whisper_server_url, pendingSettings.whisper_server_url],
      [settings.whisper_server_model, pendingSettings.whisper_server_model],
      [settings.auto_start_enabled, pendingSettings.auto_start_enabled],
      [settings.auto_start_require_enrolled, pendingSettings.auto_start_require_enrolled],
      [settings.auto_start_required_role, pendingSettings.auto_start_required_role],
      [settings.auto_end_enabled, pendingSettings.auto_end_enabled],
      [settings.miis_enabled, pendingSettings.miis_enabled],
      [settings.miis_server_url, pendingSettings.miis_server_url],
      [settings.screen_capture_enabled, pendingSettings.screen_capture_enabled],
      [settings.screen_capture_interval_secs, pendingSettings.screen_capture_interval_secs],
      [settings.charting_mode, pendingSettings.charting_mode],
      [settings.encounter_check_interval_secs, pendingSettings.encounter_check_interval_secs],
      [settings.encounter_silence_trigger_secs, pendingSettings.encounter_silence_trigger_secs],
      [settings.encounter_detection_mode, pendingSettings.encounter_detection_mode],
      [settings.presence_sensor_port, pendingSettings.presence_sensor_port],
      [settings.presence_absence_threshold_secs, pendingSettings.presence_absence_threshold_secs],
      [settings.presence_debounce_secs, pendingSettings.presence_debounce_secs],
      [settings.presence_csv_log_enabled, pendingSettings.presence_csv_log_enabled],
      [settings.shadow_active_method, pendingSettings.shadow_active_method],
      [settings.shadow_csv_log_enabled, pendingSettings.shadow_csv_log_enabled],
      [settings.native_stt_shadow_enabled, pendingSettings.native_stt_shadow_enabled],
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
