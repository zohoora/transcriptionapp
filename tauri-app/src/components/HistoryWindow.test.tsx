import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import HistoryWindow from './HistoryWindow';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useAuth } from './AuthProvider';
import type { LocalArchiveSummary, LocalArchiveDetails, LocalArchiveMetadata } from '../types';

// Mock dependencies
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({
  writeText: vi.fn(),
}));

vi.mock('./AuthProvider', () => ({
  useAuth: vi.fn(),
}));

// Stable references to prevent infinite re-render loops
const stableOllamaStatus = { connected: true };
const stableCheckConnection = vi.fn();
const stablePrewarmModel = vi.fn();
const stableTestConnection = vi.fn();

vi.mock('../hooks/useOllamaConnection', () => ({
  useOllamaConnection: vi.fn(() => ({
    status: stableOllamaStatus,
    isChecking: false,
    isPrewarming: false,
    error: null,
    checkConnection: stableCheckConnection,
    prewarmModel: stablePrewarmModel,
    testConnection: stableTestConnection,
  })),
}));

const stableSetSoapError = vi.fn();
const stableSetOllamaStatus = vi.fn();
const stableSoapOptions = {
  detail_level: 5,
  format: 'problem_based' as const,
  custom_instructions: '',
};
const stableSetSoapOptions = vi.fn();
const stableUpdateSoapDetailLevel = vi.fn();
const stableUpdateSoapFormat = vi.fn();
const stableUpdateSoapCustomInstructions = vi.fn();
const stableGenerateSoapNote = vi.fn().mockResolvedValue(null);

vi.mock('../hooks/useSoapNote', () => ({
  useSoapNote: vi.fn(() => ({
    isGeneratingSoap: false,
    soapError: null,
    setSoapError: stableSetSoapError,
    ollamaStatus: stableOllamaStatus,
    setOllamaStatus: stableSetOllamaStatus,
    soapOptions: stableSoapOptions,
    setSoapOptions: stableSetSoapOptions,
    updateSoapDetailLevel: stableUpdateSoapDetailLevel,
    updateSoapFormat: stableUpdateSoapFormat,
    updateSoapCustomInstructions: stableUpdateSoapCustomInstructions,
    generateSoapNote: stableGenerateSoapNote,
  })),
}));

// Mock child components
vi.mock('./Calendar', () => ({
  default: ({ selectedDate, onDateSelect }: { selectedDate: Date; onDateSelect: (date: Date) => void }) => (
    <div data-testid="calendar">
      <span>Selected: {selectedDate.toISOString().split('T')[0]}</span>
      <button onClick={() => onDateSelect(new Date('2025-01-15'))}>Select Jan 15</button>
    </div>
  ),
}));

vi.mock('./AudioPlayer', () => ({
  default: ({ audioUrl }: { audioUrl: string }) => (
    <div data-testid="audio-player">Audio: {audioUrl}</div>
  ),
}));

const mockInvoke = vi.mocked(invoke);
const mockGetCurrentWindow = vi.mocked(getCurrentWindow);
const mockWriteText = vi.mocked(writeText);
const mockUseAuth = vi.mocked(useAuth);

const mockAuthUnauthenticated = {
  authState: {
    is_authenticated: false,
    access_token: null,
    refresh_token: null,
    token_expiry: null,
    practitioner_id: null,
    practitioner_name: null,
  },
  isLoading: false,
  error: null,
  login: vi.fn(),
  logout: vi.fn(),
  refreshAuth: vi.fn(),
  cancelLogin: vi.fn(),
};

const mockAuthAuthenticated = {
  authState: {
    is_authenticated: true,
    access_token: 'test-token',
    refresh_token: 'test-refresh',
    token_expiry: Math.floor(Date.now() / 1000) + 3600,
    practitioner_id: 'prac-123',
    practitioner_name: 'Dr. Test',
  },
  isLoading: false,
  error: null,
  login: vi.fn(),
  logout: vi.fn(),
  refreshAuth: vi.fn(),
  cancelLogin: vi.fn(),
};

