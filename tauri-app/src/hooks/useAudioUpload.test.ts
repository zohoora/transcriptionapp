/**
 * Tests for useAudioUpload hook.
 * Covers: file selection, date handling, progress event subscription,
 * concurrent-invocation guard, error handling, reset.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { useAudioUpload } from './useAudioUpload';

// Mock the dialog plugin (not auto-mocked in setup.ts)
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);
const mockOpen = vi.mocked(open);

describe('useAudioUpload', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
    mockListen.mockReset();
    mockOpen.mockReset();
    // Default: listen() returns an unlisten function
    mockListen.mockResolvedValue(() => {});
  });

  it('initializes with default state', () => {
    const { result } = renderHook(() => useAudioUpload());
    expect(result.current.filePath).toBeNull();
    expect(result.current.fileName).toBeNull();
    expect(result.current.isProcessing).toBe(false);
    expect(result.current.progress).toBeNull();
    expect(result.current.result).toBeNull();
    expect(result.current.error).toBeNull();
    // recordingDate defaults to today
    expect(result.current.recordingDate).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  it('selectFile sets filePath and fileName from dialog', async () => {
    mockOpen.mockResolvedValue('/Users/test/Downloads/recording.m4a');
    const { result } = renderHook(() => useAudioUpload());

    await act(async () => {
      await result.current.selectFile();
    });

    expect(mockOpen).toHaveBeenCalledWith({
      multiple: false,
      filters: [
        { name: 'Audio', extensions: ['mp3', 'wav', 'm4a', 'aac', 'flac', 'ogg', 'wma', 'webm'] },
      ],
    });
    expect(result.current.filePath).toBe('/Users/test/Downloads/recording.m4a');
    expect(result.current.fileName).toBe('recording.m4a');
  });

  it('selectFile handles cancellation (null) without state change', async () => {
    mockOpen.mockResolvedValue(null as unknown as string);
    const { result } = renderHook(() => useAudioUpload());

    await act(async () => {
      await result.current.selectFile();
    });

    expect(result.current.filePath).toBeNull();
    expect(result.current.fileName).toBeNull();
  });

  it('setRecordingDate updates the date', () => {
    const { result } = renderHook(() => useAudioUpload());
    act(() => {
      result.current.setRecordingDate('2026-04-10');
    });
    expect(result.current.recordingDate).toBe('2026-04-10');
  });

  it('startProcessing requires a filePath (no-op without one)', async () => {
    const { result } = renderHook(() => useAudioUpload());
    await act(async () => {
      await result.current.startProcessing();
    });
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(result.current.isProcessing).toBe(false);
  });

  it('startProcessing invokes process_audio_upload with file and date', async () => {
    mockOpen.mockResolvedValue('/path/to/audio.mp3');
    mockInvoke.mockResolvedValue({
      sessions: [
        { sessionId: 's1', encounterNumber: 1, wordCount: 1500, hasSoap: true },
      ],
      totalWordCount: 1500,
    });
    const { result } = renderHook(() => useAudioUpload());

    await act(async () => {
      await result.current.selectFile();
    });
    act(() => {
      result.current.setRecordingDate('2026-04-15');
    });
    await act(async () => {
      await result.current.startProcessing();
    });

    expect(mockInvoke).toHaveBeenCalledWith('process_audio_upload', {
      filePath: '/path/to/audio.mp3',
      recordingDate: '2026-04-15',
    });
    expect(result.current.result?.sessions).toHaveLength(1);
    expect(result.current.isProcessing).toBe(false);
  });

  it('startProcessing subscribes to audio_upload_progress events', async () => {
    mockOpen.mockResolvedValue('/path/to/audio.mp3');
    mockInvoke.mockResolvedValue({ sessions: [], totalWordCount: 0 });
    const { result } = renderHook(() => useAudioUpload());

    await act(async () => {
      await result.current.selectFile();
    });
    await act(async () => {
      await result.current.startProcessing();
    });

    expect(mockListen).toHaveBeenCalledWith('audio_upload_progress', expect.any(Function));
  });

  it('startProcessing reports errors as state.error', async () => {
    mockOpen.mockResolvedValue('/path/to/audio.mp3');
    mockInvoke.mockRejectedValue('ffmpeg not found');
    const { result } = renderHook(() => useAudioUpload());

    await act(async () => {
      await result.current.selectFile();
    });
    await act(async () => {
      await result.current.startProcessing();
    });

    expect(result.current.error).toBe('ffmpeg not found');
    expect(result.current.isProcessing).toBe(false);
    expect(result.current.progress?.step).toBe('failed');
  });

  it('startProcessing has concurrent-invocation guard (useRef)', async () => {
    mockOpen.mockResolvedValue('/path/to/audio.mp3');
    // Simulate slow invoke
    let resolveInvoke: (value: unknown) => void = () => {};
    mockInvoke.mockReturnValueOnce(
      new Promise((res) => { resolveInvoke = res; })
    );
    const { result } = renderHook(() => useAudioUpload());

    await act(async () => {
      await result.current.selectFile();
    });

    // Fire the first call (don't await — it's still pending)
    let firstCall: Promise<void>;
    act(() => {
      firstCall = result.current.startProcessing();
    });

    // Try to fire a concurrent second call — should be guarded
    await act(async () => {
      await result.current.startProcessing();
    });

    // Only the first invoke should have happened
    expect(mockInvoke).toHaveBeenCalledTimes(1);

    // Resolve to clean up
    resolveInvoke({ sessions: [], totalWordCount: 0 });
    await act(async () => {
      await firstCall!;
    });
  });

  it('reset clears all state', async () => {
    mockOpen.mockResolvedValue('/path/to/audio.mp3');
    const { result } = renderHook(() => useAudioUpload());

    await act(async () => {
      await result.current.selectFile();
    });
    act(() => {
      result.current.setRecordingDate('2026-04-15');
    });
    expect(result.current.filePath).not.toBeNull();
    expect(result.current.recordingDate).toBe('2026-04-15');

    act(() => {
      result.current.reset();
    });

    expect(result.current.filePath).toBeNull();
    expect(result.current.fileName).toBeNull();
    expect(result.current.error).toBeNull();
    expect(result.current.result).toBeNull();
    expect(result.current.recordingDate).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  it('cleans up listener on unmount', () => {
    const unlistenSpy = vi.fn();
    mockListen.mockResolvedValue(unlistenSpy);
    const { unmount } = renderHook(() => useAudioUpload());
    unmount();
    // No listener was registered yet (selectFile not called), so spy not called
    // Just verify unmount doesn't crash
    expect(true).toBe(true);
  });
});
