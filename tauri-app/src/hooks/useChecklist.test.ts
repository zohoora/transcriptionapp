import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useChecklist } from './useChecklist';
import { invoke } from '@tauri-apps/api/core';

// Note: waitFor is imported from @testing-library/react

// Type the mock from global setup
const mockInvoke = vi.mocked(invoke);

describe('useChecklist', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset mock to default resolved value to prevent state bleeding between tests
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(undefined);
  });

  it('initializes with correct default state and runs checklist on mount', async () => {
    const mockChecklistResult = {
      checks: [],
      all_passed: true,
      can_start: true,
      summary: 'Ready',
    };
    const mockModelStatus = {
      available: true,
      path: '/path/to/model',
      error: null,
    };

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'run_checklist') return Promise.resolve(mockChecklistResult);
      if (cmd === 'check_model_status') return Promise.resolve(mockModelStatus);
      return Promise.resolve(undefined);
    });

    const { result } = renderHook(() => useChecklist());

    // Initially, checklistRunning is true
    expect(result.current.checklistRunning).toBe(true);
    expect(result.current.downloadingModel).toBeNull();

    // Wait for mount effects to complete
    await waitFor(() => {
      expect(result.current.checklistRunning).toBe(false);
    });

    // After mount, checklist and model status should be set
    expect(result.current.checklistResult).toEqual(mockChecklistResult);
    expect(result.current.modelStatus).toEqual(mockModelStatus);
  });

  it('runs checklist successfully', async () => {
    const mockChecklistResult = {
      checks: [
        { name: 'Audio', status: 'Pass', message: 'OK' },
        { name: 'Model', status: 'Pass', message: 'OK' },
      ],
      all_passed: true,
      can_start: true,
      summary: 'All checks passed',
    };

    mockInvoke.mockResolvedValue(mockChecklistResult);

    const { result } = renderHook(() => useChecklist());

    await act(async () => {
      await result.current.runChecklist();
    });

    expect(mockInvoke).toHaveBeenCalledWith('run_checklist');
    expect(result.current.checklistResult).toEqual(mockChecklistResult);
    expect(result.current.checklistRunning).toBe(false);
  });

  it('handles checklist failure gracefully', async () => {
    mockInvoke.mockRejectedValue(new Error('Checklist failed'));

    const { result } = renderHook(() => useChecklist());

    await act(async () => {
      await result.current.runChecklist();
    });

    expect(result.current.checklistResult?.all_passed).toBe(false);
    expect(result.current.checklistResult?.can_start).toBe(false);
    expect(result.current.checklistRunning).toBe(false);
  });

  it('downloads Whisper model with model name', async () => {
    mockInvoke.mockResolvedValue('/path/to/model.bin');

    const { result } = renderHook(() => useChecklist());

    await act(async () => {
      await result.current.handleDownloadModel('small');
    });

    expect(mockInvoke).toHaveBeenCalledWith('download_whisper_model', { modelName: 'small' });
    expect(result.current.downloadingModel).toBeNull();
  });

  it('downloads speaker model without model name', async () => {
    mockInvoke.mockResolvedValue('/path/to/speaker.onnx');

    const { result } = renderHook(() => useChecklist());

    await act(async () => {
      await result.current.handleDownloadModel('speaker_embedding');
    });

    expect(mockInvoke).toHaveBeenCalledWith('download_speaker_model');
  });

  it('downloads enhancement model', async () => {
    mockInvoke.mockResolvedValue('/path/to/gtcrn.onnx');

    const { result } = renderHook(() => useChecklist());

    await act(async () => {
      await result.current.handleDownloadModel('gtcrn_simple');
    });

    expect(mockInvoke).toHaveBeenCalledWith('download_enhancement_model');
  });

  it('downloads emotion model', async () => {
    mockInvoke.mockResolvedValue('/path/to/wav2small.onnx');

    const { result } = renderHook(() => useChecklist());

    await act(async () => {
      await result.current.handleDownloadModel('wav2small');
    });

    expect(mockInvoke).toHaveBeenCalledWith('download_emotion_model');
  });

  it('downloads yamnet model', async () => {
    mockInvoke.mockResolvedValue('/path/to/yamnet.onnx');

    const { result } = renderHook(() => useChecklist());

    await act(async () => {
      await result.current.handleDownloadModel('yamnet');
    });

    expect(mockInvoke).toHaveBeenCalledWith('download_yamnet_model');
  });

  it('sets downloadingModel during download', async () => {
    let resolveDownload: () => void;
    const downloadPromise = new Promise<string>((resolve) => {
      resolveDownload = () => resolve('/path/to/model');
    });

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'download_whisper_model') {
        return downloadPromise;
      }
      return Promise.resolve({});
    });

    const { result } = renderHook(() => useChecklist());

    // Start download - trigger the async operation without awaiting the result
    let downloadResultPromise: Promise<void>;
    act(() => {
      downloadResultPromise = result.current.handleDownloadModel('small');
    });

    // Check downloading state is set (state is set synchronously before the await)
    expect(result.current.downloadingModel).toBe('small');

    // Now complete the async operation and wait for it
    await act(async () => {
      resolveDownload!();
      await downloadResultPromise;
    });

    expect(result.current.downloadingModel).toBeNull();
  });

  it('refreshes checklist and model status after download', async () => {
    const mockChecklistResult = {
      checks: [],
      all_passed: true,
      can_start: true,
      summary: 'OK',
    };
    const mockModelStatus = {
      available: true,
      path: '/path/to/model',
      error: null,
    };

    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'download_whisper_model') return Promise.resolve('/path');
      if (cmd === 'run_checklist') return Promise.resolve(mockChecklistResult);
      if (cmd === 'check_model_status') return Promise.resolve(mockModelStatus);
      return Promise.resolve({});
    });

    const { result } = renderHook(() => useChecklist());

    await act(async () => {
      await result.current.handleDownloadModel('small');
    });

    expect(mockInvoke).toHaveBeenCalledWith('run_checklist');
    expect(mockInvoke).toHaveBeenCalledWith('check_model_status');
    expect(result.current.modelStatus).toEqual(mockModelStatus);
  });

  it('handles download failure gracefully', async () => {
    mockInvoke.mockRejectedValue(new Error('Download failed'));

    const { result } = renderHook(() => useChecklist());

    await act(async () => {
      await result.current.handleDownloadModel('small');
    });

    // Should not throw, just log error
    expect(result.current.downloadingModel).toBeNull();
  });

  it('can set model status directly', () => {
    const { result } = renderHook(() => useChecklist());

    const mockStatus = {
      available: true,
      path: '/path/to/model',
      error: null,
    };

    act(() => {
      result.current.setModelStatus(mockStatus);
    });

    expect(result.current.modelStatus).toEqual(mockStatus);
  });
});
