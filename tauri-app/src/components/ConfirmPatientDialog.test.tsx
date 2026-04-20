import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import ConfirmPatientDialog from './ConfirmPatientDialog';
import { invoke } from '@tauri-apps/api/core';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

const defaultProps = {
  sessionId: 'test-session',
  date: '2026-04-20',
  initialName: 'Judie Joan Guest',
  initialDob: '1945-04-08',
  sessionStartedAt: '2026-04-20T13:33:55Z',
  sessionDurationMs: 1184289,
  soapNote: 'S: headache',
  transcript: 'Dr: how are you feeling',
  onConfirmed: vi.fn(),
  onCancel: vi.fn(),
};

describe('ConfirmPatientDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders with prefilled name + dob from vision extraction', () => {
    render(<ConfirmPatientDialog {...defaultProps} />);
    expect(screen.getByDisplayValue('Judie Joan Guest')).toBeInTheDocument();
    expect(screen.getByDisplayValue('1945-04-08')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /confirm.*sync/i })).toBeInTheDocument();
  });

  it('blocks submission when name is empty', async () => {
    const user = userEvent.setup();
    render(
      <ConfirmPatientDialog
        {...defaultProps}
        initialName=""
      />,
    );
    await user.click(screen.getByRole('button', { name: /confirm.*sync/i }));
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(screen.getByRole('alert')).toHaveTextContent(/name is required/i);
  });

  it('blocks submission when DOB is not YYYY-MM-DD', async () => {
    const user = userEvent.setup();
    render(
      <ConfirmPatientDialog
        {...defaultProps}
        initialDob="bad"
      />,
    );
    await user.click(screen.getByRole('button', { name: /confirm.*sync/i }));
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it('invokes confirm_session_patient with full payload and shows both-synced status', async () => {
    const user = userEvent.setup();
    mockInvoke.mockResolvedValueOnce({
      medplumSynced: true,
      profileServiceSynced: true,
      patientId: 'mp-1',
      medplumPatientId: 'mp-1',
      confirmedAt: '2026-04-20T15:00:00Z',
      errors: [],
    });

    render(<ConfirmPatientDialog {...defaultProps} />);
    await user.click(screen.getByRole('button', { name: /confirm.*sync/i }));

    expect(mockInvoke).toHaveBeenCalledWith('confirm_session_patient', {
      sessionId: 'test-session',
      date: '2026-04-20',
      patientName: 'Judie Joan Guest',
      patientDob: '1945-04-08',
      soapNote: 'S: headache',
      transcript: 'Dr: how are you feeling',
      sessionStartedAt: '2026-04-20T13:33:55Z',
      sessionDurationMs: 1184289,
    });

    await waitFor(() =>
      expect(screen.getByText(/Medplum \(EMR\)/i)).toBeInTheDocument(),
    );
    expect(screen.getAllByText('✓')).toHaveLength(2);
    expect(screen.getByText(/Patient\/mp-1/)).toBeInTheDocument();
  });

  it('shows partial-success status when only profile-service synced', async () => {
    const user = userEvent.setup();
    mockInvoke.mockResolvedValueOnce({
      medplumSynced: false,
      profileServiceSynced: true,
      patientId: 'uuid-1',
      medplumPatientId: null,
      confirmedAt: '2026-04-20T15:00:00Z',
      errors: ['medplum: not authenticated'],
    });

    render(<ConfirmPatientDialog {...defaultProps} />);
    await user.click(screen.getByRole('button', { name: /confirm.*sync/i }));

    await waitFor(() =>
      expect(screen.getByText(/non-fatal error/i)).toBeInTheDocument(),
    );
    expect(screen.getAllByText('·')).toHaveLength(1);
    expect(screen.getAllByText('✓')).toHaveLength(1);
  });

  it('calls onConfirmed after successful sync', async () => {
    const user = userEvent.setup();
    const onConfirmed = vi.fn();
    mockInvoke.mockResolvedValueOnce({
      medplumSynced: true,
      profileServiceSynced: true,
      patientId: 'mp-1',
      medplumPatientId: 'mp-1',
      confirmedAt: '2026-04-20T15:00:00Z',
      errors: [],
    });

    render(<ConfirmPatientDialog {...defaultProps} onConfirmed={onConfirmed} />);
    await user.click(screen.getByRole('button', { name: /confirm.*sync/i }));
    await waitFor(() => expect(screen.getByText(/Medplum \(EMR\)/i)).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: /close/i }));
    expect(onConfirmed).toHaveBeenCalledTimes(1);
  });

  it('calls onCancel when Cancel pressed before sync', async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();
    render(<ConfirmPatientDialog {...defaultProps} onCancel={onCancel} />);
    await user.click(screen.getByRole('button', { name: /cancel/i }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(mockInvoke).not.toHaveBeenCalled();
  });
});
