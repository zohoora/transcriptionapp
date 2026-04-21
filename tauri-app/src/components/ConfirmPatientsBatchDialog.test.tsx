import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import ConfirmPatientsBatchDialog from './ConfirmPatientsBatchDialog';
import { invoke } from '@tauri-apps/api/core';
import type { ConfirmPatientResult, LocalArchiveDetails } from '../types';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

function makeSession(overrides: Partial<LocalArchiveDetails>): LocalArchiveDetails {
  return {
    session_id: overrides.session_id ?? 'session-1',
    transcript: overrides.transcript ?? 'Dr: hello',
    soap_note: overrides.soap_note ?? 'S: test',
    audio_path: null,
    patientNotes: overrides.patientNotes ?? null,
    metadata: {
      session_id: overrides.session_id ?? 'session-1',
      started_at: '2026-04-21T13:30:00Z',
      ended_at: '2026-04-21T13:45:00Z',
      duration_ms: 900000,
      segment_count: 50,
      word_count: 500,
      has_soap_note: true,
      has_audio: false,
      auto_ended: false,
      auto_end_reason: null,
      soap_detail_level: 5,
      soap_format: 'comprehensive',
      charting_mode: 'continuous',
      encounter_number: 1,
      patient_name: 'Lisa Hooper',
      patient_dob: '1962-02-02',
      likely_non_clinical: null,
      ...(overrides.metadata ?? {}),
    },
  } as LocalArchiveDetails;
}

const successResult: ConfirmPatientResult = {
  medplumSynced: true,
  profileServiceSynced: true,
  patientId: 'p-1',
  medplumPatientId: 'p-1',
  confirmedAt: '2026-04-21T14:00:00Z',
  errors: [],
};

describe('ConfirmPatientsBatchDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders a single row with prefilled name + DOB for N=1', () => {
    render(
      <ConfirmPatientsBatchDialog
        sessions={[makeSession({ session_id: 'a' })]}
        date="2026-04-21"
        onConfirmed={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByDisplayValue('Lisa Hooper')).toBeInTheDocument();
    expect(screen.getByDisplayValue('1962-02-02')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Confirm & sync/ })).toBeInTheDocument();
  });

  it('renders multiple rows and loops invocations serially for N=3', async () => {
    const user = userEvent.setup();
    mockInvoke.mockResolvedValue(successResult);

    const sessions = [
      makeSession({ session_id: 'a', metadata: { ...makeSession({}).metadata, patient_name: 'A', patient_dob: '1950-01-01' } }),
      makeSession({ session_id: 'b', metadata: { ...makeSession({}).metadata, patient_name: 'B', patient_dob: '1960-01-01' } }),
      makeSession({ session_id: 'c', metadata: { ...makeSession({}).metadata, patient_name: 'C', patient_dob: '1970-01-01' } }),
    ];
    render(
      <ConfirmPatientsBatchDialog
        sessions={sessions}
        date="2026-04-21"
        onConfirmed={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    const rows = screen.getAllByRole('row');
    // header + 3 body rows
    expect(rows).toHaveLength(4);

    await user.click(screen.getByRole('button', { name: /Confirm all \(3\)/ }));

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledTimes(3));

    // Calls were issued with the right sessionId + name/dob per row
    expect(mockInvoke).toHaveBeenNthCalledWith(
      1,
      'confirm_session_patient',
      expect.objectContaining({ sessionId: 'a', patientName: 'A', patientDob: '1950-01-01' }),
    );
    expect(mockInvoke).toHaveBeenNthCalledWith(
      2,
      'confirm_session_patient',
      expect.objectContaining({ sessionId: 'b', patientName: 'B' }),
    );
    expect(mockInvoke).toHaveBeenNthCalledWith(
      3,
      'confirm_session_patient',
      expect.objectContaining({ sessionId: 'c', patientName: 'C' }),
    );

    // Close button replaces Confirm all after all rows are done
    await waitFor(() =>
      expect(screen.getByRole('button', { name: /Close/ })).toBeInTheDocument(),
    );
  });

  it('rejects empty name + does not invoke', async () => {
    const user = userEvent.setup();
    const session = makeSession({
      session_id: 'a',
      metadata: { ...makeSession({}).metadata, patient_name: null },
    });
    render(
      <ConfirmPatientsBatchDialog
        sessions={[session]}
        date="2026-04-21"
        onConfirmed={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('button', { name: /Confirm & sync/ }));
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(screen.getByRole('alert')).toHaveTextContent(/name is required/i);
  });

  it('rejects DOB not in YYYY-MM-DD format', async () => {
    const user = userEvent.setup();
    const session = makeSession({
      session_id: 'a',
      metadata: { ...makeSession({}).metadata, patient_dob: '01/02/1960' },
    });
    render(
      <ConfirmPatientsBatchDialog
        sessions={[session]}
        date="2026-04-21"
        onConfirmed={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('button', { name: /Confirm & sync/ }));
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(screen.getByRole('alert')).toHaveTextContent(/YYYY-MM-DD/i);
  });

  it('filters multi-patient sessions with an inline note', () => {
    const multi = makeSession({
      session_id: 'multi',
      patientNotes: [
        { index: 1, label: 'A', content: 'S: a' },
        { index: 2, label: 'B', content: 'S: b' },
      ],
    });
    const single = makeSession({ session_id: 'single' });
    render(
      <ConfirmPatientsBatchDialog
        sessions={[multi, single]}
        date="2026-04-21"
        onConfirmed={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByRole('note')).toHaveTextContent(/1 multi-patient session skipped/);
    // Only the single-patient row renders
    const rows = screen.getAllByRole('row');
    expect(rows).toHaveLength(2); // header + 1 body
  });

  it('shows per-row failure without blocking other rows', async () => {
    const user = userEvent.setup();
    const partial: ConfirmPatientResult = {
      medplumSynced: false,
      profileServiceSynced: true,
      patientId: 'uuid-1',
      medplumPatientId: null,
      confirmedAt: '2026-04-21T14:00:00Z',
      errors: ['medplum_proxy_mint: not configured'],
    };
    mockInvoke.mockResolvedValueOnce(partial).mockResolvedValueOnce(successResult);

    const sessions = [
      makeSession({ session_id: 'fail' }),
      makeSession({ session_id: 'ok' }),
    ];
    render(
      <ConfirmPatientsBatchDialog
        sessions={sessions}
        date="2026-04-21"
        onConfirmed={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('button', { name: /Confirm all \(2\)/ }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledTimes(2));
    // Both rows completed — close button visible
    await waitFor(() =>
      expect(screen.getByRole('button', { name: /Close/ })).toBeInTheDocument(),
    );
    // The failing row surfaces its error count
    expect(screen.getByText('1')).toBeInTheDocument();
  });

  it('calls onConfirmed when at least one row synced successfully', async () => {
    const user = userEvent.setup();
    const onConfirmed = vi.fn();
    mockInvoke.mockResolvedValue(successResult);
    render(
      <ConfirmPatientsBatchDialog
        sessions={[makeSession({ session_id: 'a' })]}
        date="2026-04-21"
        onConfirmed={onConfirmed}
        onCancel={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('button', { name: /Confirm & sync/ }));
    await waitFor(() => expect(screen.getByRole('button', { name: /Close/ })).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: /Close/ }));
    expect(onConfirmed).toHaveBeenCalledTimes(1);
  });

  it('calls onCancel when all rows failed to sync', async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();
    mockInvoke.mockRejectedValue(new Error('network'));
    render(
      <ConfirmPatientsBatchDialog
        sessions={[makeSession({ session_id: 'a' })]}
        date="2026-04-21"
        onConfirmed={vi.fn()}
        onCancel={onCancel}
      />,
    );
    await user.click(screen.getByRole('button', { name: /Confirm & sync/ }));
    await waitFor(() => expect(screen.getByRole('button', { name: /Close/ })).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: /Close/ }));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it('renders a no-eligible-sessions state when all selected are multi-patient', () => {
    const multi = makeSession({
      session_id: 'multi',
      patientNotes: [
        { index: 1, label: 'A', content: 'S: a' },
        { index: 2, label: 'B', content: 'S: b' },
      ],
    });
    render(
      <ConfirmPatientsBatchDialog
        sessions={[multi]}
        date="2026-04-21"
        onConfirmed={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText(/No eligible sessions/i)).toBeInTheDocument();
    expect(screen.queryByRole('table')).not.toBeInTheDocument();
  });
});
