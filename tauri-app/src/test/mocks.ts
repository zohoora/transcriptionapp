import { vi } from 'vitest';
import type {
  Device,
  ModelStatus,
  SessionStatus,
  TranscriptUpdate,
  Settings,
  OllamaStatus,
  SoapNote,
  AudioQualitySnapshot,
} from '../types';

// Default mock data
export const mockDevices: Device[] = [
  { id: 'device-1', name: 'Built-in Microphone', is_default: true },
  { id: 'device-2', name: 'External USB Microphone', is_default: false },
];

export const mockModelStatusAvailable: ModelStatus = {
  available: true,
  path: '/Users/test/.cache/transcription-app/models/ggml-small.bin',
  error: null,
};

export const mockModelStatusUnavailable: ModelStatus = {
  available: false,
  path: '/Users/test/.cache/transcription-app/models/ggml-small.bin',
  error: 'Model file not found',
};

export const mockIdleStatus: SessionStatus = {
  state: 'idle',
  provider: null,
  elapsed_ms: 0,
  is_processing_behind: false,
};

export const mockRecordingStatus: SessionStatus = {
  state: 'recording',
  provider: 'whisper',
  elapsed_ms: 5000,
  is_processing_behind: false,
};

export const mockCompletedStatus: SessionStatus = {
  state: 'completed',
  provider: 'whisper',
  elapsed_ms: 10000,
  is_processing_behind: false,
};

export const mockTranscript: TranscriptUpdate = {
  finalized_text: 'Hello, this is a test transcription.',
  draft_text: null,
  segment_count: 1,
};

export const mockSettings: Settings = {
  whisper_model: 'small',
  language: 'en',
  input_device_id: null,
  output_format: 'text',
  vad_threshold: 0.5,
  silence_to_flush_ms: 1000,
  max_utterance_ms: 30000,
  diarization_enabled: true,
  max_speakers: 4,
  // LLM Router settings
  llm_router_url: 'http://localhost:8080',
  llm_api_key: 'test-api-key',
  llm_client_id: 'clinic-001',
  soap_model: 'gpt-4',
  fast_model: 'gpt-3.5-turbo',
  // Medplum settings
  medplum_server_url: 'http://localhost:8103',
  medplum_client_id: 'test-client-id',
  medplum_auto_sync: false,
  // Whisper server settings
  whisper_mode: 'remote',
  whisper_server_url: 'http://localhost:8001',
  whisper_server_model: 'large-v3-turbo',
  // SOAP note preferences
  soap_detail_level: 5,
  soap_format: 'problem_based',
  soap_custom_instructions: '',
  // Auto-session detection
  auto_start_enabled: false,
  greeting_sensitivity: 0.7,
  min_speech_duration_ms: 2000,
  // Speaker verification for auto-start
  auto_start_require_enrolled: false,
  auto_start_required_role: null,
  // Auto-end settings
  auto_end_enabled: false,
  auto_end_silence_ms: 180000,
  // Debug storage
  debug_storage_enabled: false,
  // MIIS
  miis_enabled: false,
  miis_server_url: 'http://localhost:7843',
  // Screen capture
  screen_capture_enabled: false,
  screen_capture_interval_secs: 60,
  // STT Router settings
  stt_alias: 'medical-streaming',
  stt_postprocess: true,
  // Continuous charting mode
  charting_mode: 'session',
  continuous_auto_copy_soap: false,
  encounter_check_interval_secs: 120,
  encounter_silence_trigger_secs: 60,
  encounter_merge_enabled: true,
  encounter_detection_model: 'faster',
  encounter_detection_nothink: true,
};

// LLM Router / Ollama types
export const mockOllamaStatusConnected: OllamaStatus = {
  connected: true,
  available_models: ['qwen3:4b', 'llama3:8b', 'mistral:7b'],
  error: null,
};

export const mockOllamaStatusDisconnected: OllamaStatus = {
  connected: false,
  available_models: [],
  error: 'Connection refused',
};

