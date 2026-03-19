import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { PhysicianProfile, Room } from '../types';

export interface UseAdminPanelResult {
  physicians: PhysicianProfile[];
  rooms: Room[];
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  createPhysician: (name: string, specialty?: string) => Promise<void>;
  updatePhysician: (id: string, updates: { name?: string; specialty?: string | null }) => Promise<void>;
  deletePhysician: (id: string) => Promise<void>;
  createRoom: (name: string, description?: string) => Promise<void>;
  updateRoom: (id: string, updates: { name?: string; description?: string | null }) => Promise<void>;
  deleteRoom: (id: string) => Promise<void>;
}

export function useAdminPanel(): UseAdminPanelResult {
  const [physicians, setPhysicians] = useState<PhysicianProfile[]>([]);
  const [rooms, setRooms] = useState<Room[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const operationInProgress = useRef(false);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      const [profiles, roomList] = await Promise.all([
        invoke<PhysicianProfile[]>('get_physicians'),
        invoke<Room[]>('get_rooms'),
      ]);
      setPhysicians(profiles);
      setRooms(roomList);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      console.error('Failed to load admin data:', e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const createPhysician = useCallback(async (name: string, specialty?: string) => {
    if (operationInProgress.current) return;
    operationInProgress.current = true;
    try {
      setError(null);
      await invoke('create_physician', { name, specialty: specialty || null });
      await refresh();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      throw e;
    } finally {
      operationInProgress.current = false;
    }
  }, [refresh]);

  const updatePhysician = useCallback(async (
    id: string,
    updates: { name?: string; specialty?: string | null },
  ) => {
    if (operationInProgress.current) return;
    operationInProgress.current = true;
    try {
      setError(null);
      await invoke('update_physician', { physicianId: id, updates });
      await refresh();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      throw e;
    } finally {
      operationInProgress.current = false;
    }
  }, [refresh]);

  const deletePhysician = useCallback(async (id: string) => {
    if (operationInProgress.current) return;
    operationInProgress.current = true;
    try {
      setError(null);
      await invoke('delete_physician', { physicianId: id });
      await refresh();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      throw e;
    } finally {
      operationInProgress.current = false;
    }
  }, [refresh]);

  const createRoom = useCallback(async (name: string, description?: string) => {
    if (operationInProgress.current) return;
    operationInProgress.current = true;
    try {
      setError(null);
      await invoke('create_room', { name, description: description || null });
      await refresh();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      throw e;
    } finally {
      operationInProgress.current = false;
    }
  }, [refresh]);

  const updateRoom = useCallback(async (
    id: string,
    updates: { name?: string; description?: string | null },
  ) => {
    if (operationInProgress.current) return;
    operationInProgress.current = true;
    try {
      setError(null);
      await invoke('update_room', { roomId: id, updates });
      await refresh();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      throw e;
    } finally {
      operationInProgress.current = false;
    }
  }, [refresh]);

  const deleteRoom = useCallback(async (id: string) => {
    if (operationInProgress.current) return;
    operationInProgress.current = true;
    try {
      setError(null);
      await invoke('delete_room', { roomId: id });
      await refresh();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      throw e;
    } finally {
      operationInProgress.current = false;
    }
  }, [refresh]);

  return {
    physicians,
    rooms,
    loading,
    error,
    refresh,
    createPhysician,
    updatePhysician,
    deletePhysician,
    createRoom,
    updateRoom,
    deleteRoom,
  };
}
