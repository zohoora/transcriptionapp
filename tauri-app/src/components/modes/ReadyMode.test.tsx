import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ReadyMode } from './ReadyMode';

describe('ReadyMode', () => {
  const defaultProps = {
    audioLevel: 50,
    errorMessage: null,
    onStart: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('start button', () => {
    it('renders enabled start button', () => {
      render(<ReadyMode {...defaultProps} />);

      const button = screen.getByRole('button', { name: /start new session/i });
      expect(button).not.toBeDisabled();
      expect(button).toHaveClass('ready');
    });

    it('calls onStart when start button is clicked', () => {
      const onStart = vi.fn();
      render(<ReadyMode {...defaultProps} onStart={onStart} />);

      fireEvent.click(screen.getByRole('button', { name: /start new session/i }));
      expect(onStart).toHaveBeenCalledTimes(1);
    });

    it('shows "Start Manually" when listening mode is active', () => {
      render(
        <ReadyMode
          {...defaultProps}
          autoStartEnabled={true}
          isListening={true}
        />
      );

      expect(screen.getByText(/start manually/i)).toBeInTheDocument();
    });
  });

  describe('error messages', () => {
    it('shows error message when present', () => {
      render(<ReadyMode {...defaultProps} errorMessage="Audio device not found" />);

      expect(screen.getByText('Audio device not found')).toBeInTheDocument();
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

  describe('listening mode indicator', () => {
    it('shows listening indicator when auto-start enabled and listening', () => {
      render(
        <ReadyMode
          {...defaultProps}
          autoStartEnabled={true}
          isListening={true}
          listeningStatus={{
            is_listening: true,
            speech_detected: false,
            speech_duration_ms: 0,
            analyzing: false,
          }}
        />
      );

      expect(screen.getByText(/listening/i)).toBeInTheDocument();
    });

    it('shows "Speech detected" when speech is detected', () => {
      render(
        <ReadyMode
          {...defaultProps}
          autoStartEnabled={true}
          isListening={true}
          listeningStatus={{
            is_listening: true,
            speech_detected: true,
            speech_duration_ms: 3000,
            analyzing: false,
          }}
        />
      );

      expect(screen.getByText(/speech detected/i)).toBeInTheDocument();
    });

    it('shows "Analyzing..." when analyzing speech', () => {
      render(
        <ReadyMode
          {...defaultProps}
          autoStartEnabled={true}
          isListening={true}
          listeningStatus={{
            is_listening: true,
            speech_detected: true,
            speech_duration_ms: 3000,
            analyzing: true,
          }}
        />
      );

      expect(screen.getByText(/analyzing/i)).toBeInTheDocument();
    });

    it('does not show listening indicator when auto-start is disabled', () => {
      render(
        <ReadyMode
          {...defaultProps}
          autoStartEnabled={false}
          isListening={false}
        />
      );

      expect(screen.queryByText(/listening/i)).not.toBeInTheDocument();
    });
  });
});
