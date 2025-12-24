import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@testing-library/react';
import App from './App';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  mockDevices,
  mockModelStatusAvailable,
  mockModelStatusUnavailable,
  createListenMock,
} from './test/mocks';

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

describe('App Snapshots', () => {
  let listenHelper: ReturnType<typeof createListenMock>;

  beforeEach(() => {
    vi.clearAllMocks();
    listenHelper = createListenMock();
    mockListen.mockImplementation(listenHelper.listen as typeof listen);
  });

  it('renders idle state correctly', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      return Promise.reject(new Error('Unknown command'));
    });

    const { container, findByText } = render(<App />);

    // Wait for initialization
    await findByText('Start');

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders model unavailable warning correctly', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusUnavailable);
      return Promise.reject(new Error('Unknown command'));
    });

    const { container, findByText } = render(<App />);

    await findByText(/Model not found/);

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders recording state correctly', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      return Promise.reject(new Error('Unknown command'));
    });

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
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      return Promise.reject(new Error('Unknown command'));
    });

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
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      return Promise.reject(new Error('Unknown command'));
    });

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

    await findByText('New Recording');

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders error state correctly', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      return Promise.reject(new Error('Unknown command'));
    });

    const { container, findByText } = render(<App />);

    await findByText('Start');

    listenHelper.emit('session_status', {
      state: 'error',
      provider: null,
      elapsed_ms: 0,
      is_processing_behind: false,
      error_message: 'Microphone access denied',
    });

    await findByText(/Error:/);

    expect(container.firstChild).toMatchSnapshot();
  });

  it('renders processing behind indicator correctly', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      return Promise.reject(new Error('Unknown command'));
    });

    const { container, findByText } = render(<App />);

    await findByText('Start');

    listenHelper.emit('session_status', {
      state: 'recording',
      provider: 'whisper',
      elapsed_ms: 60000,
      is_processing_behind: true,
    });

    await findByText('Processing...');

    expect(container.firstChild).toMatchSnapshot();
  });
});
