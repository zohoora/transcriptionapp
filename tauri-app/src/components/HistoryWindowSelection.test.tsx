import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  HistoryActionBar,
  DeleteConfirmDialog,
  EditNameDialog,
  MergeConfirmDialog,
} from './cleanup';
import type { LocalArchiveSummary } from '../types';

const mockLocalSessions: LocalArchiveSummary[] = [
  {
    session_id: 'local-1',
    date: '2025-01-07T09:00:00Z',
    duration_ms: 600000,
    word_count: 150,
    has_soap_note: true,
    has_audio: false,
    auto_ended: false,
    charting_mode: 'continuous',
    encounter_number: 1,
    patient_name: 'Alice Johnson',
    likely_non_clinical: null,
    has_feedback: null,
  },
  {
    session_id: 'local-2',
    date: '2025-01-07T10:30:00Z',
    duration_ms: 900000,
    word_count: 250,
    has_soap_note: false,
    has_audio: true,
    auto_ended: false,
    charting_mode: 'continuous',
    encounter_number: 2,
    patient_name: 'Bob Smith',
    likely_non_clinical: null,
    has_feedback: null,
  },
  {
    session_id: 'local-3',
    date: '2025-01-07T14:00:00Z',
    duration_ms: 300000,
    word_count: 80,
    has_soap_note: false,
    has_audio: false,
    auto_ended: true,
    charting_mode: 'continuous',
    encounter_number: 3,
    patient_name: null,
    likely_non_clinical: null,
    has_feedback: null,
  },
];

function renderBar(overrides: Partial<React.ComponentProps<typeof HistoryActionBar>> = {}) {
  const props: React.ComponentProps<typeof HistoryActionBar> = {
    selectedCount: 1,
    onMerge: vi.fn(),
    onDelete: vi.fn(),
    onEditName: vi.fn(),
    onConfirmPatient: vi.fn(),
    onSplit: vi.fn(),
    onRegenSoap: vi.fn(),
    ...overrides,
  };
  render(<HistoryActionBar {...props} />);
  return props;
}

