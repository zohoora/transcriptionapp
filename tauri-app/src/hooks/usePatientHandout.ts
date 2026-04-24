import { useState, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { formatErrorMessage } from '../utils';

export interface UsePatientHandoutResult {
  isGenerating: boolean;
  error: string | null;
  generateHandout: (transcript: string, sessionId: string, date: string) => Promise<void>;
}

export function usePatientHandout(): UsePatientHandoutResult {
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generatingRef = useRef(false);

  const generateHandout = useCallback(async (transcript: string, sessionId: string, date: string) => {
    if (generatingRef.current) return;
    generatingRef.current = true;
    setIsGenerating(true);
    setError(null);

    try {
      const content = await invoke<string>('generate_patient_handout', {
        transcript,
        sessionId,
        date,
      });

      // Save to archive immediately so the editor can load it on mount (avoids event race)
      await invoke('save_patient_handout', { sessionId, date, content });

      const existing = await WebviewWindow.getByLabel('patient-handout');
      if (existing) await existing.close();

      new WebviewWindow('patient-handout', {
        url: `patient-handout.html?sessionId=${encodeURIComponent(sessionId)}&date=${encodeURIComponent(date)}`,
        title: 'Patient Handout',
        width: 700,
        height: 800,
        minWidth: 500,
        minHeight: 400,
        center: true,
      });
    } catch (e) {
      setError(formatErrorMessage(e));
    } finally {
      generatingRef.current = false;
      setIsGenerating(false);
    }
  }, []);

  return { isGenerating, error, generateHandout };
}
