import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Settings, SpeakerRole, ChartingMode, EncounterDetectionMode, InfrastructureOverlay, RoomOverlay, SoapFormat } from '../types';

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
  sensor_connection_type: 'wifi' | 'usb' | 'none';
  presence_sensor_port: string;
  presence_sensor_url: string;
  presence_absence_threshold_secs: number;
  presence_debounce_secs: number;
  hybrid_confirm_window_secs: number;
  hybrid_min_words_for_sensor_split: number;
  thermal_hot_pixel_threshold_c: number;
  co2_baseline_ppm: number;
  presence_csv_log_enabled: boolean;
  // Encounter merge
  encounter_merge_enabled: boolean;
  // SOAP preferences
  soap_detail_level: number;
  soap_format: SoapFormat;
  soap_custom_instructions: string;
}

/**
 * Merge pending UI settings into a full Settings object for backend persistence.
 * Derives computed fields (diarization_enabled, whisper_mode, encounter_detection_mode, etc.).
 */
export function buildMergedSettings(settings: Settings, pending: PendingSettings): Settings {
  return {
    ...settings,
    language: pending.language,
    input_device_id: pending.device === 'default' ? null : pending.device,
    diarization_enabled: true,
    llm_router_url: pending.llm_router_url,
    llm_api_key: pending.llm_api_key,
    llm_client_id: pending.llm_client_id,
    soap_model: pending.soap_model,
    fast_model: pending.fast_model,
    medplum_server_url: pending.medplum_server_url,
    medplum_client_id: pending.medplum_client_id,
    medplum_auto_sync: pending.medplum_auto_sync,
    whisper_mode: 'remote',
    whisper_server_url: pending.whisper_server_url,
    auto_start_enabled: pending.auto_start_enabled,
    auto_start_require_enrolled: pending.auto_start_require_enrolled,
    auto_start_required_role: pending.auto_start_required_role,
    auto_end_enabled: pending.auto_end_enabled,
    miis_enabled: pending.image_source === 'miis',
    image_source: pending.image_source,
    gemini_api_key: pending.gemini_api_key,
    screen_capture_enabled: pending.screen_capture_enabled,
    charting_mode: pending.charting_mode,
    encounter_merge_enabled: pending.encounter_merge_enabled,
    encounter_detection_mode: pending.sensor_connection_type !== 'none' ? 'hybrid' : 'llm',
    presence_sensor_port: pending.sensor_connection_type === 'usb' ? pending.presence_sensor_port : '',
    presence_sensor_url: pending.sensor_connection_type === 'wifi' ? pending.presence_sensor_url : '',
    presence_absence_threshold_secs: pending.presence_absence_threshold_secs,
    presence_debounce_secs: pending.presence_debounce_secs,
    hybrid_confirm_window_secs: pending.hybrid_confirm_window_secs,
    hybrid_min_words_for_sensor_split: pending.hybrid_min_words_for_sensor_split,
    thermal_hot_pixel_threshold_c: pending.thermal_hot_pixel_threshold_c,
    co2_baseline_ppm: pending.co2_baseline_ppm,
    presence_csv_log_enabled: pending.presence_csv_log_enabled,
    soap_detail_level: pending.soap_detail_level,
    soap_format: pending.soap_format,
    soap_custom_instructions: pending.soap_custom_instructions,
  };
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
    sensor_connection_type: s.presence_sensor_url ? 'wifi' : s.presence_sensor_port ? 'usb' : 'none',
    presence_sensor_port: s.presence_sensor_port,
    presence_sensor_url: s.presence_sensor_url,
    presence_absence_threshold_secs: s.presence_absence_threshold_secs,
    presence_debounce_secs: s.presence_debounce_secs,
    hybrid_confirm_window_secs: s.hybrid_confirm_window_secs,
    hybrid_min_words_for_sensor_split: s.hybrid_min_words_for_sensor_split,
    thermal_hot_pixel_threshold_c: s.thermal_hot_pixel_threshold_c,
    co2_baseline_ppm: s.co2_baseline_ppm,
    presence_csv_log_enabled: s.presence_csv_log_enabled,
    encounter_merge_enabled: s.encounter_merge_enabled,
    soap_detail_level: s.soap_detail_level,
    soap_format: s.soap_format,
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
      const newSettings = buildMergedSettings(settings, pendingSettings);

      await invoke('set_settings', { settings: newSettings });
      setSettings(newSettings);

      // Best-effort: sync physician-tier settings to profile server
      // Only sync if physician-tier fields actually changed
      const physicianTierChanged =
        settings.language !== pendingSettings.language ||
        settings.image_source !== pendingSettings.image_source ||
        settings.gemini_api_key !== pendingSettings.gemini_api_key ||
        settings.auto_start_enabled !== pendingSettings.auto_start_enabled ||
        settings.auto_start_require_enrolled !== pendingSettings.auto_start_require_enrolled ||
        settings.auto_start_required_role !== pendingSettings.auto_start_required_role ||
        settings.auto_end_enabled !== pendingSettings.auto_end_enabled ||
        settings.charting_mode !== pendingSettings.charting_mode ||
        settings.encounter_merge_enabled !== pendingSettings.encounter_merge_enabled ||
        settings.soap_custom_instructions !== pendingSettings.soap_custom_instructions ||
        settings.soap_detail_level !== pendingSettings.soap_detail_level ||
        settings.soap_format !== pendingSettings.soap_format;

      if (physicianTierChanged) {
        try {
          const activePhysician = await invoke<{ id: string } | null>('get_active_physician');
          if (activePhysician) {
            await invoke('update_physician', {
              physicianId: activePhysician.id,
              updates: {
                language: pendingSettings.language,
                image_source: pendingSettings.image_source,
                gemini_api_key: pendingSettings.gemini_api_key,
                auto_start_enabled: pendingSettings.auto_start_enabled,
                auto_start_require_enrolled: pendingSettings.auto_start_require_enrolled,
                auto_start_required_role: pendingSettings.auto_start_required_role,
                auto_end_enabled: pendingSettings.auto_end_enabled,
                charting_mode: pendingSettings.charting_mode,
                encounter_merge_enabled: pendingSettings.encounter_merge_enabled,
                soap_detail_level: pendingSettings.soap_detail_level,
                soap_format: pendingSettings.soap_format,
                soap_custom_instructions: pendingSettings.soap_custom_instructions,
              },
            });
          }
        } catch (e) {
          console.warn('Failed to sync physician settings to server:', e);
        }
      }

      // Best-effort: sync infra + room tier changes to server (parallel, non-blocking)
      const syncPromises: Promise<unknown>[] = [];

      // Infrastructure-tier: compare all PendingSettings fields that are infrastructure-tier
      const infraTierChanged =
        settings.llm_router_url !== pendingSettings.llm_router_url ||
        settings.llm_api_key !== pendingSettings.llm_api_key ||
        settings.llm_client_id !== pendingSettings.llm_client_id ||
        settings.soap_model !== pendingSettings.soap_model ||
        settings.fast_model !== pendingSettings.fast_model ||
        settings.whisper_server_url !== pendingSettings.whisper_server_url ||
        settings.medplum_server_url !== pendingSettings.medplum_server_url ||
        settings.medplum_client_id !== pendingSettings.medplum_client_id;

      if (infraTierChanged) {
        const infraSettings: InfrastructureOverlay = {
          llm_router_url: pendingSettings.llm_router_url || undefined,
          llm_api_key: pendingSettings.llm_api_key || undefined,
          llm_client_id: pendingSettings.llm_client_id || undefined,
          soap_model: pendingSettings.soap_model || undefined,
          fast_model: pendingSettings.fast_model || undefined,
          whisper_server_url: pendingSettings.whisper_server_url || undefined,
          medplum_server_url: pendingSettings.medplum_server_url || undefined,
          medplum_client_id: pendingSettings.medplum_client_id || undefined,
        };
        syncPromises.push(
          invoke('sync_infrastructure_settings', { settings: infraSettings })
            .catch(e => console.warn('Failed to sync infrastructure settings to server:', e))
        );
      }

      // Room-tier: compare all PendingSettings fields that are room-tier
      const effectiveUrl = pendingSettings.sensor_connection_type === 'wifi' ? pendingSettings.presence_sensor_url : '';
      const effectivePort = pendingSettings.sensor_connection_type === 'usb' ? pendingSettings.presence_sensor_port : '';
      const roomTierChanged =
        settings.encounter_detection_mode !== pendingSettings.encounter_detection_mode ||
        settings.presence_sensor_url !== effectiveUrl ||
        settings.presence_sensor_port !== effectivePort ||
        settings.presence_absence_threshold_secs !== pendingSettings.presence_absence_threshold_secs ||
        settings.presence_debounce_secs !== pendingSettings.presence_debounce_secs ||
        settings.hybrid_confirm_window_secs !== pendingSettings.hybrid_confirm_window_secs ||
        settings.hybrid_min_words_for_sensor_split !== pendingSettings.hybrid_min_words_for_sensor_split ||
        settings.thermal_hot_pixel_threshold_c !== pendingSettings.thermal_hot_pixel_threshold_c ||
        settings.co2_baseline_ppm !== pendingSettings.co2_baseline_ppm ||
        settings.presence_csv_log_enabled !== pendingSettings.presence_csv_log_enabled ||
        settings.screen_capture_enabled !== pendingSettings.screen_capture_enabled;

      if (roomTierChanged) {
        const roomSettings: RoomOverlay = {
          encounter_detection_mode: pendingSettings.encounter_detection_mode,
          presence_sensor_url: effectiveUrl || undefined,
          presence_sensor_port: effectivePort || undefined,
          presence_absence_threshold_secs: pendingSettings.presence_absence_threshold_secs,
          presence_debounce_secs: pendingSettings.presence_debounce_secs,
          hybrid_confirm_window_secs: pendingSettings.hybrid_confirm_window_secs,
          hybrid_min_words_for_sensor_split: pendingSettings.hybrid_min_words_for_sensor_split,
          thermal_hot_pixel_threshold_c: pendingSettings.thermal_hot_pixel_threshold_c,
          co2_baseline_ppm: pendingSettings.co2_baseline_ppm,
          presence_csv_log_enabled: pendingSettings.presence_csv_log_enabled,
          screen_capture_enabled: pendingSettings.screen_capture_enabled,
        };
        syncPromises.push(
          invoke('sync_room_settings', { settings: roomSettings })
            .catch(e => console.warn('Failed to sync room settings to server:', e))
        );
      }

      // Fire all tier syncs in parallel (best-effort, don't block save result)
      if (syncPromises.length > 0) {
        Promise.allSettled(syncPromises);
      }

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
      [settings.encounter_detection_mode, pendingSettings.sensor_connection_type !== 'none' ? 'hybrid' : 'llm'],
      [settings.presence_sensor_port, pendingSettings.sensor_connection_type === 'usb' ? pendingSettings.presence_sensor_port : ''],
      [settings.presence_sensor_url, pendingSettings.sensor_connection_type === 'wifi' ? pendingSettings.presence_sensor_url : ''],
      [settings.presence_absence_threshold_secs, pendingSettings.presence_absence_threshold_secs],
      [settings.presence_debounce_secs, pendingSettings.presence_debounce_secs],
      [settings.hybrid_confirm_window_secs, pendingSettings.hybrid_confirm_window_secs],
      [settings.hybrid_min_words_for_sensor_split, pendingSettings.hybrid_min_words_for_sensor_split],
      [settings.thermal_hot_pixel_threshold_c, pendingSettings.thermal_hot_pixel_threshold_c],
      [settings.co2_baseline_ppm, pendingSettings.co2_baseline_ppm],
      [settings.presence_csv_log_enabled, pendingSettings.presence_csv_log_enabled],
      [settings.soap_detail_level, pendingSettings.soap_detail_level],
      [settings.soap_format, pendingSettings.soap_format],
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
