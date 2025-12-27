import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@testing-library/react';
import App from './App';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  mockDevices,
  mockModelStatusAvailable,
  mockModelStatusUnavailable,
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
      check_model_status: mockModelStatusAvailable,
      get_settings: mockSettings,
      ...overrides,
    };
    if (command in responses) {
      return Promise.resolve(responses[command]);
    }
    return Promise.reject(new Error(`Unknown command: ${command}`));
  };
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

    const { container, findByText } = render(<App />);

    // Wait for initialization
    await findByText('Start');

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders model unavailable warning correctly', async () => {
    mockInvoke.mockImplementation(createStandardMock({
      check_model_status: mockModelStatusUnavailable,
    }));

    const { container, findByText } = render(<App />);

    await findByText(/Model not found/);

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders recording state correctly', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);

    await findByText('Start');

    // Emit recording status
    listenHelper.emit('session_status', {
      state: 'recording',
      provider: 'whisper',
      elapsed_ms: 5000,
      is_processing_behind: false,
    });

    await findByText('Stop');

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders with transcript correctly', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);

    await findByText('Start');

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

    await findByText('Hello, this is a test transcription.');

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders completed state correctly', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);

    await findByText('Start');

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

    await findByText('Start');

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

    await findByText('Start');

    listenHelper.emit('session_status', {
      state: 'stopping',
      provider: 'whisper',
      elapsed_ms: 60000,
      is_processing_behind: false,
    });

    // Wait for the button to show '...' (stopping state)
    await findByText('...');

    // Button should have stopping class
    const recordButton = container.querySelector('.record-button');
    expect(recordButton).toHaveClass('stopping');

    expect(container.firstChild).toMatchSnapshot();
  });
});