const mockSessions: LocalArchiveSummary[] = [
  {
    session_id: 'session-1',
    date: '2025-01-07T10:30:00Z',
    duration_ms: 900000, // 15 min
    word_count: 350,
    has_soap_note: true,
    has_audio: true,
    auto_ended: false,
    charting_mode: null,
    encounter_number: null,
    patient_name: null,
  },
  {
    session_id: 'session-2',
    date: '2025-01-07T14:00:00Z',
    duration_ms: 1200000, // 20 min
    word_count: 500,
    has_soap_note: false,
    has_audio: false,
    auto_ended: false,
    charting_mode: null,
    encounter_number: null,
    patient_name: null,
  },
];

const mockMetadata: LocalArchiveMetadata = {
  session_id: 'session-1',
  started_at: '2025-01-07T10:30:00Z',
  ended_at: '2025-01-07T10:45:00Z',
  duration_ms: 900000,
  segment_count: 10,
  word_count: 350,
  has_soap_note: true,
  auto_ended: false,
  auto_end_reason: null,
  charting_mode: null,
  encounter_number: null,
  patient_name: null,
  likely_non_clinical: null,
};

const mockSessionDetails: LocalArchiveDetails = {
  session_id: 'session-1',
  metadata: mockMetadata,
  transcript: 'This is the transcript text.',
  soap_note: 'S: Patient complaint\nO: Vitals normal',
  audio_path: '/path/to/audio.wav',
};

// Settings with local storage enabled (avoids Medplum auth dependency)
const mockSettings = {
  debug_storage_enabled: true,
  soap_detail_level: 5,
  soap_format: 'problem_based',
  soap_custom_instructions: '',
};

