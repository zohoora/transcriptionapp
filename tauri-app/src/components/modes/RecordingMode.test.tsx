import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { RecordingMode } from './RecordingMode';
import type { AudioQualitySnapshot } from '../../types';

describe('RecordingMode', () => {
  const defaultProps = {
    elapsedMs: 65000, // 1:05
    audioQuality: null,
    biomarkers: null,
    transcriptText: '',
    draftText: null,
    isStopping: false,
    onStop: vi.fn(),
    whisperMode: 'remote' as const,
    whisperModel: 'large-v3-turbo',
    sessionNotes: '',
    onSessionNotesChange: vi.fn(),
    silenceWarning: null,
    chatMessages: [],
    chatIsLoading: false,
    chatError: null,
    onChatSendMessage: vi.fn(),
    onChatClear: vi.fn(),
    predictiveHint: '',
    predictiveHintLoading: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('session progress indicator', () => {
    it('shows session in progress message', () => {
      render(<RecordingMode {...defaultProps} />);
      expect(screen.getByText('Session in progress...')).toBeInTheDocument();
    });

    it('shows pulse dot animation', () => {
      render(<RecordingMode {...defaultProps} />);
      const pulseDots = document.querySelectorAll('.pulse-dot');
      expect(pulseDots.length).toBe(3);
    });
  });

  describe('end session button', () => {
    it('renders enabled end session button by default', () => {
      render(<RecordingMode {...defaultProps} />);

      const button = screen.getByRole('button', { name: /end session/i });
      expect(button).not.toBeDisabled();
      expect(button).not.toHaveClass('stopping');
      expect(screen.getByText('End Session')).toBeInTheDocument();
    });

    it('renders disabled button when stopping', () => {
      render(<RecordingMode {...defaultProps} isStopping={true} />);

      const button = screen.getByRole('button', { name: /ending session/i });
      expect(button).toBeDisabled();
      expect(button).toHaveClass('stopping');
      expect(screen.getByText('Ending...')).toBeInTheDocument();
    });

    it('calls onStop when end session button is clicked', () => {
      const onStop = vi.fn();
      render(<RecordingMode {...defaultProps} onStop={onStop} />);

      fireEvent.click(screen.getByRole('button', { name: /end session/i }));
      expect(onStop).toHaveBeenCalledTimes(1);
    });

    it('does not call onStop when stopping', () => {
      const onStop = vi.fn();
      render(<RecordingMode {...defaultProps} isStopping={true} onStop={onStop} />);

      fireEvent.click(screen.getByRole('button', { name: /ending session/i }));
      expect(onStop).not.toHaveBeenCalled();
    });
  });

  describe('audio quality indicator', () => {
    it('shows good audio status when quality is good', () => {
      const audioQuality: AudioQualitySnapshot = {
        timestamp_ms: 1000,
        peak_db: -3,
        rms_db: -20,
        snr_db: 25,
        clipped_ratio: 0,
        dropout_count: 0,
        total_clipped: 0,
        silence_ratio: 0.1,
        noise_floor_db: -50,
      };
      render(<RecordingMode {...defaultProps} audioQuality={audioQuality} />);

      const qualityIndicator = document.querySelector('.quality-indicator');
      expect(qualityIndicator).toHaveClass('good');
      expect(screen.getByText('Good audio')).toBeInTheDocument();
    });

    it('shows fair audio status when quality is marginal', () => {
      const audioQuality: AudioQualitySnapshot = {
        timestamp_ms: 1000,
        peak_db: -3,
        rms_db: -45, // too quiet
        snr_db: 12,
        clipped_ratio: 0,
        dropout_count: 0,
        total_clipped: 0,
        silence_ratio: 0.1,
        noise_floor_db: -50,
      };
      render(<RecordingMode {...defaultProps} audioQuality={audioQuality} />);

      const qualityIndicator = document.querySelector('.quality-indicator');
      expect(qualityIndicator).toHaveClass('fair');
      expect(screen.getByText('Fair audio')).toBeInTheDocument();
    });

    it('shows poor audio status when quality is bad', () => {
      const audioQuality: AudioQualitySnapshot = {
        timestamp_ms: 1000,
        peak_db: 0,
        rms_db: -5,
        snr_db: 5, // very low SNR
        clipped_ratio: 0.02, // clipping
        dropout_count: 0,
        total_clipped: 100,
        silence_ratio: 0.1,
        noise_floor_db: -20,
      };
      render(<RecordingMode {...defaultProps} audioQuality={audioQuality} />);

      const qualityIndicator = document.querySelector('.quality-indicator');
      expect(qualityIndicator).toHaveClass('poor');
      expect(screen.getByText('Poor audio')).toBeInTheDocument();
    });

    it('defaults to good when no audio quality data', () => {
      render(<RecordingMode {...defaultProps} audioQuality={null} />);

      const qualityIndicator = document.querySelector('.quality-indicator');
      expect(qualityIndicator).toHaveClass('good');
      expect(screen.getByText('Good audio')).toBeInTheDocument();
    });
  });

  describe('details popover', () => {
    const audioQuality: AudioQualitySnapshot = {
      timestamp_ms: 1000,
      peak_db: -3,
      rms_db: -20,
      snr_db: 25,
      clipped_ratio: 0,
      dropout_count: 0,
      total_clipped: 5,
      silence_ratio: 0.1,
      noise_floor_db: -50,
    };

    it('shows details popover when quality indicator is clicked', () => {
      render(<RecordingMode {...defaultProps} audioQuality={audioQuality} />);

      expect(screen.queryByText('Level')).not.toBeInTheDocument();

      fireEvent.click(screen.getByLabelText(/audio quality/i));

      expect(screen.getByText('Level')).toBeInTheDocument();
      expect(screen.getByText('-20 dB')).toBeInTheDocument();
      expect(screen.getByText('SNR')).toBeInTheDocument();
      expect(screen.getByText('25 dB')).toBeInTheDocument();
    });

    it('shows clipping count when present', () => {
      render(<RecordingMode {...defaultProps} audioQuality={audioQuality} />);

      fireEvent.click(screen.getByLabelText(/audio quality/i));

      expect(screen.getByText('Clips')).toBeInTheDocument();
      expect(screen.getByText('5')).toBeInTheDocument();
    });

    it('does not show clips when zero', () => {
      const noClips = { ...audioQuality, total_clipped: 0 };
      render(<RecordingMode {...defaultProps} audioQuality={noClips} />);

      fireEvent.click(screen.getByLabelText(/audio quality/i));

      expect(screen.queryByText('Clips')).not.toBeInTheDocument();
    });

    it('hides details popover when clicked again', () => {
      render(<RecordingMode {...defaultProps} audioQuality={audioQuality} />);

      fireEvent.click(screen.getByLabelText(/audio quality/i));
      expect(screen.getByText('Level')).toBeInTheDocument();

      fireEvent.click(screen.getByLabelText(/audio quality/i));
      expect(screen.queryByText('Level')).not.toBeInTheDocument();
    });
  });

  describe('transcript toggle', () => {
    it('shows "Show Transcript" button by default', () => {
      render(<RecordingMode {...defaultProps} />);

      expect(screen.getByText('Show Transcript')).toBeInTheDocument();
    });

    it('shows transcript preview when toggle is clicked', () => {
      render(
        <RecordingMode
          {...defaultProps}
          transcriptText="Hello, this is the transcript."
        />
      );

      fireEvent.click(screen.getByText('Show Transcript'));

      expect(screen.getByText('Hide Transcript')).toBeInTheDocument();
      expect(screen.getByText('Hello, this is the transcript.')).toBeInTheDocument();
    });

    it('shows "Listening..." when transcript is empty', () => {
      render(<RecordingMode {...defaultProps} transcriptText="" />);

      fireEvent.click(screen.getByText('Show Transcript'));

      expect(screen.getByText('Listening...')).toBeInTheDocument();
    });

    it('shows draft text along with finalized text', () => {
      render(
        <RecordingMode
          {...defaultProps}
          transcriptText="Finalized text."
          draftText="Draft in progress..."
        />
      );

      fireEvent.click(screen.getByText('Show Transcript'));

      expect(screen.getByText('Finalized text.')).toBeInTheDocument();
      expect(screen.getByText('Draft in progress...')).toBeInTheDocument();
    });

    it('hides transcript when toggle is clicked again', () => {
      render(
        <RecordingMode
          {...defaultProps}
          transcriptText="Hello, this is the transcript."
        />
      );

      fireEvent.click(screen.getByText('Show Transcript'));
      expect(screen.getByText('Hello, this is the transcript.')).toBeInTheDocument();

      fireEvent.click(screen.getByText('Hide Transcript'));
      expect(screen.queryByText('Hello, this is the transcript.')).not.toBeInTheDocument();
    });

    it('applies active class when transcript is shown', () => {
      render(<RecordingMode {...defaultProps} />);

      const toggle = screen.getByText('Show Transcript');
      expect(toggle).not.toHaveClass('active');

      fireEvent.click(toggle);
      expect(screen.getByText('Hide Transcript')).toHaveClass('active');
    });
  });

  describe('model indicator', () => {
    it('shows remote model indicator', () => {
      render(<RecordingMode {...defaultProps} whisperMode="remote" whisperModel="large-v3-turbo" />);

      expect(screen.getByText(/🌐 large-v3-turbo/)).toBeInTheDocument();
    });

    it('shows local model indicator', () => {
      render(<RecordingMode {...defaultProps} whisperMode="local" whisperModel="small" />);

      expect(screen.getByText(/💻 small/)).toBeInTheDocument();
    });
  });

  describe('session notes', () => {
    it('shows "Add Notes" button by default', () => {
      render(<RecordingMode {...defaultProps} />);

      expect(screen.getByText('Add Notes')).toBeInTheDocument();
    });

    it('shows notes input when toggle is clicked', () => {
      render(<RecordingMode {...defaultProps} />);

      fireEvent.click(screen.getByText('Add Notes'));

      expect(screen.getByText('Hide Notes')).toBeInTheDocument();
      expect(screen.getByPlaceholderText(/enter observations/i)).toBeInTheDocument();
    });

    it('shows "has-notes" class when notes exist', () => {
      render(<RecordingMode {...defaultProps} sessionNotes="Patient anxious" />);

      const toggle = screen.getByRole('button', { name: /add notes/i });
      expect(toggle).toHaveClass('has-notes');
    });

    it('calls onSessionNotesChange when notes are typed', () => {
      const onSessionNotesChange = vi.fn();
      render(<RecordingMode {...defaultProps} onSessionNotesChange={onSessionNotesChange} />);

      fireEvent.click(screen.getByText('Add Notes'));
      fireEvent.change(screen.getByPlaceholderText(/enter observations/i), {
        target: { value: 'Patient limping' },
      });

      expect(onSessionNotesChange).toHaveBeenCalledWith('Patient limping');
    });

    it('hides notes input when toggle is clicked again', () => {
      render(<RecordingMode {...defaultProps} />);

      fireEvent.click(screen.getByText('Add Notes'));
      expect(screen.getByPlaceholderText(/enter observations/i)).toBeInTheDocument();

      fireEvent.click(screen.getByText('Hide Notes'));
      expect(screen.queryByPlaceholderText(/enter observations/i)).not.toBeInTheDocument();
    });
  });
});
