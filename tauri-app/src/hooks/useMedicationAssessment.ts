import { useState, useCallback, useRef, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { formatErrorMessage } from '../utils';
import type {
  MedEntry,
  MedExtractionResult,
  AnalysisResult,
  Settings,
} from '../types';

export type ExtractionState =
  | 'idle'
  | 'capturing'
  | 'extracted'
  | 'failed'
  | 'permission_denied';

export interface UseMedicationAssessmentResult {
  medList: MedEntry[];
  extractionState: ExtractionState;
  extractionError: string | null;
  analysis: AnalysisResult | null;
  isAnalyzing: boolean;
  analyzeError: string | null;

  addRow: () => void;
  updateRow: (index: number, patch: Partial<MedEntry>) => void;
  deleteRow: (index: number) => void;

  extract: () => Promise<void>;
  analyze: () => Promise<void>;
}

/**
 * Owns the Medication Assessment tab state:
 *
 *  - `extract()` captures a screenshot, runs vision, and prefills the med list.
 *  - The list is freely editable. Each edit invalidates the cached analysis.
 *  - `analyze()` POSTs to the pharm-refactor service and surfaces the cards.
 *
 * The hook reads `pharm_service_url` from settings itself rather than taking
 * it as a prop, so the caller doesn't have to plumb a service URL through.
 */
export function useMedicationAssessment(): UseMedicationAssessmentResult {
  const [medList, setMedList] = useState<MedEntry[]>([]);
  const [extractionState, setExtractionState] = useState<ExtractionState>('idle');
  const [extractionError, setExtractionError] = useState<string | null>(null);
  const [analysis, setAnalysis] = useState<AnalysisResult | null>(null);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [analyzeError, setAnalyzeError] = useState<string | null>(null);

  const extractingRef = useRef(false);
  const analyzingRef = useRef(false);

  const medListRef = useRef<MedEntry[]>(medList);
  medListRef.current = medList;

  const mountedRef = useRef(true);
  useEffect(() => {
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // Skip the setter when the value isn't actually changing — avoids waking
  // every component subscribed to `analysis` on each keystroke.
  const clearAnalysisIfPresent = useCallback(() => {
    setAnalysis((prev) => (prev === null ? prev : null));
    setAnalyzeError((prev) => (prev === null ? prev : null));
  }, []);

  const extract = useCallback(async () => {
    if (extractingRef.current) return;
    extractingRef.current = true;
    if (mountedRef.current) {
      setExtractionState('capturing');
      setExtractionError(null);
    }
    try {
      const result = await invoke<MedExtractionResult>('capture_screenshot_for_meds');
      if (!mountedRef.current) return;
      if (result.likelyBlank) {
        setExtractionState('permission_denied');
        return;
      }
      setMedList(result.medications);
      setExtractionState('extracted');
      clearAnalysisIfPresent();
    } catch (e) {
      if (!mountedRef.current) return;
      setExtractionError(formatErrorMessage(e));
      setExtractionState('failed');
    } finally {
      extractingRef.current = false;
    }
  }, [clearAnalysisIfPresent]);

  const analyze = useCallback(async () => {
    if (analyzingRef.current) return;
    const meds = medListRef.current;
    if (meds.length === 0) {
      setAnalyzeError('Add at least one medication before analyzing.');
      return;
    }
    let pharmServiceUrl: string;
    try {
      const settings = await invoke<Settings>('get_settings');
      pharmServiceUrl = settings.pharm_service_url;
    } catch (e) {
      setAnalyzeError(`Couldn't load settings: ${formatErrorMessage(e)}`);
      return;
    }
    if (!pharmServiceUrl || !pharmServiceUrl.trim()) {
      setAnalyzeError('Pharmacotherapy service URL is not configured.');
      return;
    }

    analyzingRef.current = true;
    if (mountedRef.current) {
      setIsAnalyzing(true);
      setAnalyzeError(null);
    }
    try {
      const result = await invoke<AnalysisResult>('analyze_medications', {
        pharmServiceUrl,
        medications: meds,
        patientAge: null,
        patientEgfr: null,
        context: null,
      });
      if (!mountedRef.current) return;
      setAnalysis(result);
    } catch (e) {
      if (!mountedRef.current) return;
      setAnalyzeError(formatErrorMessage(e));
    } finally {
      analyzingRef.current = false;
      if (mountedRef.current) setIsAnalyzing(false);
    }
  }, []);

  const addRow = useCallback(() => {
    setMedList((prev) => [...prev, { name: '' }]);
  }, []);

  const updateRow = useCallback(
    (index: number, patch: Partial<MedEntry>) => {
      setMedList((prev) =>
        prev.map((m, i) => {
          if (i !== index) return m;
          const next: MedEntry = { ...m, ...patch };
          if (patch.dose !== undefined && patch.dose === '') delete next.dose;
          if (patch.frequency !== undefined && patch.frequency === '') delete next.frequency;
          return next;
        })
      );
      clearAnalysisIfPresent();
    },
    [clearAnalysisIfPresent]
  );

  const deleteRow = useCallback(
    (index: number) => {
      setMedList((prev) => prev.filter((_, i) => i !== index));
      clearAnalysisIfPresent();
    },
    [clearAnalysisIfPresent]
  );

  return {
    medList,
    extractionState,
    extractionError,
    analysis,
    isAnalyzing,
    analyzeError,
    addRow,
    updateRow,
    deleteRow,
    extract,
    analyze,
  };
}
