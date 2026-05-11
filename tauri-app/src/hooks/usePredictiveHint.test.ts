import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { usePredictiveHint } from './usePredictiveHint';

const mockInvoke = vi.mocked(invoke);

const TRANSCRIPT_A = Array(30).fill('patient').join(' '); // 30 words >= MIN_WORDS_FOR_HINT (20)
const TRANSCRIPT_B = Array(30).fill('chest').join(' ');

const HINT_RESPONSE_A = {
  hint: 'Consider chest pain workup',
  concepts: [{ text: 'chest pain', weight: 0.9 }],
  image_prompt: null,
  differential_diagnoses: [
    { diagnosis: 'MI', likelihood: 'likely', key_findings: ['chest pain'] },
  ],
};

const HINT_RESPONSE_B = {
  hint: 'Consider PE workup',
  concepts: [{ text: 'shortness of breath', weight: 0.8 }],
  image_prompt: null,
  differential_diagnoses: [
    { diagnosis: 'PE', likelihood: 'possible', key_findings: ['SOB'] },
  ],
};

describe('usePredictiveHint', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('resetKey', () => {
    it('clears hint, concepts, imagePrompt, and differentialDiagnoses when resetKey changes (while isRecording stays true)', async () => {
      vi.useFakeTimers();
      mockInvoke.mockResolvedValue(HINT_RESPONSE_A);

      const { result, rerender } = renderHook(
        ({ resetKey }: { resetKey: string }) =>
          usePredictiveHint({ transcript: TRANSCRIPT_A, isRecording: true, resetKey }),
        { initialProps: { resetKey: 'enc-1' } }
      );

      // Advance past INITIAL_DELAY_MS (5s) so the first hint fires.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(5100);
      });

      expect(result.current.hint).toBe('Consider chest pain workup');
      expect(result.current.differentialDiagnoses).toHaveLength(1);
      expect(result.current.concepts).toHaveLength(1);

      // Change resetKey while isRecording stays true.
      rerender({ resetKey: 'enc-2' });

      expect(result.current.hint).toBe('');
      expect(result.current.differentialDiagnoses).toEqual([]);
      expect(result.current.concepts).toEqual([]);
      expect(result.current.imagePrompt).toBeNull();
      expect(result.current.lastUpdated).toBeNull();
    });

    it('re-arms the 5s INITIAL_DELAY_MS when resetKey changes so the next hint fires fast', async () => {
      vi.useFakeTimers();
      mockInvoke.mockResolvedValue(HINT_RESPONSE_A);

      const { rerender } = renderHook(
        ({ resetKey }: { resetKey: string }) =>
          usePredictiveHint({ transcript: TRANSCRIPT_A, isRecording: true, resetKey }),
        { initialProps: { resetKey: 'enc-1' } }
      );

      // First hint after initial 5s delay.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(5100);
      });
      expect(mockInvoke).toHaveBeenCalledTimes(1);

      // Reset and switch the response so we can detect the new fire.
      mockInvoke.mockResolvedValue(HINT_RESPONSE_B);
      rerender({ resetKey: 'enc-2' });

      // Within 5s after reset, a NEW initial-delay timer should fire.
      await act(async () => {
        await vi.advanceTimersByTimeAsync(5100);
      });

      expect(mockInvoke).toHaveBeenCalledTimes(2);
    });

    it('without resetKey, behavior matches the existing isRecording-only contract', async () => {
      vi.useFakeTimers();
      mockInvoke.mockResolvedValue(HINT_RESPONSE_A);

      const { result, rerender } = renderHook(
        ({ recording }: { recording: boolean }) =>
          usePredictiveHint({ transcript: TRANSCRIPT_A, isRecording: recording }),
        { initialProps: { recording: true } }
      );

      await act(async () => {
        await vi.advanceTimersByTimeAsync(5100);
      });
      expect(result.current.hint).toBe('Consider chest pain workup');

      // Stopping recording is the only way state clears in the legacy contract.
      rerender({ recording: false });
      expect(result.current.hint).toBe('');
      expect(result.current.differentialDiagnoses).toEqual([]);
    });
  });

  describe('legacy isRecording behavior', () => {
    it('clears state when isRecording goes false', async () => {
      vi.useFakeTimers();
      mockInvoke.mockResolvedValue(HINT_RESPONSE_A);

      const { result, rerender } = renderHook(
        ({ recording }: { recording: boolean }) =>
          usePredictiveHint({ transcript: TRANSCRIPT_A, isRecording: recording }),
        { initialProps: { recording: true } }
      );

      await act(async () => {
        await vi.advanceTimersByTimeAsync(5100);
      });
      expect(result.current.hint).not.toBe('');

      rerender({ recording: false });
      expect(result.current.hint).toBe('');
    });

    it('does not call invoke when transcript has fewer than 20 words', async () => {
      vi.useFakeTimers();
      mockInvoke.mockResolvedValue(HINT_RESPONSE_A);

      renderHook(() =>
        usePredictiveHint({ transcript: 'just a few words', isRecording: true })
      );

      await act(async () => {
        await vi.advanceTimersByTimeAsync(35000);
      });
      expect(mockInvoke).not.toHaveBeenCalled();
    });
  });
});
