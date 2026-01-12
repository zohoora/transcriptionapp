import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ReviewMode } from './ReviewMode';
import type { AudioQualitySnapshot, BiomarkerUpdate, SoapNote, AuthState, SoapOptions, MultiPatientSoapResult } from '../../types';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';

// Mock the clipboard plugin
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
  writeText: vi.fn().mockResolvedValue(undefined),
}));

const mockWriteText = vi.mocked(writeText);

// Helper to wrap a SoapNote in a MultiPatientSoapResult
const createSoapResult = (soap: SoapNote): MultiPatientSoapResult => ({
  notes: [{
    patient_label: 'Patient 1',
    speaker_id: 'Speaker 1',
    soap,
  }],
  physician_speaker: 'Speaker 2',
  generated_at: soap.generated_at,
  model_used: soap.model_used,
});

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

  const defaultSoapOptions: SoapOptions = {
    detail_level: 5,
    format: 'problem_based',
    custom_instructions: '',
  };

  const defaultProps = {
    elapsedMs: 120000, // 2:00
    audioQuality: null,
    originalTranscript: 'Original transcript text.',
    editedTranscript: 'Original transcript text.',
    onTranscriptEdit: vi.fn(),
    soapResult: null as MultiPatientSoapResult | null,
    isGeneratingSoap: false,
    soapError: null,
    llmConnected: true,
    onGenerateSoap: vi.fn(),
    soapOptions: defaultSoapOptions,
    onSoapDetailLevelChange: vi.fn(),
    onSoapFormatChange: vi.fn(),
    onSoapCustomInstructionsChange: vi.fn(),
    biomarkers: null,
    whisperMode: 'remote' as const,
    whisperModel: 'large-v3-turbo',
    authState: defaultAuthState,
    isSyncing: false,
    syncSuccess: false,
    syncError: null,
    syncedEncounter: null,
    isAddingSoap: false,
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

  describe('tabs', () => {
    it('shows Transcript, SOAP, and Insights tabs', () => {
      render(<ReviewMode {...defaultProps} />);

      // Get tab buttons by their class
      const tabButtons = screen.getAllByRole('button').filter(btn => btn.classList.contains('review-tab'));
      expect(tabButtons).toHaveLength(3);
      expect(tabButtons[0]).toHaveTextContent('Transcript');
      expect(tabButtons[1]).toHaveTextContent('SOAP');
      expect(tabButtons[2]).toHaveTextContent('Insights');
    });

    it('starts on SOAP tab by default', () => {
      render(<ReviewMode {...defaultProps} />);

      // SOAP tab should be active by default
      expect(screen.getByText('Generate SOAP Note')).toBeInTheDocument();
    });

    it('disables SOAP tab when no transcript', () => {
      render(<ReviewMode {...defaultProps} editedTranscript="" originalTranscript="" />);

      // Find the SOAP tab button by its class and content
      const tabButtons = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('SOAP')
      );
      expect(tabButtons[0]).toBeDisabled();
    });

    it('shows edited badge on Transcript tab when modified', () => {
      render(
        <ReviewMode
          {...defaultProps}
          originalTranscript="Original text"
          editedTranscript="Modified text"
        />
      );

      expect(screen.getByText('edited')).toBeInTheDocument();
    });

    it('shows checkmark on SOAP tab when note is generated', () => {
      const soapNote: SoapNote = {
        subjective: 'S',
        objective: 'O',
        assessment: 'A',
        plan: 'P',
        generated_at: '2024-12-15T10:30:00Z',
        model_used: 'qwen3:4b',
        raw_response: null,
      };
      render(<ReviewMode {...defaultProps} soapResult={createSoapResult(soapNote)} />);

      // Navigate to see the checkmark badge - use class filter to get the tab button
      const tabButtons = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('SOAP')
      );
      expect(tabButtons[0]).toContainHTML('✓');
    });
  });

  describe('transcript tab', () => {
    const navigateToTranscriptTab = () => {
      const tabButtons = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('Transcript')
      );
      if (tabButtons.length > 0) {
        fireEvent.click(tabButtons[0]);
      }
    };

    it('shows transcript content', () => {
      render(<ReviewMode {...defaultProps} />);
      navigateToTranscriptTab();

      expect(screen.getByText('Original transcript text.')).toBeInTheDocument();
    });

    it('shows "No transcript recorded" when empty', () => {
      render(<ReviewMode {...defaultProps} editedTranscript="" originalTranscript="" />);
      navigateToTranscriptTab();

      expect(screen.getByText('No transcript recorded')).toBeInTheDocument();
    });
  });

  describe('edit mode', () => {
    const navigateToTranscriptTab = () => {
      const tabButtons = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('Transcript')
      );
      if (tabButtons.length > 0) {
        fireEvent.click(tabButtons[0]);
      }
    };

    it('enters edit mode when Edit button is clicked', () => {
      render(<ReviewMode {...defaultProps} />);
      navigateToTranscriptTab();

      fireEvent.click(screen.getByText('Edit'));

      expect(screen.getByText('Done')).toBeInTheDocument();
      expect(screen.getByRole('textbox')).toBeInTheDocument();
    });

    it('shows textarea with current transcript in edit mode', () => {
      render(<ReviewMode {...defaultProps} />);
      navigateToTranscriptTab();

      fireEvent.click(screen.getByText('Edit'));

      const textarea = screen.getByRole('textbox');
      expect(textarea).toHaveValue('Original transcript text.');
    });

    it('calls onTranscriptEdit when text is changed', () => {
      const onTranscriptEdit = vi.fn();
      render(<ReviewMode {...defaultProps} onTranscriptEdit={onTranscriptEdit} />);
      navigateToTranscriptTab();

      fireEvent.click(screen.getByText('Edit'));
      fireEvent.change(screen.getByRole('textbox'), {
        target: { value: 'Updated text' },
      });

      expect(onTranscriptEdit).toHaveBeenCalledWith('Updated text');
    });

    it('exits edit mode when Done is clicked', () => {
      render(<ReviewMode {...defaultProps} />);
      navigateToTranscriptTab();

      fireEvent.click(screen.getByText('Edit'));
      expect(screen.getByText('Done')).toBeInTheDocument();

      fireEvent.click(screen.getByText('Done'));
      expect(screen.getByText('Edit')).toBeInTheDocument();
    });
  });

  describe('copy functionality', () => {
    const navigateToTranscriptTab = () => {
      const tabButtons = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('Transcript')
      );
      if (tabButtons.length > 0) {
        fireEvent.click(tabButtons[0]);
      }
    };

    it('copies transcript when Copy button is clicked', async () => {
      render(<ReviewMode {...defaultProps} />);
      navigateToTranscriptTab();

      fireEvent.click(screen.getByText('Copy'));

      await waitFor(() => {
        expect(mockWriteText).toHaveBeenCalledWith('Original transcript text.');
      });
    });

    it('shows "Copied!" feedback after copy', async () => {
      render(<ReviewMode {...defaultProps} />);
      navigateToTranscriptTab();

      fireEvent.click(screen.getByText('Copy'));

      await waitFor(() => {
        expect(screen.getByText('Copied!')).toBeInTheDocument();
      });
    });
  });

  describe('SOAP tab', () => {
    const navigateToSoapTab = () => {
      // Find the SOAP tab button by its class (review-tab) and text content
      const tabButtons = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('SOAP')
      );
      if (tabButtons.length > 0) {
        fireEvent.click(tabButtons[0]);
      }
    };

    it('shows Generate button when no SOAP note', () => {
      render(<ReviewMode {...defaultProps} />);
      navigateToSoapTab();

      expect(screen.getByText('Generate SOAP Note')).toBeInTheDocument();
    });

    it('disables Generate button when LLM not connected', () => {
      render(<ReviewMode {...defaultProps} llmConnected={false} />);
      navigateToSoapTab();

      expect(screen.getByText('LLM not connected')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: /llm not connected/i })).toBeDisabled();
    });

    it('calls onGenerateSoap when Generate button is clicked', () => {
      const onGenerateSoap = vi.fn();
      render(<ReviewMode {...defaultProps} onGenerateSoap={onGenerateSoap} />);
      navigateToSoapTab();

      fireEvent.click(screen.getByText('Generate SOAP Note'));

      expect(onGenerateSoap).toHaveBeenCalledTimes(1);
    });

    it('shows loading state when generating SOAP', () => {
      render(<ReviewMode {...defaultProps} isGeneratingSoap={true} />);
      navigateToSoapTab();

      expect(screen.getByText('Generating SOAP note...')).toBeInTheDocument();
      expect(document.querySelector('.spinner-small')).toBeInTheDocument();
    });

    it('shows error with retry button when SOAP generation fails', () => {
      render(<ReviewMode {...defaultProps} soapError="LLM connection failed" />);
      navigateToSoapTab();

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
      render(<ReviewMode {...defaultProps} soapResult={createSoapResult(soapNote)} />);
      navigateToSoapTab();

      // In the new UI, SOAP labels are single letters: S, O, A, P
      expect(screen.getByText('S')).toBeInTheDocument();
      expect(screen.getByText('Patient reports headache for 2 days.')).toBeInTheDocument();
      expect(screen.getByText('O')).toBeInTheDocument();
      expect(screen.getByText('BP 120/80, HR 72.')).toBeInTheDocument();
      expect(screen.getByText('A')).toBeInTheDocument();
      expect(screen.getByText('Tension headache.')).toBeInTheDocument();
      expect(screen.getByText('P')).toBeInTheDocument();
      expect(screen.getByText('Rest, OTC analgesics.')).toBeInTheDocument();
    });

    it('shows model for generated SOAP', () => {
      const soapNote: SoapNote = {
        subjective: 'S',
        objective: 'O',
        assessment: 'A',
        plan: 'P',
        generated_at: '2024-12-15T10:30:00Z',
        model_used: 'qwen3:4b',
        raw_response: null,
      };
      render(<ReviewMode {...defaultProps} soapResult={createSoapResult(soapNote)} />);
      navigateToSoapTab();

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
      render(<ReviewMode {...defaultProps} soapResult={createSoapResult(soapNote)} />);
      navigateToSoapTab();

      // Find Copy button in SOAP tab
      fireEvent.click(screen.getByText('Copy'));

      await waitFor(() => {
        expect(mockWriteText).toHaveBeenCalled();
      });

      // Verify the SOAP content format
      const lastCall = mockWriteText.mock.calls[mockWriteText.mock.calls.length - 1][0];
      expect(lastCall).toContain('SUBJECTIVE:');
      expect(lastCall).toContain('Subjective text');
    });
  });

  describe('Insights tab', () => {
    const biomarkers: BiomarkerUpdate = {
      cough_count: 3,
      cough_rate_per_min: 1.5,
      turn_count: 10,
      vitality_session_mean: 0.7,
      stability_session_mean: 0.8,
      speaker_metrics: [
        { speaker_id: 'SPEAKER_1', turn_count: 5, talk_time_ms: 60000 },
        { speaker_id: 'SPEAKER_2', turn_count: 5, talk_time_ms: 55000 },
      ],
      conversation_dynamics: {
        overlap_count: 2,
        total_interruption_count: 1,
        mean_response_latency_ms: 500,
        long_pause_count: 3,
        total_silence_ms: 10000,
        silence_ratio: 0.1,
        engagement_score: 75,
        per_speaker: [],
      },
    };

    const navigateToInsightsTab = () => {
      fireEvent.click(screen.getByRole('button', { name: /insights/i }));
    };

    it('shows audio quality card when available', () => {
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
      render(<ReviewMode {...defaultProps} audioQuality={audioQuality} biomarkers={biomarkers} />);
      navigateToInsightsTab();

      expect(screen.getByText('Audio Quality')).toBeInTheDocument();
      expect(screen.getByText('-20 dB')).toBeInTheDocument();
      expect(screen.getByText('25 dB')).toBeInTheDocument();
    });

    it('shows speaker metrics when available', () => {
      render(<ReviewMode {...defaultProps} biomarkers={biomarkers} />);
      navigateToInsightsTab();

      expect(screen.getByText('Speakers')).toBeInTheDocument();
      expect(screen.getByText('SPEAKER_1')).toBeInTheDocument();
      expect(screen.getByText('SPEAKER_2')).toBeInTheDocument();
    });

    it('shows conversation dynamics when available', () => {
      render(<ReviewMode {...defaultProps} biomarkers={biomarkers} />);
      navigateToInsightsTab();

      expect(screen.getByText('Conversation')).toBeInTheDocument();
      expect(screen.getByText('500ms')).toBeInTheDocument();
    });

    it('shows vocal biomarkers when available', () => {
      render(<ReviewMode {...defaultProps} biomarkers={biomarkers} />);
      navigateToInsightsTab();

      expect(screen.getByText('Vocal Biomarkers')).toBeInTheDocument();
      expect(screen.getByText('Vitality')).toBeInTheDocument();
      expect(screen.getByText('Stability')).toBeInTheDocument();
    });

    it('shows empty state when no insights available', () => {
      render(<ReviewMode {...defaultProps} biomarkers={null} audioQuality={null} />);
      navigateToInsightsTab();

      expect(screen.getByText('No insights available')).toBeInTheDocument();
    });
  });

  describe('sync status bar', () => {
    it('shows login prompt when auto-sync enabled but not authenticated', () => {
      render(<ReviewMode {...defaultProps} autoSyncEnabled={true} />);

      expect(screen.getByText(/sign in to sync/i)).toBeInTheDocument();
      expect(screen.getByText('Sign In')).toBeInTheDocument();
    });

    it('does not show login prompt when authenticated', () => {
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          autoSyncEnabled={true}
        />
      );

      expect(screen.queryByText(/sign in to sync/i)).not.toBeInTheDocument();
    });

    it('does not show anything when auto-sync disabled and not authenticated', () => {
      render(<ReviewMode {...defaultProps} autoSyncEnabled={false} />);

      expect(screen.queryByText(/sign in to sync/i)).not.toBeInTheDocument();
      expect(screen.queryByText(/synced/i)).not.toBeInTheDocument();
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

    it('shows syncing state', () => {
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          autoSyncEnabled={true}
          isSyncing={true}
        />
      );

      expect(screen.getByText(/syncing/i)).toBeInTheDocument();
    });

    it('shows synced state', () => {
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          autoSyncEnabled={true}
          syncedEncounter={{ encounterId: '123', encounterFhirId: '456', syncedAt: '2024-01-01T00:00:00Z', hasSoap: false }}
        />
      );

      expect(screen.getByText(/synced to medplum/i)).toBeInTheDocument();
    });

    it('shows synced with SOAP state', () => {
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          autoSyncEnabled={true}
          syncedEncounter={{ encounterId: '123', encounterFhirId: '456', syncedAt: '2024-01-01T00:00:00Z', hasSoap: true }}
        />
      );

      expect(screen.getByText(/synced with soap/i)).toBeInTheDocument();
    });

    it('shows sync error', () => {
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          autoSyncEnabled={true}
          syncError="Network error"
        />
      );

      expect(screen.getByText('Network error')).toBeInTheDocument();
    });

    it('clears sync error when Dismiss is clicked', () => {
      const onClearSyncError = vi.fn();
      render(
        <ReviewMode
          {...defaultProps}
          authState={authenticatedState}
          autoSyncEnabled={true}
          syncError="Network error"
          onClearSyncError={onClearSyncError}
        />
      );

      fireEvent.click(screen.getByText('Dismiss'));
      expect(onClearSyncError).toHaveBeenCalledTimes(1);
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
    const navigateToSoapTab = () => {
      const tabButtons = screen.getAllByRole('button').filter(
        btn => btn.classList.contains('review-tab') && btn.textContent?.includes('SOAP')
      );
      if (tabButtons.length > 0) {
        fireEvent.click(tabButtons[0]);
      }
    };

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
      render(<ReviewMode {...defaultProps} soapResult={createSoapResult(soapNote)} />);
      navigateToSoapTab();

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
      render(<ReviewMode {...defaultProps} soapResult={createSoapResult(soapNote)} />);
      navigateToSoapTab();

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
      render(<ReviewMode {...defaultProps} soapResult={createSoapResult(soapNote)} />);
      navigateToSoapTab();

      expect(screen.queryByText('Raw Response')).not.toBeInTheDocument();
    });
  });
});
