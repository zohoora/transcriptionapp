import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axe } from 'vitest-axe';
import { toHaveNoViolations } from 'vitest-axe/matchers';
import App from './App';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import {
  mockDevices,
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
      get_settings: mockSettings,
      set_settings: mockSettings,
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

describe('Accessibility Tests', () => {
  let listenHelper: ReturnType<typeof createListenMock>;

  beforeEach(() => {
    vi.clearAllMocks();
    listenHelper = createListenMock();
    mockListen.mockImplementation(listenHelper.listen as typeof listen);
  });

  it('idle state has no accessibility violations', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container } = render(<App />);
    await waitForAppReady();

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('recording state has no accessibility violations', async () => {
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await waitForAppReady();

    listenHelper.emit('session_status', {
      state: 'recording',
      provider: 'whisper',
      elapsed_ms: 5000,
      is_processing_behind: false,
    });

    await findByText('STOP');

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('transcript display has no accessibility violations', async () => {
    const user = userEvent.setup();
    mockInvoke.mockImplementation(createStandardMock());

    const { container, findByText } = render(<App />);
    await waitForAppReady();

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

    // Click "Show Transcript" to reveal the preview (hidden by default in new UI)
    await waitFor(() => {
      expect(screen.getByText('Show Transcript')).toBeInTheDocument();
    });
    await user.click(screen.getByText('Show Transcript'));

    await findByText(/This is a test transcript/);

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('error state has no accessibility violations', async () => {
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

    const results = await axe(container);
    expect(results).toHaveNoViolations();
  });

  it('completed state has no accessibility violations', async () => {
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
    await waitForAppReady();

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

      const { findByRole } = render(<App />);
      await waitForAppReady();

      const startButton = await findByRole('button', { name: /start/i });
      expect(startButton).toBeInTheDocument();
      expect(startButton).not.toBeDisabled();
    });

    it('settings button is accessible', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      const { findByRole } = render(<App />);
      await waitForAppReady();

      const settingsBtn = await findByRole('button', { name: /settings/i });
      expect(settingsBtn).toBeInTheDocument();
    });

    it('device select in settings is accessible', async () => {
      const user = userEvent.setup();
      mockInvoke.mockImplementation(createStandardMock());

      const { findByText, findByRole } = render(<App />);
      await waitForAppReady();

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

      const { findByText } = render(<App />);
      await waitForAppReady();

      // Trigger recording mode
      listenHelper.emit('session_status', {
        state: 'recording',
        provider: 'whisper',
        elapsed_ms: 5000,
        is_processing_behind: false,
      });

      // Then complete the session to get to review mode where Copy button appears
      listenHelper.emit('session_status', {
        state: 'completed',
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

      // The copy button has the .copy-btn class and contains the text "Copy"
      const copyButton = document.querySelector('.copy-btn');
      expect(copyButton).toBeInTheDocument();
      expect(copyButton).toHaveTextContent('Copy');
    });

    it('transcript section header is clickable for collapse/expand', async () => {
      mockInvoke.mockImplementation(createStandardMock());

      const { findByText } = render(<App />);
      await waitForAppReady();

      // Trigger completed state to see transcript in review mode
      listenHelper.emit('session_status', {
        state: 'completed',
        provider: 'whisper',
        elapsed_ms: 5000,
        is_processing_behind: false,
      });

      listenHelper.emit('transcript_update', {
        finalized_text: 'Some transcript text',
        draft_text: null,
        segment_count: 1,
      });

      const transcriptHeader = await findByText('Transcript');
      expect(transcriptHeader).toBeInTheDocument();
      // The clickable area is now a button with review-section-header-left class
      const headerButton = transcriptHeader.closest('button.review-section-header-left');
      expect(headerButton).toBeTruthy();
      expect(headerButton).toHaveAttribute('aria-expanded', 'true');
    });
  });
});
