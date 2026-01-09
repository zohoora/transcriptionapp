import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import App from './App';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  mockDevices,
  mockSettings,
  createListenMock,
} from './test/mocks';

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

// Helper to create standard mock implementation
function createStandardMock(overrides: Record<string, unknown> = {}) {
  return (command: string) => {
    const responses: Record<string, unknown> = {
      list_input_devices: mockDevices,
      get_settings: mockSettings,
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

describe('App Snapshots', () => {
  let listenHelper: ReturnType<typeof createListenMock>;

  beforeEach(() => {
    vi.clearAllMocks();
    listenHelper = createListenMock();
    mockListen.mockImplementation(listenHelper.listen as typeof listen);
  });

  it('renders idle state correctly', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container } = render(<App />);
    await waitForAppReady();

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders recording state correctly', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await waitForAppReady();

    // Emit recording status
    listenHelper.emit('session_status', {
      state: 'recording',
      provider: 'whisper',
      elapsed_ms: 5000,
      is_processing_behind: false,
    });

    await findByText('End Session');

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders with transcript correctly', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await waitForAppReady();

    // Emit recording status with transcript
    listenHelper.emit('session_status', {
      state: 'recording',
      provider: 'whisper',
      elapsed_ms: 10000,
      is_processing_behind: false,
    });

    listenHelper.emit('transcript_update', {
      finalized_text: 'Hello, this is a test transcription.\n\nThis is the second paragraph.',
      draft_text: 'Still speaking...',
      segment_count: 2,
    });

    // Click "Show Transcript" to reveal the preview (hidden by default in new UI)
    await waitFor(() => {
      expect(screen.getByText('Show Transcript')).toBeInTheDocument();
    });
    await user.click(screen.getByText('Show Transcript'));

    // In recording mode, transcript is in a floating preview
    await findByText(/Hello, this is a test transcription/);

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders completed state correctly', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await waitForAppReady();

    listenHelper.emit('session_status', {
      state: 'completed',
      provider: 'whisper',
      elapsed_ms: 30000,
      is_processing_behind: false,
    });

    listenHelper.emit('transcript_update', {
      finalized_text: 'Final transcript text here.',
      draft_text: null,
      segment_count: 1,
    });

    await findByText('New Session');

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders error state correctly', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await waitForAppReady();

    listenHelper.emit('session_status', {
      state: 'error',
      provider: null,
      elapsed_ms: 0,
      is_processing_behind: false,
      error_message: 'Microphone access denied',
    });

    await findByText('Microphone access denied');

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders stopping state correctly', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await waitForAppReady();

    listenHelper.emit('session_status', {
      state: 'stopping',
      provider: 'whisper',
      elapsed_ms: 60000,
      is_processing_behind: false,
    });

    // In the new UI, stopping state shows "Ending..." text
    await findByText('Ending...');

    // Button should be disabled during stopping
    const stopButton = container.querySelector('.stop-button');
    expect(stopButton).toBeDisabled();

    expect(container.firstChild).toMatchSnapshot();
  });
});
