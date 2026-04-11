/**
 * IPC Contract Tests
 *
 * These tests verify that the TypeScript types match the expected
 * shapes from the Rust backend. This catches type mismatches between
 * frontend and backend early.
 *
 * Types are imported from the canonical types file rather than being
 * redefined inline, ensuring these tests stay in sync with real usage.
 */

import { describe, it, expect } from 'vitest';
import type { Settings, SessionStatus, TranscriptUpdate, Device, ModelStatus } from './types';

// Type guard functions for runtime validation

function isSessionStatus(obj: unknown): obj is SessionStatus {
  if (typeof obj !== 'object' || obj === null) return false;
  const o = obj as Record<string, unknown>;

  const validStates = ['idle', 'preparing', 'recording', 'stopping', 'completed', 'error'];
  if (!validStates.includes(o.state as string)) return false;

  const validProviders = ['whisper', 'apple', null];
  if (!validProviders.includes(o.provider as string | null)) return false;

  if (typeof o.elapsed_ms !== 'number') return false;
  if (typeof o.is_processing_behind !== 'boolean') return false;

  if (o.error_message !== undefined && typeof o.error_message !== 'string') return false;

  return true;
}

function isTranscriptUpdate(obj: unknown): obj is TranscriptUpdate {
  if (typeof obj !== 'object' || obj === null) return false;
  const o = obj as Record<string, unknown>;

  if (typeof o.finalized_text !== 'string') return false;
  if (o.draft_text !== null && typeof o.draft_text !== 'string') return false;
  if (typeof o.segment_count !== 'number') return false;

  return true;
}

function isDevice(obj: unknown): obj is Device {
  if (typeof obj !== 'object' || obj === null) return false;
  const o = obj as Record<string, unknown>;

  if (typeof o.id !== 'string') return false;
  if (typeof o.name !== 'string') return false;
  if (typeof o.is_default !== 'boolean') return false;

  return true;
}

function isModelStatus(obj: unknown): obj is ModelStatus {
  if (typeof obj !== 'object' || obj === null) return false;
  const o = obj as Record<string, unknown>;

  if (typeof o.available !== 'boolean') return false;
  if (o.path !== null && typeof o.path !== 'string') return false;
  if (o.error !== null && typeof o.error !== 'string') return false;

  return true;
}

/**
 * Validates a representative sample of Settings fields across all categories.
 * Checks ~20 fields spanning audio, LLM, Medplum, auto-detection, SOAP,
 * continuous mode, presence sensor, and image settings.
 */
function isSettings(obj: unknown): obj is Settings {
  if (typeof obj !== 'object' || obj === null) return false;
  const o = obj as Record<string, unknown>;

  // Audio settings
  if (typeof o.whisper_model !== 'string') return false;
  if (o.input_device_id !== null && typeof o.input_device_id !== 'string') return false;
  if (typeof o.vad_threshold !== 'number') return false;
  if (typeof o.diarization_enabled !== 'boolean') return false;

  // LLM Router settings
  if (typeof o.llm_router_url !== 'string') return false;
  if (typeof o.soap_model !== 'string') return false;
  if (typeof o.fast_model !== 'string') return false;

  // Medplum EMR settings
  if (typeof o.medplum_server_url !== 'string') return false;
  if (typeof o.medplum_auto_sync !== 'boolean') return false;

  // SOAP preferences
  if (typeof o.soap_detail_level !== 'number') return false;
  if (typeof o.soap_custom_instructions !== 'string') return false;

  // Auto-session detection
  if (typeof o.auto_start_enabled !== 'boolean') return false;
  if (typeof o.auto_end_enabled !== 'boolean') return false;
  if (typeof o.auto_end_silence_ms !== 'number') return false;

  // STT Router
  if (typeof o.stt_alias !== 'string') return false;
  if (typeof o.stt_postprocess !== 'boolean') return false;

  // Continuous charting mode
  if (typeof o.encounter_check_interval_secs !== 'number') return false;
  if (typeof o.encounter_merge_enabled !== 'boolean') return false;

  // Presence sensor
  if (typeof o.presence_absence_threshold_secs !== 'number') return false;

  // Image generation
  if (typeof o.image_source !== 'string') return false;

  return true;
}

