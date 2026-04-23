import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

// Capture event listeners registered by the hook
let listeners: Record<string, Function> = {};
let unlistenFns: Record<string, ReturnType<typeof vi.fn>> = {};

beforeEach(() => {
  vi.clearAllMocks();
  listeners = {};
  unlistenFns = {};
  mockListen.mockImplementation(async (event: string, handler: Function) => {
    listeners[event] = handler;
    const unlisten = vi.fn();
    unlistenFns[event] = unlisten;
    return unlisten;
  });
  mockInvoke.mockResolvedValue(undefined);
});

afterEach(() => {
  vi.restoreAllMocks();
});

// Helper to simulate a backend event
function emitEvent(eventName: string, payload: unknown) {
  if (listeners[eventName]) {
    listeners[eventName]({ payload });
  }
}

describe('useContinuousMode', () => {
  // Lazy import to avoid hoisting issues
  async function loadHook() {
    const { useContinuousMode } = await import('./useContinuousMode');
    return useContinuousMode;
  }

  describe('initial state', () => {
    it('starts with inactive state and idle stats', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      expect(result.current.isActive).toBe(false);
      expect(result.current.isStopping).toBe(false);
      expect(result.current.stats.state).toBe('idle');
      expect(result.current.stats.encounters_detected).toBe(0);
      expect(result.current.stats.buffer_word_count).toBe(0);
      expect(result.current.liveTranscript).toBe('');
      expect(result.current.audioQuality).toBeNull();
      expect(result.current.encounterNotes).toEqual([]);
      expect(result.current.currentPatientName).toBeNull();
      expect(result.current.error).toBeNull();
    });

    it('sets up continuous_mode_event listener on mount', async () => {
      const useContinuousMode = await loadHook();
      renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalledWith('continuous_mode_event', expect.any(Function));
      });
    });
  });

  describe('event handling', () => {
    it('sets isActive to true on started event', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      act(() => {
        emitEvent('continuous_mode_event', { type: 'started' });
      });

      expect(result.current.isActive).toBe(true);
      expect(result.current.error).toBeNull();
    });

    it('resets all state on stopped event', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      // First activate
      act(() => {
        emitEvent('continuous_mode_event', { type: 'started' });
      });
      expect(result.current.isActive).toBe(true);

      // Then stop
      act(() => {
        emitEvent('continuous_mode_event', { type: 'stopped' });
      });

      expect(result.current.isActive).toBe(false);
      expect(result.current.isStopping).toBe(false);
      expect(result.current.stats.state).toBe('idle');
      expect(result.current.liveTranscript).toBe('');
      expect(result.current.audioQuality).toBeNull();
      expect(result.current.encounterNotes).toEqual([]);
      expect(result.current.currentPatientName).toBeNull();
    });

    it('sets error and deactivates on error event', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      // Activate first
      act(() => {
        emitEvent('continuous_mode_event', { type: 'started' });
      });
      expect(result.current.isActive).toBe(true);

      // Error event
      act(() => {
        emitEvent('continuous_mode_event', { type: 'error', error: 'Pipeline failed' });
      });

      expect(result.current.isActive).toBe(false);
      expect(result.current.isStopping).toBe(false);
      expect(result.current.error).toBe('Pipeline failed');
    });

    it('uses "Unknown error" when error event has no error field', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      act(() => {
        emitEvent('continuous_mode_event', { type: 'error' });
      });

      expect(result.current.error).toBe('Unknown error');
    });

    it('clears encounter notes on encounter_detected event', async () => {
      mockInvoke.mockImplementation((cmd: string, args?: { text?: string }) => {
        if (cmd === 'submit_continuous_encounter_note') {
          return Promise.resolve({
            id: 'note-1',
            text: args?.text ?? '',
            timestampMs: 1_700_000_000_000,
          });
        }
        return Promise.resolve(undefined);
      });
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      await act(async () => {
        await result.current.submitEncounterNote('Some notes');
      });
      expect(result.current.encounterNotes).toHaveLength(1);

      // Encounter detected should clear notes + current patient name
      act(() => {
        emitEvent('continuous_mode_event', { type: 'encounter_detected' });
      });

      expect(result.current.encounterNotes).toEqual([]);
      expect(result.current.currentPatientName).toBeNull();
    });

    it('tracks current patient name from patient_name_updated events', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      act(() => {
        emitEvent('continuous_mode_event', {
          type: 'patient_name_updated',
          name: 'Jane Smith',
          dob: '1970-01-15',
        });
      });
      expect(result.current.currentPatientName).toBe('Jane Smith');

      // Tracker clear (e.g. DOB invalidation) → name absent → null
      act(() => {
        emitEvent('continuous_mode_event', { type: 'patient_name_updated' });
      });
      expect(result.current.currentPatientName).toBeNull();
    });
  });

  describe('transcript preview', () => {
    it('subscribes to continuous_transcript_preview when active', async () => {
      const useContinuousMode = await loadHook();
      renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      // Activate
      act(() => {
        emitEvent('continuous_mode_event', { type: 'started' });
      });

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalledWith('continuous_transcript_preview', expect.any(Function));
      });
    });

    it('updates liveTranscript from payload', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      // Activate
      act(() => {
        emitEvent('continuous_mode_event', { type: 'started' });
      });

      await waitFor(() => {
        expect(listeners['continuous_transcript_preview']).toBeDefined();
      });

      act(() => {
        emitEvent('continuous_transcript_preview', { finalized_text: 'Hello doctor' });
      });

      expect(result.current.liveTranscript).toBe('Hello doctor');
    });

    it('subscribes to transcript unconditionally but ignores events when not active', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      // Listener is registered unconditionally to avoid dropping events during transitions
      expect(listeners['continuous_transcript_preview']).toBeDefined();

      // But events are ignored when not active
      act(() => {
        emitEvent('continuous_transcript_preview', { finalized_text: 'Should be ignored' });
      });
      expect(result.current.liveTranscript).toBe('');
    });
  });

  describe('audio quality listener', () => {
    it('subscribes to audio_quality when active', async () => {
      const useContinuousMode = await loadHook();
      renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      act(() => {
        emitEvent('continuous_mode_event', { type: 'started' });
      });

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalledWith('audio_quality', expect.any(Function));
      });
    });

    it('updates audioQuality from payload', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      act(() => {
        emitEvent('continuous_mode_event', { type: 'started' });
      });

      await waitFor(() => {
        expect(listeners['audio_quality']).toBeDefined();
      });

      const mockQuality = {
        timestamp_ms: 1000,
        peak_db: -3,
        rms_db: -20,
        snr_db: 25,
        clipped_ratio: 0,
        clipped_samples: 0,
        dropout_count: 0,
        total_clipped: 0,
        total_samples: 44100,
        silence_ratio: 0.1,
        noise_floor_db: -50,
      };

      act(() => {
        emitEvent('audio_quality', mockQuality);
      });

      expect(result.current.audioQuality).toEqual(mockQuality);
    });
  });

  describe('start/stop/newPatient', () => {
    it('invokes start_continuous_mode on start', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await act(async () => {
        await result.current.start();
      });

      expect(mockInvoke).toHaveBeenCalledWith('start_continuous_mode');
    });

    it('clears error before starting', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      // Simulate a prior error
      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });
      act(() => {
        emitEvent('continuous_mode_event', { type: 'error', error: 'Old error' });
      });
      expect(result.current.error).toBe('Old error');

      // Start should clear error
      await act(async () => {
        await result.current.start();
      });

      expect(result.current.error).toBeNull();
    });

    it('sets error if start fails', async () => {
      mockInvoke.mockRejectedValueOnce('Start failed');
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await act(async () => {
        await result.current.start();
      });

      expect(result.current.error).toBe('Start failed');
    });

    it('invokes stop_continuous_mode on stop', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await act(async () => {
        await result.current.stop();
      });

      expect(mockInvoke).toHaveBeenCalledWith('stop_continuous_mode');
    });

    it('sets isStopping to true when stop is called', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await act(async () => {
        await result.current.stop();
      });

      expect(result.current.isStopping).toBe(true);
    });

    it('force resets on stop error', async () => {
      // Use mockImplementation to control which commands fail
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'stop_continuous_mode') {
          throw 'Stop failed';
        }
        return undefined;
      });

      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      // Activate first
      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });
      act(() => {
        emitEvent('continuous_mode_event', { type: 'started' });
      });

      await act(async () => {
        await result.current.stop();
      });

      expect(result.current.error).toBe('Stop failed');
      expect(result.current.isActive).toBe(false);
      expect(result.current.isStopping).toBe(false);
    });

    it('invokes trigger_new_patient', async () => {
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await act(async () => {
        await result.current.triggerNewPatient();
      });

      expect(mockInvoke).toHaveBeenCalledWith('trigger_new_patient');
    });

    it('sets error if triggerNewPatient fails', async () => {
      mockInvoke.mockRejectedValueOnce('Trigger failed');
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await act(async () => {
        await result.current.triggerNewPatient();
      });

      expect(result.current.error).toBe('Trigger failed');
    });
  });

  describe('submit-able encounter notes', () => {
    it('appends a chip when submit_continuous_encounter_note returns a record', async () => {
      let callCount = 0;
      mockInvoke.mockImplementation((cmd: string, args?: { text?: string }) => {
        if (cmd === 'submit_continuous_encounter_note') {
          callCount += 1;
          return Promise.resolve({
            id: `note-${callCount}`,
            text: args?.text ?? '',
            timestampMs: 1_700_000_000_000 + callCount,
          });
        }
        return Promise.resolve(undefined);
      });
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await act(async () => {
        await result.current.submitEncounterNote('Knee injection 40mg');
      });
      expect(mockInvoke).toHaveBeenCalledWith(
        'submit_continuous_encounter_note',
        { text: 'Knee injection 40mg' },
      );
      expect(result.current.encounterNotes).toHaveLength(1);
      expect(result.current.encounterNotes[0].text).toBe('Knee injection 40mg');

      await act(async () => {
        await result.current.submitEncounterNote('Follow-up in 2 weeks');
      });
      expect(result.current.encounterNotes).toHaveLength(2);
      expect(result.current.encounterNotes.map((n) => n.id)).toEqual(['note-1', 'note-2']);
    });

    it('removes a chip on deleteEncounterNote', async () => {
      mockInvoke.mockImplementation((cmd: string, args?: { text?: string }) => {
        if (cmd === 'submit_continuous_encounter_note') {
          return Promise.resolve({
            id: 'note-42',
            text: args?.text ?? '',
            timestampMs: 1_700_000_000_000,
          });
        }
        return Promise.resolve(undefined);
      });
      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await act(async () => {
        await result.current.submitEncounterNote('Test note');
      });
      expect(result.current.encounterNotes).toHaveLength(1);

      await act(async () => {
        await result.current.deleteEncounterNote('note-42');
      });
      expect(result.current.encounterNotes).toEqual([]);
      expect(mockInvoke).toHaveBeenCalledWith('delete_continuous_encounter_note', { id: 'note-42' });
    });
  });

  describe('status polling', () => {
    it('invokes get_continuous_mode_status when active', async () => {
      const mockStats = {
        state: 'recording' as const,
        recording_since: '2026-03-26T08:00:00Z',
        encounters_detected: 2,
        recent_encounters: [],
        last_error: null,
        buffer_word_count: 150,
        buffer_started_at: null,
      };

      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_continuous_mode_status') return mockStats;
        return undefined;
      });

      const useContinuousMode = await loadHook();
      const { result } = renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      // Activate
      act(() => {
        emitEvent('continuous_mode_event', { type: 'started' });
      });

      // Immediate fetch
      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('get_continuous_mode_status');
      });

      expect(result.current.stats.encounters_detected).toBe(2);
      expect(result.current.stats.buffer_word_count).toBe(150);
    });
  });

  describe('cleanup', () => {
    it('calls listen and registers unlisten for continuous_mode_event', async () => {
      const useContinuousMode = await loadHook();
      const { unmount } = renderHook(() => useContinuousMode());

      // Verify listener was registered
      await waitFor(() => {
        expect(mockListen).toHaveBeenCalledWith('continuous_mode_event', expect.any(Function));
      });

      // The unlisten fn should exist
      expect(unlistenFns['continuous_mode_event']).toBeDefined();

      unmount();

      // After unmount the unlisten fn should have been called
      // (needs a microtask tick since listen returns a promise)
      await new Promise(resolve => setTimeout(resolve, 0));
      expect(unlistenFns['continuous_mode_event']).toHaveBeenCalled();
    });

    it('registers listeners for transcript and audio quality when active', async () => {
      const useContinuousMode = await loadHook();
      renderHook(() => useContinuousMode());

      await waitFor(() => {
        expect(listeners['continuous_mode_event']).toBeDefined();
      });

      // Activate
      act(() => {
        emitEvent('continuous_mode_event', { type: 'started' });
      });

      await waitFor(() => {
        expect(listeners['continuous_transcript_preview']).toBeDefined();
        expect(listeners['audio_quality']).toBeDefined();
      });

      // Verify the unlisten fns exist for transcript and audio quality
      expect(unlistenFns['continuous_transcript_preview']).toBeDefined();
      expect(unlistenFns['audio_quality']).toBeDefined();
    });
  });
});
