import { useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Settings } from '../types';

/**
 * Hook that manages screen capture lifecycle tied to recording sessions.
 * Starts periodic capture when recording begins (if enabled in settings),
 * stops and cleans up when recording ends.
 */
export function useScreenCapture(
  isRecording: boolean,
  settings: Settings | null,
) {
  const captureRunningRef = useRef(false);

  useEffect(() => {
    if (isRecording && settings?.screen_capture_enabled && !captureRunningRef.current) {
      // Start capture
      const intervalSecs = settings.screen_capture_interval_secs || 30;
      invoke('start_screen_capture', { intervalSecs })
        .then(() => {
          captureRunningRef.current = true;
          console.log(`Screen capture started (${intervalSecs}s interval)`);
        })
        .catch((err) => {
          console.error('Failed to start screen capture:', err);
        });
    } else if (!isRecording && captureRunningRef.current) {
      // Stop capture
      invoke('stop_screen_capture')
        .then(() => {
          captureRunningRef.current = false;
          console.log('Screen capture stopped');
        })
        .catch((err) => {
          console.error('Failed to stop screen capture:', err);
        });
    }
  }, [isRecording, settings?.screen_capture_enabled, settings?.screen_capture_interval_secs]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (captureRunningRef.current) {
        invoke('stop_screen_capture').catch(() => {});
        captureRunningRef.current = false;
      }
    };
  }, []);
}
