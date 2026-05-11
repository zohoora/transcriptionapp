import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { useMiisImages } from './useMiisImages';

const mockInvoke = vi.mocked(invoke);

const CONCEPTS = [{ text: 'knee anatomy', weight: 0.9 }];

const SUGGEST_RESPONSE = {
  suggestions: [
    {
      image_id: 1,
      score: 0.9,
      sha256: 'abc',
      title: 'Knee',
      description: null,
      thumb_url: '/thumb/1.png',
      display_url: '/display/1.png',
    },
  ],
  suggestion_set_id: 'set-1',
};

describe('useMiisImages', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
  });

  describe('session-change reset', () => {
    it('clears suggestions when sessionId changes from one value to another', async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'miis_suggest') return Promise.resolve(SUGGEST_RESPONSE);
        return Promise.resolve(undefined);
      });

      const { result, rerender } = renderHook(
        ({ sessionId }: { sessionId: string | null }) =>
          useMiisImages({
            sessionId,
            concepts: CONCEPTS,
            enabled: true,
            serverUrl: 'http://miis.local',
          }),
        { initialProps: { sessionId: 'enc-1' as string | null } }
      );

      // Wait for the fetch effect to populate suggestions for enc-1.
      await waitFor(() => expect(result.current.suggestions).toHaveLength(1));

      // Switch to a new encounter session ID — suggestions should clear.
      rerender({ sessionId: 'enc-2' });

      // The clear should be synchronous (state-only) — no waitFor needed.
      expect(result.current.suggestions).toEqual([]);
      expect(result.current.suggestionSetId).toBeNull();
    });

    it('still clears when sessionId is set to null (regression guard)', async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'miis_suggest') return Promise.resolve(SUGGEST_RESPONSE);
        return Promise.resolve(undefined);
      });

      const { result, rerender } = renderHook(
        ({ sessionId }: { sessionId: string | null }) =>
          useMiisImages({
            sessionId,
            concepts: CONCEPTS,
            enabled: true,
            serverUrl: 'http://miis.local',
          }),
        { initialProps: { sessionId: 'enc-1' as string | null } }
      );

      await waitFor(() => expect(result.current.suggestions).toHaveLength(1));

      rerender({ sessionId: null });

      expect(result.current.suggestions).toEqual([]);
      expect(result.current.suggestionSetId).toBeNull();
    });
  });
});
