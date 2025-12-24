/**
 * IPC Contract Tests
 *
 * These tests verify that the TypeScript types match the expected
 * shapes from the Rust backend. This catches type mismatches between
 * frontend and backend early.
 */

import { describe, it, expect } from 'vitest';

// Types that must match the Rust backend
interface SessionStatus {
  state: 'idle' | 'preparing' | 'recording' | 'stopping' | 'completed' | 'error';
  provider: 'whisper' | 'apple' | null;
  elapsed_ms: number;
  is_processing_behind: boolean;
  error_message?: string;
}

interface TranscriptUpdate {
  finalized_text: string;
  draft_text: string | null;
  segment_count: number;
}

interface Device {
  id: string;
  name: string;
  is_default: boolean;
}

interface ModelStatus {
  available: boolean;
  path: string | null;
  error: string | null;
}

interface Settings {
  whisper_model: string;
  language: string;
  input_device_id: string | null;
  output_format: string;
  vad_threshold: number;
  silence_to_flush_ms: number;
  max_utterance_ms: number;
}

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

function isSettings(obj: unknown): obj is Settings {
  if (typeof obj !== 'object' || obj === null) return false;
  const o = obj as Record<string, unknown>;

  if (typeof o.whisper_model !== 'string') return false;
  if (typeof o.language !== 'string') return false;
  if (o.input_device_id !== null && typeof o.input_device_id !== 'string') return false;
  if (typeof o.output_format !== 'string') return false;
  if (typeof o.vad_threshold !== 'number') return false;
  if (typeof o.silence_to_flush_ms !== 'number') return false;
  if (typeof o.max_utterance_ms !== 'number') return false;

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
    it('validates complete settings', () => {
      const settings = {
        whisper_model: 'small',
        language: 'en',
        input_device_id: null,
        output_format: 'paragraphs',
        vad_threshold: 0.5,
        silence_to_flush_ms: 500,
        max_utterance_ms: 25000,
      };
      expect(isSettings(settings)).toBe(true);
    });

    it('validates settings with device id', () => {
      const settings = {
        whisper_model: 'medium',
        language: 'fr',
        input_device_id: 'device-123',
        output_format: 'sentences',
        vad_threshold: 0.6,
        silence_to_flush_ms: 600,
        max_utterance_ms: 30000,
      };
      expect(isSettings(settings)).toBe(true);
    });

    it('rejects wrong number types', () => {
      const settings = {
        whisper_model: 'small',
        language: 'en',
        input_device_id: null,
        output_format: 'paragraphs',
        vad_threshold: '0.5', // should be number
        silence_to_flush_ms: 500,
        max_utterance_ms: 25000,
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

    it('get_settings returns Settings', () => {
      const mockResponse: Settings = {
        whisper_model: 'small',
        language: 'en',
        input_device_id: null,
        output_format: 'paragraphs',
        vad_threshold: 0.5,
        silence_to_flush_ms: 500,
        max_utterance_ms: 25000,
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
