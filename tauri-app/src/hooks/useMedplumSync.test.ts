import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useMedplumSync } from './useMedplumSync';
import { invoke } from '@tauri-apps/api/core';

// Type the mock from global setup
const mockInvoke = vi.mocked(invoke);

describe('useMedplumSync', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset mock to default resolved value to prevent state bleeding between tests
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(undefined);
  });

  it('initializes with correct default state', () => {
    const { result } = renderHook(() => useMedplumSync());

    expect(result.current.medplumConnected).toBe(false);
    expect(result.current.medplumError).toBeNull();
    expect(result.current.isSyncing).toBe(false);
    expect(result.current.syncError).toBeNull();
    expect(result.current.syncSuccess).toBe(false);
  });

  it('syncs to Medplum successfully', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_audio_file_path') return Promise.resolve('/path/to/audio.wav');
      if (cmd === 'medplum_quick_sync') {
        return Promise.resolve({
          success: true,
          status: {
            transcript_synced: true,
            soap_note_synced: true,
            audio_synced: true,
            encounter_synced: true,
          },
          error: null,
        });
      }
      return Promise.resolve(null);
    });

    const { result } = renderHook(() => useMedplumSync());

    await act(async () => {
      await result.current.syncToMedplum({
        authState: { is_authenticated: true },
        transcript: 'Test transcript',
        soapNote: null,
        elapsedMs: 5000,
      });
    });

    expect(mockInvoke).toHaveBeenCalledWith('get_audio_file_path');
    expect(mockInvoke).toHaveBeenCalledWith('medplum_quick_sync', {
      transcript: 'Test transcript',
      soapNote: null,
      audioFilePath: '/path/to/audio.wav',
      sessionDurationMs: 5000,
    });
    expect(result.current.syncSuccess).toBe(true);
    expect(result.current.syncError).toBeNull();
  });

  it('formats SOAP note correctly when syncing', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_audio_file_path') return Promise.resolve(null);
      if (cmd === 'medplum_quick_sync') {
        return Promise.resolve({ success: true, error: null });
      }
      return Promise.resolve(null);
    });

    const { result } = renderHook(() => useMedplumSync());

    const soapNote = {
      content: 'SOAP note content from LLM',
      generated_at: '2025-01-01T00:00:00Z',
      model_used: 'qwen3:4b',
    };

    await act(async () => {
      await result.current.syncToMedplum({
        authState: { is_authenticated: true },
        transcript: 'Transcript',
        soapNote,
        elapsedMs: 1000,
      });
    });

    expect(mockInvoke).toHaveBeenCalledWith('medplum_quick_sync', {
      transcript: 'Transcript',
      soapNote: 'SOAP note content from LLM',
      audioFilePath: null,
      sessionDurationMs: 1000,
    });
  });

  it('does not sync when not authenticated', async () => {
    const { result } = renderHook(() => useMedplumSync());

    await act(async () => {
      await result.current.syncToMedplum({
        authState: { is_authenticated: false },
        transcript: 'Test',
        soapNote: null,
        elapsedMs: 1000,
      });
    });

    expect(mockInvoke).not.toHaveBeenCalled();
    expect(result.current.syncSuccess).toBe(false);
  });

  it('handles sync failure', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_audio_file_path') return Promise.resolve(null);
      if (cmd === 'medplum_quick_sync') {
        return Promise.resolve({
          success: false,
          error: 'Transcript upload failed',
        });
      }
      return Promise.resolve(null);
    });

    const { result } = renderHook(() => useMedplumSync());

    await act(async () => {
      await result.current.syncToMedplum({
        authState: { is_authenticated: true },
        transcript: 'Test',
        soapNote: null,
        elapsedMs: 1000,
      });
    });

    expect(result.current.syncSuccess).toBe(false);
    expect(result.current.syncError).toBe('Transcript upload failed');
  });

  it('handles sync exception', async () => {
    mockInvoke.mockRejectedValue(new Error('Network error'));

    const { result } = renderHook(() => useMedplumSync());

    await act(async () => {
      await result.current.syncToMedplum({
        authState: { is_authenticated: true },
        transcript: 'Test',
        soapNote: null,
        elapsedMs: 1000,
      });
    });

    expect(result.current.syncSuccess).toBe(false);
    expect(result.current.syncError).toBe('Network error');
  });

  it('sets isSyncing during sync', async () => {
    let resolveSync: (value: unknown) => void;
    const syncPromise = new Promise((resolve) => {
      resolveSync = resolve;
    });

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_audio_file_path') return Promise.resolve(null);
      if (cmd === 'medplum_quick_sync') return syncPromise;
      return Promise.resolve(null);
    });

    const { result } = renderHook(() => useMedplumSync());

    // Start sync - trigger the async operation without awaiting the result
    let syncResultPromise: Promise<void>;
    act(() => {
      syncResultPromise = result.current.syncToMedplum({
        authState: { is_authenticated: true },
        transcript: 'Test',
        soapNote: null,
        elapsedMs: 1000,
      });
    });

    // Should be syncing (state is set synchronously before the await)
    expect(result.current.isSyncing).toBe(true);

    // Now complete the async operation and wait for it
    await act(async () => {
      resolveSync!({ success: true, error: null });
      await syncResultPromise;
    });

    expect(result.current.isSyncing).toBe(false);
  });

  it('resets sync state correctly', () => {
    const { result } = renderHook(() => useMedplumSync());

    // Set some state
    act(() => {
      result.current.setSyncSuccess(true);
      result.current.setSyncError('Some error');
    });

    expect(result.current.syncSuccess).toBe(true);
    expect(result.current.syncError).toBe('Some error');

    // Reset
    act(() => {
      result.current.resetSyncState();
    });

    expect(result.current.syncSuccess).toBe(false);
    expect(result.current.syncError).toBeNull();
  });

  it('can set medplum connected state', () => {
    const { result } = renderHook(() => useMedplumSync());

    act(() => {
      result.current.setMedplumConnected(true);
    });

    expect(result.current.medplumConnected).toBe(true);

    act(() => {
      result.current.setMedplumConnected(false);
    });

    expect(result.current.medplumConnected).toBe(false);
  });

  it('can set medplum error', () => {
    const { result } = renderHook(() => useMedplumSync());

    act(() => {
      result.current.setMedplumError('Connection failed');
    });

    expect(result.current.medplumError).toBe('Connection failed');

    act(() => {
      result.current.setMedplumError(null);
    });

    expect(result.current.medplumError).toBeNull();
  });

  it('clears previous error on new sync', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_audio_file_path') return Promise.resolve(null);
      if (cmd === 'medplum_quick_sync') {
        return Promise.resolve({ success: true, error: null });
      }
      return Promise.resolve(null);
    });

    const { result } = renderHook(() => useMedplumSync());

    // Set an error first
    act(() => {
      result.current.setSyncError('Previous error');
    });
    expect(result.current.syncError).toBe('Previous error');

    // Sync should clear error
    await act(async () => {
      await result.current.syncToMedplum({
        authState: { is_authenticated: true },
        transcript: 'Test',
        soapNote: null,
        elapsedMs: 1000,
      });
    });

    expect(result.current.syncError).toBeNull();
  });
});
