import { useState, useEffect, useCallback, useRef } from 'react';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export interface UpdateStatus {
  checking: boolean;
  available: boolean;
  downloading: boolean;
  version: string | null;
  error: string | null;
}

export function useAppUpdater() {
  const [status, setStatus] = useState<UpdateStatus>({
    checking: false,
    available: false,
    downloading: false,
    version: null,
    error: null,
  });
  const checkedRef = useRef(false);

  // Check for updates on mount (once)
  useEffect(() => {
    if (checkedRef.current) return;
    checkedRef.current = true;

    const checkForUpdate = async () => {
      setStatus(prev => ({ ...prev, checking: true }));
      try {
        const update = await check();
        if (update) {
          setStatus(prev => ({
            ...prev,
            checking: false,
            available: true,
            version: update.version,
          }));
        } else {
          setStatus(prev => ({ ...prev, checking: false }));
        }
      } catch (e) {
        console.warn('Update check failed:', e);
        setStatus(prev => ({ ...prev, checking: false, error: String(e) }));
      }
    };

    // Delay check to not block startup
    const timer = setTimeout(checkForUpdate, 5000);
    return () => clearTimeout(timer);
  }, []);

  const installUpdate = useCallback(async () => {
    setStatus(prev => ({ ...prev, downloading: true, error: null }));
    try {
      const update = await check();
      if (!update) return;

      await update.downloadAndInstall();
      await relaunch();
    } catch (e) {
      console.error('Update install failed:', e);
      setStatus(prev => ({ ...prev, downloading: false, error: String(e) }));
    }
  }, []);

  const dismissUpdate = useCallback(() => {
    setStatus(prev => ({ ...prev, available: false }));
  }, []);

  return { status, installUpdate, dismissUpdate };
}
