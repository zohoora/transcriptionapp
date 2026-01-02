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
  mockAudioQualityGood,
  mockAudioQualityQuiet,
  mockAudioQualityClipped,
  mockAudioQualityNoisy,
  mockAudioQualityDropout,
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
      run_checklist: { checks: [], all_passed: true, can_start: true, summary: 'Ready' },
      // Medplum/Ollama commands used on init
      medplum_try_restore_session: undefined,
      check_ollama_status: { connected: false, available_models: [], error: null },
      medplum_check_connection: false,
      ...overrides,
    };
    if (command in responses) {
      return Promise.resolve(responses[command]);
    }
    return Promise.reject(new Error(`Unknown command: ${command}`));
  };
}

// Helper to wait for the app to finish loading (checklist running completes)
async function waitForAppReady() {
  await waitFor(() => {
    // In the new mode-based UI, when checks pass and app is ready,
    // the START button should be available in ReadyMode
    expect(screen.getByText('START')).toBeInTheDocument();
  }, { timeout: 3000 });
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
    it('loads devices, settings, and runs checklist on mount', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('list_input_devices');
        expect(mockInvoke).toHaveBeenCalledWith('run_checklist');
        expect(mockInvoke).toHaveBeenCalledWith('get_settings');
      });
    });

    it('shows Start button with available model', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
      });
    });

    it('shows warning when model is not available', async () => {
      mockInvoke.mockImplementation(createStandardMock({
        check_model_status: mockModelStatusUnavailable,
      }));

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Model not found')).toBeInTheDocument();
      });
    });

    it('disables record button when model is not available', async () => {
      mockInvoke.mockImplementation(createStandardMock({
        check_model_status: mockModelStatusUnavailable,
      }));

      render(<App />);
      await waitForAppReady();

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
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
      });

      const settingsBtn = screen.getByRole('button', { name: /settings/i });
      await user.click(settingsBtn);

      await waitFor(() => {
        expect(screen.getByText('Settings')).toBeInTheDocument();
        // 'Model' appears twice now (Whisper and Ollama), so use getAllByText
        expect(screen.getAllByText('Model').length).toBeGreaterThanOrEqual(1);
        expect(screen.getByText('Language')).toBeInTheDocument();
        expect(screen.getByText('Microphone')).toBeInTheDocument();
      });
    });

    it('closes settings drawer when close button is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
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
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
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
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
      });

      await user.click(screen.getByText('START'));

      // Note: deviceId 'default' is converted to null before calling the backend
      expect(mockInvoke).toHaveBeenCalledWith('start_session', { deviceId: null });
    });

    it('shows Stop button during recording', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
      });

      // Emit recording status
      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('STOP')).toBeInTheDocument();
        expect(screen.queryByText('START')).not.toBeInTheDocument();
      });
    });

    it('calls stop_session when Stop button is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('STOP')).toBeInTheDocument();
      });

      await user.click(screen.getByText('STOP'));

      expect(mockInvoke).toHaveBeenCalledWith('stop_session');
    });

    it('shows New Session button after completion', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
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
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
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
    it('shows "Ready to record" status when idle', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Ready to record')).toBeInTheDocument();
      });
    });

    it('shows "Listening..." when transcript preview is shown during recording with no transcript', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      // Click "Show Transcript" to reveal the preview
      await waitFor(() => {
        expect(screen.getByText('Show Transcript')).toBeInTheDocument();
      });
      await user.click(screen.getByText('Show Transcript'));

      await waitFor(() => {
        // Look for Listening... in the transcript preview placeholder
        const placeholder = document.querySelector('.transcript-preview-placeholder');
        expect(placeholder).toHaveTextContent('Listening...');
      });
    });

    it('displays transcript in review mode when completed', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
      });

      // Complete recording with transcript to enter review mode
      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      await waitFor(() => {
        expect(screen.getByText('Hello, this is a test transcription.')).toBeInTheDocument();
      });
    });

    it('displays draft text when transcript preview is shown during recording', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
        listenHelper.emit('transcript_update', {
          finalized_text: 'Hello, this is a test.',
          draft_text: 'Still processing...',
          segment_count: 2,
        });
      });

      // Click "Show Transcript" to reveal the preview
      await waitFor(() => {
        expect(screen.getByText('Show Transcript')).toBeInTheDocument();
      });
      await user.click(screen.getByText('Show Transcript'));

      await waitFor(() => {
        expect(screen.getByText('Hello, this is a test.')).toBeInTheDocument();
        expect(screen.getByText('Still processing...')).toBeInTheDocument();
      });
    });

    it('shows "No transcript recorded" when completed with empty transcript', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('No transcript recorded')).toBeInTheDocument();
      });
    });
  });

  describe('transcript collapsing', () => {
    it('can collapse and expand transcript section in review mode', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      // Go to review mode with a transcript
      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      await waitFor(() => {
        expect(screen.getByText('Transcript')).toBeInTheDocument();
      });

      // Should show transcript content initially (expanded by default)
      expect(screen.getByText('Hello, this is a test transcription.')).toBeInTheDocument();

      // Click to collapse
      await user.click(screen.getByText('Transcript'));

      // Transcript text should be hidden (collapsed)
      await waitFor(() => {
        const content = document.querySelector('.review-transcript-content');
        expect(content).toBeNull();
      });

      // Click to expand again
      await user.click(screen.getByText('Transcript'));

      await waitFor(() => {
        expect(screen.getByText('Hello, this is a test transcription.')).toBeInTheDocument();
      });
    });
  });

  describe('copy functionality', () => {
    it('shows Copy button in review mode with transcript', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      // Go to review mode with a transcript
      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      await waitFor(() => {
        expect(screen.getByText('Copy')).toBeInTheDocument();
      });
    });

    it('Copy button is enabled when transcript is available in review mode', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      // Go to review mode with a transcript
      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
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
      await waitForAppReady();

      // Go to review mode with a transcript
      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
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
      await waitForAppReady();

      // Go to review mode with a transcript
      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
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
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
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
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        const timer = document.querySelector('.timer-large');
        expect(timer).toBeInTheDocument();
        // Timer uses local Date.now(), so we just check it shows a valid format (m:ss or mm:ss or h:mm:ss)
        expect(timer?.textContent).toMatch(/^\d{1,2}:\d{2}(:\d{2})?$/);
      });
    });

    it('shows timer in recording mode', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        const timer = document.querySelector('.timer-large');
        expect(timer).toBeInTheDocument();
        // Recording mode should be active
        expect(document.querySelector('.recording-mode')).toBeInTheDocument();
      });
    });

    it('shows preparing status dot when preparing', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
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
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
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
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
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

    it('shows error message in status area when error occurs', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
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
        // Error message should be displayed in the UI
        expect(screen.getByText('Test error')).toBeInTheDocument();
      });
    });

    it('handles initialization error gracefully', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.reject(new Error('No devices'));
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        if (command === 'get_settings') return Promise.resolve(mockSettings);
        if (command === 'run_checklist') return Promise.resolve({ checks: [], all_passed: true, can_start: true, summary: 'Ready' });
        return Promise.reject(new Error('Unknown command'));
      });

      // Should not throw
      render(<App />);
      await waitForAppReady();

      // App should still render with Start button
      await waitFor(() => {
        expect(screen.getByText('START')).toBeInTheDocument();
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
  // Test the timer display during recording
  it('shows timer during recording', async () => {
    const mockInvoke = vi.mocked(invoke);
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      if (command === 'run_checklist') return Promise.resolve({ checks: [], all_passed: true, can_start: true, summary: 'Ready' });
      return Promise.reject(new Error('Unknown command'));
    });

    const listenHelper = createListenMock();
    const mockListen = vi.mocked(listen);
    mockListen.mockImplementation(listenHelper.listen as typeof listen);

    render(<App />);
    await waitForAppReady();

    await waitFor(() => {
      expect(screen.getByText('START')).toBeInTheDocument();
    });

    // Start recording - timer uses local Date.now() so we just check it shows a valid format
    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
    });

    await waitFor(() => {
      const timer = document.querySelector('.timer-large');
      expect(timer).toBeInTheDocument();
      // Timer should have m:ss or mm:ss format initially
      expect(timer?.textContent).toMatch(/^\d{1,2}:\d{2}(:\d{2})?$/);
    });
  });
});

