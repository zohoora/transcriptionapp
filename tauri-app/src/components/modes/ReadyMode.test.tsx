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

      const button = screen.getByRole('button', { name: /start recording/i });
      expect(button).not.toBeDisabled();
      expect(button).toHaveClass('ready');
    });

    it('calls onStart when start button is clicked', () => {
      const onStart = vi.fn();
      render(<ReadyMode {...defaultProps} onStart={onStart} />);

      fireEvent.click(screen.getByRole('button', { name: /start recording/i }));
      expect(onStart).toHaveBeenCalledTimes(1);
    });
  });

  describe('status text', () => {
    it('shows "Ready to record" by default', () => {
      render(<ReadyMode {...defaultProps} />);

      expect(screen.getByText(/ready to record/i)).toBeInTheDocument();
    });

    it('shows error message when present', () => {
      render(<ReadyMode {...defaultProps} errorMessage="Audio device not found" />);

      expect(screen.getByText('Audio device not found')).toBeInTheDocument();
      expect(screen.queryByText(/ready to record/i)).not.toBeInTheDocument();
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
});
