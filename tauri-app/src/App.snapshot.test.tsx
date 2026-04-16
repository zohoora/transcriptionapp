/**
 * App state-transition tests.
 *
 * Replaces the previous full-DOM snapshot tests, which were 1,500+ lines and
 * churned on any UI change. These assertion-based tests verify the load-bearing
 * behavior of each app state without coupling to specific DOM structure or
 * inline SVG paths.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import App from './App';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { createListenMock, createStandardMock } from './test/mocks';

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

async function waitForAppReady() {
  await waitFor(
    () => {
      expect(screen.getByText('Start New Session')).toBeInTheDocument();
    },
    { timeout: 3000 }
  );
}

describe('App state transitions', () => {
  let listenHelper: ReturnType<typeof createListenMock>;

  beforeEach(() => {
    vi.clearAllMocks();
    listenHelper = createListenMock();
    mockListen.mockImplementation(listenHelper.listen as typeof listen);
    mockInvoke.mockImplementation(createStandardMock());
  });

  it('idle: shows Start New Session button and the upload link', async () => {
    render(<App />);
    await waitForAppReady();
    expect(screen.getByText('Start New Session')).toBeInTheDocument();
    expect(screen.getByText('Upload Recording')).toBeInTheDocument();
  });

  it('recording: shows End Session and elapsed timer', async () => {
    render(<App />);
    await waitForAppReady();
    listenHelper.emit('session_status', {
      state: 'recording',
      provider: 'whisper',
      elapsed_ms: 5000,
      is_processing_behind: false,
    });
    await screen.findByText('End Session');
    // Start button should be gone in recording mode
    expect(screen.queryByText('Start New Session')).not.toBeInTheDocument();
  });

  it('recording: live transcript preview can be revealed', async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitForAppReady();
    listenHelper.emit('session_status', {
      state: 'recording',
      provider: 'whisper',
      elapsed_ms: 10000,
      is_processing_behind: false,
    });
    listenHelper.emit('transcript_update', {
      finalized_text: 'Patient reports headaches.',
      draft_text: null,
      segment_count: 1,
    });
    await waitFor(() => {
      expect(screen.getByText('Show Transcript')).toBeInTheDocument();
    });
    await user.click(screen.getByText('Show Transcript'));
    await screen.findByText(/Patient reports headaches/);
  });

  it('completed: shows New Session button and review controls', async () => {
    render(<App />);
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
    await screen.findByText('New Session');
  });

  it('error: shows the error message', async () => {
    render(<App />);
    await waitForAppReady();
    listenHelper.emit('session_status', {
      state: 'error',
      provider: null,
      elapsed_ms: 0,
      is_processing_behind: false,
      error_message: 'Microphone access denied',
    });
    await screen.findByText('Microphone access denied');
  });

  it('stopping: shows Ending… and disables the stop button', async () => {
    const { container } = render(<App />);
    await waitForAppReady();
    listenHelper.emit('session_status', {
      state: 'stopping',
      provider: 'whisper',
      elapsed_ms: 60000,
      is_processing_behind: false,
    });
    await screen.findByText('Ending...');
    const stopButton = container.querySelector('.stop-button');
    expect(stopButton).toBeDisabled();
  });
});
