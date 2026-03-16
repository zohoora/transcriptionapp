import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import FeedbackPanel from './FeedbackPanel';
import { invoke } from '@tauri-apps/api/core';
import type { SessionFeedback } from '../types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

const defaultProps = {
  sessionId: 'test-session-1',
  date: '2025-01-15',
  feedback: null as SessionFeedback | null,
  onFeedbackChange: vi.fn(),
  isMultiPatient: false,
  activePatient: 0,
  patientCount: 1,
  isContinuousMode: false,
};

describe('FeedbackPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockResolvedValue(undefined);
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders thumbs up and down buttons', () => {
    render(<FeedbackPanel {...defaultProps} />);

    expect(screen.getByTitle('Good note')).toBeInTheDocument();
    expect(screen.getByTitle('Needs improvement')).toBeInTheDocument();
  });

  it('renders "Add details" link', () => {
    render(<FeedbackPanel {...defaultProps} />);
    expect(screen.getByText('Add details')).toBeInTheDocument();
  });

  it('toggles thumbs up on click', async () => {
    const onFeedbackChange = vi.fn();
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(<FeedbackPanel {...defaultProps} onFeedbackChange={onFeedbackChange} />);

    await user.click(screen.getByTitle('Good note'));

    expect(onFeedbackChange).toHaveBeenCalledWith(
      expect.objectContaining({ qualityRating: 'good' }),
    );
  });

  it('deselects same rating on second click', async () => {
    const onFeedbackChange = vi.fn();
    const existingFeedback: SessionFeedback = {
      schemaVersion: 1,
      createdAt: '2025-01-15T10:00:00Z',
      updatedAt: '2025-01-15T10:00:00Z',
      qualityRating: 'good',
      detectionFeedback: null,
      patientFeedback: [],
      comments: null,
    };
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(
      <FeedbackPanel {...defaultProps} feedback={existingFeedback} onFeedbackChange={onFeedbackChange} />,
    );

    await user.click(screen.getByTitle('Good note'));

    expect(onFeedbackChange).toHaveBeenCalledWith(
      expect.objectContaining({ qualityRating: null }),
    );
  });

  it('hides detection feedback for session mode', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(<FeedbackPanel {...defaultProps} isContinuousMode={false} />);

    // Expand details
    await user.click(screen.getByText('Add details'));

    // Detection section should not be present
    expect(screen.queryByText('Detection Quality')).not.toBeInTheDocument();
    // Content section should be present
    expect(screen.getByText('Content Issues')).toBeInTheDocument();
  });

  it('shows detection feedback for continuous mode', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(<FeedbackPanel {...defaultProps} isContinuousMode={true} />);

    // Expand details
    await user.click(screen.getByText('Add details'));

    // Detection section should be present
    expect(screen.getByText('Detection Quality')).toBeInTheDocument();
    expect(screen.getByText('Merged different encounters')).toBeInTheDocument();
    expect(screen.getByText('Fragment of a larger encounter')).toBeInTheDocument();
  });

  it('shows multi-patient label when applicable', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(
      <FeedbackPanel {...defaultProps} isMultiPatient={true} activePatient={0} patientCount={2} />,
    );

    await user.click(screen.getByText('Add details'));

    expect(screen.getByText(/Patient 1 of 2/)).toBeInTheDocument();
  });

  it('auto-saves with debounce', async () => {
    const onFeedbackChange = vi.fn();
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(<FeedbackPanel {...defaultProps} onFeedbackChange={onFeedbackChange} />);

    await user.click(screen.getByTitle('Good note'));

    // Should not have saved yet (debounce)
    expect(mockInvoke).not.toHaveBeenCalledWith('save_session_feedback', expect.anything());

    // Advance past debounce
    await vi.advanceTimersByTimeAsync(600);

    expect(mockInvoke).toHaveBeenCalledWith('save_session_feedback', expect.objectContaining({
      sessionId: 'test-session-1',
      date: '2025-01-15',
    }));
  });

  it('renders content issue checkboxes', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(<FeedbackPanel {...defaultProps} />);

    await user.click(screen.getByText('Add details'));

    expect(screen.getByText('Missed clinical details')).toBeInTheDocument();
    expect(screen.getByText('Inaccurate information')).toBeInTheDocument();
    expect(screen.getByText('Wrong patient attribution')).toBeInTheDocument();
    expect(screen.getByText('Hallucinated content')).toBeInTheDocument();
  });

  it('renders comments textarea when expanded', async () => {
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(<FeedbackPanel {...defaultProps} />);

    await user.click(screen.getByText('Add details'));

    expect(screen.getByPlaceholderText('Any other feedback...')).toBeInTheDocument();
  });
});