describe('Audio Quality', () => {
  let listenHelper: ReturnType<typeof createListenMock>;

  beforeEach(() => {
    vi.clearAllMocks();
    listenHelper = createListenMock();
    mockListen.mockImplementation(listenHelper.listen as typeof listen);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('shows status bars during recording', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      if (command === 'run_checklist') return Promise.resolve({ checks: [], all_passed: true, can_start: true, summary: 'Ready' });
      return Promise.reject(new Error('Unknown command'));
    });

    render(<App />);
    await waitForAppReady();

    // Start recording
    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityGood);
    });

    await waitFor(() => {
      const statusBars = document.querySelector('.status-bars');
      expect(statusBars).toBeInTheDocument();
    });
  });

  it('shows "good" status bars for good audio quality', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      if (command === 'run_checklist') return Promise.resolve({ checks: [], all_passed: true, can_start: true, summary: 'Ready' });
      return Promise.reject(new Error('Unknown command'));
    });

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityGood);
    });

    await waitFor(() => {
      const statusBars = document.querySelector('.status-bars');
      expect(statusBars).toHaveClass('good');
    });
  });

  it('shows "fair" status bars for quiet audio', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      if (command === 'run_checklist') return Promise.resolve({ checks: [], all_passed: true, can_start: true, summary: 'Ready' });
      return Promise.reject(new Error('Unknown command'));
    });

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityQuiet);
    });

    await waitFor(() => {
      const statusBars = document.querySelector('.status-bars');
      expect(statusBars).toHaveClass('fair');
    });
  });

  it('shows "poor" status bars for clipped audio', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      if (command === 'run_checklist') return Promise.resolve({ checks: [], all_passed: true, can_start: true, summary: 'Ready' });
      return Promise.reject(new Error('Unknown command'));
    });

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityClipped);
    });

    await waitFor(() => {
      const statusBars = document.querySelector('.status-bars');
      expect(statusBars).toHaveClass('poor');
    });
  });

  it('shows details popover when status bars are clicked', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      if (command === 'run_checklist') return Promise.resolve({ checks: [], all_passed: true, can_start: true, summary: 'Ready' });
      return Promise.reject(new Error('Unknown command'));
    });

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityGood);
    });

    await waitFor(() => {
      expect(document.querySelector('.status-bars')).toBeInTheDocument();
    });

    // Click status bars to open popover
    const statusBars = document.querySelector('.status-bars') as HTMLElement;
    await user.click(statusBars);

    await waitFor(() => {
      expect(document.querySelector('.recording-details-popover')).toBeInTheDocument();
      expect(screen.getByText('Level')).toBeInTheDocument();
      expect(screen.getByText('SNR')).toBeInTheDocument();
    });
  });

  it('shows clips count in popover when audio is clipped', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      if (command === 'run_checklist') return Promise.resolve({ checks: [], all_passed: true, can_start: true, summary: 'Ready' });
      return Promise.reject(new Error('Unknown command'));
    });

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityClipped);
    });

    await waitFor(() => {
      expect(document.querySelector('.status-bars')).toBeInTheDocument();
    });

    // Click status bars to open popover
    const statusBars = document.querySelector('.status-bars') as HTMLElement;
    await user.click(statusBars);

    await waitFor(() => {
      expect(screen.getByText('Clips')).toBeInTheDocument();
      expect(screen.getByText('50')).toBeInTheDocument();
    });
  });

  it('displays Level and SNR metrics in popover', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      if (command === 'run_checklist') return Promise.resolve({ checks: [], all_passed: true, can_start: true, summary: 'Ready' });
      return Promise.reject(new Error('Unknown command'));
    });

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityGood);
    });

    await waitFor(() => {
      expect(document.querySelector('.status-bars')).toBeInTheDocument();
    });

    // Click status bars to open popover
    const statusBars = document.querySelector('.status-bars') as HTMLElement;
    await user.click(statusBars);

    await waitFor(() => {
      expect(screen.getByText('Level')).toBeInTheDocument();
      expect(screen.getByText('SNR')).toBeInTheDocument();
      expect(screen.getByText('-18 dB')).toBeInTheDocument();
      expect(screen.getByText('25 dB')).toBeInTheDocument();
    });
  });

  it('can toggle details popover', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      if (command === 'run_checklist') return Promise.resolve({ checks: [], all_passed: true, can_start: true, summary: 'Ready' });
      return Promise.reject(new Error('Unknown command'));
    });

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityGood);
    });

    await waitFor(() => {
      expect(document.querySelector('.status-bars')).toBeInTheDocument();
    });

    const statusBars = document.querySelector('.status-bars') as HTMLElement;

    // Click to open popover
    await user.click(statusBars);

    await waitFor(() => {
      expect(document.querySelector('.recording-details-popover')).toBeInTheDocument();
    });

    // Click again to close popover
    await user.click(statusBars);

    await waitFor(() => {
      expect(document.querySelector('.recording-details-popover')).not.toBeInTheDocument();
    });
  });
});

