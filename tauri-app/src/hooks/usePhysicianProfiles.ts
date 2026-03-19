import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { PhysicianProfile } from '../types';

export interface UsePhysicianProfilesResult {
  physicians: PhysicianProfile[];
  activePhysician: PhysicianProfile | null;
  loading: boolean;
  selectPhysician: (id: string) => Promise<PhysicianProfile>;
  deselectPhysician: () => Promise<void>;
  refresh: () => Promise<void>;
}

export function usePhysicianProfiles(): UsePhysicianProfilesResult {
  const [physicians, setPhysicians] = useState<PhysicianProfile[]>([]);
  const [activePhysician, setActivePhysician] = useState<PhysicianProfile | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const [profiles, active] = await Promise.all([
        invoke<PhysicianProfile[]>('get_physicians'),
        invoke<PhysicianProfile | null>('get_active_physician'),
      ]);
      setPhysicians(profiles);
      setActivePhysician(active);
    } catch (e) {
      console.error('Failed to load physicians:', e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  const selectPhysician = useCallback(async (id: string) => {
    const profile = await invoke<PhysicianProfile>('select_physician', { physicianId: id });
    setActivePhysician(profile);
    return profile;
  }, []);

  const deselectPhysician = useCallback(async () => {
    await invoke('deselect_physician');
    setActivePhysician(null);
  }, []);

  return { physicians, activePhysician, loading, selectPhysician, deselectPhysician, refresh };
}
