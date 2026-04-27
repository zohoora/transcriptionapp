/**
 * useSoapNote Hook
 *
 * Manages SOAP note generation via Ollama LLM, including multi-patient support.
 *
 * ## Multi-Patient SOAP Generation
 *
 * The hook supports automatic detection of multiple patients (up to 4) in a single
 * recording session. The LLM analyzes the transcript to:
 *
 * 1. Identify the physician (asks questions, examines, diagnoses)
 * 2. Identify patients (describe symptoms, answer questions)
 * 3. Generate separate SOAP notes for each patient
 *
 * No manual speaker mapping is required - the LLM determines roles from context.
 *
 * ## Usage
 *
 * ```typescript
 * const { generateSoapNote, isGeneratingSoap, soapError } = useSoapNote();
 *
 * // Generate multi-patient SOAP notes
 * const result = await generateSoapNote(transcript, audioEvents);
 *
 * // Result contains:
 * // - notes: PatientSoapNote[] (1-4 patients)
 * // - physician_speaker: string | null (e.g., "Speaker 2")
 * // - generated_at: string
 * // - model_used: string
 * ```
 *
 * @see MultiPatientSoapResult for the return type structure
 * @see ADR-0012 for architecture decisions
 */
import { useState, useCallback, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import type { CoughEvent, OllamaStatus, SoapNote, SoapOptions, SoapFormat, MultiPatientSoapResult, Settings } from '../types';
import { DEFAULT_SOAP_OPTIONS } from '../types';
import { clamp, formatErrorMessage } from '../utils';

export interface UseSoapNoteResult {
  isGeneratingSoap: boolean;
  soapError: string | null;
  ollamaStatus: OllamaStatus | null;
  ollamaModels: string[];
  soapOptions: SoapOptions;
  /** Generate multi-patient SOAP notes with auto-detection */
  generateSoapNote: (transcript: string, audioEvents?: CoughEvent[], options?: SoapOptions, sessionId?: string, modelOverride?: string) => Promise<MultiPatientSoapResult | null>;
  /** Legacy: Generate single-patient SOAP note (for backward compatibility) */
  generateSingleSoapNote: (transcript: string, audioEvents?: CoughEvent[], options?: SoapOptions, sessionId?: string) => Promise<SoapNote | null>;
  /** Experimental: Generate vision SOAP note using transcript + screenshots */
  generateVisionSoapNote: (transcript: string, audioEvents?: CoughEvent[], options?: SoapOptions, sessionId?: string, imagePath?: string) => Promise<SoapNote | null>;
  setOllamaStatus: (status: OllamaStatus | null) => void;
  setOllamaModels: (models: string[]) => void;
  setSoapError: (error: string | null) => void;
  setSoapOptions: (options: SoapOptions) => void;
  updateSoapDetailLevel: (level: number) => void;
  updateSoapFormat: (format: SoapFormat) => void;
  updateSoapCustomInstructions: (instructions: string) => void;
  updateSessionCustomInstructions: (instructions: string) => void;
}

export function useSoapNote(): UseSoapNoteResult {
  const [isGeneratingSoap, setIsGeneratingSoap] = useState(false);
  const [soapError, setSoapError] = useState<string | null>(null);
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus | null>(null);
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);
  const [soapOptions, setSoapOptions] = useState<SoapOptions>(DEFAULT_SOAP_OPTIONS);

  // Synchronous guard to prevent concurrent generation calls.
  // useState is async so checking isGeneratingSoap doesn't prevent double-clicks.
  const generationInFlight = useRef(false);

  // Load initial SOAP options from settings on mount
  useEffect(() => {
    const loadSoapSettings = async () => {
      try {
        const settings = await invoke<Settings>('get_settings');
        setSoapOptions({
          detail_level: settings.soap_detail_level ?? DEFAULT_SOAP_OPTIONS.detail_level,
          format: settings.soap_format ?? DEFAULT_SOAP_OPTIONS.format,
          custom_instructions: settings.soap_custom_instructions ?? DEFAULT_SOAP_OPTIONS.custom_instructions,
          session_custom_instructions: '',
        });
      } catch (e) {
        console.warn('Failed to load SOAP settings, using defaults:', e);
      }
    };
    loadSoapSettings();
  }, []);

  // Persist SOAP options to settings (detail_level and format only —
  // custom_instructions is managed by SettingsDrawer, not ReviewMode)
  const persistSoapOptions = useCallback(async (options: SoapOptions) => {
    try {
      const currentSettings = await invoke<Settings>('get_settings');
      await invoke('set_settings', {
        settings: {
          ...currentSettings,
          soap_detail_level: options.detail_level,
          soap_format: options.format,
        },
      });
    } catch (e) {
      console.warn('Failed to persist SOAP settings:', e);
    }
  }, []);

  // Update individual SOAP options
  const updateSoapDetailLevel = useCallback((level: number) => {
    setSoapOptions(prev => ({ ...prev, detail_level: clamp(level, 1, 10) }));
  }, []);

  const updateSoapFormat = useCallback((format: SoapFormat) => {
    setSoapOptions(prev => ({ ...prev, format }));
  }, []);

  const updateSoapCustomInstructions = useCallback((instructions: string) => {
    setSoapOptions(prev => ({ ...prev, custom_instructions: instructions }));
  }, []);

  const updateSessionCustomInstructions = useCallback((instructions: string) => {
    setSoapOptions(prev => ({ ...prev, session_custom_instructions: instructions }));
  }, []);

  // Shared guard/loading/error wrapper for all SOAP generation methods.
  // Prevents concurrent calls, manages loading state, and handles errors.
  async function withSoapGuard<T>(
    transcript: string,
    generate: () => Promise<T>,
  ): Promise<T | null> {
    if (!transcript.trim()) return null;
    if (generationInFlight.current) {
      console.warn('SOAP generation already in progress, skipping duplicate call');
      return null;
    }

    generationInFlight.current = true;
    setIsGeneratingSoap(true);
    setSoapError(null);

    try {
      return await generate();
    } catch (e) {
      console.error('Failed to generate SOAP note:', e);
      setSoapError(formatErrorMessage(e));
      return null;
    } finally {
      generationInFlight.current = false;
      setIsGeneratingSoap(false);
    }
  }

  // Auto-copy SOAP text to clipboard (best-effort, non-blocking on failure)
  async function copyToClipboard(text: string): Promise<void> {
    try {
      await writeText(text);
      console.log('SOAP note copied to clipboard');
    } catch (clipErr) {
      console.warn('Failed to copy SOAP note to clipboard:', clipErr);
    }
  }

  // Generate multi-patient SOAP notes with auto-detection
  const generateSoapNote = useCallback(async (
    transcript: string,
    audioEvents?: CoughEvent[],
    options?: SoapOptions,
    sessionId?: string,
    modelOverride?: string
  ): Promise<MultiPatientSoapResult | null> => {
    return withSoapGuard(transcript, async () => {
      const finalOptions = options || soapOptions;

      const result = await invoke<MultiPatientSoapResult>('generate_soap_note_auto_detect', {
        transcript,
        audioEvents,
        options: finalOptions,
        sessionId: sessionId || null,
        modelOverride: modelOverride || null,
      });

      if (result && result.notes.length > 0) {
        const clipboardContent = result.notes
          .map(note => note.content)
          .join('\n\n---\n\n');
        await copyToClipboard(clipboardContent);
        await persistSoapOptions(finalOptions);
      }

      return result;
    });
  }, [soapOptions, persistSoapOptions]);

  // Legacy single-patient SOAP note generation (for backward compatibility)
  const generateSingleSoapNote = useCallback(async (
    transcript: string,
    audioEvents?: CoughEvent[],
    options?: SoapOptions,
    sessionId?: string
  ): Promise<SoapNote | null> => {
    return withSoapGuard(transcript, async () => {
      const finalOptions = options || soapOptions;

      const result = await invoke<SoapNote>('generate_soap_note', {
        transcript,
        audioEvents,
        options: finalOptions,
        sessionId: sessionId || null,
      });

      if (result && result.content) {
        await copyToClipboard(result.content);
      }

      return result;
    });
  }, [soapOptions]);

  // Vision SOAP note generation (experimental — uses transcript + screenshots)
  const generateVisionSoapNote = useCallback(async (
    transcript: string,
    audioEvents?: CoughEvent[],
    options?: SoapOptions,
    sessionId?: string,
    imagePath?: string
  ): Promise<SoapNote | null> => {
    return withSoapGuard(transcript, async () => {
      const finalOptions = options || soapOptions;

      const result = await invoke<SoapNote>('generate_vision_soap_note', {
        transcript,
        audioEvents,
        options: finalOptions,
        sessionId: sessionId || null,
        imagePath: imagePath || null,
      });

      if (result && result.content) {
        await copyToClipboard(result.content);
      }

      return result;
    });
  }, [soapOptions]);

  return {
    isGeneratingSoap,
    soapError,
    ollamaStatus,
    ollamaModels,
    soapOptions,
    generateSoapNote,
    generateSingleSoapNote,
    generateVisionSoapNote,
    setOllamaStatus,
    setOllamaModels,
    setSoapError,
    setSoapOptions,
    updateSoapDetailLevel,
    updateSoapFormat,
    updateSoapCustomInstructions,
    updateSessionCustomInstructions,
  };
}