describe('IPC Contracts', () => {
  describe('SessionStatus', () => {
    it('validates correct idle status', () => {
      const status = {
        state: 'idle',
        provider: null,
        elapsed_ms: 0,
        is_processing_behind: false,
      };
      expect(isSessionStatus(status)).toBe(true);
    });

    it('validates correct recording status', () => {
      const status = {
        state: 'recording',
        provider: 'whisper',
        elapsed_ms: 5000,
        is_processing_behind: false,
      };
      expect(isSessionStatus(status)).toBe(true);
    });

    it('validates status with error message', () => {
      const status = {
        state: 'error',
        provider: null,
        elapsed_ms: 0,
        is_processing_behind: false,
        error_message: 'Microphone access denied',
      };
      expect(isSessionStatus(status)).toBe(true);
    });

    it('rejects invalid state', () => {
      const status = {
        state: 'invalid_state',
        provider: null,
        elapsed_ms: 0,
        is_processing_behind: false,
      };
      expect(isSessionStatus(status)).toBe(false);
    });

    it('rejects missing required fields', () => {
      const status = {
        state: 'idle',
        provider: null,
        // missing elapsed_ms and is_processing_behind
      };
      expect(isSessionStatus(status)).toBe(false);
    });

    it('rejects wrong types', () => {
      const status = {
        state: 'idle',
        provider: null,
        elapsed_ms: '5000', // should be number
        is_processing_behind: false,
      };
      expect(isSessionStatus(status)).toBe(false);
    });
  });

  describe('TranscriptUpdate', () => {
    it('validates correct transcript with text', () => {
      const transcript = {
        finalized_text: 'Hello world',
        draft_text: null,
        segment_count: 1,
      };
      expect(isTranscriptUpdate(transcript)).toBe(true);
    });

    it('validates transcript with draft', () => {
      const transcript = {
        finalized_text: 'Hello',
        draft_text: 'world',
        segment_count: 2,
      };
      expect(isTranscriptUpdate(transcript)).toBe(true);
    });

    it('validates empty transcript', () => {
      const transcript = {
        finalized_text: '',
        draft_text: null,
        segment_count: 0,
      };
      expect(isTranscriptUpdate(transcript)).toBe(true);
    });

    it('rejects missing fields', () => {
      const transcript = {
        finalized_text: 'Hello',
        // missing draft_text and segment_count
      };
      expect(isTranscriptUpdate(transcript)).toBe(false);
    });
  });

  describe('Device', () => {
    it('validates correct device', () => {
      const device = {
        id: 'device-1',
        name: 'Built-in Microphone',
        is_default: true,
      };
      expect(isDevice(device)).toBe(true);
    });

    it('validates non-default device', () => {
      const device = {
        id: 'usb-mic',
        name: 'USB Microphone',
        is_default: false,
      };
      expect(isDevice(device)).toBe(true);
    });

    it('rejects wrong types', () => {
      const device = {
        id: 123, // should be string
        name: 'Microphone',
        is_default: true,
      };
      expect(isDevice(device)).toBe(false);
    });
  });

  describe('ModelStatus', () => {
    it('validates available model', () => {
      const status = {
        available: true,
        path: '/path/to/model.bin',
        error: null,
      };
      expect(isModelStatus(status)).toBe(true);
    });

    it('validates unavailable model with error', () => {
      const status = {
        available: false,
        path: '/path/to/model.bin',
        error: 'Model file not found',
      };
      expect(isModelStatus(status)).toBe(true);
    });

    it('validates model with null path', () => {
      const status = {
        available: false,
        path: null,
        error: 'Could not determine model path',
      };
      expect(isModelStatus(status)).toBe(true);
    });
  });

  describe('Settings', () => {
    it('validates complete settings object from mock', () => {
      // Use the canonical mock settings which must satisfy the Settings type
      const settings: Settings = {
        whisper_model: 'small',
        input_device_id: null,
        output_format: 'paragraphs',
        vad_threshold: 0.5,
        silence_to_flush_ms: 500,
        max_utterance_ms: 25000,
        diarization_enabled: true,
        max_speakers: 4,
        llm_router_url: 'http://localhost:8080',
        llm_api_key: 'test-key',
        llm_client_id: 'clinic-001',
        soap_model: 'soap-model-fast',
        soap_model_fast: 'soap-model-fast',
        fast_model: 'fast-model',
        medplum_server_url: 'http://localhost:8103',
        medplum_client_id: 'test-client',
        medplum_auto_sync: false,
        whisper_mode: 'remote',
        whisper_server_url: 'http://localhost:8001',
        whisper_server_model: 'large-v3-turbo',
        soap_detail_level: 5,
        soap_format: 'problem_based',
        soap_custom_instructions: '',
        auto_start_enabled: false,
        greeting_sensitivity: 0.7,
        min_speech_duration_ms: 2000,
        auto_start_require_enrolled: false,
        auto_start_required_role: null,
        auto_end_enabled: false,
        auto_end_silence_ms: 180000,
        debug_storage_enabled: false,
        miis_enabled: false,
        miis_server_url: 'http://localhost:7843',
        image_source: 'ai',
        gemini_api_key: '',
        screen_capture_enabled: false,
        screen_capture_interval_secs: 60,
        stt_alias: 'medical-streaming',
        stt_postprocess: true,
        charting_mode: 'session',
        continuous_auto_copy_soap: false,
        encounter_check_interval_secs: 120,
        encounter_silence_trigger_secs: 60,
        encounter_merge_enabled: true,
        encounter_detection_model: 'fast-model',
        encounter_detection_nothink: false,
        encounter_detection_mode: 'hybrid',
        presence_sensor_port: '',
        presence_sensor_url: '',
        presence_absence_threshold_secs: 180,
        presence_debounce_secs: 15,
        presence_csv_log_enabled: true,
        shadow_active_method: 'sensor',
        shadow_csv_log_enabled: true,
        hybrid_confirm_window_secs: 180,
        hybrid_min_words_for_sensor_split: 500,
        thermal_hot_pixel_threshold_c: 28.0,
        co2_baseline_ppm: 420.0,
      };
      expect(isSettings(settings)).toBe(true);
    });

    it('validates settings with device id', () => {
      const partial = {
        whisper_model: 'medium',
        input_device_id: 'device-123',
        vad_threshold: 0.6,
        diarization_enabled: false,
        llm_router_url: 'http://localhost:8080',
        soap_model: 'gpt-4',
        fast_model: 'gpt-3.5-turbo',
        medplum_server_url: 'http://localhost:8103',
        medplum_auto_sync: true,
        soap_detail_level: 7,
        soap_custom_instructions: 'Focus on cardiovascular',
        auto_start_enabled: true,
        auto_end_enabled: true,
        auto_end_silence_ms: 120000,
        stt_alias: 'medical-streaming',
        stt_postprocess: true,
        encounter_check_interval_secs: 90,
        encounter_merge_enabled: false,
        presence_absence_threshold_secs: 120,
        image_source: 'off',
      };
      expect(isSettings(partial)).toBe(true);
    });

    it('rejects wrong number types', () => {
      const settings = {
        whisper_model: 'small',
        input_device_id: null,
        vad_threshold: '0.5', // should be number
        diarization_enabled: true,
        llm_router_url: 'http://localhost:8080',
        soap_model: 'gpt-4',
        fast_model: 'fast',
        medplum_server_url: 'http://localhost:8103',
        medplum_auto_sync: false,
        soap_detail_level: 5,
        soap_custom_instructions: '',
        auto_start_enabled: false,
        auto_end_enabled: false,
        auto_end_silence_ms: 180000,
        stt_alias: 'medical-streaming',
        stt_postprocess: true,
        encounter_check_interval_secs: 120,
        encounter_merge_enabled: true,
        presence_absence_threshold_secs: 180,
        image_source: 'ai',
      };
      expect(isSettings(settings)).toBe(false);
    });

    it('rejects missing required boolean fields', () => {
      const settings = {
        whisper_model: 'small',
        input_device_id: null,
        vad_threshold: 0.5,
        // missing diarization_enabled
        llm_router_url: 'http://localhost:8080',
        soap_model: 'gpt-4',
        fast_model: 'fast',
        medplum_server_url: 'http://localhost:8103',
        medplum_auto_sync: false,
        soap_detail_level: 5,
        soap_custom_instructions: '',
        auto_start_enabled: false,
        auto_end_enabled: false,
        auto_end_silence_ms: 180000,
        stt_alias: 'medical-streaming',
        stt_postprocess: true,
        encounter_check_interval_secs: 120,
        encounter_merge_enabled: true,
        presence_absence_threshold_secs: 180,
        image_source: 'ai',
      };
      expect(isSettings(settings)).toBe(false);
    });
  });

  describe('Command contracts', () => {
    it('list_input_devices returns Device[]', () => {
      const mockResponse: Device[] = [
        { id: 'mic-1', name: 'Microphone 1', is_default: true },
        { id: 'mic-2', name: 'Microphone 2', is_default: false },
      ];

      expect(mockResponse.every(isDevice)).toBe(true);
    });

    it('check_model_status returns ModelStatus', () => {
      const mockResponse: ModelStatus = {
        available: true,
        path: '/path/to/model',
        error: null,
      };

      expect(isModelStatus(mockResponse)).toBe(true);
    });

    it('get_settings returns Settings with all categories', () => {
      // Verify that a full Settings object passes the type guard
      const mockResponse: Settings = {
        whisper_model: 'small',
        input_device_id: null,
        output_format: 'paragraphs',
        vad_threshold: 0.5,
        silence_to_flush_ms: 500,
        max_utterance_ms: 25000,
        diarization_enabled: true,
        max_speakers: 4,
        llm_router_url: 'http://localhost:8080',
        llm_api_key: '',
        llm_client_id: '',
        soap_model: 'soap-model-fast',
        soap_model_fast: 'soap-model-fast',
        fast_model: 'fast-model',
        medplum_server_url: 'http://localhost:8103',
        medplum_client_id: '',
        medplum_auto_sync: false,
        whisper_mode: 'remote',
        whisper_server_url: 'http://localhost:8001',
        whisper_server_model: 'large-v3-turbo',
        soap_detail_level: 5,
        soap_format: 'problem_based',
        soap_custom_instructions: '',
        auto_start_enabled: false,
        greeting_sensitivity: null,
        min_speech_duration_ms: null,
        auto_start_require_enrolled: false,
        auto_start_required_role: null,
        auto_end_enabled: false,
        auto_end_silence_ms: 180000,
        debug_storage_enabled: false,
        miis_enabled: false,
        miis_server_url: '',
        image_source: 'ai',
        gemini_api_key: '',
        screen_capture_enabled: false,
        screen_capture_interval_secs: 30,
        stt_alias: 'medical-streaming',
        stt_postprocess: true,
        charting_mode: 'session',
        continuous_auto_copy_soap: false,
        encounter_check_interval_secs: 120,
        encounter_silence_trigger_secs: 45,
        encounter_merge_enabled: true,
        encounter_detection_model: 'fast-model',
        encounter_detection_nothink: false,
        encounter_detection_mode: 'hybrid',
        presence_sensor_port: '',
        presence_sensor_url: '',
        presence_absence_threshold_secs: 180,
        presence_debounce_secs: 15,
        presence_csv_log_enabled: true,
        shadow_active_method: 'sensor',
        shadow_csv_log_enabled: true,
        hybrid_confirm_window_secs: 180,
        hybrid_min_words_for_sensor_split: 500,
        thermal_hot_pixel_threshold_c: 28.0,
        co2_baseline_ppm: 420.0,
      };

      expect(isSettings(mockResponse)).toBe(true);
    });
  });

  describe('Event contracts', () => {
    it('session_status event payload is SessionStatus', () => {
      const payload: SessionStatus = {
        state: 'recording',
        provider: 'whisper',
        elapsed_ms: 10000,
        is_processing_behind: false,
      };

      expect(isSessionStatus(payload)).toBe(true);
    });

    it('transcript_update event payload is TranscriptUpdate', () => {
      const payload: TranscriptUpdate = {
        finalized_text: 'Test transcript',
        draft_text: 'Still speaking...',
        segment_count: 2,
      };

      expect(isTranscriptUpdate(payload)).toBe(true);
    });
  });
});
