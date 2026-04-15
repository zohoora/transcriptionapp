import { useState, useCallback, useRef, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import type { AudioUploadProgress, AudioUploadResult } from '../types';

interface AudioUploadState {
  filePath: string | null;
  fileName: string | null;
  recordingDate: string;
  isProcessing: boolean;
  progress: AudioUploadProgress | null;
  result: AudioUploadResult | null;
  error: string | null;
}

function todayStr(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

export function useAudioUpload() {
  const [state, setState] = useState<AudioUploadState>({
    filePath: null,
    fileName: null,
    recordingDate: todayStr(),
    isProcessing: false,
    progress: null,
    result: null,
    error: null,
  });

  const unlistenRef = useRef<UnlistenFn | null>(null);
  const processingRef = useRef(false);

  // Cleanup listener on unmount
  useEffect(() => {
    return () => {
      unlistenRef.current?.();
    };
  }, []);

  const selectFile = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: 'Audio',
            extensions: ['mp3', 'wav', 'm4a', 'aac', 'flac', 'ogg', 'wma', 'webm'],
          },
        ],
      });

      if (selected) {
        const path = selected; // open() with multiple: false returns string | null
        const name = path.split('/').pop() || path;
        setState((prev) => ({ ...prev, filePath: path, fileName: name, error: null }));
      }
    } catch (e) {
      console.error('File selection failed:', e);
    }
  }, []);

  const setRecordingDate = useCallback((date: string) => {
    setState((prev) => ({ ...prev, recordingDate: date }));
  }, []);

  const startProcessing = useCallback(async () => {
    if (!state.filePath || processingRef.current) return;
    processingRef.current = true;

    setState((prev) => ({
      ...prev,
      isProcessing: true,
      progress: null,
      result: null,
      error: null,
    }));

    // Listen for progress events
    unlistenRef.current?.();
    unlistenRef.current = await listen<AudioUploadProgress>(
      'audio_upload_progress',
      (event) => {
        setState((prev) => ({ ...prev, progress: event.payload }));
      }
    );

    try {
      const result = await invoke<AudioUploadResult>('process_audio_upload', {
        filePath: state.filePath,
        recordingDate: state.recordingDate,
      });
      setState((prev) => ({
        ...prev,
        isProcessing: false,
        result,
        progress: { step: 'complete' },
      }));
    } catch (e) {
      const errorMsg = typeof e === 'string' ? e : (e as Error).message || 'Processing failed';
      setState((prev) => ({
        ...prev,
        isProcessing: false,
        error: errorMsg,
        progress: { step: 'failed', error: errorMsg },
      }));
    } finally {
      processingRef.current = false;
      unlistenRef.current?.();
      unlistenRef.current = null;
    }
  }, [state.filePath, state.recordingDate]);

  const reset = useCallback(() => {
    unlistenRef.current?.();
    unlistenRef.current = null;
    setState({
      filePath: null,
      fileName: null,
      recordingDate: todayStr(),
      isProcessing: false,
      progress: null,
      result: null,
      error: null,
    });
  }, []);

  return {
    ...state,
    selectFile,
    setRecordingDate,
    startProcessing,
    reset,
  };
}
