import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { RecordingMode } from './RecordingMode';
import type { AudioQualitySnapshot, BiomarkerUpdate } from '../../types';

describe('RecordingMode', () => {
  const defaultProps = {
    elapsedMs: 65000, // 1:05
    audioQuality: null,
    biomarkers: null,
    transcriptText: '',
    draftText: null,
    isStopping: false,
    onStop: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('timer display', () => {
    it('formats time as M:SS for times under an hour', () => {
      render(<RecordingMode {...defaultProps} elapsedMs={65000} />);
      expect(screen.getByText('1:05')).toBeInTheDocument();
    });

    it('formats time as H:MM:SS for times over an hour', () => {
      render(<RecordingMode {...defaultProps} elapsedMs={3725000} />); // 1:02:05
      expect(screen.getByText('1:02:05')).toBeInTheDocument();
    });

    it('shows 0:00 for zero elapsed time', () => {
      render(<RecordingMode {...defaultProps} elapsedMs={0} />);
      expect(screen.getByText('0:00')).toBeInTheDocument();
    });

    it('handles large times correctly', () => {
      render(<RecordingMode {...defaultProps} elapsedMs={36061000} />); // 10:01:01
      expect(screen.getByText('10:01:01')).toBeInTheDocument();
    });
  });

  describe('stop button', () => {
    it('renders enabled stop button by default', () => {
      render(<RecordingMode {...defaultProps} />);

      const button = screen.getByRole('button', { name: /stop recording/i });
      expect(button).not.toBeDisabled();
      expect(button).not.toHaveClass('stopping');
      expect(screen.getByText('STOP')).toBeInTheDocument();
    });

    it('renders disabled stop button when stopping', () => {
      render(<RecordingMode {...defaultProps} isStopping={true} />);

      const button = screen.getByRole('button', { name: /stopping/i });
      expect(button).toBeDisabled();
      expect(button).toHaveClass('stopping');
      expect(screen.getByText('Stopping...')).toBeInTheDocument();
    });

    it('calls onStop when stop button is clicked', () => {
      const onStop = vi.fn();
      render(<RecordingMode {...defaultProps} onStop={onStop} />);

      fireEvent.click(screen.getByRole('button', { name: /stop recording/i }));
      expect(onStop).toHaveBeenCalledTimes(1);
    });

    it('does not call onStop when stopping', () => {
      const onStop = vi.fn();
      render(<RecordingMode {...defaultProps} isStopping={true} onStop={onStop} />);

      fireEvent.click(screen.getByRole('button', { name: /stopping/i }));
      expect(onStop).not.toHaveBeenCalled();
    });
  });

  describe('recording indicator', () => {
    it('shows REC indicator', () => {
      render(<RecordingMode {...defaultProps} />);

      expect(screen.getByText('REC')).toBeInTheDocument();
      expect(document.querySelector('.rec-dot')).toBeInTheDocument();
    });
  });

  describe('audio quality status bars', () => {
    it('shows good status when audio quality is good', () => {
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

      const statusBars = document.querySelector('.status-bars');
      expect(statusBars).toHaveClass('good');
    });

    it('shows fair status when audio quality is marginal', () => {
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

      const statusBars = document.querySelector('.status-bars');
      expect(statusBars).toHaveClass('fair');
    });

    it('shows poor status when audio quality is bad', () => {
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

      const statusBars = document.querySelector('.status-bars');
      expect(statusBars).toHaveClass('poor');
    });

    it('defaults to good when no audio quality data', () => {
      render(<RecordingMode {...defaultProps} audioQuality={null} />);

      const statusBars = document.querySelector('.status-bars');
      expect(statusBars).toHaveClass('good');
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

    it('shows details popover when status bars are clicked', () => {
      render(<RecordingMode {...defaultProps} audioQuality={audioQuality} />);

      expect(screen.queryByText('Level')).not.toBeInTheDocument();

      fireEvent.click(screen.getByLabelText(/audio quality status/i));

      expect(screen.getByText('Level')).toBeInTheDocument();
      expect(screen.getByText('-20 dB')).toBeInTheDocument();
      expect(screen.getByText('SNR')).toBeInTheDocument();
      expect(screen.getByText('25 dB')).toBeInTheDocument();
    });

    it('shows clipping count when present', () => {
      render(<RecordingMode {...defaultProps} audioQuality={audioQuality} />);

      fireEvent.click(screen.getByLabelText(/audio quality status/i));

      expect(screen.getByText('Clips')).toBeInTheDocument();
      expect(screen.getByText('5')).toBeInTheDocument();
    });

    it('does not show clips when zero', () => {
      const noClips = { ...audioQuality, total_clipped: 0 };
      render(<RecordingMode {...defaultProps} audioQuality={noClips} />);

      fireEvent.click(screen.getByLabelText(/audio quality status/i));

      expect(screen.queryByText('Clips')).not.toBeInTheDocument();
    });

    // Note: Cough count display was removed from UI
    // Audio events (including coughs) are now sent to LLM for SOAP note generation instead

    it('hides details popover when clicked again', () => {
      render(<RecordingMode {...defaultProps} audioQuality={audioQuality} />);

      fireEvent.click(screen.getByLabelText(/audio quality status/i));
      expect(screen.getByText('Level')).toBeInTheDocument();

      fireEvent.click(screen.getByLabelText(/audio quality status/i));
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
});
