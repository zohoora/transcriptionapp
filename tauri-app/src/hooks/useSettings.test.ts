import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useSettings } from './useSettings';
import { invoke } from '@tauri-apps/api/core';

// Type the mock from global setup
const mockInvoke = vi.mocked(invoke);

const mockSettings = {
  whisper_model: 'small',
  language: 'en',
  input_device_id: 'device-1',
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

describe('useSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(mockSettings);
  });

  it('loads settings on mount', async () => {
    const { result } = renderHook(() => useSettings());

    expect(result.current.isLoading).toBe(true);

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(mockInvoke).toHaveBeenCalledWith('get_settings');
    expect(result.current.settings).toEqual(mockSettings);
    expect(result.current.pendingSettings).toEqual({
      model: 'small',
      language: 'en',
      device: 'device-1',
      diarization_enabled: true,
      max_speakers: 4,
      ollama_server_url: 'http://localhost:11434',
      ollama_model: 'qwen3:4b',
      medplum_server_url: 'http://localhost:8103',
      medplum_client_id: 'test-client',
      medplum_auto_sync: false,
    });
  });

  it('handles null input_device_id as "default"', async () => {
    mockInvoke.mockResolvedValue({
      ...mockSettings,
      input_device_id: null,
    });

    const { result } = renderHook(() => useSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.pendingSettings?.device).toBe('default');
  });

  it('handles load error gracefully', async () => {
    mockInvoke.mockRejectedValue(new Error('Failed to load'));

    const { result } = renderHook(() => useSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.error).toBe('Error: Failed to load');
    expect(result.current.settings).toBeNull();
  });

  it('updates pending settings', async () => {
    const { result } = renderHook(() => useSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    act(() => {
      result.current.setPendingSettings({
        ...result.current.pendingSettings!,
        model: 'medium',
        language: 'fa',
      });
    });

    expect(result.current.pendingSettings?.model).toBe('medium');
    expect(result.current.pendingSettings?.language).toBe('fa');
  });

  it('detects unsaved changes', async () => {
    const { result } = renderHook(() => useSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.hasUnsavedChanges).toBe(false);

    act(() => {
      result.current.setPendingSettings({
        ...result.current.pendingSettings!,
        model: 'medium',
      });
    });

    expect(result.current.hasUnsavedChanges).toBe(true);
  });

  it('saves settings successfully', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_settings') return Promise.resolve(mockSettings);
      if (cmd === 'set_settings') return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });

    const { result } = renderHook(() => useSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    // Modify settings
    act(() => {
      result.current.setPendingSettings({
        ...result.current.pendingSettings!,
        model: 'large',
      });
    });

    // Save
    let saveResult: boolean;
    await act(async () => {
      saveResult = await result.current.saveSettings();
    });

    expect(saveResult!).toBe(true);
    expect(mockInvoke).toHaveBeenCalledWith('set_settings', {
      settings: expect.objectContaining({
        whisper_model: 'large',
      }),
    });
    expect(result.current.settings?.whisper_model).toBe('large');
  });

  it('converts "default" device to null when saving', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_settings') return Promise.resolve(mockSettings);
      if (cmd === 'set_settings') return Promise.resolve(undefined);
      return Promise.resolve(undefined);
    });

    const { result } = renderHook(() => useSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    act(() => {
      result.current.setPendingSettings({
        ...result.current.pendingSettings!,
        device: 'default',
      });
    });

    await act(async () => {
      await result.current.saveSettings();
    });

    expect(mockInvoke).toHaveBeenCalledWith('set_settings', {
      settings: expect.objectContaining({
        input_device_id: null,
      }),
    });
  });

  it('handles save error gracefully', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_settings') return Promise.resolve(mockSettings);
      if (cmd === 'set_settings') return Promise.reject(new Error('Save failed'));
      return Promise.resolve(undefined);
    });

    const { result } = renderHook(() => useSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    let saveResult: boolean;
    await act(async () => {
      saveResult = await result.current.saveSettings();
    });

    expect(saveResult!).toBe(false);
    expect(result.current.error).toBe('Error: Save failed');
  });

  it('sets isSaving during save', async () => {
    let resolveSave: () => void;
    const savePromise = new Promise<void>((resolve) => {
      resolveSave = resolve;
    });

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_settings') return Promise.resolve(mockSettings);
      if (cmd === 'set_settings') return savePromise;
      return Promise.resolve(undefined);
    });

    const { result } = renderHook(() => useSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    let saveResultPromise: Promise<boolean>;
    act(() => {
      saveResultPromise = result.current.saveSettings();
    });

    expect(result.current.isSaving).toBe(true);

    await act(async () => {
      resolveSave!();
      await saveResultPromise;
    });

    expect(result.current.isSaving).toBe(false);
  });

  it('reloads settings', async () => {
    const { result } = renderHook(() => useSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    // Change the mock response
    mockInvoke.mockResolvedValue({
      ...mockSettings,
      whisper_model: 'tiny',
    });

    await act(async () => {
      await result.current.reloadSettings();
    });

    expect(result.current.settings?.whisper_model).toBe('tiny');
    expect(result.current.pendingSettings?.model).toBe('tiny');
  });

  it('returns false from saveSettings when settings are null', async () => {
    mockInvoke.mockRejectedValue(new Error('Failed'));

    const { result } = renderHook(() => useSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.settings).toBeNull();

    let saveResult: boolean;
    await act(async () => {
      saveResult = await result.current.saveSettings();
    });

    expect(saveResult!).toBe(false);
  });

  it('clears error on reload', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('First load failed'));

    const { result } = renderHook(() => useSettings());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.error).toBe('Error: First load failed');

    mockInvoke.mockResolvedValue(mockSettings);

    await act(async () => {
      await result.current.reloadSettings();
    });

    expect(result.current.error).toBeNull();
  });
});
