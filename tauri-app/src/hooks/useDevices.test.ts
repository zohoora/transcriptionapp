import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useDevices } from './useDevices';
import { invoke } from '@tauri-apps/api/core';

// Type the mock from global setup
const mockInvoke = vi.mocked(invoke);

const mockDevices = [
  { id: 'device-1', name: 'Built-in Microphone', is_default: true },
  { id: 'device-2', name: 'USB Microphone', is_default: false },
  { id: 'device-3', name: 'Bluetooth Headset', is_default: false },
];

describe('useDevices', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(mockDevices);
  });

  it('loads devices on mount', async () => {
    const { result } = renderHook(() => useDevices());

    expect(result.current.isLoading).toBe(true);

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(mockInvoke).toHaveBeenCalledWith('list_input_devices');
    expect(result.current.devices).toEqual(mockDevices);
  });

  it('handles load error gracefully', async () => {
    mockInvoke.mockRejectedValue(new Error('Failed to list devices'));

    const { result } = renderHook(() => useDevices());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.error).toBe('Error: Failed to list devices');
    expect(result.current.devices).toEqual([]);
  });

  it('returns default device', async () => {
    const { result } = renderHook(() => useDevices());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.defaultDevice).toEqual({
      id: 'device-1',
      name: 'Built-in Microphone',
      is_default: true,
    });
  });

  it('returns undefined for defaultDevice when no default exists', async () => {
    mockInvoke.mockResolvedValue([
      { id: 'device-1', name: 'Mic 1', is_default: false },
      { id: 'device-2', name: 'Mic 2', is_default: false },
    ]);

    const { result } = renderHook(() => useDevices());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.defaultDevice).toBeUndefined();
  });

  it('gets device by ID', async () => {
    const { result } = renderHook(() => useDevices());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    const device = result.current.getDeviceById('device-2');
    expect(device).toEqual({
      id: 'device-2',
      name: 'USB Microphone',
      is_default: false,
    });
  });

  it('returns undefined for unknown device ID', async () => {
    const { result } = renderHook(() => useDevices());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    const device = result.current.getDeviceById('unknown-id');
    expect(device).toBeUndefined();
  });

  it('refreshes devices', async () => {
    const { result } = renderHook(() => useDevices());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.devices).toHaveLength(3);

    // Change mock response for refresh
    const newDevices = [
      { id: 'new-device', name: 'New Microphone', is_default: true },
    ];
    mockInvoke.mockResolvedValue(newDevices);

    await act(async () => {
      await result.current.refreshDevices();
    });

    expect(result.current.devices).toEqual(newDevices);
    expect(mockInvoke).toHaveBeenCalledTimes(2); // Initial + refresh
  });

  it('clears error on refresh', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('First load failed'));

    const { result } = renderHook(() => useDevices());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.error).toBe('Error: First load failed');

    mockInvoke.mockResolvedValue(mockDevices);

    await act(async () => {
      await result.current.refreshDevices();
    });

    expect(result.current.error).toBeNull();
    expect(result.current.devices).toEqual(mockDevices);
  });

  it('handles empty device list', async () => {
    mockInvoke.mockResolvedValue([]);

    const { result } = renderHook(() => useDevices());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.devices).toEqual([]);
    expect(result.current.defaultDevice).toBeUndefined();
    expect(result.current.error).toBeNull();
  });

  it('sets isLoading during refresh', async () => {
    let resolveRefresh: (value: unknown) => void;
    const refreshPromise = new Promise((resolve) => {
      resolveRefresh = resolve;
    });

    const { result } = renderHook(() => useDevices());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    // Set up delayed response for refresh
    mockInvoke.mockReturnValue(refreshPromise);

    let refreshResultPromise: Promise<void>;
    act(() => {
      refreshResultPromise = result.current.refreshDevices();
    });

    expect(result.current.isLoading).toBe(true);

    await act(async () => {
      resolveRefresh!(mockDevices);
      await refreshResultPromise;
    });

    expect(result.current.isLoading).toBe(false);
  });
});
