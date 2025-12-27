import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import App from './App';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import {
  mockDevices,
  mockModelStatusAvailable,
  mockModelStatusUnavailable,
  mockIdleStatus,
  mockRecordingStatus,
  mockCompletedStatus,
  mockTranscript,
  mockSettings,
  createListenMock,
} from './test/mocks';

// Type the mocks
const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);
const mockWriteText = vi.mocked(writeText);

// Helper to create standard mock implementation
function createStandardMock(overrides: Record<string, unknown> = {}) {
  return (command: string) => {
    const responses: Record<string, unknown> = {
      list_input_devices: mockDevices,
      check_model_status: mockModelStatusAvailable,
      get_settings: mockSettings,
      start_session: undefined,
      stop_session: undefined,
      reset_session: undefined,
      set_settings: mockSettings,
      ...overrides,
    };
    if (command in responses) {
      return Promise.resolve(responses[command]);
    }
    return Promise.reject(new Error(`Unknown command: ${command}`));
  };
}

describe('App', () => {
  let listenHelper: ReturnType<typeof createListenMock>;

  beforeEach(() => {
    vi.clearAllMocks();
    listenHelper = createListenMock();
    mockListen.mockImplementation(listenHelper.listen as typeof listen);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  describe('initialization', () => {
    it('loads devices, model status, and settings on mount', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('list_input_devices');
        expect(mockInvoke).toHaveBeenCalledWith('check_model_status');
        expect(mockInvoke).toHaveBeenCalledWith('get_settings');
      });
    });

    it('shows Start button with available model', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });
    });

    it('shows warning when model is not available', async () => {
      mockInvoke.mockImplementation(createStandardMock({
        check_model_status: mockModelStatusUnavailable,
      }));

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Model not found. Check settings.')).toBeInTheDocument();
      });
    });

    it('disables record button when model is not available', async () => {
      mockInvoke.mockImplementation(createStandardMock({
        check_model_status: mockModelStatusUnavailable,
      }));

      render(<App />);

      await waitFor(() => {
        const recordButton = screen.getByRole('button', { name: /start/i });
        expect(recordButton).toBeDisabled();
      });
    });
  });

  describe('settings drawer', () => {
    it('opens settings drawer when gear button is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      const settingsBtn = screen.getByRole('button', { name: /settings/i });
      await user.click(settingsBtn);

      await waitFor(() => {
        expect(screen.getByText('Settings')).toBeInTheDocument();
        expect(screen.getByText('Model')).toBeInTheDocument();
        expect(screen.getByText('Language')).toBeInTheDocument();
        expect(screen.getByText('Microphone')).toBeInTheDocument();
      });
    });

    it('closes settings drawer when close button is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      // Open settings
      const settingsBtn = screen.getByRole('button', { name: /settings/i });
      await user.click(settingsBtn);

      await waitFor(() => {
        expect(screen.getByText('Settings')).toBeInTheDocument();
      });

      // Close settings
      const closeBtn = screen.getByRole('button', { name: '×' });
      await user.click(closeBtn);

      await waitFor(() => {
        expect(screen.queryByText('Save Settings')).not.toBeInTheDocument();
      });
    });

    it('saves settings when Save Settings is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      // Open settings
      const settingsBtn = screen.getByRole('button', { name: /settings/i });
      await user.click(settingsBtn);

      await waitFor(() => {
        expect(screen.getByText('Save Settings')).toBeInTheDocument();
      });

      // Save settings
      await user.click(screen.getByText('Save Settings'));

      expect(mockInvoke).toHaveBeenCalledWith('set_settings', expect.any(Object));
    });
  });

  describe('session control', () => {
    it('starts session when record button is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      await user.click(screen.getByText('Start'));

      expect(mockInvoke).toHaveBeenCalledWith('start_session', { deviceId: 'default' });
    });

    it('shows Stop button during recording', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      // Emit recording status
      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('Stop')).toBeInTheDocument();
        expect(screen.queryByText('Start')).not.toBeInTheDocument();
      });
    });

    it('calls stop_session when Stop button is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('Stop')).toBeInTheDocument();
      });

      await user.click(screen.getByText('Stop'));

      expect(mockInvoke).toHaveBeenCalledWith('stop_session');
    });

    it('shows New Session button after completion', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('New Session')).toBeInTheDocument();
      });
    });

    it('resets session when New Session button is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('New Session')).toBeInTheDocument();
      });

      await user.click(screen.getByText('New Session'));

      expect(mockInvoke).toHaveBeenCalledWith('reset_session');
    });
  });

  describe('transcript display', () => {
    it('shows placeholder when idle and no transcript', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Tap Start to begin')).toBeInTheDocument();
      });
    });

    it('shows "Listening..." when recording with no transcript', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('Listening...')).toBeInTheDocument();
      });
    });

    it('displays transcript when received', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      await waitFor(() => {
        expect(screen.getByText('Hello, this is a test transcription.')).toBeInTheDocument();
      });
    });

    it('displays draft text when available', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
        listenHelper.emit('transcript_update', {
          finalized_text: 'Hello, this is a test.',
          draft_text: 'Still processing...',
          segment_count: 2,
        });
      });

      await waitFor(() => {
        expect(screen.getByText('Hello, this is a test.')).toBeInTheDocument();
        expect(screen.getByText('Still processing...')).toBeInTheDocument();
      });
    });

    it('shows "No transcript" when completed with empty transcript', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('No transcript')).toBeInTheDocument();
      });
    });
  });

  describe('transcript collapsing', () => {
    it('can collapse and expand transcript section', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Transcript')).toBeInTheDocument();
      });

      // Should show placeholder initially
      expect(screen.getByText('Tap Start to begin')).toBeInTheDocument();

      // Click to collapse
      await user.click(screen.getByText('Transcript'));

      // Placeholder should be hidden (collapsed)
      await waitFor(() => {
        const content = document.querySelector('.transcript-content');
        expect(content).toHaveClass('collapsed');
      });

      // Click to expand again
      await user.click(screen.getByText('Transcript'));

      await waitFor(() => {
        const content = document.querySelector('.transcript-content');
        expect(content).not.toHaveClass('collapsed');
      });
    });
  });

  describe('copy functionality', () => {
    it('shows Copy button in transcript header', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Copy')).toBeInTheDocument();
      });
    });

    it('disables Copy button when no transcript', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        const copyBtn = screen.getByText('Copy');
        expect(copyBtn).toBeDisabled();
      });
    });

    it('enables Copy button when transcript is available', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      await waitFor(() => {
        const copyBtn = screen.getByText('Copy');
        expect(copyBtn).not.toBeDisabled();
      });
    });

    it('copies transcript to clipboard when Copy is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      await waitFor(() => {
        expect(screen.getByText('Copy')).not.toBeDisabled();
      });

      await user.click(screen.getByText('Copy'));

      expect(mockWriteText).toHaveBeenCalledWith('Hello, this is a test transcription.');
    });

    it('shows "Copied!" feedback after successful copy', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());
      mockWriteText.mockResolvedValue();

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      await waitFor(() => {
        expect(screen.getByText('Copy')).not.toBeDisabled();
      });

      await user.click(screen.getByText('Copy'));

      await waitFor(() => {
        expect(screen.getByText('Copied!')).toBeInTheDocument();
      });
    });
  });

  describe('status indicators', () => {
    it('shows status dot with recording class during recording', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        const statusDot = document.querySelector('.status-dot');
        expect(statusDot).toHaveClass('recording');
      });
    });

    it('shows elapsed time during recording', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', { ...mockRecordingStatus, elapsed_ms: 65000 });
      });

      await waitFor(() => {
        expect(screen.getByText('01:05')).toBeInTheDocument();
      });
    });

    it('shows active timer class during recording', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        const timer = document.querySelector('.timer');
        expect(timer).toHaveClass('active');
      });
    });

    it('shows preparing status dot when preparing', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', {
          state: 'preparing',
          provider: null,
          elapsed_ms: 0,
          is_processing_behind: false,
        });
      });

      await waitFor(() => {
        const statusDot = document.querySelector('.status-dot');
        expect(statusDot).toHaveClass('preparing');
      });
    });

    it('shows stopping status dot when stopping', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', {
          state: 'stopping',
          provider: 'whisper',
          elapsed_ms: 10000,
          is_processing_behind: false,
        });
      });

      await waitFor(() => {
        const statusDot = document.querySelector('.status-dot');
        expect(statusDot).toHaveClass('stopping');
      });
    });
  });

  describe('error handling', () => {
    it('displays error message when session has error', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', {
          state: 'error',
          provider: null,
          elapsed_ms: 0,
          is_processing_behind: false,
          error_message: 'Failed to access microphone',
        });
      });

      await waitFor(() => {
        expect(screen.getByText('Failed to access microphone')).toBeInTheDocument();
      });
    });

    it('shows error banner with error-banner class', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', {
          state: 'error',
          provider: null,
          elapsed_ms: 0,
          is_processing_behind: false,
          error_message: 'Test error',
        });
      });

      await waitFor(() => {
        const errorBanner = document.querySelector('.error-banner');
        expect(errorBanner).toBeInTheDocument();
      });
    });

    it('handles initialization error gracefully', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.reject(new Error('No devices'));
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        if (command === 'get_settings') return Promise.resolve(mockSettings);
        return Promise.reject(new Error('Unknown command'));
      });

      // Should not throw
      render(<App />);

      // App should still render with Start button
      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });
    });
  });

  describe('app title', () => {
    it('displays Scribe as app title', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Scribe')).toBeInTheDocument();
      });
    });
  });
});

describe('formatTime', () => {
  // Test the formatTime function by checking rendered output
  it('formats seconds correctly', async () => {
    const mockInvoke = vi.mocked(invoke);
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve([]);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      return Promise.reject(new Error('Unknown command'));
    });

    const listenHelper = createListenMock();
    const mockListen = vi.mocked(listen);
    mockListen.mockImplementation(listenHelper.listen as typeof listen);

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText('Start')).toBeInTheDocument();
    });

    // Test various time formats
    act(() => {
      listenHelper.emit('session_status', { ...mockRecordingStatus, elapsed_ms: 5000 });
    });
    await waitFor(() => {
      expect(screen.getByText('00:05')).toBeInTheDocument();
    });

    act(() => {
      listenHelper.emit('session_status', { ...mockRecordingStatus, elapsed_ms: 65000 });
    });
    await waitFor(() => {
      expect(screen.getByText('01:05')).toBeInTheDocument();
    });

    act(() => {
      listenHelper.emit('session_status', { ...mockRecordingStatus, elapsed_ms: 3665000 });
    });
    await waitFor(() => {
      expect(screen.getByText('01:01:05')).toBeInTheDocument();
    });
  });
});
