import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axe } from 'vitest-axe';
import { toHaveNoViolations } from 'vitest-axe/matchers';
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

expect.extend({ toHaveNoViolations });

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

// Helper to create standard mock implementation
function createStandardMock(overrides: Record<string, unknown> = {}) {
  return (command: string) => {
    const responses: Record<string, unknown> = {
      list_input_devices: mockDevices,
      check_model_status: mockModelStatusAvailable,
      get_settings: mockSettings,
      set_settings: mockSettings,
      run_checklist: { checks: [], all_passed: true, can_start: true, summary: 'Ready' },
      ...overrides,
    };
    if (command in responses) {
      return Promise.resolve(responses[command]);
    }
    return Promise.reject(new Error(`Unknown command: ${command}`));
  };
}

// Helper to dismiss the checklist overlay by clicking Continue
async function dismissChecklist() {
  await waitFor(() => {
    expect(screen.getByText('Continue')).toBeInTheDocument();
  });
  fireEvent.click(screen.getByText('Continue'));
}

describe('Accessibility Tests', () => {
  let listenHelper: ReturnType<typeof createListenMock>;

  beforeEach(() => {
    vi.clearAllMocks();
    listenHelper = createListenMock();
    mockListen.mockImplementation(listenHelper.listen as typeof listen);
  });

  it('idle state has no accessibility violations', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await dismissChecklist();
    await findByText('Start');

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('recording state has no accessibility violations', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await dismissChecklist();
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
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await dismissChecklist();
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
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await dismissChecklist();
    await findByText('Start');

    listenHelper.emit('session_status', {
      state: 'error',
      provider: null,
      elapsed_ms: 0,
      is_processing_behind: false,
      error_message: 'Microphone access denied',
    });

    await findByText('Microphone access denied');

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('model unavailable warning has no accessibility violations', async () => {
    mockInvoke.mockImplementation(createStandardMock({
      check_model_status: mockModelStatusUnavailable,
    }));

    const { container, findByText } = render(<App />);
    await dismissChecklist();
    await findByText(/Model not found/);

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('completed state has no accessibility violations', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await dismissChecklist();
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

    await findByText('New Session');

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('settings drawer has no accessibility violations', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText, findByRole } = render(<App />);
    await dismissChecklist();
    await findByText('Start');

    // Open settings drawer
    const settingsBtn = await findByRole('button', { name: /settings/i });
    await user.click(settingsBtn);

    await findByText('Settings');

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  // Test specific interactive elements
  describe('Interactive Elements', () => {
    it('buttons are accessible', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      const { findByText, findByRole } = render(<App />);
      await dismissChecklist();
      await findByText('Start');

      const startButton = await findByRole('button', { name: /start/i });
      expect(startButton).toBeInTheDocument();
      expect(startButton).not.toBeDisabled();
    });

    it('settings button is accessible', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      const { findByText, findByRole } = render(<App />);
      await dismissChecklist();
      await findByText('Start');

      const settingsBtn = await findByRole('button', { name: /settings/i });
      expect(settingsBtn).toBeInTheDocument();
    });

    it('device select in settings is accessible', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      const { findByText, findByRole } = render(<App />);
      await dismissChecklist();
      await findByText('Start');

      // Open settings drawer
      const settingsBtn = await findByRole('button', { name: /settings/i });
      await user.click(settingsBtn);

      await findByText('Microphone');

      // Find all comboboxes in settings
      const selects = document.querySelectorAll('.settings-select');
      expect(selects.length).toBeGreaterThan(0);
    });

    it('copy button has accessible name when visible', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      const { findByText, findByRole } = render(<App />);
      await dismissChecklist();
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

    it('transcript header is clickable for collapse/expand', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      const { findByText } = render(<App />);
      await dismissChecklist();
      await findByText('Start');

      const transcriptHeader = await findByText('Transcript');
      expect(transcriptHeader).toBeInTheDocument();
      expect(transcriptHeader.closest('.transcript-header')).toBeTruthy();
    });
  });
});
