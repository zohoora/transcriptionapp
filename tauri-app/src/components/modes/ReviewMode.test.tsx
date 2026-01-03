import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ReviewMode } from './ReviewMode';
import type { AudioQualitySnapshot, BiomarkerUpdate, SoapNote, AuthState } from '../../types';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';

// Mock the clipboard plugin
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
  writeText: vi.fn().mockResolvedValue(undefined),
}));

const mockWriteText = vi.mocked(writeText);

describe('ReviewMode', () => {
  const defaultAuthState: AuthState = {
    is_authenticated: false,
    user_email: null,
    user_name: null,
    expires_at: null,
  };

  const authenticatedState: AuthState = {
    is_authenticated: true,
    user_email: 'doctor@example.com',
    user_name: 'Dr. Smith',
    expires_at: '2025-12-31T23:59:59Z',
  };

  const defaultProps = {
    elapsedMs: 120000, // 2:00
    audioQuality: null,
    originalTranscript: 'Original transcript text.',
    editedTranscript: 'Original transcript text.',
    onTranscriptEdit: vi.fn(),
    soapNote: null,
    isGeneratingSoap: false,
    soapError: null,
    ollamaConnected: true,
    onGenerateSoap: vi.fn(),
    biomarkers: null,
    authState: defaultAuthState,
    isSyncing: false,
    syncSuccess: false,
    syncError: null,
    onSync: vi.fn(),
    onClearSyncError: vi.fn(),
    onNewSession: vi.fn(),
    onLogin: vi.fn(),
    onCancelLogin: vi.fn(),
    authLoading: false,
    autoSyncEnabled: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('session summary', () => {
    it('shows completion checkmark and duration', () => {
      render(<ReviewMode {...defaultProps} />);

      expect(screen.getByText('Complete')).toBeInTheDocument();
      expect(screen.getByText('2:00')).toBeInTheDocument();
    });

    it('formats duration with hours when applicable', () => {
      render(<ReviewMode {...defaultProps} elapsedMs={3725000} />); // 1:02:05
      expect(screen.getByText('1:02:05')).toBeInTheDocument();
    });

    it('shows quality badge based on audio quality', () => {
      const goodQuality: AudioQualitySnapshot = {
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
      render(<ReviewMode {...defaultProps} audioQuality={goodQuality} />);

      expect(screen.getByText('Good')).toBeInTheDocument();
    });

    it('shows Unknown quality when no audio data', () => {
      render(<ReviewMode {...defaultProps} audioQuality={null} />);

      expect(screen.getByText('Unknown')).toBeInTheDocument();
    });
  });

  describe('transcript section', () => {
    it('shows transcript content', () => {
      render(<ReviewMode {...defaultProps} />);

      expect(screen.getByText('Transcript')).toBeInTheDocument();
      expect(screen.getByText('Original transcript text.')).toBeInTheDocument();
    });

    it('shows "No transcript recorded" when empty', () => {
      render(<ReviewMode {...defaultProps} editedTranscript="" originalTranscript="" />);

      expect(screen.getByText('No transcript recorded')).toBeInTheDocument();
    });

    it('shows edited badge when transcript is modified', () => {
      render(
        <ReviewMode
          {...defaultProps}
          originalTranscript="Original text"
          editedTranscript="Modified text"
        />
      );

      expect(screen.getByText('edited')).toBeInTheDocument();
    });

    it('does not show edited badge when transcript is unchanged', () => {
      render(<ReviewMode {...defaultProps} />);

      expect(screen.queryByText('edited')).not.toBeInTheDocument();
    });

    it('collapses transcript section when header is clicked', () => {
      render(<ReviewMode {...defaultProps} />);

      expect(screen.getByText('Original transcript text.')).toBeInTheDocument();

      fireEvent.click(screen.getByLabelText(/collapse transcript/i));

      expect(screen.queryByText('Original transcript text.')).not.toBeInTheDocument();
    });

    it('expands transcript section when collapsed header is clicked', () => {
      render(<ReviewMode {...defaultProps} />);

      // Collapse first
      fireEvent.click(screen.getByLabelText(/collapse transcript/i));
      expect(screen.queryByText('Original transcript text.')).not.toBeInTheDocument();

      // Expand
      fireEvent.click(screen.getByLabelText(/expand transcript/i));
      expect(screen.getByText('Original transcript text.')).toBeInTheDocument();
    });
  });

  describe('edit mode', () => {
    it('enters edit mode when Edit button is clicked', () => {
      render(<ReviewMode {...defaultProps} />);

      fireEvent.click(screen.getByText('Edit'));

      expect(screen.getByText('Done')).toBeInTheDocument();
      expect(screen.getByRole('textbox')).toBeInTheDocument();
    });

    it('shows textarea with current transcript in edit mode', () => {
      render(<ReviewMode {...defaultProps} />);

      fireEvent.click(screen.getByText('Edit'));

      const textarea = screen.getByRole('textbox');
      expect(textarea).toHaveValue('Original transcript text.');
    });

    it('calls onTranscriptEdit when text is changed', () => {
      const onTranscriptEdit = vi.fn();
      render(<ReviewMode {...defaultProps} onTranscriptEdit={onTranscriptEdit} />);

      fireEvent.click(screen.getByText('Edit'));
      fireEvent.change(screen.getByRole('textbox'), {
        target: { value: 'Updated text' },
      });

      expect(onTranscriptEdit).toHaveBeenCalledWith('Updated text');
    });

    it('exits edit mode when Done is clicked', () => {
      render(<ReviewMode {...defaultProps} />);

      fireEvent.click(screen.getByText('Edit'));
      expect(screen.getByText('Done')).toBeInTheDocument();

      fireEvent.click(screen.getByText('Done'));
      expect(screen.getByText('Edit')).toBeInTheDocument();
    });
  });

  describe('copy functionality', () => {
    it('copies transcript when Copy button is clicked', async () => {
      render(<ReviewMode {...defaultProps} />);

      fireEvent.click(screen.getByText('Copy'));

      await waitFor(() => {
        expect(mockWriteText).toHaveBeenCalledWith('Original transcript text.');
      });
    });

    it('shows "Copied!" feedback after copy', async () => {
      render(<ReviewMode {...defaultProps} />);

      fireEvent.click(screen.getByText('Copy'));

      await waitFor(() => {
        expect(screen.getByText('Copied!')).toBeInTheDocument();
      });
    });
  });

  describe('SOAP note section', () => {
    it('does not show SOAP section when transcript is empty', () => {
      render(<ReviewMode {...defaultProps} editedTranscript="" />);

      expect(screen.queryByText('SOAP Note')).not.toBeInTheDocument();
    });

    it('shows Generate button when no SOAP note', () => {
      render(<ReviewMode {...defaultProps} />);

      expect(screen.getByText('Generate SOAP Note')).toBeInTheDocument();
    });

    it('disables Generate button when Ollama not connected', () => {
      render(<ReviewMode {...defaultProps} ollamaConnected={false} />);

      expect(screen.getByText('Ollama not connected')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /ollama not connected/i })).toBeDisabled();
    });

    it('calls onGenerateSoap when Generate button is clicked', () => {
      const onGenerateSoap = vi.fn();
      render(<ReviewMode {...defaultProps} onGenerateSoap={onGenerateSoap} />);

      fireEvent.click(screen.getByText('Generate SOAP Note'));

      expect(onGenerateSoap).toHaveBeenCalledTimes(1);
    });

    it('shows loading state when generating SOAP', () => {
      render(<ReviewMode {...defaultProps} isGeneratingSoap={true} />);

      expect(screen.getByText('Generating SOAP note...')).toBeInTheDocument();
      expect(document.querySelector('.spinner-small')).toBeInTheDocument();
    });

    it('shows error with retry button when SOAP generation fails', () => {
      render(<ReviewMode {...defaultProps} soapError="LLM connection failed" />);

      expect(screen.getByText('LLM connection failed')).toBeInTheDocument();
      expect(screen.getByText('Retry')).toBeInTheDocument();
    });

    it('shows SOAP note content when available', () => {
      const soapNote: SoapNote = {
        subjective: 'Patient reports headache for 2 days.',
        objective: 'BP 120/80, HR 72.',
        assessment: 'Tension headache.',
        plan: 'Rest, OTC analgesics.',
        generated_at: '2024-12-15T10:30:00Z',
        model_used: 'qwen3:4b',
        raw_response: null,
      };
      render(<ReviewMode {...defaultProps} soapNote={soapNote} />);

      expect(screen.getByText('SUBJECTIVE')).toBeInTheDocument();
      expect(screen.getByText('Patient reports headache for 2 days.')).toBeInTheDocument();
      expect(screen.getByText('OBJECTIVE')).toBeInTheDocument();
      expect(screen.getByText('BP 120/80, HR 72.')).toBeInTheDocument();
      expect(screen.getByText('ASSESSMENT')).toBeInTheDocument();
      expect(screen.getByText('Tension headache.')).toBeInTheDocument();
      expect(screen.getByText('PLAN')).toBeInTheDocument();
      expect(screen.getByText('Rest, OTC analgesics.')).toBeInTheDocument();
    });

    it('shows model and timestamp for generated SOAP', () => {
      const soapNote: SoapNote = {
        subjective: 'S',
        objective: 'O',
        assessment: 'A',
        plan: 'P',
        generated_at: '2024-12-15T10:30:00Z',
        model_used: 'qwen3:4b',
        raw_response: null,
      };
      render(<ReviewMode {...defaultProps} soapNote={soapNote} />);

      expect(screen.getByText(/qwen3:4b/)).toBeInTheDocument();
    });

    it('copies SOAP note when Copy button is clicked', async () => {
      const soapNote: SoapNote = {
        subjective: 'Subjective text',
        objective: 'Objective text',
        assessment: 'Assessment text',
        plan: 'Plan text',
        generated_at: '2024-12-15T10:30:00Z',
        model_used: 'qwen3:4b',
        raw_response: null,
      };
      render(<ReviewMode {...defaultProps} soapNote={soapNote} />);

      // Find the Copy button in SOAP section (second one, after transcript Copy)
      const copyButtons = screen.getAllByText('Copy');
      expect(copyButtons.length).toBe(2);

      fireEvent.click(copyButtons[1]); // Second Copy button is for SOAP

      // Just verify the clipboard was called - async may complete
      await waitFor(() => {
        expect(mockWriteText).toHaveBeenCalled();
      });

      // Verify the SOAP content format
      const lastCall = mockWriteText.mock.calls[mockWriteText.mock.calls.length - 1][0];
      expect(lastCall).toContain('SUBJECTIVE:');
      expect(lastCall).toContain('Subjective text');
    });
  });

  describe('session insights', () => {
    const biomarkers: BiomarkerUpdate = {
      cough_count: 3,
      cough_rate_per_min: 1.5,
      turn_count: 10,
      vitality_session_mean: 0.7,
      stability_session_mean: 0.8,
      latest_emotions: [],
      speaker_metrics: [
        { speaker_id: 'SPEAKER_1', turn_count: 5, talk_time_ms: 60000 },
        { speaker_id: 'SPEAKER_2', turn_count: 5, talk_time_ms: 55000 },
      ],
      conversation_dynamics: {
        overlap_count: 2,
        interruption_count: 1,
        mean_response_latency_ms: 500,
        long_pause_count: 3,
        total_silence_ms: 10000,
        silence_ratio: 0.1,
        engagement_score: 75,
        per_speaker: [],
      },
    };

    it('shows insights section when biomarkers present', () => {
      render(<ReviewMode {...defaultProps} biomarkers={biomarkers} />);

      expect(screen.getByText('Session Insights')).toBeInTheDocument();
    });

    it('does not show insights section when no biomarkers', () => {
      render(<ReviewMode {...defaultProps} biomarkers={null} />);

      expect(screen.queryByText('Session Insights')).not.toBeInTheDocument();
    });

    it('shows speaker count in summary', () => {
      render(<ReviewMode {...defaultProps} biomarkers={biomarkers} />);

      expect(screen.getByText('2 speakers')).toBeInTheDocument();
    });

    it('expands insights section when clicked', () => {
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
      render(
        <ReviewMode
          {...defaultProps}
          biomarkers={biomarkers}
          audioQuality={audioQuality}
        />
      );

      fireEvent.click(screen.getByLabelText(/expand session insights/i));

      expect(screen.getByText('Audio Quality')).toBeInTheDocument();
      expect(screen.getByText('Speakers')).toBeInTheDocument();
      // Note: Cough display was removed from UI (audio events are sent to LLM for SOAP generation instead)
    });

    it('shows conversation dynamics when available', () => {
      render(<ReviewMode {...defaultProps} biomarkers={biomarkers} />);

      fireEvent.click(screen.getByLabelText(/expand session insights/i));

      expect(screen.getByText('Conversation')).toBeInTheDocument();
      expect(screen.getByText(/500ms/)).toBeInTheDocument();
    });
  });

  describe('sync to Medplum', () => {
    it('shows sync button when authenticated and has transcript', () => {
      render(<ReviewMode {...defaultProps} authState={authenticatedState} />);

      expect(screen.getByText('Sync to Medplum')).toBeInTheDocument();
    });

    it('does not show sync button when not authenticated', () => {
      render(<ReviewMode {...defaultProps} />);

      expect(screen.queryByText('Sync to Medplum')).not.toBeInTheDocument();
    });

    it('does not show sync button when transcript is empty', () => {
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          editedTranscript=""
        />
      );

      expect(screen.queryByText('Sync to Medplum')).not.toBeInTheDocument();
    });

    it('calls onSync when sync button is clicked', () => {
      const onSync = vi.fn();
      render(
        <ReviewMode {...defaultProps} authState={authenticatedState} onSync={onSync} />
      );

      fireEvent.click(screen.getByText('Sync to Medplum'));
      expect(onSync).toHaveBeenCalledTimes(1);
    });

    it('shows syncing state', () => {
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          isSyncing={true}
        />
      );

      expect(screen.getByText('Syncing...')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /syncing/i })).toBeDisabled();
    });

    it('shows synced state', () => {
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          syncSuccess={true}
        />
      );

      expect(screen.getByText(/synced/i)).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /synced/i })).toBeDisabled();
    });

    it('shows sync error toast', () => {
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          syncError="Network error"
        />
      );

      expect(screen.getByText('Network error')).toBeInTheDocument();
    });

    it('clears sync error when dismiss is clicked', () => {
      const onClearSyncError = vi.fn();
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          syncError="Network error"
          onClearSyncError={onClearSyncError}
        />
      );

      fireEvent.click(screen.getByText('×'));
      expect(onClearSyncError).toHaveBeenCalledTimes(1);
    });
  });

  describe('login banner', () => {
    it('shows login banner when auto-sync enabled but not authenticated', () => {
      render(<ReviewMode {...defaultProps} autoSyncEnabled={true} />);

      expect(screen.getByText(/sign in to sync/i)).toBeInTheDocument();
      expect(screen.getByText('Sign In')).toBeInTheDocument();
    });

    it('does not show login banner when authenticated', () => {
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          autoSyncEnabled={true}
        />
      );

      expect(screen.queryByText(/sign in to sync/i)).not.toBeInTheDocument();
    });

    it('does not show login banner when auto-sync disabled', () => {
      render(<ReviewMode {...defaultProps} autoSyncEnabled={false} />);

      expect(screen.queryByText(/sign in to sync/i)).not.toBeInTheDocument();
    });

    it('calls onLogin when Sign In is clicked', () => {
      const onLogin = vi.fn();
      render(<ReviewMode {...defaultProps} autoSyncEnabled={true} onLogin={onLogin} />);

      fireEvent.click(screen.getByText('Sign In'));
      expect(onLogin).toHaveBeenCalledTimes(1);
    });

    it('shows loading state when auth is in progress', () => {
      render(<ReviewMode {...defaultProps} autoSyncEnabled={true} authLoading={true} />);

      expect(screen.getByText('Signing in...')).toBeInTheDocument();
      expect(screen.getByText('Cancel')).toBeInTheDocument();
    });

    it('calls onCancelLogin when Cancel is clicked', () => {
      const onCancelLogin = vi.fn();
      render(
        <ReviewMode
          {...defaultProps}
          autoSyncEnabled={true}
          authLoading={true}
          onCancelLogin={onCancelLogin}
        />
      );

      fireEvent.click(screen.getByText('Cancel'));
      expect(onCancelLogin).toHaveBeenCalledTimes(1);
    });
  });

  describe('new session button', () => {
    it('shows New Session button', () => {
      render(<ReviewMode {...defaultProps} />);

      expect(screen.getByText('New Session')).toBeInTheDocument();
    });

    it('calls onNewSession when clicked', () => {
      const onNewSession = vi.fn();
      render(<ReviewMode {...defaultProps} onNewSession={onNewSession} />);

      fireEvent.click(screen.getByText('New Session'));
      expect(onNewSession).toHaveBeenCalledTimes(1);
    });
  });

  describe('debug raw response', () => {
    it('shows raw response toggle when raw_response present', () => {
      const soapNote: SoapNote = {
        subjective: 'S',
        objective: 'O',
        assessment: 'A',
        plan: 'P',
        generated_at: '2024-12-15T10:30:00Z',
        model_used: 'qwen3:4b',
        raw_response: '{"subjective": "S", "objective": "O"}',
      };
      render(<ReviewMode {...defaultProps} soapNote={soapNote} />);

      expect(screen.getByText('Raw Response')).toBeInTheDocument();
    });

    it('expands raw response when clicked', () => {
      const soapNote: SoapNote = {
        subjective: 'S',
        objective: 'O',
        assessment: 'A',
        plan: 'P',
        generated_at: '2024-12-15T10:30:00Z',
        model_used: 'qwen3:4b',
        raw_response: '{"debug": "data"}',
      };
      render(<ReviewMode {...defaultProps} soapNote={soapNote} />);

      fireEvent.click(screen.getByText('Raw Response'));

      expect(screen.getByText('{"debug": "data"}')).toBeInTheDocument();
    });

    it('does not show raw response toggle when null', () => {
      const soapNote: SoapNote = {
        subjective: 'S',
        objective: 'O',
        assessment: 'A',
        plan: 'P',
        generated_at: '2024-12-15T10:30:00Z',
        model_used: 'qwen3:4b',
        raw_response: null,
      };
      render(<ReviewMode {...defaultProps} soapNote={soapNote} />);

      expect(screen.queryByText('Raw Response')).not.toBeInTheDocument();
    });
  });
});
