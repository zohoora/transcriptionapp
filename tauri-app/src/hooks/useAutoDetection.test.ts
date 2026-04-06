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

describe('useAutoDetection', () => {
  // Lazy import to avoid hoisting issues with vi.mock
  async function loadHook() {
    const { useAutoDetection } = await import('./useAutoDetection');
    return useAutoDetection;
  }

  function createCallbacks() {
    return {
      onStartRecording: vi.fn(),
      onGreetingConfirmed: vi.fn(),
      onGreetingRejected: vi.fn(),
      onGreetingDetected: vi.fn(),
    };
  }

  describe('initial state', () => {
    it('starts with isListening false and no error', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      expect(result.current.isListening).toBe(false);
      expect(result.current.isPendingConfirmation).toBe(false);
      expect(result.current.listeningStatus).toBeNull();
      expect(result.current.error).toBeNull();
    });

    it('sets up listening_event listener on mount', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      renderHook(() => useAutoDetection(false, callbacks));

      await waitFor(() => {
        expect(mockListen).toHaveBeenCalledWith('listening_event', expect.any(Function));
      });
    });
  });

  describe('startListening', () => {
    it('invokes start_listening and sets isListening to true', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      await act(async () => {
        await result.current.startListening('device-1');
      });

      expect(mockInvoke).toHaveBeenCalledWith('start_listening', { deviceId: 'device-1' });
      expect(result.current.isListening).toBe(true);
      expect(result.current.listeningStatus).toEqual({
        is_listening: true,
        speech_detected: false,
        speech_duration_ms: 0,
        analyzing: false,
      });
    });

    it('sets error when invoke fails', async () => {
      mockInvoke.mockRejectedValueOnce(new Error('Mic unavailable'));
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      await act(async () => {
        await result.current.startListening(null);
      });

      expect(result.current.isListening).toBe(false);
      expect(result.current.error).toBe('Mic unavailable');
    });

    it('does nothing if already listening', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      await act(async () => {
        await result.current.startListening('device-1');
      });
      expect(result.current.isListening).toBe(true);

      await act(async () => {
        await result.current.startListening('device-2');
      });

      // Should only have been called once
      expect(mockInvoke).toHaveBeenCalledTimes(1);
    });
  });

  describe('stopListening', () => {
    it('invokes stop_listening and clears state', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      // Start first
      await act(async () => {
        await result.current.startListening(null);
      });
      expect(result.current.isListening).toBe(true);

      // Then stop
      await act(async () => {
        await result.current.stopListening();
      });

      expect(mockInvoke).toHaveBeenCalledWith('stop_listening');
      expect(result.current.isListening).toBe(false);
      expect(result.current.listeningStatus).toBeNull();
    });

    it('does nothing if not listening', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      await act(async () => {
        await result.current.stopListening();
      });

      expect(mockInvoke).not.toHaveBeenCalled();
    });

    it('clears state even if invoke fails', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      // Start listening
      await act(async () => {
        await result.current.startListening(null);
      });

      // Make stop fail
      mockInvoke.mockRejectedValueOnce(new Error('Stop failed'));

      await act(async () => {
        await result.current.stopListening();
      });

      // Should still be cleaned up
      expect(result.current.isListening).toBe(false);
      expect(result.current.listeningStatus).toBeNull();
    });
  });

  describe('event handling', () => {
    it('start_recording triggers onStartRecording callback', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      renderHook(() => useAutoDetection(false, callbacks));

      await waitFor(() => {
        expect(listeners['listening_event']).toBeDefined();
      });

      act(() => {
        emitEvent('listening_event', { type: 'start_recording' });
      });

      await waitFor(() => {
        expect(callbacks.onStartRecording).toHaveBeenCalledTimes(1);
      });
    });

    it('start_recording sets isPendingConfirmation to true', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      await waitFor(() => {
        expect(listeners['listening_event']).toBeDefined();
      });

      act(() => {
        emitEvent('listening_event', { type: 'start_recording' });
      });

      expect(result.current.isPendingConfirmation).toBe(true);
    });

    it('greeting_confirmed triggers onGreetingConfirmed with transcript and confidence', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      await waitFor(() => {
        expect(listeners['listening_event']).toBeDefined();
      });

      act(() => {
        emitEvent('listening_event', {
          type: 'greeting_confirmed',
          transcript: 'Hello doctor',
          confidence: 0.95,
        });
      });

      await waitFor(() => {
        expect(callbacks.onGreetingConfirmed).toHaveBeenCalledWith('Hello doctor', 0.95);
      });
      expect(result.current.isPendingConfirmation).toBe(false);
      expect(result.current.isListening).toBe(false);
      expect(result.current.listeningStatus).toBeNull();
    });

    it('greeting_rejected triggers onGreetingRejected with reason', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      await waitFor(() => {
        expect(listeners['listening_event']).toBeDefined();
      });

      act(() => {
        emitEvent('listening_event', {
          type: 'greeting_rejected',
          reason: 'Not medical speech',
        });
      });

      await waitFor(() => {
        expect(callbacks.onGreetingRejected).toHaveBeenCalledWith('Not medical speech');
      });
      expect(result.current.isPendingConfirmation).toBe(false);
    });

    it('error event sets error state', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      await waitFor(() => {
        expect(listeners['listening_event']).toBeDefined();
      });

      act(() => {
        emitEvent('listening_event', {
          type: 'error',
          message: 'VAD failed to initialize',
        });
      });

      expect(result.current.error).toBe('VAD failed to initialize');
      expect(result.current.isPendingConfirmation).toBe(false);
    });

    it('error event uses "Unknown error" when no message provided', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      await waitFor(() => {
        expect(listeners['listening_event']).toBeDefined();
      });

      act(() => {
        emitEvent('listening_event', { type: 'error' });
      });

      expect(result.current.error).toBe('Unknown error');
    });

    it('stopped event clears all state', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      await waitFor(() => {
        expect(listeners['listening_event']).toBeDefined();
      });

      // Set some state via start_recording
      act(() => {
        emitEvent('listening_event', { type: 'start_recording' });
      });
      expect(result.current.isPendingConfirmation).toBe(true);

      // Stopped should clear
      act(() => {
        emitEvent('listening_event', { type: 'stopped' });
      });

      expect(result.current.isListening).toBe(false);
      expect(result.current.isPendingConfirmation).toBe(false);
      expect(result.current.listeningStatus).toBeNull();
    });

    it('speech_detected updates listeningStatus', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { result } = renderHook(() => useAutoDetection(false, callbacks));

      await waitFor(() => {
        expect(listeners['listening_event']).toBeDefined();
      });

      act(() => {
        emitEvent('listening_event', {
          type: 'speech_detected',
          duration_ms: 1500,
        });
      });

      expect(result.current.listeningStatus).toEqual(
        expect.objectContaining({
          speech_detected: true,
          speech_duration_ms: 1500,
        })
      );
    });
  });

  describe('cleanup', () => {
    it('calls unlisten on unmount', async () => {
      const useAutoDetection = await loadHook();
      const callbacks = createCallbacks();
      const { unmount } = renderHook(() => useAutoDetection(false, callbacks));

      await waitFor(() => {
        expect(listeners['listening_event']).toBeDefined();
      });

      expect(unlistenFns['listening_event']).toBeDefined();

      unmount();

      // After unmount, unlisten should have been called
      await new Promise(resolve => setTimeout(resolve, 0));
      expect(unlistenFns['listening_event']).toHaveBeenCalled();
    });
  });
});
