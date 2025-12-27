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
