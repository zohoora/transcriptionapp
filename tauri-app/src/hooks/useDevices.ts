import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Device } from '../types';

export interface UseDevicesResult {
  devices: Device[];
  isLoading: boolean;
  error: string | null;
  refreshDevices: () => Promise<void>;
  getDeviceById: (id: string) => Device | undefined;
  defaultDevice: Device | undefined;
}

/**
 * Hook for managing audio input device listing.
 * Loads devices on mount and provides refresh functionality.
 */
export function useDevices(): UseDevicesResult {
  const [devices, setDevices] = useState<Device[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const initialLoadRef = useRef(false);

  // Load devices
  const loadDevices = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<Device[]>('list_input_devices');
      setDevices(result);
    } catch (e) {
      console.error('Failed to load devices:', e);
      setError(String(e));
      setDevices([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Load devices on mount (only once)
  useEffect(() => {
    if (initialLoadRef.current) return;
    initialLoadRef.current = true;
    loadDevices();
  }, [loadDevices]);

  // Get device by ID
  const getDeviceById = useCallback(
    (id: string): Device | undefined => {
      return devices.find((d) => d.id === id);
    },
    [devices]
  );

  // Get default device
  const defaultDevice = devices.find((d) => d.is_default);

  return {
    devices,
    isLoading,
    error,
    refreshDevices: loadDevices,
    getDeviceById,
    defaultDevice,
  };
}
