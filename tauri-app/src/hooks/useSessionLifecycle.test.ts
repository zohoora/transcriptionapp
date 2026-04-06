import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useSessionLifecycle } from './useSessionLifecycle';
import type { SessionLifecycleHandlers } from './useSessionLifecycle';

function createMockHandlers(): SessionLifecycleHandlers {
  return {
    sessionStart: vi.fn().mockResolvedValue(undefined),
    sessionReset: vi.fn().mockResolvedValue(undefined),
    resetSyncState: vi.fn(),
    clearChat: vi.fn(),
    clearSoapError: vi.fn(),
    clearSessionCustomInstructions: vi.fn(),
  };
}

describe('useSessionLifecycle', () => {
  let handlers: SessionLifecycleHandlers;

  beforeEach(() => {
    vi.clearAllMocks();
    handlers = createMockHandlers();
  });

  describe('initial state', () => {
    it('starts with empty sessionNotes', () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));
      expect(result.current.sessionNotes).toBe('');
    });
  });

  describe('sessionNotes state management', () => {
    it('updates sessionNotes via setSessionNotes', () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      act(() => {
        result.current.setSessionNotes('Patient appears fatigued');
      });

      expect(result.current.sessionNotes).toBe('Patient appears fatigued');
    });

    it('can clear sessionNotes', () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      act(() => {
        result.current.setSessionNotes('Some notes');
      });
      expect(result.current.sessionNotes).toBe('Some notes');

      act(() => {
        result.current.setSessionNotes('');
      });
      expect(result.current.sessionNotes).toBe('');
    });
  });

  describe('startSession', () => {
    it('calls all reset callbacks and sessionStart', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      await act(async () => {
        await result.current.startSession('device-1');
      });

      expect(handlers.resetSyncState).toHaveBeenCalledTimes(1);
      expect(handlers.clearChat).toHaveBeenCalledTimes(1);
      expect(handlers.clearSoapError).toHaveBeenCalledTimes(1);
      expect(handlers.clearSessionCustomInstructions).toHaveBeenCalledTimes(1);
      expect(handlers.sessionStart).toHaveBeenCalledWith('device-1');
    });

    it('clears sessionNotes on start', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      act(() => {
        result.current.setSessionNotes('Old notes');
      });
      expect(result.current.sessionNotes).toBe('Old notes');

      await act(async () => {
        await result.current.startSession(null);
      });

      expect(result.current.sessionNotes).toBe('');
    });

    it('passes null deviceId through to sessionStart', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      await act(async () => {
        await result.current.startSession(null);
      });

      expect(handlers.sessionStart).toHaveBeenCalledWith(null);
    });
  });

  describe('resetSession', () => {
    it('calls all reset callbacks and sessionReset', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      await act(async () => {
        await result.current.resetSession();
      });

      expect(handlers.resetSyncState).toHaveBeenCalledTimes(1);
      expect(handlers.clearChat).toHaveBeenCalledTimes(1);
      expect(handlers.clearSoapError).toHaveBeenCalledTimes(1);
      expect(handlers.clearSessionCustomInstructions).toHaveBeenCalledTimes(1);
      expect(handlers.sessionReset).toHaveBeenCalledTimes(1);
    });

    it('clears sessionNotes on reset', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      act(() => {
        result.current.setSessionNotes('Notes from session');
      });

      await act(async () => {
        await result.current.resetSession();
      });

      expect(result.current.sessionNotes).toBe('');
    });
  });

  describe('startSessionAutoDetect', () => {
    it('calls all reset callbacks and sessionStart', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      await act(async () => {
        await result.current.startSessionAutoDetect('device-1');
      });

      expect(handlers.resetSyncState).toHaveBeenCalledTimes(1);
      expect(handlers.clearChat).toHaveBeenCalledTimes(1);
      expect(handlers.clearSoapError).toHaveBeenCalledTimes(1);
      expect(handlers.clearSessionCustomInstructions).toHaveBeenCalledTimes(1);
      expect(handlers.sessionStart).toHaveBeenCalledWith('device-1');
    });

    it('sets autoStartPending so handleGreetingRejected will reset', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      await act(async () => {
        await result.current.startSessionAutoDetect('device-1');
      });

      // handleGreetingRejected should return true (was pending)
      let wasReset = false;
      await act(async () => {
        wasReset = await result.current.handleGreetingRejected();
      });

      expect(wasReset).toBe(true);
      expect(handlers.sessionReset).toHaveBeenCalledTimes(1);
    });
  });

  describe('confirmAutoStart', () => {
    it('clears pending state so handleGreetingRejected does not reset', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      // Start auto-detected session
      await act(async () => {
        await result.current.startSessionAutoDetect('device-1');
      });

      // Confirm the auto-start
      act(() => {
        result.current.confirmAutoStart();
      });

      // Now greeting rejection should not reset
      let wasReset = false;
      await act(async () => {
        wasReset = await result.current.handleGreetingRejected();
      });

      expect(wasReset).toBe(false);
      expect(handlers.sessionReset).not.toHaveBeenCalled();
    });
  });

  describe('handleGreetingRejected', () => {
    it('returns true and resets session when auto-start is pending', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      await act(async () => {
        await result.current.startSessionAutoDetect(null);
      });

      let wasReset = false;
      await act(async () => {
        wasReset = await result.current.handleGreetingRejected();
      });

      expect(wasReset).toBe(true);
      expect(handlers.sessionReset).toHaveBeenCalledTimes(1);
    });

    it('returns false and does not reset for manual sessions', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      // Start a manual session (not auto-detect)
      await act(async () => {
        await result.current.startSession('device-1');
      });

      let wasReset = false;
      await act(async () => {
        wasReset = await result.current.handleGreetingRejected();
      });

      expect(wasReset).toBe(false);
      expect(handlers.sessionReset).not.toHaveBeenCalled();
    });

    it('returns false when no session has been started', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      let wasReset = false;
      await act(async () => {
        wasReset = await result.current.handleGreetingRejected();
      });

      expect(wasReset).toBe(false);
    });

    it('only resets once (clears pending flag after first rejection)', async () => {
      const { result } = renderHook(() => useSessionLifecycle(handlers));

      await act(async () => {
        await result.current.startSessionAutoDetect(null);
      });

      // First rejection: should reset
      let wasReset1 = false;
      await act(async () => {
        wasReset1 = await result.current.handleGreetingRejected();
      });
      expect(wasReset1).toBe(true);

      // Second rejection: should not reset (already cleared)
      let wasReset2 = false;
      await act(async () => {
        wasReset2 = await result.current.handleGreetingRejected();
      });
      expect(wasReset2).toBe(false);
      expect(handlers.sessionReset).toHaveBeenCalledTimes(1);
    });
  });
});