describe('HistoryWindow', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetCurrentWindow.mockReturnValue({
      close: vi.fn().mockResolvedValue(undefined),
    } as unknown as ReturnType<typeof getCurrentWindow>);
    mockWriteText.mockResolvedValue(undefined);
    mockUseAuth.mockReturnValue(mockAuthAuthenticated);

    // Default mock: local storage mode, returns empty sessions
    mockInvoke.mockImplementation((cmd: string) => {
      switch (cmd) {
        case 'get_settings':
          return Promise.resolve(mockSettings);
        case 'get_local_session_dates':
          return Promise.resolve([]);
        case 'get_local_sessions_by_date':
          return Promise.resolve([]);
        default:
          return Promise.resolve(null);
      }
    });
  });

  describe('basic rendering', () => {
    it('renders title and close button', async () => {
      render(<HistoryWindow />);

      expect(screen.getByText('Session History')).toBeInTheDocument();
      expect(screen.getByRole('button', { name: 'Close' })).toBeInTheDocument();
    });

    it('closes window when close clicked', async () => {
      const mockClose = vi.fn().mockResolvedValue(undefined);
      mockGetCurrentWindow.mockReturnValue({ close: mockClose } as unknown as ReturnType<typeof getCurrentWindow>);

      const user = userEvent.setup();
      render(<HistoryWindow />);

      await user.click(screen.getByRole('button', { name: 'Close' }));

      expect(mockClose).toHaveBeenCalled();
    });

    it('renders calendar', async () => {
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByTestId('calendar')).toBeInTheDocument();
      });
    });
  });

  describe('local archive - list view', () => {
    it('fetches sessions for selected date', async () => {
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith('get_local_sessions_by_date', expect.any(Object));
      });
    });

    it('renders session list with word counts', async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(mockSettings);
        if (cmd === 'get_local_session_dates') return Promise.resolve([]);
        if (cmd === 'get_local_sessions_by_date') return Promise.resolve(mockSessions);
        return Promise.resolve(null);
      });

      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('350 words')).toBeInTheDocument();
        expect(screen.getByText('500 words')).toBeInTheDocument();
      });
    });

    it('shows SOAP badge for sessions with SOAP note', async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(mockSettings);
        if (cmd === 'get_local_session_dates') return Promise.resolve([]);
        if (cmd === 'get_local_sessions_by_date') return Promise.resolve(mockSessions);
        return Promise.resolve(null);
      });

      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('SOAP')).toBeInTheDocument();
      });
    });

    it('shows empty message when no sessions', async () => {
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('No sessions recorded on this date')).toBeInTheDocument();
      });
    });

    it('shows error message on fetch failure', async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(mockSettings);
        if (cmd === 'get_local_session_dates') return Promise.resolve([]);
        if (cmd === 'get_local_sessions_by_date') return Promise.reject(new Error('Disk error'));
        return Promise.resolve(null);
      });

      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('Disk error')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
      });
    });

    it('retries fetch on retry button click', async () => {
      let callCount = 0;
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(mockSettings);
        if (cmd === 'get_local_session_dates') return Promise.resolve([]);
        if (cmd === 'get_local_sessions_by_date') {
          callCount++;
          if (callCount === 1) return Promise.reject(new Error('Disk error'));
          return Promise.resolve(mockSessions);
        }
        return Promise.resolve(null);
      });

      const user = userEvent.setup();
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('Disk error')).toBeInTheDocument();
      });

      await user.click(screen.getByRole('button', { name: 'Retry' }));

      await waitFor(() => {
        expect(screen.getByText('350 words')).toBeInTheDocument();
      });
    });

    it('fetches new sessions when date changes', async () => {
      let fetchCount = 0;
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(mockSettings);
        if (cmd === 'get_local_session_dates') return Promise.resolve([]);
        if (cmd === 'get_local_sessions_by_date') {
          fetchCount++;
          return Promise.resolve([]);
        }
        return Promise.resolve(null);
      });

      const user = userEvent.setup();
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(fetchCount).toBeGreaterThanOrEqual(1);
      });

      const initialCount = fetchCount;
      await user.click(screen.getByText('Select Jan 15'));

      await waitFor(() => {
        expect(fetchCount).toBeGreaterThan(initialCount);
      });
    });
  });

  describe('local archive - detail view', () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(mockSettings);
        if (cmd === 'get_local_session_dates') return Promise.resolve([]);
        if (cmd === 'get_local_sessions_by_date') return Promise.resolve(mockSessions);
        if (cmd === 'get_local_session_details') return Promise.resolve(mockSessionDetails);
        return Promise.resolve(null);
      });
    });

    it('navigates to detail view when session clicked', async () => {
      const user = userEvent.setup();
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('350 words')).toBeInTheDocument();
      });

      await user.click(screen.getByText('350 words'));

      await waitFor(() => {
        expect(screen.getByText('Session Details')).toBeInTheDocument();
      });
    });

    it('shows back button in detail view', async () => {
      const user = userEvent.setup();
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('350 words')).toBeInTheDocument();
      });

      await user.click(screen.getByText('350 words'));

      await waitFor(() => {
        expect(screen.getByRole('button', { name: /Back/i })).toBeInTheDocument();
      });
    });

    it('displays transcript in detail view', async () => {
      const user = userEvent.setup();
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('350 words')).toBeInTheDocument();
      });

      await user.click(screen.getByText('350 words'));

      await waitFor(() => {
        expect(screen.getByText('Session Details')).toBeInTheDocument();
      });

      // SOAP tab is active when soap_note exists, switch to Transcript
      await user.click(screen.getByRole('button', { name: /Transcript/i }));

      await waitFor(() => {
        expect(screen.getByText('This is the transcript text.')).toBeInTheDocument();
      });
    });

    it('displays SOAP note in detail view', async () => {
      const user = userEvent.setup();
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('350 words')).toBeInTheDocument();
      });

      await user.click(screen.getByText('350 words'));

      // SOAP tab auto-selected when soap_note exists
      await waitFor(() => {
        expect(screen.getByText(/Patient complaint/)).toBeInTheDocument();
      });
    });

    it('navigates back to list view', async () => {
      const user = userEvent.setup();
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('350 words')).toBeInTheDocument();
      });

      await user.click(screen.getByText('350 words'));

      await waitFor(() => {
        expect(screen.getByText('Session Details')).toBeInTheDocument();
      });

      await user.click(screen.getByRole('button', { name: /Back/i }));

      await waitFor(() => {
        expect(screen.getByTestId('calendar')).toBeInTheDocument();
      });
    });

    it('shows empty message when no transcript', async () => {
      const detailsNoTranscript = { ...mockSessionDetails, transcript: null, soap_note: null };
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(mockSettings);
        if (cmd === 'get_local_session_dates') return Promise.resolve([]);
        if (cmd === 'get_local_sessions_by_date') return Promise.resolve(mockSessions);
        if (cmd === 'get_local_session_details') return Promise.resolve(detailsNoTranscript);
        return Promise.resolve(null);
      });

      const user = userEvent.setup();
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('350 words')).toBeInTheDocument();
      });

      await user.click(screen.getByText('350 words'));

      // No soap_note → Transcript tab is active by default
      await waitFor(() => {
        expect(screen.getByText('No transcript recorded')).toBeInTheDocument();
      });
    });

    it('shows empty SOAP state when no SOAP note', async () => {
      const detailsNoSoap = { ...mockSessionDetails, soap_note: null };
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(mockSettings);
        if (cmd === 'get_local_session_dates') return Promise.resolve([]);
        if (cmd === 'get_local_sessions_by_date') return Promise.resolve(mockSessions);
        if (cmd === 'get_local_session_details') return Promise.resolve(detailsNoSoap);
        return Promise.resolve(null);
      });

      const user = userEvent.setup();
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('350 words')).toBeInTheDocument();
      });

      await user.click(screen.getByText('350 words'));

      // No soap_note → Transcript tab active, transcript visible
      await waitFor(() => {
        expect(screen.getByText('This is the transcript text.')).toBeInTheDocument();
      });
    });
  });

  describe('copy functionality', () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(mockSettings);
        if (cmd === 'get_local_session_dates') return Promise.resolve([]);
        if (cmd === 'get_local_sessions_by_date') return Promise.resolve(mockSessions);
        if (cmd === 'get_local_session_details') return Promise.resolve(mockSessionDetails);
        return Promise.resolve(null);
      });
    });

    it('copies transcript when copy button clicked', async () => {
      const user = userEvent.setup();
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('350 words')).toBeInTheDocument();
      });

      await user.click(screen.getByText('350 words'));

      await waitFor(() => {
        expect(screen.getByText('Session Details')).toBeInTheDocument();
      });

      // Switch to Transcript tab (SOAP tab active when soap_note exists)
      await user.click(screen.getByRole('button', { name: /Transcript/i }));

      await waitFor(() => {
        expect(screen.getByText('This is the transcript text.')).toBeInTheDocument();
      });

      const copyButton = screen.getByRole('button', { name: 'Copy' });
      await user.click(copyButton);

      expect(mockWriteText).toHaveBeenCalledWith('This is the transcript text.');
    });
  });

  describe('error handling', () => {
    it('handles detail fetch error', async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(mockSettings);
        if (cmd === 'get_local_session_dates') return Promise.resolve([]);
        if (cmd === 'get_local_sessions_by_date') return Promise.resolve(mockSessions);
        if (cmd === 'get_local_session_details') return Promise.reject(new Error('Failed to load details'));
        return Promise.resolve(null);
      });

      const user = userEvent.setup();
      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('350 words')).toBeInTheDocument();
      });

      await user.click(screen.getByText('350 words'));

      await waitFor(() => {
        expect(screen.getByText('Failed to load details')).toBeInTheDocument();
      });
    });

    it('handles string error', async () => {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === 'get_settings') return Promise.resolve(mockSettings);
        if (cmd === 'get_local_session_dates') return Promise.resolve([]);
        if (cmd === 'get_local_sessions_by_date') return Promise.reject('String error message');
        return Promise.resolve(null);
      });

      render(<HistoryWindow />);

      await waitFor(() => {
        expect(screen.getByText('String error message')).toBeInTheDocument();
      });
    });
  });
});
