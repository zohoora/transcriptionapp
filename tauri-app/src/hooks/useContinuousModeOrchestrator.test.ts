import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';

// Mock all sub-hooks before importing the orchestrator
const mockStart = vi.fn();
const mockStop = vi.fn();
const mockTriggerNewPatient = vi.fn();
const mockSetEncounterNotes = vi.fn();
const mockResetBiomarkers = vi.fn();
const mockMiisRecordImpression = vi.fn();
const mockMiisRecordClick = vi.fn();
const mockMiisRecordDismiss = vi.fn();
const mockMiisGetImageUrl = vi.fn((p: string) => `http://miis/${p}`);
const mockAiDismiss = vi.fn();

vi.mock('./useContinuousMode', () => ({
  useContinuousMode: vi.fn(() => ({
    isActive: false,
    isStopping: false,
    stats: {
      state: 'idle',
      recording_since: '',
      encounters_detected: 0,
      last_encounter_at: null,
      last_encounter_words: null,
      last_encounter_patient_name: null,
      last_error: null,
      buffer_word_count: 0,
      buffer_started_at: null,
    },
    liveTranscript: '',
    audioQuality: null,
    encounterNotes: '',
    setEncounterNotes: mockSetEncounterNotes,
    start: mockStart,
    stop: mockStop,
    triggerNewPatient: mockTriggerNewPatient,
    error: null,
  })),
}));

vi.mock('./usePatientBiomarkers', () => ({
  usePatientBiomarkers: vi.fn(() => ({
    biomarkers: null,
    trends: { vitalityTrend: 'insufficient', stabilityTrend: 'insufficient' },
    reset: mockResetBiomarkers,
  })),
}));

vi.mock('./usePredictiveHint', () => ({
  usePredictiveHint: vi.fn(() => ({
    hint: '',
    concepts: [],
    imagePrompt: null,
    isLoading: false,
  })),
}));

vi.mock('./useMiisImages', () => ({
  useMiisImages: vi.fn(() => ({
    suggestions: [],
    isLoading: false,
    error: null,
    recordImpression: mockMiisRecordImpression,
    recordClick: mockMiisRecordClick,
    recordDismiss: mockMiisRecordDismiss,
    getImageUrl: mockMiisGetImageUrl,
  })),
}));

vi.mock('./useAiImages', () => ({
  useAiImages: vi.fn(() => ({
    images: [],
    isLoading: false,
    error: null,
    dismissImage: mockAiDismiss,
  })),
}));

describe('useContinuousModeOrchestrator', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  async function loadHook() {
    const { useContinuousModeOrchestrator } = await import('./useContinuousModeOrchestrator');
    return useContinuousModeOrchestrator;
  }

  describe('delegation', () => {
    it('delegates isActive from useContinuousMode', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      expect(result.current.isActive).toBe(false);
    });

    it('delegates isStopping from useContinuousMode', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      expect(result.current.isStopping).toBe(false);
    });

    it('delegates stats from useContinuousMode', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      expect(result.current.stats.state).toBe('idle');
      expect(result.current.stats.encounters_detected).toBe(0);
    });

    it('delegates biomarkers from usePatientBiomarkers', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      expect(result.current.biomarkers).toBeNull();
      expect(result.current.biomarkerTrends.vitalityTrend).toBe('insufficient');
    });

    it('delegates predictiveHint from usePredictiveHint', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      expect(result.current.predictiveHint).toBe('');
      expect(result.current.predictiveHintLoading).toBe(false);
    });

    it('delegates MIIS suggestions from useMiisImages', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      expect(result.current.miisSuggestions).toEqual([]);
      expect(result.current.miisLoading).toBe(false);
      expect(result.current.miisError).toBeNull();
    });

    it('delegates AI images from useAiImages', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      expect(result.current.aiImages).toEqual([]);
      expect(result.current.aiLoading).toBe(false);
      expect(result.current.aiError).toBeNull();
    });

    it('delegates onStart from useContinuousMode start', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      result.current.onStart();
      expect(mockStart).toHaveBeenCalledTimes(1);
    });

    it('delegates onStop from useContinuousMode stop', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      result.current.onStop();
      expect(mockStop).toHaveBeenCalledTimes(1);
    });

    it('delegates onEncounterNotesChange from setEncounterNotes', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      result.current.onEncounterNotesChange('test notes');
      expect(mockSetEncounterNotes).toHaveBeenCalledWith('test notes');
    });

    it('delegates onMiisImpression, onMiisClick, onMiisDismiss', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      result.current.onMiisImpression(1);
      result.current.onMiisClick(2);
      result.current.onMiisDismiss(3);
      expect(mockMiisRecordImpression).toHaveBeenCalledWith(1);
      expect(mockMiisRecordClick).toHaveBeenCalledWith(2);
      expect(mockMiisRecordDismiss).toHaveBeenCalledWith(3);
    });

    it('delegates onAiDismiss', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      result.current.onAiDismiss(0);
      expect(mockAiDismiss).toHaveBeenCalledWith(0);
    });
  });

  describe('handleNewPatient', () => {
    it('calls resetBiomarkers then triggerNewPatient', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );

      await act(async () => {
        await result.current.onNewPatient();
      });

      expect(mockResetBiomarkers).toHaveBeenCalledTimes(1);
      expect(mockTriggerNewPatient).toHaveBeenCalledTimes(1);

      // resetBiomarkers should be called before triggerNewPatient
      const resetOrder = mockResetBiomarkers.mock.invocationCallOrder[0];
      const triggerOrder = mockTriggerNewPatient.mock.invocationCallOrder[0];
      expect(resetOrder).toBeLessThan(triggerOrder);
    });
  });

  describe('image source configuration', () => {
    it('sets miisEnabled to false when image_source is off', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: { image_source: 'off' } as never })
      );
      expect(result.current.miisEnabled).toBe(false);
      expect(result.current.imageSource).toBe('off');
    });

    it('sets miisEnabled to true when image_source is miis', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: { image_source: 'miis' } as never })
      );
      expect(result.current.miisEnabled).toBe(true);
      expect(result.current.imageSource).toBe('miis');
    });

    it('sets miisEnabled to true when image_source is ai', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: { image_source: 'ai' } as never })
      );
      expect(result.current.miisEnabled).toBe(true);
      expect(result.current.imageSource).toBe('ai');
    });

    it('defaults to off when settings is null', async () => {
      const useContinuousModeOrchestrator = await loadHook();
      const { result } = renderHook(() =>
        useContinuousModeOrchestrator({ settings: null })
      );
      expect(result.current.miisEnabled).toBe(false);
      expect(result.current.imageSource).toBe('off');
    });
  });

  describe('session ID per encounter', () => {
    it('passes useContinuousMode hooks to sub-hooks', async () => {
      const { useContinuousMode } = await import('./useContinuousMode');
      const { usePatientBiomarkers } = await import('./usePatientBiomarkers');
      const mockUseContinuousMode = vi.mocked(useContinuousMode);
      const mockUsePatientBiomarkers = vi.mocked(usePatientBiomarkers);

      const useContinuousModeOrchestrator = await loadHook();
      renderHook(() => useContinuousModeOrchestrator({ settings: null }));

      // useContinuousMode is called
      expect(mockUseContinuousMode).toHaveBeenCalled();
      // usePatientBiomarkers is called with isActive from useContinuousMode
      expect(mockUsePatientBiomarkers).toHaveBeenCalledWith(false);
    });
  });
});
