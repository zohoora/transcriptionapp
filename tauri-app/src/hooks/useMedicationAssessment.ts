import { useState, useCallback, useRef, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { formatErrorMessage } from '../utils';
import type {
  MedEntry,
  MedExtractionResult,
  AnalysisResult,
  AnalysisStrategy,
  PatientConditions,
  ClarifyingQuestion,
  QuestionsResponse,
  AIPlanResponse,
  Settings,
} from '../types';

export type ExtractionState =
  | 'idle'
  | 'capturing'
  | 'extracted'
  | 'failed'
  | 'permission_denied';

const EMPTY_CONDITIONS: PatientConditions = {
  ckd: false,
  hepatic: false,
  falls_risk: false,
  dementia: false,
  diabetes: false,
  afib: false,
  heart_failure: false,
  osteoporosis: false,
};

export interface UseMedicationAssessmentResult {
  // Med extraction / list
  medList: MedEntry[];
  extractionState: ExtractionState;
  extractionError: string | null;
  isParsing: boolean;
  parseError: string | null;

  // Analysis
  analysis: AnalysisResult | null;
  isAnalyzing: boolean;
  analyzeError: string | null;

  // Patient context
  patientAge: number | null;
  patientEgfr: number | null;
  patientConditions: PatientConditions;
  strategy: AnalysisStrategy;

  // Plan flow
  clarifyingQuestions: ClarifyingQuestion[];
  questionAnswers: Record<string, string>;
  isLoadingQuestions: boolean;
  questionsError: string | null;
  aiPlan: AIPlanResponse | null;
  isGeneratingPlan: boolean;
  planError: string | null;

  // Actions
  addRow: () => void;
  updateRow: (index: number, patch: Partial<MedEntry>) => void;
  deleteRow: (index: number) => void;
  extract: () => Promise<void>;
  parseTypedMeds: (text: string) => Promise<boolean>;
  analyze: () => Promise<void>;

  setPatientAge: (age: number | null) => void;
  setPatientEgfr: (egfr: number | null) => void;
  setPatientCondition: (key: keyof PatientConditions, value: boolean) => void;
  setStrategy: (strategy: AnalysisStrategy) => void;

  startPlanFlow: () => Promise<void>;
  setQuestionAnswer: (questionId: string, answer: string) => void;
  submitAnswersAndGeneratePlan: () => Promise<void>;
  skipQuestionsAndGeneratePlan: () => Promise<void>;
  dismissPlan: () => void;
}

export function useMedicationAssessment(): UseMedicationAssessmentResult {
  const [medList, setMedList] = useState<MedEntry[]>([]);
  const [extractionState, setExtractionState] = useState<ExtractionState>('idle');
  const [extractionError, setExtractionError] = useState<string | null>(null);
  const [analysis, setAnalysis] = useState<AnalysisResult | null>(null);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [analyzeError, setAnalyzeError] = useState<string | null>(null);

  const [isParsing, setIsParsing] = useState(false);
  const [parseError, setParseError] = useState<string | null>(null);

  const [patientAge, setPatientAgeState] = useState<number | null>(null);
  const [patientEgfr, setPatientEgfrState] = useState<number | null>(null);
  const [patientConditions, setPatientConditions] = useState<PatientConditions>(EMPTY_CONDITIONS);
  const [strategy, setStrategyState] = useState<AnalysisStrategy>('safety_first');

  const [clarifyingQuestions, setClarifyingQuestions] = useState<ClarifyingQuestion[]>([]);
  const [questionAnswers, setQuestionAnswers] = useState<Record<string, string>>({});
  const [isLoadingQuestions, setIsLoadingQuestions] = useState(false);
  const [questionsError, setQuestionsError] = useState<string | null>(null);
  const [aiPlan, setAiPlan] = useState<AIPlanResponse | null>(null);
  const [isGeneratingPlan, setIsGeneratingPlan] = useState(false);
  const [planError, setPlanError] = useState<string | null>(null);

  const extractingRef = useRef(false);
  const analyzingRef = useRef(false);
  const parsingRef = useRef(false);
  const loadingQuestionsRef = useRef(false);
  const generatingPlanRef = useRef(false);
  // Flipped by every explicit medList mutation. A late-arriving screenshot
  // extraction reads this to decide whether to apply its result — clinician
  // edits always win over speculative vision output.
  const userHasEditedRef = useRef(false);

  const medListRef = useRef<MedEntry[]>(medList);
  medListRef.current = medList;
  // questionAnswersRef is read at click time by submitAnswersAndGeneratePlan;
  // can't be derived from the closure without a useCallback dep that would
  // re-allocate on every keystroke.
  const questionAnswersRef = useRef<Record<string, string>>(questionAnswers);
  questionAnswersRef.current = questionAnswers;

  const mountedRef = useRef(true);
  useEffect(() => {
    return () => {
      mountedRef.current = false;
    };
  }, []);

  // Same-reference no-op guard on every setter avoids re-rendering all
  // subscribers of `analysis` / `aiPlan` / `clarifyingQuestions` on each
  // keystroke. A plan computed off stale meds/context is just as wrong as
  // a stale analysis, so both are cleared together.
  const clearCachedResults = useCallback(() => {
    setAnalysis((prev) => (prev === null ? prev : null));
    setAnalyzeError((prev) => (prev === null ? prev : null));
    setAiPlan((prev) => (prev === null ? prev : null));
    setPlanError((prev) => (prev === null ? prev : null));
    setClarifyingQuestions((prev) => (prev.length === 0 ? prev : []));
    setQuestionAnswers((prev) => (Object.keys(prev).length === 0 ? prev : {}));
    setQuestionsError((prev) => (prev === null ? prev : null));
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
      if (!userHasEditedRef.current) {
        setMedList(result.medications);
      }
      setExtractionState('extracted');
      clearCachedResults();
    } catch (e) {
      if (!mountedRef.current) return;
      setExtractionError(formatErrorMessage(e));
      setExtractionState('failed');
    } finally {
      extractingRef.current = false;
    }
  }, [clearCachedResults]);

  const parseTypedMeds = useCallback(
    async (text: string): Promise<boolean> => {
      if (parsingRef.current) return false;
      const trimmed = text.trim();
      if (!trimmed) {
        setParseError('Type a medication list first.');
        return false;
      }
      parsingRef.current = true;
      userHasEditedRef.current = true;
      if (mountedRef.current) {
        setIsParsing(true);
        setParseError(null);
      }
      try {
        const result = await invoke<MedEntry[]>('parse_medications_from_text', {
          text: trimmed,
          currentMedications: medListRef.current,
        });
        if (!mountedRef.current) return false;
        setMedList(result);
        clearCachedResults();
        return true;
      } catch (e) {
        if (mountedRef.current) setParseError(formatErrorMessage(e));
        return false;
      } finally {
        parsingRef.current = false;
        if (mountedRef.current) setIsParsing(false);
      }
    },
    [clearCachedResults]
  );

  // Resolves the pharm service URL from settings; returns null + sets the
  // given error setter on failure so callers can short-circuit cleanly.
  const resolvePharmServiceUrl = useCallback(
    async (setErr: (msg: string) => void): Promise<string | null> => {
      let url: string;
      try {
        const settings = await invoke<Settings>('get_settings');
        url = settings.pharm_service_url;
      } catch (e) {
        setErr(`Couldn't load settings: ${formatErrorMessage(e)}`);
        return null;
      }
      if (!url || !url.trim()) {
        setErr('Pharmacotherapy service URL is not configured.');
        return null;
      }
      return url;
    },
    []
  );

  // Closure over current patient-context state — passed to every pharm
  // call. React rebinds this on each render but the cost is negligible
  // versus the alternative of mirroring four state values into refs.
  const buildContextPayload = useCallback(
    () => ({
      patientAge,
      patientEgfr,
      context: patientConditions,
      strategy,
    }),
    [patientAge, patientEgfr, patientConditions, strategy]
  );

  const analyze = useCallback(async () => {
    if (analyzingRef.current) return;
    const meds = medListRef.current;
    if (meds.length === 0) {
      setAnalyzeError('Add at least one medication before analyzing.');
      return;
    }
    const pharmServiceUrl = await resolvePharmServiceUrl(setAnalyzeError);
    if (!pharmServiceUrl) return;

    analyzingRef.current = true;
    if (mountedRef.current) {
      setIsAnalyzing(true);
      setAnalyzeError(null);
    }
    try {
      const result = await invoke<AnalysisResult>('analyze_medications', {
        pharmServiceUrl,
        medications: meds,
        ...buildContextPayload(),
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
  }, [resolvePharmServiceUrl, buildContextPayload]);

  const generatePlan = useCallback(
    async (answers: Record<string, string> | null) => {
      if (generatingPlanRef.current) return;
      const meds = medListRef.current;
      if (meds.length === 0) {
        setPlanError('Add at least one medication before generating a plan.');
        return;
      }
      const pharmServiceUrl = await resolvePharmServiceUrl(setPlanError);
      if (!pharmServiceUrl) return;

      generatingPlanRef.current = true;
      if (mountedRef.current) {
        setIsGeneratingPlan(true);
        setPlanError(null);
      }
      try {
        const result = await invoke<AIPlanResponse>('generate_plan_with_answers', {
          pharmServiceUrl,
          medications: meds,
          ...buildContextPayload(),
          answers,
        });
        if (!mountedRef.current) return;
        if (result.success) {
          setAiPlan(result);
        } else {
          setPlanError(result.error ?? 'Plan generation failed.');
        }
      } catch (e) {
        if (!mountedRef.current) return;
        setPlanError(formatErrorMessage(e));
      } finally {
        generatingPlanRef.current = false;
        if (mountedRef.current) setIsGeneratingPlan(false);
      }
    },
    [resolvePharmServiceUrl, buildContextPayload]
  );

  const startPlanFlow = useCallback(async () => {
    if (loadingQuestionsRef.current || generatingPlanRef.current) return;
    const meds = medListRef.current;
    if (meds.length === 0) {
      setQuestionsError('Add at least one medication before generating a plan.');
      return;
    }
    const pharmServiceUrl = await resolvePharmServiceUrl(setQuestionsError);
    if (!pharmServiceUrl) return;

    loadingQuestionsRef.current = true;
    if (mountedRef.current) {
      setIsLoadingQuestions(true);
      setQuestionsError(null);
      setPlanError(null);
      setQuestionAnswers({});
    }
    try {
      const result = await invoke<QuestionsResponse>('get_plan_clarifying_questions', {
        pharmServiceUrl,
        medications: meds,
        ...buildContextPayload(),
      });
      if (!mountedRef.current) return;
      if (!result.success) {
        setQuestionsError(result.error ?? 'Failed to fetch clarifying questions.');
        return;
      }
      if (result.questions.length === 0) {
        setClarifyingQuestions([]);
        // Fire-and-forget — UI already shows "Generating plan..." once
        // isGeneratingPlan flips inside generatePlan().
        void generatePlan(null);
        return;
      }
      setClarifyingQuestions(result.questions);
    } catch (e) {
      if (!mountedRef.current) return;
      setQuestionsError(formatErrorMessage(e));
    } finally {
      loadingQuestionsRef.current = false;
      if (mountedRef.current) setIsLoadingQuestions(false);
    }
  }, [resolvePharmServiceUrl, buildContextPayload, generatePlan]);

  const setQuestionAnswer = useCallback((questionId: string, answer: string) => {
    setQuestionAnswers((prev) => ({ ...prev, [questionId]: answer }));
  }, []);

  const submitAnswersAndGeneratePlan = useCallback(async () => {
    // Answers may be partial — pharm service tolerates missing keys.
    await generatePlan({ ...questionAnswersRef.current });
  }, [generatePlan]);

  const skipQuestionsAndGeneratePlan = useCallback(async () => {
    setQuestionAnswers((prev) => (Object.keys(prev).length === 0 ? prev : {}));
    await generatePlan(null);
  }, [generatePlan]);

  const dismissPlan = useCallback(() => {
    clearCachedResults();
    // clearCachedResults already nulls aiPlan/planError/clarifyingQuestions/
    // questionAnswers/questionsError; nothing extra to clear here.
  }, [clearCachedResults]);

  const setPatientAge = useCallback(
    (age: number | null) => {
      setPatientAgeState(age);
      clearCachedResults();
    },
    [clearCachedResults]
  );

  const setPatientEgfr = useCallback(
    (egfr: number | null) => {
      setPatientEgfrState(egfr);
      clearCachedResults();
    },
    [clearCachedResults]
  );

  const setPatientCondition = useCallback(
    (key: keyof PatientConditions, value: boolean) => {
      setPatientConditions((prev) => ({ ...prev, [key]: value }));
      clearCachedResults();
    },
    [clearCachedResults]
  );

  const setStrategy = useCallback(
    (next: AnalysisStrategy) => {
      setStrategyState(next);
      clearCachedResults();
    },
    [clearCachedResults]
  );

  const addRow = useCallback(() => {
    userHasEditedRef.current = true;
    setMedList((prev) => [...prev, { name: '' }]);
  }, []);

  const updateRow = useCallback(
    (index: number, patch: Partial<MedEntry>) => {
      userHasEditedRef.current = true;
      setMedList((prev) =>
        prev.map((m, i) => {
          if (i !== index) return m;
          const next: MedEntry = { ...m, ...patch };
          if (patch.dose !== undefined && patch.dose === '') delete next.dose;
          if (patch.frequency !== undefined && patch.frequency === '') delete next.frequency;
          return next;
        })
      );
      clearCachedResults();
    },
    [clearCachedResults]
  );

  const deleteRow = useCallback(
    (index: number) => {
      userHasEditedRef.current = true;
      setMedList((prev) => prev.filter((_, i) => i !== index));
      clearCachedResults();
    },
    [clearCachedResults]
  );

  return {
    medList,
    extractionState,
    extractionError,
    analysis,
    isAnalyzing,
    analyzeError,
    isParsing,
    parseError,

    patientAge,
    patientEgfr,
    patientConditions,
    strategy,

    clarifyingQuestions,
    questionAnswers,
    isLoadingQuestions,
    questionsError,
    aiPlan,
    isGeneratingPlan,
    planError,

    addRow,
    updateRow,
    deleteRow,
    extract,
    parseTypedMeds,
    analyze,

    setPatientAge,
    setPatientEgfr,
    setPatientCondition,
    setStrategy,

    startPlanFlow,
    setQuestionAnswer,
    submitAnswersAndGeneratePlan,
    skipQuestionsAndGeneratePlan,
    dismissPlan,
  };
}
