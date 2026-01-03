import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { CoughEvent, OllamaStatus, SoapNote } from '../types';
import { formatErrorMessage } from '../utils';

/** Audio event to pass to SOAP generation (matches Rust AudioEvent) */
export interface AudioEvent {
  timestamp_ms: number;
  duration_ms: number;
  confidence: number;
  label: string;
}

export interface UseSoapNoteResult {
  isGeneratingSoap: boolean;
  soapError: string | null;
  ollamaStatus: OllamaStatus | null;
  ollamaModels: string[];
  generateSoapNote: (transcript: string, audioEvents?: CoughEvent[]) => Promise<SoapNote | null>;
  setOllamaStatus: (status: OllamaStatus | null) => void;
  setOllamaModels: (models: string[]) => void;
  setSoapError: (error: string | null) => void;
}

export function useSoapNote(): UseSoapNoteResult {
  const [isGeneratingSoap, setIsGeneratingSoap] = useState(false);
  const [soapError, setSoapError] = useState<string | null>(null);
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus | null>(null);
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);

  // Generate SOAP note
  const generateSoapNote = useCallback(async (
    transcript: string,
    audioEvents?: CoughEvent[]
  ): Promise<SoapNote | null> => {
    if (!transcript.trim()) return null;

    setIsGeneratingSoap(true);
    setSoapError(null);

    try {
      // Convert CoughEvent to AudioEvent format expected by backend
      const events = audioEvents?.map(e => ({
        timestamp_ms: e.timestamp_ms,
        duration_ms: e.duration_ms,
        confidence: e.confidence,
        label: e.label,
      }));

      const result = await invoke<SoapNote>('generate_soap_note', {
        transcript,
        audioEvents: events,
      });
      return result;
    } catch (e) {
      console.error('Failed to generate SOAP note:', e);
      setSoapError(formatErrorMessage(e));
      return null;
    } finally {
      setIsGeneratingSoap(false);
    }
  }, []);

  return {
    isGeneratingSoap,
    soapError,
    ollamaStatus,
    ollamaModels,
    generateSoapNote,
    setOllamaStatus,
    setOllamaModels,
    setSoapError,
  };
}
