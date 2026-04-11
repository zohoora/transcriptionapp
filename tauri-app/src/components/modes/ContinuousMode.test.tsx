import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ContinuousMode } from './ContinuousMode';
import type { ContinuousModeStats, AudioQualitySnapshot } from '../../types';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(''),
}));
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
  writeText: vi.fn().mockResolvedValue(undefined),
}));

// Mock child components to isolate ContinuousMode
vi.mock('../PatientPulse', () => ({
  PatientPulse: () => <div data-testid="patient-pulse" />,
}));
vi.mock('../ImageSuggestions', () => ({
  ImageSuggestions: () => <div data-testid="image-suggestions" />,
}));
vi.mock('../ClinicalChat', () => ({
  MarkdownContent: ({ content }: { content: string }) => <div>{content}</div>,
}));

const IDLE_STATS: ContinuousModeStats = {
  state: 'idle',
  recording_since: '',
  encounters_detected: 0,
  recent_encounters: [],
  last_error: null,
  buffer_word_count: 0,
  buffer_started_at: null,
  is_sleeping: false,
  sleep_resume_at: null,
};

const ACTIVE_STATS: ContinuousModeStats = {
  state: 'recording',
  recording_since: new Date(Date.now() - 3600000).toISOString(), // 1 hour ago
  encounters_detected: 3,
  recent_encounters: [
    {
      sessionId: 'session-123',
      time: new Date(Date.now() - 600000).toISOString(), // 10 min ago
      patientName: 'John Smith',
    },
  ],
  last_error: null,
  buffer_word_count: 120,
  buffer_started_at: new Date(Date.now() - 300000).toISOString(), // 5 min ago
  is_sleeping: false,
  sleep_resume_at: null,
};

function makeDefaultProps(overrides: Partial<Parameters<typeof ContinuousMode>[0]> = {}) {
  return {
    isActive: false,
    isStopping: false,
    stats: IDLE_STATS,
    liveTranscript: '',
    error: null,
    predictiveHint: '',
    predictiveHintLoading: false,
    differentialDiagnoses: [],
    audioQuality: null,
    biomarkers: null,
    biomarkerTrends: { vitalityTrend: 'insufficient' as const, stabilityTrend: 'insufficient' as const },
    encounterNotes: '',
    onEncounterNotesChange: vi.fn(),
    miisSuggestions: [],
    miisLoading: false,
    miisError: null,
    miisEnabled: false,
    onMiisImpression: vi.fn(),
    onMiisClick: vi.fn(),
    onMiisDismiss: vi.fn(),
    miisGetImageUrl: vi.fn((p: string) => p),
    aiImages: [],
    aiLoading: false,
    aiError: null,
    onAiDismiss: vi.fn(),
    imageSource: 'off' as const,
    onStart: vi.fn(),
    onStop: vi.fn(),
    onNewPatient: vi.fn(),
    onViewHistory: vi.fn(),
    ...overrides,
  };
}

