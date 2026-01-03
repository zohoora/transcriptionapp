import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useOllamaConnection } from './useOllamaConnection';
import { invoke } from '@tauri-apps/api/core';

// Type the mock from global setup
const mockInvoke = vi.mocked(invoke);

const mockSettings = {
  whisper_model: 'small',
  language: 'en',
  input_device_id: null,
  output_format: 'json',
  vad_threshold: 0.5,
  silence_to_flush_ms: 500,
  max_utterance_ms: 30000,
  diarization_enabled: true,
  max_speakers: 4,
  ollama_server_url: 'http://localhost:11434',
  ollama_model: 'qwen3:4b',
  medplum_server_url: 'http://localhost:8103',
  medplum_client_id: 'test-client',
  medplum_auto_sync: false,
};

describe('useOllamaConnection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
  });

  it('checks connection on mount', async () => {
    const mockStatus = {
      connected: true,
      available_models: ['qwen3:4b', 'llama3:8b'],
      error: null,
    };
    mockInvoke.mockResolvedValue(mockStatus);

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    expect(mockInvoke).toHaveBeenCalledWith('check_ollama_status');
    expect(result.current.status).toEqual(mockStatus);
    expect(result.current.error).toBeNull();
  });

  it('handles connection check error', async () => {
    mockInvoke.mockRejectedValue(new Error('Connection refused'));

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    expect(result.current.status?.connected).toBe(false);
    expect(result.current.error).toBe('Connection refused');
  });

  it('handles disconnected status with error message', async () => {
    const mockStatus = {
      connected: false,
      available_models: [],
      error: 'Server not running',
    };
    mockInvoke.mockResolvedValue(mockStatus);

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    expect(result.current.status?.connected).toBe(false);
    expect(result.current.error).toBe('Server not running');
  });

  it('refreshes connection status', async () => {
    mockInvoke.mockResolvedValue({
      connected: false,
      available_models: [],
      error: 'Not connected',
    });

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    expect(result.current.status?.connected).toBe(false);

    // Change mock for refresh
    mockInvoke.mockResolvedValue({
      connected: true,
      available_models: ['qwen3:4b'],
      error: null,
    });

    await act(async () => {
      await result.current.checkConnection();
    });

    expect(result.current.status?.connected).toBe(true);
    expect(result.current.error).toBeNull();
  });

  it('tests connection with new settings', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'check_ollama_status') {
        return Promise.resolve({
          connected: true,
          available_models: ['qwen3:4b'],
          error: null,
        });
      }
      if (cmd === 'set_settings') {
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    let testResult: boolean;
    await act(async () => {
      testResult = await result.current.testConnection(
        'http://newserver:11434',
        'llama3:8b',
        mockSettings
      );
    });

    expect(testResult!).toBe(true);
    expect(mockInvoke).toHaveBeenCalledWith('set_settings', {
      settings: expect.objectContaining({
        ollama_server_url: 'http://newserver:11434',
        ollama_model: 'llama3:8b',
      }),
    });
  });

  it('handles test connection failure', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'check_ollama_status') {
        return Promise.resolve({
          connected: false,
          available_models: [],
          error: 'Connection refused',
        });
      }
      if (cmd === 'set_settings') {
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    let testResult: boolean;
    await act(async () => {
      testResult = await result.current.testConnection(
        'http://badserver:11434',
        'qwen3:4b',
        mockSettings
      );
    });

    expect(testResult!).toBe(false);
    expect(result.current.error).toBe('Connection refused');
  });

  it('handles test connection exception', async () => {
    mockInvoke.mockRejectedValue(new Error('Network error'));

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    mockInvoke.mockRejectedValue(new Error('Network error'));

    let testResult: boolean;
    await act(async () => {
      testResult = await result.current.testConnection(
        'http://server:11434',
        'model',
        mockSettings
      );
    });

    expect(testResult!).toBe(false);
    expect(result.current.error).toBe('Network error');
  });

  it('sets isChecking during check', async () => {
    let resolveCheck: (value: unknown) => void;
    const checkPromise = new Promise((resolve) => {
      resolveCheck = resolve;
    });

    mockInvoke.mockReturnValue(checkPromise);

    const { result } = renderHook(() => useOllamaConnection());

    // Should be checking on mount
    expect(result.current.isChecking).toBe(true);

    await act(async () => {
      resolveCheck!({
        connected: true,
        available_models: [],
        error: null,
      });
    });

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });
  });

  it('clears error on successful check', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('First check failed'));

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    expect(result.current.error).toBe('First check failed');

    mockInvoke.mockResolvedValue({
      connected: true,
      available_models: ['model1'],
      error: null,
    });

    await act(async () => {
      await result.current.checkConnection();
    });

    expect(result.current.error).toBeNull();
  });

  it('returns available models when connected', async () => {
    const models = ['qwen3:4b', 'llama3:8b', 'mistral:7b'];
    mockInvoke.mockResolvedValue({
      connected: true,
      available_models: models,
      error: null,
    });

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    expect(result.current.status?.available_models).toEqual(models);
  });
});
