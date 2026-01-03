import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ReadyMode } from './ReadyMode';
import type { ModelStatus, ChecklistResult, CheckStatus } from '../../types';

describe('ReadyMode', () => {
  const defaultProps = {
    modelStatus: { available: true, model_name: 'small', path: '/path/to/model' } as ModelStatus,
    modelName: 'small',
    checklistRunning: false,
    checklistResult: null,
    onRunChecklist: vi.fn(),
    onDownloadModel: vi.fn(),
    downloadingModel: null,
    audioLevel: 50,
    errorMessage: null,
    canStart: true,
    onStart: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('start button', () => {
    it('renders enabled start button when canStart is true', () => {
      render(<ReadyMode {...defaultProps} />);

      const button = screen.getByRole('button', { name: /start recording/i });
      expect(button).not.toBeDisabled();
      expect(button).toHaveClass('ready');
    });

    it('renders disabled start button when canStart is false', () => {
      render(<ReadyMode {...defaultProps} canStart={false} />);

      const button = screen.getByRole('button', { name: /start recording/i });
      expect(button).toBeDisabled();
      expect(button).toHaveClass('disabled');
    });

    it('calls onStart when start button is clicked', () => {
      const onStart = vi.fn();
      render(<ReadyMode {...defaultProps} onStart={onStart} />);

      fireEvent.click(screen.getByRole('button', { name: /start recording/i }));
      expect(onStart).toHaveBeenCalledTimes(1);
    });

    it('does not call onStart when button is disabled', () => {
      const onStart = vi.fn();
      render(<ReadyMode {...defaultProps} canStart={false} onStart={onStart} />);

      fireEvent.click(screen.getByRole('button', { name: /start recording/i }));
      expect(onStart).not.toHaveBeenCalled();
    });
  });

  describe('status text', () => {
    it('shows "Ready to record" when all checks pass', () => {
      render(<ReadyMode {...defaultProps} />);

      expect(screen.getByText(/ready to record/i)).toBeInTheDocument();
      expect(screen.getByText('small')).toBeInTheDocument();
    });

    it('shows error message when present', () => {
      render(<ReadyMode {...defaultProps} errorMessage="Audio device not found" />);

      expect(screen.getByText('Audio device not found')).toBeInTheDocument();
      expect(screen.queryByText(/ready to record/i)).not.toBeInTheDocument();
    });

    it('shows "Running checks..." when checklist is running', () => {
      render(<ReadyMode {...defaultProps} checklistRunning={true} />);

      // Multiple "Running checks..." elements exist (status text and loading text)
      expect(screen.getAllByText(/running checks/i).length).toBeGreaterThan(0);
    });

    it('shows "Model not found" when model is unavailable', () => {
      render(
        <ReadyMode
          {...defaultProps}
          modelStatus={{ available: false, model_name: 'small', path: null }}
        />
      );

      expect(screen.getByText(/model not found/i)).toBeInTheDocument();
    });

    it('shows checklist summary when checks fail', () => {
      const checklistResult: ChecklistResult = {
        can_start: false,
        summary: '1 check failed',
        checks: [
          {
            id: 'whisper_model',
            name: 'Whisper Model',
            status: 'fail' as CheckStatus,
            message: 'Model not found',
            action: null,
          },
        ],
      };
      render(<ReadyMode {...defaultProps} checklistResult={checklistResult} />);

      expect(screen.getByText('1 check failed')).toBeInTheDocument();
    });
  });

  describe('mic level preview', () => {
    it('renders mic level bar with correct width', () => {
      render(<ReadyMode {...defaultProps} audioLevel={75} />);

      const fill = document.querySelector('.mic-level-fill');
      expect(fill).toHaveStyle({ width: '75%' });
    });

    it('clamps audio level to 0-100 range', () => {
      const { rerender } = render(<ReadyMode {...defaultProps} audioLevel={150} />);

      let fill = document.querySelector('.mic-level-fill');
      expect(fill).toHaveStyle({ width: '100%' });

      rerender(<ReadyMode {...defaultProps} audioLevel={-50} />);
      fill = document.querySelector('.mic-level-fill');
      expect(fill).toHaveStyle({ width: '0%' });
    });

    it('defaults to 0% when audioLevel is undefined', () => {
      render(<ReadyMode {...defaultProps} audioLevel={undefined} />);

      const fill = document.querySelector('.mic-level-fill');
      expect(fill).toHaveStyle({ width: '0%' });
    });
  });

  describe('inline checklist', () => {
    it('shows loading spinner when checklist is running', () => {
      render(<ReadyMode {...defaultProps} checklistRunning={true} />);

      // Multiple "Running checks..." elements exist (status text and loading text)
      expect(screen.getAllByText(/running checks/i).length).toBeGreaterThan(0);
      expect(document.querySelector('.spinner-small')).toBeInTheDocument();
    });

    it('shows checklist items when checks fail', () => {
      const checklistResult: ChecklistResult = {
        can_start: false,
        summary: '2 checks failed',
        checks: [
          {
            id: 'whisper_model',
            name: 'Whisper Model',
            status: 'fail' as CheckStatus,
            message: 'Model not found',
            action: { download_model: { model_name: 'small' } },
          },
          {
            id: 'audio_device',
            name: 'Audio Device',
            status: 'pass' as CheckStatus,
            message: null,
            action: null,
          },
        ],
      };
      render(<ReadyMode {...defaultProps} checklistResult={checklistResult} />);

      expect(screen.getByText('Whisper Model')).toBeInTheDocument();
      expect(screen.getByText('Audio Device')).toBeInTheDocument();
      expect(screen.getByText('Model not found')).toBeInTheDocument();
    });

    it('does not show checklist when all checks pass', () => {
      const checklistResult: ChecklistResult = {
        can_start: true,
        summary: 'All checks passed',
        checks: [
          {
            id: 'whisper_model',
            name: 'Whisper Model',
            status: 'pass' as CheckStatus,
            message: null,
            action: null,
          },
        ],
      };
      render(<ReadyMode {...defaultProps} checklistResult={checklistResult} />);

      expect(screen.queryByText('Whisper Model')).not.toBeInTheDocument();
    });

    it('shows download button for failed model checks', () => {
      const checklistResult: ChecklistResult = {
        can_start: false,
        summary: '1 check failed',
        checks: [
          {
            id: 'whisper_model',
            name: 'Whisper Model',
            status: 'fail' as CheckStatus,
            message: 'Model not found',
            action: { download_model: { model_name: 'small' } },
          },
        ],
      };
      render(<ReadyMode {...defaultProps} checklistResult={checklistResult} />);

      expect(screen.getByRole('button', { name: /get/i })).toBeInTheDocument();
    });

    it('calls onDownloadModel when download button is clicked', () => {
      const onDownloadModel = vi.fn();
      const checklistResult: ChecklistResult = {
        can_start: false,
        summary: '1 check failed',
        checks: [
          {
            id: 'whisper_model',
            name: 'Whisper Model',
            status: 'fail' as CheckStatus,
            message: 'Model not found',
            action: { download_model: { model_name: 'small' } },
          },
        ],
      };
      render(
        <ReadyMode
          {...defaultProps}
          checklistResult={checklistResult}
          onDownloadModel={onDownloadModel}
        />
      );

      fireEvent.click(screen.getByRole('button', { name: /get/i }));
      expect(onDownloadModel).toHaveBeenCalledWith('small');
    });

    it('shows "..." on download button when downloading', () => {
      const checklistResult: ChecklistResult = {
        can_start: false,
        summary: '1 check failed',
        checks: [
          {
            id: 'whisper_model',
            name: 'Whisper Model',
            status: 'fail' as CheckStatus,
            message: 'Model not found',
            action: { download_model: { model_name: 'small' } },
          },
        ],
      };
      render(
        <ReadyMode
          {...defaultProps}
          checklistResult={checklistResult}
          downloadingModel="small"
        />
      );

      expect(screen.getByText('...')).toBeInTheDocument();
    });

    it('disables download button when downloading', () => {
      const checklistResult: ChecklistResult = {
        can_start: false,
        summary: '1 check failed',
        checks: [
          {
            id: 'whisper_model',
            name: 'Whisper Model',
            status: 'fail' as CheckStatus,
            message: 'Model not found',
            action: { download_model: { model_name: 'small' } },
          },
        ],
      };
      render(
        <ReadyMode
          {...defaultProps}
          checklistResult={checklistResult}
          downloadingModel="small"
        />
      );

      expect(screen.getByText('...')).toBeDisabled();
    });

    it('shows re-check button when checklist has failures', () => {
      const checklistResult: ChecklistResult = {
        can_start: false,
        summary: '1 check failed',
        checks: [
          {
            id: 'audio_device',
            name: 'Audio Device',
            status: 'fail' as CheckStatus,
            message: 'No audio device',
            action: null,
          },
        ],
      };
      render(<ReadyMode {...defaultProps} checklistResult={checklistResult} />);

      expect(screen.getByRole('button', { name: /re-check/i })).toBeInTheDocument();
    });

    it('calls onRunChecklist when re-check button is clicked', () => {
      const onRunChecklist = vi.fn();
      const checklistResult: ChecklistResult = {
        can_start: false,
        summary: '1 check failed',
        checks: [
          {
            id: 'audio_device',
            name: 'Audio Device',
            status: 'fail' as CheckStatus,
            message: 'No audio device',
            action: null,
          },
        ],
      };
      render(
        <ReadyMode
          {...defaultProps}
          checklistResult={checklistResult}
          onRunChecklist={onRunChecklist}
        />
      );

      fireEvent.click(screen.getByRole('button', { name: /re-check/i }));
      expect(onRunChecklist).toHaveBeenCalledTimes(1);
    });
  });

  describe('check status icons', () => {
    it('shows correct icons for different statuses', () => {
      const checklistResult: ChecklistResult = {
        can_start: false,
        summary: 'Mixed statuses',
        checks: [
          {
            id: 'check1',
            name: 'Pass Check',
            status: 'pass' as CheckStatus,
            message: null,
            action: null,
          },
          {
            id: 'check2',
            name: 'Fail Check',
            status: 'fail' as CheckStatus,
            message: 'Failed',
            action: null,
          },
          {
            id: 'check3',
            name: 'Warning Check',
            status: 'warning' as CheckStatus,
            message: 'Warning',
            action: null,
          },
        ],
      };
      render(<ReadyMode {...defaultProps} checklistResult={checklistResult} />);

      // Check for presence of check items with correct classes
      const passItem = document.querySelector('.check-pass');
      const failItem = document.querySelector('.check-fail');
      const warningItem = document.querySelector('.check-warning');

      expect(passItem).toBeInTheDocument();
      expect(failItem).toBeInTheDocument();
      expect(warningItem).toBeInTheDocument();
    });
  });
});
