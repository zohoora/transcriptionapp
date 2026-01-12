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
  // LLM Router settings (new)
  llm_router_url: 'http://localhost:8080',
  llm_api_key: 'test-api-key',
  llm_client_id: 'clinic-001',
  soap_model: 'gpt-4',
  fast_model: 'gpt-3.5-turbo',
  // Medplum settings
  medplum_server_url: 'http://localhost:8103',
  medplum_client_id: 'test-client',
  medplum_auto_sync: false,
  // Whisper server settings
  whisper_mode: 'remote' as const,
  whisper_server_url: 'http://localhost:8001',
  whisper_server_model: 'large-v3-turbo',
  // SOAP options
  soap_detail_level: 5,
  soap_format: 'problem_based' as const,
  soap_custom_instructions: '',
  // Auto-session detection
  auto_start_enabled: false,
  greeting_sensitivity: 0.7,
  min_speech_duration_ms: 2000,
};

describe('useOllamaConnection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
  });

  it('checks connection on mount', async () => {
    const mockStatus = {
      connected: true,
      available_models: ['gpt-4', 'gpt-3.5-turbo'],
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
      available_models: ['gpt-4'],
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
          available_models: ['gpt-4'],
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
        'http://newrouter:8080',
        'new-api-key',
        'clinic-002',
        'claude-3-opus',
        'claude-3-haiku',
        mockSettings
      );
    });

    expect(testResult!).toBe(true);
    expect(mockInvoke).toHaveBeenCalledWith('set_settings', {
      settings: expect.objectContaining({
        llm_router_url: 'http://newrouter:8080',
        llm_api_key: 'new-api-key',
        llm_client_id: 'clinic-002',
        soap_model: 'claude-3-opus',
        fast_model: 'claude-3-haiku',
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
        'http://badserver:8080',
        'bad-key',
        'clinic-003',
        'gpt-4',
        'gpt-3.5-turbo',
        mockSettings
      );
    });

    expect(testResult!).toBe(false);
    expect(result.current.error).toBe('Connection refused');
  });

  it('handles test connection exception', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'check_ollama_status') {
        return Promise.resolve({
          connected: true,
          available_models: ['gpt-4'],
          error: null,
        });
      }
      if (cmd === 'set_settings') {
        return Promise.reject(new Error('Failed to save'));
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
        'http://router:8080',
        'api-key',
        'clinic-001',
        'gpt-4',
        'gpt-3.5-turbo',
        mockSettings
      );
    });

    expect(testResult!).toBe(false);
    expect(result.current.error).toBe('Failed to save');
  });

  it('prewarms model', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'check_ollama_status') {
        return Promise.resolve({
          connected: true,
          available_models: ['gpt-4'],
          error: null,
        });
      }
      if (cmd === 'prewarm_ollama_model') {
        return Promise.resolve(undefined);
      }
      return Promise.resolve(undefined);
    });

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    // Wait for auto-prewarm to complete
    await waitFor(() => {
      expect(result.current.isPrewarming).toBe(false);
    });

    expect(mockInvoke).toHaveBeenCalledWith('prewarm_ollama_model');
  });

  it('handles prewarm failure gracefully', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'check_ollama_status') {
        return Promise.resolve({
          connected: true,
          available_models: ['gpt-4'],
          error: null,
        });
      }
      if (cmd === 'prewarm_ollama_model') {
        return Promise.reject(new Error('Prewarm failed'));
      }
      return Promise.resolve(undefined);
    });

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    // Wait for auto-prewarm to complete (should fail gracefully)
    await waitFor(() => {
      expect(result.current.isPrewarming).toBe(false);
    });

    // Should not set error state for prewarm failures
    expect(result.current.error).toBeNull();
  });

  it('prevents duplicate prewarm calls', async () => {
    let prewarmCount = 0;
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'check_ollama_status') {
        return Promise.resolve({
          connected: true,
          available_models: ['gpt-4'],
          error: null,
        });
      }
      if (cmd === 'prewarm_ollama_model') {
        prewarmCount++;
        return new Promise(resolve => setTimeout(resolve, 100));
      }
      return Promise.resolve(undefined);
    });

    const { result } = renderHook(() => useOllamaConnection());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    // Wait for auto-prewarm to complete
    await waitFor(() => {
      expect(result.current.isPrewarming).toBe(false);
    }, { timeout: 200 });

    // Try calling prewarm again manually
    await act(async () => {
      await result.current.prewarmModel();
    });

    // Should only have called prewarm once (auto-prewarm)
    expect(prewarmCount).toBe(1);
  });
});
