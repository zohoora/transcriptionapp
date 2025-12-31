import { vi } from 'vitest';

export interface Device {
  id: string;
  name: string;
  is_default: boolean;
}

export interface ModelStatus {
  available: boolean;
  path: string | null;
  error: string | null;
}

export interface SessionStatus {
  state: 'idle' | 'preparing' | 'recording' | 'stopping' | 'completed' | 'error';
  provider: 'whisper' | 'apple' | null;
  elapsed_ms: number;
  is_processing_behind: boolean;
  error_message?: string;
}

export interface TranscriptUpdate {
  finalized_text: string;
  draft_text: string | null;
  segment_count: number;
}

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

export interface Settings {
  whisper_model: string;
  language: string;
  input_device_id: string | null;
  output_format: string;
  vad_threshold: number;
  silence_to_flush_ms: number;
  max_utterance_ms: number;
  diarization_enabled: boolean;
  max_speakers: number;
  ollama_server_url: string;
  ollama_model: string;
  medplum_server_url: string;
  medplum_client_id: string;
  medplum_auto_sync: boolean;
}

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
  ollama_server_url: 'http://localhost:11434',
  ollama_model: 'qwen3:4b',
  medplum_server_url: 'http://localhost:8103',
  medplum_client_id: 'test-client-id',
  medplum_auto_sync: false,
};

// Ollama types
export interface OllamaStatus {
  connected: boolean;
  available_models: string[];
  error: string | null;
}

export interface SoapNote {
  subjective: string;
  objective: string;
  assessment: string;
  plan: string;
  generated_at: string;
  model_used: string;
}

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
  subjective: 'Patient reports persistent cough for 3 days, accompanied by mild fever and fatigue.',
  objective: 'Temperature 38.2C, respiratory rate normal, lungs clear on auscultation.',
  assessment: 'Likely viral upper respiratory infection.',
  plan: 'Rest and hydration, OTC fever reducer as needed, follow up if symptoms worsen or persist beyond 7 days.',
  generated_at: '2025-01-15T14:32:00Z',
  model_used: 'qwen3:4b',
};

// Audio Quality types
export interface AudioQualitySnapshot {
  timestamp_ms: number;
  peak_db: number;
  rms_db: number;
  clipped_samples: number;
  clipped_ratio: number;
  noise_floor_db: number;
  snr_db: number;
  silence_ratio: number;
  dropout_count: number;
  total_clipped: number;
  total_samples: number;
}

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

// Clipped audio
export const mockAudioQualityClipped: AudioQualitySnapshot = {
  timestamp_ms: 1000,
  peak_db: 0,
  rms_db: -6,
  clipped_samples: 50,
  clipped_ratio: 0.003,
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
