import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import App from './App';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import {
  mockDevices,
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
      get_settings: mockSettings,
      start_session: undefined,
      stop_session: undefined,
      reset_session: undefined,
      set_settings: mockSettings,
      // Checklist commands
      run_checklist: {
        checks: [
          { name: 'Audio Device', status: 'pass', message: 'OK' },
          { name: 'Model', status: 'pass', message: 'OK' },
        ],
        all_passed: true,
        can_start: true,
        summary: 'All checks passed',
      },
      check_model_status: { available: true, path: '/models/model.bin', error: null },
      check_microphone_permission: { status: 'authorized', authorized: true, message: 'OK' },
      open_microphone_settings: undefined,
      // Medplum/Ollama commands used on init
      medplum_try_restore_session: undefined,
      check_ollama_status: { connected: false, available_models: [], error: null },
      medplum_check_connection: false,
      // Listening commands
      start_listening: undefined,
      stop_listening: undefined,
      get_listening_status: { is_listening: false, speech_detected: false, speech_duration_ms: 0, analyzing: false },
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
    // the "Start New Session" button should be available in ReadyMode
    expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
    it('loads devices and settings on mount', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('list_input_devices');
        expect(mockInvoke).toHaveBeenCalledWith('get_settings');
      });
    });

    it('shows Start button', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
      });

      const settingsBtn = screen.getByRole('button', { name: /settings/i });
      await user.click(settingsBtn);

      await waitFor(() => {
        expect(screen.getByText('Settings')).toBeInTheDocument();
        // Check for key settings labels
        expect(screen.getByText('Server Model')).toBeInTheDocument();
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
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
      });

      await user.click(screen.getByText('Start New Session'));

      // Note: deviceId 'default' is converted to null before calling the backend
      expect(mockInvoke).toHaveBeenCalledWith('start_session', { deviceId: null });
    });

    it('shows Stop button during recording', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
      });

      // Emit recording status
      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('End Session')).toBeInTheDocument();
        expect(screen.queryByText('Start New Session')).not.toBeInTheDocument();
      });
    });

    it('calls stop_session when Stop button is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('End Session')).toBeInTheDocument();
      });

      await user.click(screen.getByText('End Session'));

      expect(mockInvoke).toHaveBeenCalledWith('stop_session');
    });

    it('shows New Session button after completion', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
    it('shows start button when idle', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
      });
    });

    it('shows "Listening..." when transcript preview is shown during recording with no transcript', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
      });

      // Complete recording with transcript to enter review mode
      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      // ReviewMode defaults to SOAP tab, navigate to Transcript tab
      await waitFor(() => {
        const tabs = screen.getAllByRole('button').filter(btn => btn.classList.contains('review-tab'));
        expect(tabs.length).toBeGreaterThan(0);
      });
      const transcriptTab = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('Transcript')
      )[0];
      await user.click(transcriptTab);

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
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
      });

      // ReviewMode defaults to SOAP tab, navigate to Transcript tab
      await waitFor(() => {
        const tabs = screen.getAllByRole('button').filter(btn => btn.classList.contains('review-tab'));
        expect(tabs.length).toBeGreaterThan(0);
      });
      const transcriptTab = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('Transcript')
      )[0];
      await user.click(transcriptTab);

      await waitFor(() => {
        expect(screen.getByText('No transcript recorded')).toBeInTheDocument();
      });
    });
  });

  describe('review mode tabs', () => {
    it('can switch between tabs in review mode', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      // Go to review mode with a transcript
      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      // Should be in ReviewMode with tabs
      await waitFor(() => {
        const tabs = screen.getAllByRole('button').filter(btn => btn.classList.contains('review-tab'));
        expect(tabs.length).toBe(3); // Transcript, SOAP, Insights
      });

      // SOAP tab is default, switch to Transcript tab
      const transcriptTab = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('Transcript')
      )[0];
      await user.click(transcriptTab);

      await waitFor(() => {
        expect(screen.getByText('Hello, this is a test transcription.')).toBeInTheDocument();
      });

      // Switch to Insights tab
      const insightsTab = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('Insights')
      )[0];
      await user.click(insightsTab);

      await waitFor(() => {
        expect(screen.getByText('No insights available')).toBeInTheDocument();
      });
    });
  });

  describe('copy functionality', () => {
    const navigateToTranscriptTab = async (user: ReturnType<typeof userEvent.setup>) => {
      await waitFor(() => {
        const tabs = screen.getAllByRole('button').filter(btn => btn.classList.contains('review-tab'));
        expect(tabs.length).toBeGreaterThan(0);
      });
      const transcriptTab = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('Transcript')
      )[0];
      await user.click(transcriptTab);
    };

    it('shows Copy button in review mode with transcript', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      // Go to review mode with a transcript
      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      await navigateToTranscriptTab(user);

      await waitFor(() => {
        expect(screen.getByText('Copy')).toBeInTheDocument();
      });
    });

    it('Copy button is enabled when transcript is available in review mode', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      // Go to review mode with a transcript
      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      await navigateToTranscriptTab(user);

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

      await navigateToTranscriptTab(user);

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

      await navigateToTranscriptTab(user);

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
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        const statusDot = document.querySelector('.status-dot');
        expect(statusDot).toHaveClass('recording');
      });
    });

    it('shows session progress indicator during recording', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        // Recording mode shows "Session in progress..." instead of timer
        expect(screen.getByText('Session in progress...')).toBeInTheDocument();
        // Recording mode should be active
        expect(document.querySelector('.recording-mode')).toBeInTheDocument();
      });
    });

    it('shows recording mode UI during recording', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        // Recording mode should be active
        expect(document.querySelector('.recording-mode')).toBeInTheDocument();
        // End Session button should be visible
        expect(screen.getByText('End Session')).toBeInTheDocument();
      });
    });

    it('shows preparing status dot when preparing', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      render(<App />);
      await waitForAppReady();

      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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
        if (command === 'get_settings') return Promise.resolve(mockSettings);
        if (command === 'medplum_try_restore_session') return Promise.resolve(undefined);
        if (command === 'check_ollama_status') return Promise.resolve({ connected: false, available_models: [], error: null });
        if (command === 'medplum_check_connection') return Promise.resolve(false);
        return Promise.reject(new Error('Unknown command'));
      });

      // Should not throw
      render(<App />);
      await waitForAppReady();

      // App should still render with Start button
      await waitFor(() => {
        expect(screen.getByText('Start New Session')).toBeInTheDocument();
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

describe('Recording Mode UI', () => {
  // Test the recording mode display
  it('shows recording mode during recording', async () => {
    const mockInvoke = vi.mocked(invoke);
    mockInvoke.mockImplementation(createStandardMock());

    const listenHelper = createListenMock();
    const mockListen = vi.mocked(listen);
    mockListen.mockImplementation(listenHelper.listen as typeof listen);

    render(<App />);
    await waitForAppReady();

    await waitFor(() => {
      expect(screen.getByText('Start New Session')).toBeInTheDocument();
    });

    // Start recording
    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
    });

    await waitFor(() => {
      // Recording mode shows session progress indicator
      expect(screen.getByText('Session in progress...')).toBeInTheDocument();
      // Recording mode should be active
      expect(document.querySelector('.recording-mode')).toBeInTheDocument();
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

  it('shows quality indicator during recording', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    render(<App />);
    await waitForAppReady();

    // Start recording
    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityGood);
    });

    await waitFor(() => {
      const qualityIndicator = document.querySelector('.quality-indicator');
      expect(qualityIndicator).toBeInTheDocument();
    });
  });

  it('shows "good" quality indicator for good audio quality', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityGood);
    });

    await waitFor(() => {
      const qualityIndicator = document.querySelector('.quality-indicator');
      expect(qualityIndicator).toHaveClass('good');
      expect(screen.getByText('Good audio')).toBeInTheDocument();
    });
  });

  it('shows "fair" quality indicator for quiet audio', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityQuiet);
    });

    await waitFor(() => {
      const qualityIndicator = document.querySelector('.quality-indicator');
      expect(qualityIndicator).toHaveClass('fair');
      expect(screen.getByText('Fair audio')).toBeInTheDocument();
    });
  });

  it('shows "poor" quality indicator for clipped audio', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityClipped);
    });

    await waitFor(() => {
      const qualityIndicator = document.querySelector('.quality-indicator');
      expect(qualityIndicator).toHaveClass('poor');
      expect(screen.getByText('Poor audio')).toBeInTheDocument();
    });
  });

  it('shows details popover when quality indicator is clicked', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation(createStandardMock());

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityGood);
    });

    await waitFor(() => {
      expect(document.querySelector('.quality-indicator')).toBeInTheDocument();
    });

    // Click quality indicator to open popover
    const qualityIndicator = document.querySelector('.quality-indicator') as HTMLElement;
    await user.click(qualityIndicator);

    await waitFor(() => {
      expect(document.querySelector('.recording-details-popover')).toBeInTheDocument();
      expect(screen.getByText('Level')).toBeInTheDocument();
      expect(screen.getByText('SNR')).toBeInTheDocument();
    });
  });

  it('shows clips count in popover when audio is clipped', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation(createStandardMock());

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityClipped);
    });

    await waitFor(() => {
      expect(document.querySelector('.quality-indicator')).toBeInTheDocument();
    });

    // Click quality indicator to open popover
    const qualityIndicator = document.querySelector('.quality-indicator') as HTMLElement;
    await user.click(qualityIndicator);

    await waitFor(() => {
      expect(screen.getByText('Clips')).toBeInTheDocument();
      expect(screen.getByText('50')).toBeInTheDocument();
    });
  });

  it('displays Level and SNR metrics in popover', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation(createStandardMock());

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityGood);
    });

    await waitFor(() => {
      expect(document.querySelector('.quality-indicator')).toBeInTheDocument();
    });

    // Click quality indicator to open popover
    const qualityIndicator = document.querySelector('.quality-indicator') as HTMLElement;
    await user.click(qualityIndicator);

    await waitFor(() => {
      expect(screen.getByText('Level')).toBeInTheDocument();
      expect(screen.getByText('SNR')).toBeInTheDocument();
      expect(screen.getByText('-18 dB')).toBeInTheDocument();
      expect(screen.getByText('25 dB')).toBeInTheDocument();
    });
  });

  it('can toggle details popover', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation(createStandardMock());

    render(<App />);
    await waitForAppReady();

    act(() => {
      listenHelper.emit('session_status', mockRecordingStatus);
      listenHelper.emit('audio_quality', mockAudioQualityGood);
    });

    await waitFor(() => {
      expect(document.querySelector('.quality-indicator')).toBeInTheDocument();
    });

    const qualityIndicator = document.querySelector('.quality-indicator') as HTMLElement;

    // Click to open popover
    await user.click(qualityIndicator);

    await waitFor(() => {
      expect(document.querySelector('.recording-details-popover')).toBeInTheDocument();
    });

    // Click again to close popover
    await user.click(qualityIndicator);

    await waitFor(() => {
      expect(document.querySelector('.recording-details-popover')).not.toBeInTheDocument();
    });
  });
});

// Download Model tests removed - checklist functionality was simplified
