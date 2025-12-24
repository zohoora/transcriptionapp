import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor } from '@testing-library/react';
import { axe } from 'vitest-axe';
import { toHaveNoViolations } from 'vitest-axe/matchers';
import App from './App';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  mockDevices,
  mockModelStatusAvailable,
  mockModelStatusUnavailable,
  createListenMock,
} from './test/mocks';

expect.extend({ toHaveNoViolations });

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

describe('Accessibility Tests', () => {
  let listenHelper: ReturnType<typeof createListenMock>;

  beforeEach(() => {
    vi.clearAllMocks();
    listenHelper = createListenMock();
    mockListen.mockImplementation(listenHelper.listen as typeof listen);
  });

  it('idle state has no accessibility violations', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
      return Promise.reject(new Error('Unknown command'));
    });

    const { container, findByText } = render(<App />);
    await findByText('Start');

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('recording state has no accessibility violations', async () => {
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
      elapsed_ms: 5000,
      is_processing_behind: false,
    });

    await findByText('Stop');

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('transcript display has no accessibility violations', async () => {
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
      elapsed_ms: 10000,
      is_processing_behind: false,
    });

    listenHelper.emit('transcript_update', {
      finalized_text: 'This is a test transcript with multiple paragraphs.\n\nSecond paragraph here.',
      draft_text: 'Currently speaking...',
      segment_count: 2,
    });

    await findByText(/This is a test transcript/);

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('error state has no accessibility violations', async () => {
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

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('model unavailable warning has no accessibility violations', async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === 'list_input_devices') return Promise.resolve(mockDevices);
      if (command === 'check_model_status') return Promise.resolve(mockModelStatusUnavailable);
      return Promise.reject(new Error('Unknown command'));
    });

    const { container, findByText } = render(<App />);
    await findByText(/Model not found/);

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('completed state has no accessibility violations', async () => {
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
      finalized_text: 'Final transcript.',
      draft_text: null,
      segment_count: 1,
    });

    await findByText('New Recording');

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  // Test specific interactive elements
  describe('Interactive Elements', () => {
    it('buttons are accessible', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      const { findByText, findByRole } = render(<App />);
      await findByText('Start');

      const startButton = await findByRole('button', { name: /start/i });
      expect(startButton).toBeInTheDocument();
      expect(startButton).not.toBeDisabled();
    });

    it('device select is accessible', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      const { findByRole } = render(<App />);

      const select = await findByRole('combobox');
      expect(select).toBeInTheDocument();
    });

    it('copy button has accessible name when visible', async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === 'list_input_devices') return Promise.resolve(mockDevices);
        if (command === 'check_model_status') return Promise.resolve(mockModelStatusAvailable);
        return Promise.reject(new Error('Unknown command'));
      });

      const { findByText, findByRole } = render(<App />);
      await findByText('Start');

      listenHelper.emit('session_status', {
        state: 'recording',
        provider: 'whisper',
        elapsed_ms: 5000,
        is_processing_behind: false,
      });

      listenHelper.emit('transcript_update', {
        finalized_text: 'Some text to copy',
        draft_text: null,
        segment_count: 1,
      });

      await findByText('Copy');

      const copyButton = await findByRole('button', { name: /copy/i });
      expect(copyButton).toBeInTheDocument();
    });
  });
});
