import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useOperationalDefaults } from './useOperationalDefaults';
import type { OperationalDefaults } from '../types';

const mockInvoke = vi.mocked(invoke);

const mockDefaults: OperationalDefaults = {
  version: 3,
  sleep_start_hour: 22,
  sleep_end_hour: 6,
  thermal_hot_pixel_threshold_c: 28.0,
  co2_baseline_ppm: 420.0,
  encounter_check_interval_secs: 120,
  encounter_silence_trigger_secs: 45,
  soap_model: 'server-soap',
  soap_model_fast: 'server-soap-fast',
  fast_model: 'server-fast',
  encounter_detection_model: 'server-detect',
};

describe('useOperationalDefaults', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(mockDefaults);
  });

  it('fetches defaults on mount', async () => {
    const { result } = renderHook(() => useOperationalDefaults());

    expect(result.current.isLoading).toBe(true);

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(mockInvoke).toHaveBeenCalledWith('get_operational_defaults');
    expect(result.current.defaults).toEqual(mockDefaults);
    expect(result.current.error).toBeNull();
  });

  it('surfaces errors without throwing', async () => {
    mockInvoke.mockRejectedValue(new Error('Server unreachable'));

    const { result } = renderHook(() => useOperationalDefaults());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.error).toContain('Server unreachable');
    expect(result.current.defaults).toBeNull();
  });

  it('refresh() re-fetches from backend', async () => {
    const { result } = renderHook(() => useOperationalDefaults());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(result.current.defaults?.soap_model).toBe('server-soap');

    // Server pushes a new default — refresh should pick it up.
    const updated: OperationalDefaults = { ...mockDefaults, soap_model: 'server-soap-v2', version: 4 };
    mockInvoke.mockResolvedValue(updated);

    await act(async () => {
      await result.current.refresh();
    });

    expect(mockInvoke).toHaveBeenCalledTimes(2);
    expect(result.current.defaults?.soap_model).toBe('server-soap-v2');
    expect(result.current.defaults?.version).toBe(4);
  });

  it('clears error when refresh succeeds after an earlier failure', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('First call failed'));

    const { result } = renderHook(() => useOperationalDefaults());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.error).toContain('First call failed');

    mockInvoke.mockResolvedValue(mockDefaults);

    await act(async () => {
      await result.current.refresh();
    });

    expect(result.current.error).toBeNull();
    expect(result.current.defaults).toEqual(mockDefaults);
  });
});
