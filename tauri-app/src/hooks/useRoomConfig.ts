import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface RoomConfig {
  room_name: string;
  profile_server_url: string;
  room_id?: string | null;
  active_physician_id?: string | null;
}

export interface UseRoomConfigResult {
  roomConfig: RoomConfig | null;
  loading: boolean;
  saveRoomConfig: (roomName: string, serverUrl: string) => Promise<void>;
  testConnection: (url: string) => Promise<boolean>;
  reload: () => Promise<void>;
}

export function useRoomConfig(): UseRoomConfigResult {
  const [roomConfig, setRoomConfig] = useState<RoomConfig | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invoke<RoomConfig | null>('get_room_config')
      .then(config => {
        setRoomConfig(config);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  const saveRoomConfig = useCallback(async (roomName: string, serverUrl: string) => {
    await invoke('save_room_config', { roomName, profileServerUrl: serverUrl });
    setRoomConfig({ room_name: roomName, profile_server_url: serverUrl });
  }, []);

  const testConnection = useCallback(async (url: string): Promise<boolean> => {
    try {
      return await invoke<boolean>('test_profile_server', { url });
    } catch {
      return false;
    }
  }, []);

  const reload = useCallback(async () => {
    const config = await invoke<RoomConfig | null>('get_room_config');
    setRoomConfig(config);
  }, []);

  return { roomConfig, loading, saveRoomConfig, testConnection, reload };
}