describe('HistoryActionBar', () => {
  it('renders nothing when no sessions are selected', () => {
    const { container } = render(
      <HistoryActionBar
        selectedCount={0}
        onMerge={vi.fn()}
        onDelete={vi.fn()}
        onEditName={vi.fn()}
        onConfirmPatient={vi.fn()}
        onSplit={vi.fn()}
        onRegenSoap={vi.fn()}
      />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('shows single-select actions (Delete / Edit Name / Confirm / Split / Regen SOAP)', () => {
    renderBar({ selectedCount: 1 });
    expect(screen.getByText('1 selected')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Delete' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Edit Name' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Confirm Patient' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Split' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Regen SOAP' })).toBeInTheDocument();
    // Merge not available for single-select.
    expect(screen.queryByRole('button', { name: 'Merge' })).not.toBeInTheDocument();
  });

  it('shows multi-select actions with Confirm Patient included', () => {
    renderBar({ selectedCount: 3 });
    expect(screen.getByText('3 selected')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Merge' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Delete' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Confirm Patient' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Regen SOAP' })).toBeInTheDocument();
    // Single-only actions are hidden for multi-select.
    expect(screen.queryByRole('button', { name: 'Edit Name' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Split' })).not.toBeInTheDocument();
  });

  it('wires Delete / Edit Name / Confirm Patient / Split / Regen SOAP callbacks for single-select', async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn();
    const onEditName = vi.fn();
    const onConfirmPatient = vi.fn();
    const onSplit = vi.fn();
    const onRegenSoap = vi.fn();
    renderBar({
      selectedCount: 1,
      onDelete,
      onEditName,
      onConfirmPatient,
      onSplit,
      onRegenSoap,
    });
    await user.click(screen.getByRole('button', { name: 'Delete' }));
    await user.click(screen.getByRole('button', { name: 'Edit Name' }));
    await user.click(screen.getByRole('button', { name: 'Confirm Patient' }));
    await user.click(screen.getByRole('button', { name: 'Split' }));
    await user.click(screen.getByRole('button', { name: 'Regen SOAP' }));
    expect(onDelete).toHaveBeenCalledOnce();
    expect(onEditName).toHaveBeenCalledOnce();
    expect(onConfirmPatient).toHaveBeenCalledOnce();
    expect(onSplit).toHaveBeenCalledOnce();
    expect(onRegenSoap).toHaveBeenCalledOnce();
  });

  it('wires Merge / Delete / Confirm Patient / Regen SOAP callbacks for multi-select', async () => {
    const user = userEvent.setup();
    const onMerge = vi.fn();
    const onDelete = vi.fn();
    const onConfirmPatient = vi.fn();
    const onRegenSoap = vi.fn();
    renderBar({
      selectedCount: 2,
      onMerge,
      onDelete,
      onConfirmPatient,
      onRegenSoap,
    });
    await user.click(screen.getByRole('button', { name: 'Merge' }));
    await user.click(screen.getByRole('button', { name: 'Delete' }));
    await user.click(screen.getByRole('button', { name: 'Confirm Patient' }));
    await user.click(screen.getByRole('button', { name: 'Regen SOAP' }));
    expect(onMerge).toHaveBeenCalledOnce();
    expect(onDelete).toHaveBeenCalledOnce();
    expect(onConfirmPatient).toHaveBeenCalledOnce();
    expect(onRegenSoap).toHaveBeenCalledOnce();
  });

  it('reflects the selected count verbatim in the multi-select label', () => {
    renderBar({ selectedCount: 5 });
    expect(screen.getByText('5 selected')).toBeInTheDocument();
  });
});

describe('DeleteConfirmDialog', () => {
  it('renders single session delete title', () => {
    render(
      <DeleteConfirmDialog
        sessions={[mockLocalSessions[0]]}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText('Delete Session?')).toBeInTheDocument();
    expect(screen.getByText(/permanently deleted/)).toBeInTheDocument();
  });

  it('renders multi-session delete title', () => {
    render(
      <DeleteConfirmDialog
        sessions={mockLocalSessions.slice(0, 2)}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText('Delete 2 Sessions?')).toBeInTheDocument();
  });

  it('shows encounter info for continuous mode sessions', () => {
    render(
      <DeleteConfirmDialog
        sessions={[mockLocalSessions[0]]}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText(/Encounter #1/)).toBeInTheDocument();
    expect(screen.getByText(/Alice Johnson/)).toBeInTheDocument();
  });

  it('calls onConfirm when delete button clicked', async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    render(
      <DeleteConfirmDialog
        sessions={[mockLocalSessions[0]]}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'Delete Session' }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it('calls onCancel when cancel button clicked', async () => {
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <DeleteConfirmDialog
        sessions={[mockLocalSessions[0]]}
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('shows Delete Sessions (plural) for multi-session', () => {
    render(
      <DeleteConfirmDialog
        sessions={mockLocalSessions}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByRole('button', { name: 'Delete Sessions' })).toBeInTheDocument();
  });
});

describe('EditNameDialog', () => {
  it('renders with current name pre-filled', () => {
    render(
      <EditNameDialog currentName="Alice Johnson" onConfirm={vi.fn()} onCancel={vi.fn()} />,
    );
    expect(screen.getByText('Edit Patient Name')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Alice Johnson')).toBeInTheDocument();
  });

  it('renders with empty input when no current name', () => {
    render(<EditNameDialog currentName={null} onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(
      screen.getByPlaceholderText('Patient name (leave empty to clear)'),
    ).toBeInTheDocument();
    expect(screen.getByDisplayValue('')).toBeInTheDocument();
  });

  it('calls onConfirm with updated name on submit', async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    render(<EditNameDialog currentName="Alice" onConfirm={onConfirm} onCancel={vi.fn()} />);
    const input = screen.getByDisplayValue('Alice');
    await user.clear(input);
    await user.type(input, 'Jane Doe');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(onConfirm).toHaveBeenCalledWith('Jane Doe');
  });

  it('calls onCancel when cancel clicked', async () => {
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(<EditNameDialog currentName="Alice" onConfirm={vi.fn()} onCancel={onCancel} />);
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onCancel).toHaveBeenCalledOnce();
  });
});

describe('MergeConfirmDialog', () => {
  it('renders merge title with session count', () => {
    render(
      <MergeConfirmDialog
        sessions={mockLocalSessions.slice(0, 2)}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText('Merge 2 Sessions')).toBeInTheDocument();
    expect(screen.getByText(/chronological order/)).toBeInTheDocument();
  });

  it('shows combined word count', () => {
    render(
      <MergeConfirmDialog
        sessions={mockLocalSessions.slice(0, 2)}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText(/~400 words/)).toBeInTheDocument();
  });

  it('marks earliest session as keeper', () => {
    render(
      <MergeConfirmDialog
        sessions={mockLocalSessions.slice(0, 2)}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText('keeps')).toBeInTheDocument();
  });

  it('warns about SOAP invalidation when sessions have SOAP notes', () => {
    render(
      <MergeConfirmDialog
        sessions={mockLocalSessions.slice(0, 2)}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText(/SOAP notes will be invalidated/)).toBeInTheDocument();
  });

  it('does not warn about SOAP when no sessions have notes', () => {
    const sessionsWithoutSoap = mockLocalSessions.slice(1, 3);
    render(
      <MergeConfirmDialog
        sessions={sessionsWithoutSoap}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.queryByText(/SOAP notes will be invalidated/)).not.toBeInTheDocument();
  });

  it('calls onConfirm when merge button clicked', async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    render(
      <MergeConfirmDialog
        sessions={mockLocalSessions.slice(0, 2)}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'Merge Sessions' }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it('calls onCancel when cancel clicked', async () => {
    const onCancel = vi.fn();
    const user = userEvent.setup();
    render(
      <MergeConfirmDialog
        sessions={mockLocalSessions.slice(0, 2)}
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('renders 3 sessions in merge dialog', () => {
    render(
      <MergeConfirmDialog
        sessions={mockLocalSessions}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText('Merge 3 Sessions')).toBeInTheDocument();
    expect(screen.getByText(/~480 words/)).toBeInTheDocument();
  });
});
