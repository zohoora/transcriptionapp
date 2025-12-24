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
  createListenMock,
} from './test/mocks';

// Type the mocks
const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);
const mockWriteText = vi.mocked(writeText);

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
    it('loads devices and model status on mount', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('list_input_devices');
        expect(mockInvoke).toHaveBeenCalledWith('check_model_status');
      });
    });

    it('shows ready state with available model', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Ready')).toBeInTheDocument();
        expect(screen.getByText('Start')).toBeInTheDocument();
      });
    });

    it('shows warning when model is not available', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusUnavailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText(/Model not found:/)).toBeInTheDocument();
        expect(screen.getByText(/Model file not found/)).toBeInTheDocument();
      });
    });

    it('disables Start button when model is not available', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusUnavailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.queryByText('Start')).not.toBeInTheDocument();
      });
    });
  });

  describe('device selection', () => {
    it('shows default device option and device list', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        const select = screen.getByRole('combobox');
        expect(select).toBeInTheDocument();
        expect(screen.getByText('Default Device')).toBeInTheDocument();
      });
    });

    it('displays all available devices in the dropdown', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Built-in Microphone (default)')).toBeInTheDocument();
        expect(screen.getByText('External USB Microphone')).toBeInTheDocument();
      });
    });
  });

  describe('session control', () => {
    it('starts session when Start button is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        if (command === 'start_session') return Promise.resolve(undefined);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      await user.click(screen.getByText('Start'));

      expect(mockInvoke).toHaveBeenCalledWith('start_session', { deviceId: 'default' });
    });

    it('uses selected device when starting session', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        if (command === 'start_session') return Promise.resolve(undefined);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByRole('combobox')).toBeInTheDocument();
      });

      const select = screen.getByRole('combobox');
      await user.selectOptions(select, 'device-2');
      await user.click(screen.getByText('Start'));

      expect(mockInvoke).toHaveBeenCalledWith('start_session', { deviceId: 'device-2' });
    });

    it('shows Stop button during recording', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

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
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        if (command === 'stop_session') return Promise.resolve(undefined);
        return Promise.reject(new Error('Unknown command'));
      });

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

    it('shows New Recording button after completion', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('New Recording')).toBeInTheDocument();
      });
    });

    it('resets session when New Recording button is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        if (command === 'reset_session') return Promise.resolve(undefined);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockCompletedStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('New Recording')).toBeInTheDocument();
      });

      await user.click(screen.getByText('New Recording'));

      expect(mockInvoke).toHaveBeenCalledWith('reset_session');
    });
  });

  describe('transcript display', () => {
    it('shows placeholder when idle and no transcript', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Click Start to begin recording')).toBeInTheDocument();
      });
    });

    it('shows "Listening..." when recording', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

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
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

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
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

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
  });

  describe('copy functionality', () => {
    it('shows Copy button when transcript is available during recording', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      await waitFor(() => {
        expect(screen.getByText('Copy')).toBeInTheDocument();
      });
    });

    it('copies transcript to clipboard when Copy is clicked', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
        listenHelper.emit('transcript_update', mockTranscript);
      });

      await waitFor(() => {
        expect(screen.getByText('Copy')).toBeInTheDocument();
      });

      await user.click(screen.getByText('Copy'));

      expect(mockWriteText).toHaveBeenCalledWith('Hello, this is a test transcription.');
    });

    it('shows "Copied!" feedback after successful copy', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });
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
        expect(screen.getByText('Copy')).toBeInTheDocument();
      });

      await user.click(screen.getByText('Copy'));

      await waitFor(() => {
        expect(screen.getByText('Copied!')).toBeInTheDocument();
      });
    });
  });

  describe('status indicators', () => {
    it('shows recording indicator during recording', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', mockRecordingStatus);
      });

      await waitFor(() => {
        expect(screen.getByText('Recording')).toBeInTheDocument();
        expect(screen.getByText('Whisper')).toBeInTheDocument();
      });
    });

    it('shows elapsed time during recording', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

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

    it('shows processing indicator when behind', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      render(<App />);

      await waitFor(() => {
        expect(screen.getByText('Start')).toBeInTheDocument();
      });

      act(() => {
        listenHelper.emit('session_status', { ...mockRecordingStatus, is_processing_behind: true });
      });

      await waitFor(() => {
        expect(screen.getByText('Processing...')).toBeInTheDocument();
      });
    });

    it('shows preparing indicator when preparing', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

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
        // "Preparing..." appears in both status indicator and button
        const preparingElements = screen.getAllByText('Preparing...');
        expect(preparingElements.length).toBeGreaterThanOrEqual(1);
      });
    });

    it('shows stopping indicator when stopping', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

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
        expect(screen.getByText('Finishing...')).toBeInTheDocument();
      });
    });
  });

  describe('error handling', () => {
    it('displays error message when session has error', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

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
        expect(screen.getByText(/Error:/)).toBeInTheDocument();
        expect(screen.getByText(/Failed to access microphone/)).toBeInTheDocument();
      });
    });

    it('handles initialization error gracefully', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.reject(new Error('No devices'));
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      // Should not throw
      render(<App />);

      // App should still render
      await waitFor(() => {
        expect(screen.getByText('Ready')).toBeInTheDocument();
      });
    });
  });
});

describe('formatTime', () => {
  // Test the formatTime function by checking rendered output
  it('formats seconds correctly', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve([]);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      return Promise.reject(new Error('Unknown command'));
    });

    const listenHelper = createListenMock();
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
