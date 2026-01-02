import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useSessionState } from './useSessionState';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

// Type the mocks from global setup
const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);
const mockUnlisten = vi.fn();

describe('useSessionState', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListen.mockImplementation(() => Promise.resolve(mockUnlisten));
    mockInvoke.mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('initializes with idle state', () => {
    const { result } = renderHook(() => useSessionState());

    expect(result.current.status.state).toBe('idle');
    expect(result.current.transcript.finalized_text).toBe('');
    expect(result.current.biomarkers).toBeNull();
    expect(result.current.audioQuality).toBeNull();
    expect(result.current.isIdle).toBe(true);
    expect(result.current.isRecording).toBe(false);
  });

  it('sets up event listeners on mount', async () => {
    renderHook(() => useSessionState());

    await waitFor(() => {
      expect(mockListen).toHaveBeenCalledWith('session_status', expect.any(Function));
      expect(mockListen).toHaveBeenCalledWith('transcript_update', expect.any(Function));
      expect(mockListen).toHaveBeenCalledWith('biomarker_update', expect.any(Function));
      expect(mockListen).toHaveBeenCalledWith('audio_quality', expect.any(Function));
    });
  });

  it('handles start session', async () => {
    mockInvoke.mockResolvedValue(undefined);

    const { result } = renderHook(() => useSessionState());

    await act(async () => {
      await result.current.handleStart('device-1');
    });

    expect(mockInvoke).toHaveBeenCalledWith('start_session', { deviceId: 'device-1' });
  });

  it('handles start session with null device', async () => {
    mockInvoke.mockResolvedValue(undefined);

    const { result } = renderHook(() => useSessionState());

    await act(async () => {
      await result.current.handleStart(null);
    });

    expect(mockInvoke).toHaveBeenCalledWith('start_session', { deviceId: null });
  });

  it('handles stop session', async () => {
    mockInvoke.mockResolvedValue(undefined);

    const { result } = renderHook(() => useSessionState());

    await act(async () => {
      await result.current.handleStop();
    });

    expect(mockInvoke).toHaveBeenCalledWith('stop_session');
  });

  it('handles reset session', async () => {
    mockInvoke.mockResolvedValue(undefined);

    const { result } = renderHook(() => useSessionState());

    await act(async () => {
      await result.current.handleReset();
    });

    expect(mockInvoke).toHaveBeenCalledWith('reset_session');
    expect(result.current.transcript.finalized_text).toBe('');
    expect(result.current.editedTranscript).toBe('');
    expect(result.current.biomarkers).toBeNull();
  });

  it('resets state on handleStart', async () => {
    mockInvoke.mockResolvedValue(undefined);

    const { result } = renderHook(() => useSessionState());

    // Set some state first
    act(() => {
      result.current.setEditedTranscript('some text');
    });

    expect(result.current.editedTranscript).toBe('some text');

    // Start should reset
    await act(async () => {
      await result.current.handleStart(null);
    });

    expect(result.current.editedTranscript).toBe('');
  });

  it('updates editedTranscript via setter', () => {
    const { result } = renderHook(() => useSessionState());

    act(() => {
      result.current.setEditedTranscript('new transcript text');
    });

    expect(result.current.editedTranscript).toBe('new transcript text');
  });

  it('updates soapNote via setter', () => {
    const { result } = renderHook(() => useSessionState());

    const mockSoapNote = {
      subjective: 'test',
      objective: 'test',
      assessment: 'test',
      plan: 'test',
      generated_at: '2025-01-01T00:00:00Z',
      model_used: 'test-model',
    };

    act(() => {
      result.current.setSoapNote(mockSoapNote);
    });

    expect(result.current.soapNote).toEqual(mockSoapNote);
  });

  it('derives isRecording correctly', async () => {
    let statusCallback: ((event: { payload: unknown }) => void) | null = null;
    mockListen.mockImplementation((event: string, callback: (event: { payload: unknown }) => void) => {
      if (event === 'session_status') {
        statusCallback = callback;
      }
      return Promise.resolve(mockUnlisten);
    });

    const { result } = renderHook(() => useSessionState());

    await waitFor(() => {
      expect(statusCallback).not.toBeNull();
    });

    // Simulate recording status event
    act(() => {
      statusCallback!({
        payload: {
          state: 'recording',
          provider: 'whisper',
          elapsed_ms: 1000,
          is_processing_behind: false,
        },
      });
    });

    expect(result.current.isRecording).toBe(true);
    expect(result.current.isIdle).toBe(false);
  });

  it('derives isCompleted correctly', async () => {
    let statusCallback: ((event: { payload: unknown }) => void) | null = null;
    mockListen.mockImplementation((event: string, callback: (event: { payload: unknown }) => void) => {
      if (event === 'session_status') {
        statusCallback = callback;
      }
      return Promise.resolve(mockUnlisten);
    });

    const { result } = renderHook(() => useSessionState());

    await waitFor(() => {
      expect(statusCallback).not.toBeNull();
    });

    act(() => {
      statusCallback!({
        payload: {
          state: 'completed',
          provider: 'whisper',
          elapsed_ms: 5000,
          is_processing_behind: false,
        },
      });
    });

    expect(result.current.isCompleted).toBe(true);
    expect(result.current.isRecording).toBe(false);
  });

  it('cleans up listeners on unmount', async () => {
    const { unmount } = renderHook(() => useSessionState());

    await waitFor(() => {
      expect(mockListen).toHaveBeenCalled();
    });

    unmount();

    // Each listener should have its unlisten called
    await waitFor(() => {
      expect(mockUnlisten).toHaveBeenCalled();
    });
  });
});
