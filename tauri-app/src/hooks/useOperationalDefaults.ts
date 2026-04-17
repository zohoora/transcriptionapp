import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { OperationalDefaults } from '../types';

/**
 * Phase 3: surface the server-pushed `OperationalDefaults` so the Settings
 * drawer can show "Clinic default: …" hints and the reset-to-default link.
 *
 * The backend command is backed by `SharedServerConfig`, which itself falls
 * through server → cache → compiled defaults, so `invoke` is expected to
 * succeed. We still surface an `error` for defensive rendering.
 *
 * Cache strategy: fetched once on mount, refreshed explicitly via `refresh()`
 * (e.g. after a settings save or a `clear_user_edited_field` call).
 */
export interface UseOperationalDefaultsResult {
  defaults: OperationalDefaults | null;
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

export function useOperationalDefaults(): UseOperationalDefaultsResult {
  const [defaults, setDefaults] = useState<OperationalDefaults | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const initialLoadRef = useRef(false);

  const load = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const result = await invoke<OperationalDefaults>('get_operational_defaults');
      setDefaults(result);
    } catch (e) {
      console.error('Failed to load operational defaults:', e);
      setError(String(e));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    if (initialLoadRef.current) return;
    initialLoadRef.current = true;
    load();
  }, [load]);

  return {
    defaults,
    isLoading,
    error,
    refresh: load,
  };
}