export const mockSoapNote: SoapNote = {
  content: 'S: Patient reports persistent cough for 3 days, accompanied by mild fever and fatigue.\n\nO: Temperature 38.2C, respiratory rate normal, lungs clear on auscultation.\n\nA: Likely viral upper respiratory infection.\n\nP: Rest and hydration, OTC fever reducer as needed, follow up if symptoms worsen or persist beyond 7 days.',
  generated_at: '2025-01-15T14:32:00Z',
  model_used: 'qwen3:4b',
};

// Good audio quality - no issues
export const mockAudioQualityGood: AudioQualitySnapshot = {
  timestamp_ms: 1000,
  peak_db: -6,
  rms_db: -18,
  clipped_samples: 0,
  clipped_ratio: 0,
  noise_floor_db: -45,
  snr_db: 25,
  silence_ratio: 0.2,
  dropout_count: 0,
  total_clipped: 0,
  total_samples: 16000,
};

// Too quiet audio
export const mockAudioQualityQuiet: AudioQualitySnapshot = {
  timestamp_ms: 1000,
  peak_db: -35,
  rms_db: -45,
  clipped_samples: 0,
  clipped_ratio: 0,
  noise_floor_db: -50,
  snr_db: 15,
  silence_ratio: 0.5,
  dropout_count: 0,
  total_clipped: 0,
  total_samples: 16000,
};

// Clipped audio (clipped_ratio >= 0.01 for 'poor' quality)
export const mockAudioQualityClipped: AudioQualitySnapshot = {
  timestamp_ms: 1000,
  peak_db: 0,
  rms_db: -6,
  clipped_samples: 50,
  clipped_ratio: 0.015,
  noise_floor_db: -40,
  snr_db: 20,
  silence_ratio: 0.1,
  dropout_count: 0,
  total_clipped: 50,
  total_samples: 16000,
};

// Low SNR (noisy) audio
export const mockAudioQualityNoisy: AudioQualitySnapshot = {
  timestamp_ms: 1000,
  peak_db: -10,
  rms_db: -20,
  clipped_samples: 0,
  clipped_ratio: 0,
  noise_floor_db: -25,
  snr_db: 5,
  silence_ratio: 0.3,
  dropout_count: 0,
  total_clipped: 0,
  total_samples: 16000,
};

// Audio with dropouts
export const mockAudioQualityDropout: AudioQualitySnapshot = {
  timestamp_ms: 1000,
  peak_db: -10,
  rms_db: -18,
  clipped_samples: 0,
  clipped_ratio: 0,
  noise_floor_db: -40,
  snr_db: 20,
  silence_ratio: 0.2,
  dropout_count: 3,
  total_clipped: 0,
  total_samples: 16000,
};

// Helper to create invoke mock that responds to different commands
export function createInvokeMock(responses: Record<string, unknown>) {
  return vi.fn((command: string, _args?: unknown) => {
    if (command in responses) {
      return Promise.resolve(responses[command]);
    }
    return Promise.reject(new Error(`Unknown command: ${command}`));
  });
}

// Helper to create listen mock with event callbacks
type EventCallback = (event: { payload: unknown }) => void;
type EventListeners = Map<string, EventCallback[]>;

export function createListenMock() {
  const listeners: EventListeners = new Map();

  const listenFn = vi.fn((eventName: string, callback: EventCallback) => {
    if (!listeners.has(eventName)) {
      listeners.set(eventName, []);
    }
    listeners.get(eventName)!.push(callback);

    // Return unlisten function
    return Promise.resolve(() => {
      const callbacks = listeners.get(eventName);
      if (callbacks) {
        const index = callbacks.indexOf(callback);
        if (index > -1) {
          callbacks.splice(index, 1);
        }
      }
    });
  });

  // Helper to emit events for testing
  const emit = (eventName: string, payload: unknown) => {
    const callbacks = listeners.get(eventName);
    if (callbacks) {
      callbacks.forEach(cb => cb({ payload }));
    }
  };

  return { listen: listenFn, emit };
}