describe('ContinuousMode', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('idle state', () => {
    it('shows start button when inactive', () => {
      render(<ContinuousMode {...makeDefaultProps()} />);
      expect(screen.getByText('Start Session')).toBeInTheDocument();
    });

    it('shows heading and description', () => {
      render(<ContinuousMode {...makeDefaultProps()} />);
      expect(screen.getByText('End-of-Day Charting')).toBeInTheDocument();
      expect(screen.getByText(/listens throughout the day/i)).toBeInTheDocument();
    });

    it('displays error when present', () => {
      render(<ContinuousMode {...makeDefaultProps({ error: 'Connection lost' })} />);
      expect(screen.getByText('Connection lost')).toBeInTheDocument();
    });

    it('does not display error when null', () => {
      render(<ContinuousMode {...makeDefaultProps()} />);
      expect(screen.queryByText('Connection lost')).not.toBeInTheDocument();
    });

    it('calls onStart when start button is clicked', () => {
      const onStart = vi.fn();
      render(<ContinuousMode {...makeDefaultProps({ onStart })} />);
      fireEvent.click(screen.getByText('Start Session'));
      expect(onStart).toHaveBeenCalledTimes(1);
    });

    it('shows view history button', () => {
      const onViewHistory = vi.fn();
      render(<ContinuousMode {...makeDefaultProps({ onViewHistory })} />);
      fireEvent.click(screen.getByText('View Past Sessions'));
      expect(onViewHistory).toHaveBeenCalledTimes(1);
    });
  });

  describe('active state', () => {
    it('shows status dot and text when active', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      expect(screen.getByText('Continuous mode active')).toBeInTheDocument();
    });

    it('shows checking text when state is checking', () => {
      const checkingStats = { ...ACTIVE_STATS, state: 'checking' as const };
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: checkingStats })} />);
      expect(screen.getByText('Checking for encounters...')).toBeInTheDocument();
    });

    it('shows stopping text when isStopping', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, isStopping: true, stats: ACTIVE_STATS })} />);
      expect(screen.getByText('Ending... finalizing notes')).toBeInTheDocument();
    });

    it('shows encounter timer with buffer word count', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      expect(screen.getByText(/120 words/)).toBeInTheDocument();
    });

    it('shows buffer info with current encounter', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      // Current encounter shows words
      expect(screen.getByText(/120 words/)).toBeInTheDocument();
    });

    it('shows waiting message when no buffer', () => {
      const noBuffer = { ...ACTIVE_STATS, buffer_started_at: null, buffer_word_count: 0 };
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: noBuffer })} />);
      expect(screen.getByText('Waiting for next patient...')).toBeInTheDocument();
    });

    it('renders audio quality indicator', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      expect(screen.getByLabelText(/audio quality/i)).toBeInTheDocument();
    });

    it('does not show start button when active', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      expect(screen.queryByText('Start Session')).not.toBeInTheDocument();
    });
  });

  describe('new patient button', () => {
    it('fires onNewPatient callback', () => {
      const onNewPatient = vi.fn();
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS, onNewPatient })} />);
      fireEvent.click(screen.getByText('End Previous / Start New'));
      expect(onNewPatient).toHaveBeenCalledTimes(1);
    });

    it('is disabled when stopping', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, isStopping: true, stats: ACTIVE_STATS })} />);
      const btn = screen.getByText('End Previous / Start New').closest('button')!;
      expect(btn).toBeDisabled();
    });

    it('has 2-second cooldown after click', () => {
      vi.useFakeTimers();
      const onNewPatient = vi.fn();
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS, onNewPatient })} />);

      fireEvent.click(screen.getByText('End Previous / Start New'));
      expect(onNewPatient).toHaveBeenCalledTimes(1);

      // Second click within 2s should be ignored (cooldown via ref)
      fireEvent.click(screen.getByText('End Previous / Start New'));
      expect(onNewPatient).toHaveBeenCalledTimes(1);

      // After 2s cooldown, should work again
      vi.advanceTimersByTime(2000);
      fireEvent.click(screen.getByText('End Previous / Start New'));
      expect(onNewPatient).toHaveBeenCalledTimes(2);
      vi.useRealTimers();
    });
  });

  describe('transcript toggle', () => {
    it('shows "Show Transcript" by default', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      expect(screen.getByText('Show Transcript')).toBeInTheDocument();
    });

    it('toggles to show transcript preview', () => {
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: ACTIVE_STATS,
        liveTranscript: 'Patient says hello',
      })} />);

      fireEvent.click(screen.getByText('Show Transcript'));
      expect(screen.getByText('Hide Transcript')).toBeInTheDocument();
      expect(screen.getByText('Patient says hello')).toBeInTheDocument();
    });

    it('shows placeholder when transcript is empty', () => {
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: ACTIVE_STATS,
        liveTranscript: '',
      })} />);

      fireEvent.click(screen.getByText('Show Transcript'));
      expect(screen.getByText('Waiting for speech...')).toBeInTheDocument();
    });

    it('hides transcript when toggled again', () => {
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: ACTIVE_STATS,
        liveTranscript: 'Some text',
      })} />);

      fireEvent.click(screen.getByText('Show Transcript'));
      expect(screen.getByText('Some text')).toBeInTheDocument();

      fireEvent.click(screen.getByText('Hide Transcript'));
      expect(screen.queryByText('Some text')).not.toBeInTheDocument();
    });
  });

  describe('encounter notes', () => {
    it('shows "Add Notes" button by default', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      expect(screen.getByText('Add Notes')).toBeInTheDocument();
    });

    it('shows notes input when toggle is clicked', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      fireEvent.click(screen.getByRole('button', { name: /add notes/i }));
      expect(screen.getByText('Hide Notes')).toBeInTheDocument();
      expect(screen.getByPlaceholderText(/enter observations/i)).toBeInTheDocument();
    });

    it('shows "has-notes" class when notes exist', () => {
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: ACTIVE_STATS,
        encounterNotes: 'Some observation',
      })} />);
      const toggle = screen.getByRole('button', { name: /add notes/i });
      expect(toggle).toHaveClass('has-notes');
    });

    it('fires onEncounterNotesChange when typing', () => {
      const onEncounterNotesChange = vi.fn();
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: ACTIVE_STATS,
        onEncounterNotesChange,
      })} />);

      fireEvent.click(screen.getByRole('button', { name: /add notes/i }));
      fireEvent.change(screen.getByPlaceholderText(/enter observations/i), {
        target: { value: 'Patient limping' },
      });
      expect(onEncounterNotesChange).toHaveBeenCalledWith('Patient limping');
    });

    it('hides notes input when toggled again', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      fireEvent.click(screen.getByRole('button', { name: /add notes/i }));
      expect(screen.getByPlaceholderText(/enter observations/i)).toBeInTheDocument();

      fireEvent.click(screen.getByRole('button', { name: /hide notes/i }));
      expect(screen.queryByPlaceholderText(/enter observations/i)).not.toBeInTheDocument();
    });
  });

  describe('end session button', () => {
    it('fires onStop when clicked', () => {
      const onStop = vi.fn();
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS, onStop })} />);
      fireEvent.click(screen.getByText('End Session'));
      expect(onStop).toHaveBeenCalledTimes(1);
    });

    it('is disabled when stopping', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, isStopping: true, stats: ACTIVE_STATS })} />);
      const btn = screen.getByText('Ending...').closest('button')!;
      expect(btn).toBeDisabled();
    });

    it('shows "Ending..." text when stopping', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, isStopping: true, stats: ACTIVE_STATS })} />);
      expect(screen.getByText('Ending...')).toBeInTheDocument();
      expect(screen.queryByText('End Session')).not.toBeInTheDocument();
    });
  });

  describe('sensor status display', () => {
    it('shows sensor connected present when sensor_connected and state present', () => {
      const sensorStats: ContinuousModeStats = {
        ...ACTIVE_STATS,
        sensor_connected: true,
        sensor_state: 'present',
      };
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: sensorStats })} />);
      expect(screen.getByText(/Sensor: Present/)).toBeInTheDocument();
    });

    it('shows sensor absent when sensor_state is absent', () => {
      const sensorStats: ContinuousModeStats = {
        ...ACTIVE_STATS,
        sensor_connected: true,
        sensor_state: 'absent',
      };
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: sensorStats })} />);
      expect(screen.getByText(/Sensor: Absent/)).toBeInTheDocument();
    });

    it('shows sensor disconnected when not connected', () => {
      const sensorStats: ContinuousModeStats = {
        ...ACTIVE_STATS,
        sensor_connected: false,
        sensor_state: 'unknown',
      };
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: sensorStats })} />);
      expect(screen.getByText(/Sensor: Disconnected/)).toBeInTheDocument();
    });

    it('hides sensor display when sensor_connected is undefined', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      expect(screen.queryByText(/Sensor:/)).not.toBeInTheDocument();
    });
  });

  describe('shadow mode display', () => {
    it('shows shadow indicator when shadow_mode_active is true', () => {
      const shadowStats: ContinuousModeStats = {
        ...ACTIVE_STATS,
        shadow_mode_active: true,
        shadow_method: 'sensor',
        last_shadow_outcome: 'would_split',
      };
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: shadowStats })} />);
      expect(screen.getByText(/Shadow \(SENSOR\)/)).toBeInTheDocument();
      expect(screen.getByText(/Would split/)).toBeInTheDocument();
    });

    it('shows observing when shadow is not would_split', () => {
      const shadowStats: ContinuousModeStats = {
        ...ACTIVE_STATS,
        shadow_mode_active: true,
        shadow_method: 'llm',
        last_shadow_outcome: 'would_not_split',
      };
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: shadowStats })} />);
      expect(screen.getByText(/Shadow \(LLM\)/)).toBeInTheDocument();
      expect(screen.getByText(/Observing.../)).toBeInTheDocument();
    });

    it('hides shadow display when not active', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      expect(screen.queryByText(/Shadow/)).not.toBeInTheDocument();
    });
  });

  describe('recent encounters list', () => {
    it('shows recent encounters when available', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      expect(screen.getByText('Recent encounters')).toBeInTheDocument();
      expect(screen.getByText('John Smith')).toBeInTheDocument();
    });

    it('hides recent encounters when list is empty', () => {
      const noEncounters = { ...ACTIVE_STATS, recent_encounters: [] };
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: noEncounters })} />);
      expect(screen.queryByText('Recent encounters')).not.toBeInTheDocument();
    });
  });

  describe('predictive hint', () => {
    it('shows hint when available', () => {
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: ACTIVE_STATS,
        predictiveHint: 'Consider checking blood pressure',
      })} />);
      expect(screen.getByText('Pssst...')).toBeInTheDocument();
      expect(screen.getByText('Consider checking blood pressure')).toBeInTheDocument();
    });

    it('shows loading state for hint', () => {
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: ACTIVE_STATS,
        predictiveHintLoading: true,
      })} />);
      expect(screen.getByText('Pssst...')).toBeInTheDocument();
      expect(screen.getByText('Thinking...')).toBeInTheDocument();
    });

    it('hides hint when not available and not loading', () => {
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: ACTIVE_STATS,
        predictiveHint: '',
        predictiveHintLoading: false,
      })} />);
      expect(screen.queryByText('Pssst...')).not.toBeInTheDocument();
    });
  });

  describe('error display in active state', () => {
    it('shows error when present', () => {
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: ACTIVE_STATS,
        error: 'LLM connection lost',
      })} />);
      expect(screen.getByText('LLM connection lost')).toBeInTheDocument();
    });

    it('shows stats.last_error when no prop error', () => {
      const statsWithError = { ...ACTIVE_STATS, last_error: 'SOAP generation failed' };
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: statsWithError,
      })} />);
      expect(screen.getByText('SOAP generation failed')).toBeInTheDocument();
    });
  });

  describe('image suggestions', () => {
    it('renders ImageSuggestions when miisEnabled', () => {
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: ACTIVE_STATS,
        miisEnabled: true,
      })} />);
      expect(screen.getByTestId('image-suggestions')).toBeInTheDocument();
    });

    it('does not render ImageSuggestions when not enabled', () => {
      render(<ContinuousMode {...makeDefaultProps({
        isActive: true,
        stats: ACTIVE_STATS,
        miisEnabled: false,
      })} />);
      expect(screen.queryByTestId('image-suggestions')).not.toBeInTheDocument();
    });
  });

  describe('patient pulse', () => {
    it('renders PatientPulse in active state', () => {
      render(<ContinuousMode {...makeDefaultProps({ isActive: true, stats: ACTIVE_STATS })} />);
      expect(screen.getByTestId('patient-pulse')).toBeInTheDocument();
    });
  });
});
