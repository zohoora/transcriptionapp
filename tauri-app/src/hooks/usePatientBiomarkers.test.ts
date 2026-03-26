import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { listen } from '@tauri-apps/api/event';
import type { BiomarkerUpdate, SpeakerBiomarkers } from '../types';

const mockListen = vi.mocked(listen);

// Capture event listeners
let listeners: Record<string, Function> = {};

beforeEach(() => {
  vi.clearAllMocks();
  listeners = {};
  mockListen.mockImplementation(async (event: string, handler: Function) => {
    listeners[event] = handler;
    return vi.fn();
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

function emitEvent(eventName: string, payload: unknown) {
  if (listeners[eventName]) {
    listeners[eventName]({ payload });
  }
}

function makeSpeaker(overrides: Partial<SpeakerBiomarkers> = {}): SpeakerBiomarkers {
  return {
    speaker_id: 'patient-1',
    vitality_mean: 100,
    stability_mean: 50,
    utterance_count: 5,
    talk_time_ms: 10000,
    turn_count: 3,
    mean_turn_duration_ms: 3333,
    median_turn_duration_ms: 3000,
    is_clinician: false,
    ...overrides,
  };
}

function makeBiomarkerUpdate(speakers: SpeakerBiomarkers[]): BiomarkerUpdate {
  return {
    cough_count: 0,
    cough_rate_per_min: 0,
    turn_count: 5,
    avg_turn_duration_ms: 3000,
    talk_time_ratio: 0.5,
    vitality_session_mean: null,
    stability_session_mean: null,
    speaker_metrics: speakers,
    recent_events: [],
    conversation_dynamics: null,
  };
}

describe('usePatientBiomarkers', () => {
  async function loadHook() {
    const { usePatientBiomarkers } = await import('./usePatientBiomarkers');
    return usePatientBiomarkers;
  }

  describe('initialization', () => {
    it('starts with null biomarkers and insufficient trends when inactive', async () => {
      const usePatientBiomarkers = await loadHook();
      const { result } = renderHook(() => usePatientBiomarkers(false));

      expect(result.current.biomarkers).toBeNull();
      expect(result.current.trends.vitalityTrend).toBe('insufficient');
      expect(result.current.trends.stabilityTrend).toBe('insufficient');
    });

    it('starts with null biomarkers when active', async () => {
      const usePatientBiomarkers = await loadHook();
      const { result } = renderHook(() => usePatientBiomarkers(true));

      expect(result.current.biomarkers).toBeNull();
      expect(result.current.trends.vitalityTrend).toBe('insufficient');
      expect(result.current.trends.stabilityTrend).toBe('insufficient');
    });
  });

  describe('biomarker listener', () => {
    it('subscribes to biomarker_update when active', async () => {
      const usePatientBiomarkers = await loadHook();
      renderHook(() => usePatientBiomarkers(true));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalledWith('biomarker_update', expect.any(Function));
      });
    });

    it('does not subscribe when inactive', async () => {
      const usePatientBiomarkers = await loadHook();
      renderHook(() => usePatientBiomarkers(false));

      // Should not listen to biomarker_update when inactive
      expect(listeners['biomarker_update']).toBeUndefined();
    });

    it('stores latest biomarker update', async () => {
      const usePatientBiomarkers = await loadHook();
      const { result } = renderHook(() => usePatientBiomarkers(true));

      await waitFor(() => {
        expect(listeners['biomarker_update']).toBeDefined();
      });

      const update = makeBiomarkerUpdate([makeSpeaker({ utterance_count: 1 })]);

      act(() => {
        emitEvent('biomarker_update', update);
      });

      expect(result.current.biomarkers).toEqual(update);
    });
  });

  describe('baseline capture', () => {
    it('remains insufficient below 3 utterances', async () => {
      const usePatientBiomarkers = await loadHook();
      const { result } = renderHook(() => usePatientBiomarkers(true));

      await waitFor(() => {
        expect(listeners['biomarker_update']).toBeDefined();
      });

      // Only 2 utterances -- below BASELINE_MIN_UTTERANCES (3)
      const update = makeBiomarkerUpdate([makeSpeaker({ utterance_count: 2 })]);
      act(() => {
        emitEvent('biomarker_update', update);
      });

      expect(result.current.trends.vitalityTrend).toBe('insufficient');
      expect(result.current.trends.stabilityTrend).toBe('insufficient');
    });

    it('captures baseline at 3+ utterances', async () => {
      const usePatientBiomarkers = await loadHook();
      const { result } = renderHook(() => usePatientBiomarkers(true));

      await waitFor(() => {
        expect(listeners['biomarker_update']).toBeDefined();
      });

      // 3 utterances -- reaches BASELINE_MIN_UTTERANCES
      const update = makeBiomarkerUpdate([
        makeSpeaker({ utterance_count: 3, vitality_mean: 100, stability_mean: 50 }),
      ]);
      act(() => {
        emitEvent('biomarker_update', update);
      });

      // After baseline capture, trends should be computed (stable since current == baseline)
      expect(result.current.trends.vitalityTrend).toBe('stable');
      expect(result.current.trends.stabilityTrend).toBe('stable');
    });
  });

  describe('trend computation', () => {
    async function setupWithBaseline(usePatientBiomarkers: ReturnType<typeof loadHook> extends Promise<infer T> ? T : never) {
      const { result } = renderHook(() => usePatientBiomarkers(true));

      await waitFor(() => {
        expect(listeners['biomarker_update']).toBeDefined();
      });

      // Establish baseline with vitality=100, stability=50
      const baseline = makeBiomarkerUpdate([
        makeSpeaker({ utterance_count: 3, vitality_mean: 100, stability_mean: 50 }),
      ]);
      act(() => {
        emitEvent('biomarker_update', baseline);
      });

      return result;
    }

    it('reports improving when value increases >15%', async () => {
      const usePatientBiomarkers = await loadHook();
      const result = await setupWithBaseline(usePatientBiomarkers);

      // Vitality up from 100 to 120 (+20% > 15% threshold)
      const update = makeBiomarkerUpdate([
        makeSpeaker({ utterance_count: 5, vitality_mean: 120, stability_mean: 50 }),
      ]);
      act(() => {
        emitEvent('biomarker_update', update);
      });

      expect(result.current.trends.vitalityTrend).toBe('improving');
      expect(result.current.trends.stabilityTrend).toBe('stable');
    });

    it('reports declining when value decreases >15%', async () => {
      const usePatientBiomarkers = await loadHook();
      const result = await setupWithBaseline(usePatientBiomarkers);

      // Stability down from 50 to 40 (-20% > 15% threshold)
      const update = makeBiomarkerUpdate([
        makeSpeaker({ utterance_count: 5, vitality_mean: 100, stability_mean: 40 }),
      ]);
      act(() => {
        emitEvent('biomarker_update', update);
      });

      expect(result.current.trends.vitalityTrend).toBe('stable');
      expect(result.current.trends.stabilityTrend).toBe('declining');
    });

    it('reports stable when within 15% threshold', async () => {
      const usePatientBiomarkers = await loadHook();
      const result = await setupWithBaseline(usePatientBiomarkers);

      // Small changes within 15%: vitality 100->110 (+10%), stability 50->45 (-10%)
      const update = makeBiomarkerUpdate([
        makeSpeaker({ utterance_count: 5, vitality_mean: 110, stability_mean: 45 }),
      ]);
      act(() => {
        emitEvent('biomarker_update', update);
      });

      expect(result.current.trends.vitalityTrend).toBe('stable');
      expect(result.current.trends.stabilityTrend).toBe('stable');
    });
  });

  describe('encounter boundary', () => {
    it('resets on encounter_detected event', async () => {
      const usePatientBiomarkers = await loadHook();
      const { result } = renderHook(() => usePatientBiomarkers(true));

      await waitFor(() => {
        expect(listeners['biomarker_update']).toBeDefined();
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      // Set up some biomarker data
      const update = makeBiomarkerUpdate([
        makeSpeaker({ utterance_count: 5, vitality_mean: 100, stability_mean: 50 }),
      ]);
      act(() => {
        emitEvent('biomarker_update', update);
      });
      expect(result.current.biomarkers).not.toBeNull();

      // Encounter detected should reset everything
      act(() => {
        emitEvent('continuous_mode_event', { type: 'encounter_detected' });
      });

      expect(result.current.biomarkers).toBeNull();
      expect(result.current.trends.vitalityTrend).toBe('insufficient');
      expect(result.current.trends.stabilityTrend).toBe('insufficient');
    });
  });

  describe('manual reset', () => {
    it('clears all state', async () => {
      const usePatientBiomarkers = await loadHook();
      const { result } = renderHook(() => usePatientBiomarkers(true));

      await waitFor(() => {
        expect(listeners['biomarker_update']).toBeDefined();
      });

      // Set up some data
      const update = makeBiomarkerUpdate([
        makeSpeaker({ utterance_count: 5, vitality_mean: 100, stability_mean: 50 }),
      ]);
      act(() => {
        emitEvent('biomarker_update', update);
      });
      expect(result.current.biomarkers).not.toBeNull();

      // Manual reset
      act(() => {
        result.current.reset();
      });

      expect(result.current.biomarkers).toBeNull();
      expect(result.current.trends.vitalityTrend).toBe('insufficient');
      expect(result.current.trends.stabilityTrend).toBe('insufficient');
    });
  });

  describe('deactivation', () => {
    it('resets when isActive becomes false', async () => {
      const usePatientBiomarkers = await loadHook();
      const { result, rerender } = renderHook(
        ({ isActive }) => usePatientBiomarkers(isActive),
        { initialProps: { isActive: true } }
      );

      await waitFor(() => {
        expect(listeners['biomarker_update']).toBeDefined();
      });

      // Set up some data
      const update = makeBiomarkerUpdate([
        makeSpeaker({ utterance_count: 5, vitality_mean: 100, stability_mean: 50 }),
      ]);
      act(() => {
        emitEvent('biomarker_update', update);
      });
      expect(result.current.biomarkers).not.toBeNull();

      // Deactivate
      rerender({ isActive: false });

      expect(result.current.biomarkers).toBeNull();
      expect(result.current.trends.vitalityTrend).toBe('insufficient');
      expect(result.current.trends.stabilityTrend).toBe('insufficient');
    });
  });
});