describe('Download Model', () => {
  it('passes modelName argument when downloading Whisper model', async () => {
    const mockInvoke = vi.mocked(invoke);
    const downloadPromise = Promise.resolve();

    mockInvoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusUnavailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      if (command === 'run_checklist') {
        return Promise.resolve({
          checks: [{
            id: 'whisper_model',
            name: 'Whisper Model',
            status: 'fail',
            message: 'Model not found',
            action: { download_model: { model_name: 'small' } }
          }],
          all_passed: false,
          can_start: false,
          summary: 'Model missing'
        });
      }
      if (command === 'download_whisper_model') {
        // Verify the modelName argument is passed
        expect(args).toEqual({ modelName: 'small' });
        return downloadPromise;
      }
      return Promise.reject(new Error('Unknown command'));
    });

    const user = userEvent.setup();
    render(<App />);
    await waitForAppReady();

    // Find and click the download button for the Whisper model
    await waitFor(() => {
      const downloadBtn = screen.getByRole('button', { name: /get/i });
      expect(downloadBtn).toBeInTheDocument();
    });

    const downloadBtn = screen.getByRole('button', { name: /get/i });
    await user.click(downloadBtn);

    // Verify download_whisper_model was called with the correct argument
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('download_whisper_model', { modelName: 'small' });
    });
  });

  it('does not pass modelName for non-Whisper models', async () => {
    const mockInvoke = vi.mocked(invoke);

    mockInvoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      if (command === 'get_settings') return Promise.resolve(mockSettings);
      if (command === 'run_checklist') {
        return Promise.resolve({
          checks: [{
            id: 'speaker_model',
            name: 'Speaker Model',
            status: 'fail',
            message: 'Model not found',
            action: { download_model: { model_name: 'speaker_embedding' } }
          }],
          all_passed: false,
          can_start: false,
          summary: 'Model missing'
        });
      }
      if (command === 'download_speaker_model') {
        // Should be called without arguments
        expect(args).toBeUndefined();
        return Promise.resolve();
      }
      return Promise.reject(new Error('Unknown command'));
    });

    const user = userEvent.setup();
    render(<App />);
    await waitForAppReady();

    await waitFor(() => {
      const downloadBtn = screen.getByRole('button', { name: /get/i });
      expect(downloadBtn).toBeInTheDocument();
    });

    const downloadBtn = screen.getByRole('button', { name: /get/i });
    await user.click(downloadBtn);

    // Verify download_speaker_model was called without arguments
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith('download_speaker_model');
    });
  });
});
