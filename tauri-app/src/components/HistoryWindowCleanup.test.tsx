import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  CleanupActionBar,
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
  },
];

describe('CleanupActionBar', () => {
  it('shows hint text when no sessions selected', () => {
    render(
      <CleanupActionBar
        selectedCount={0}
        onMerge={vi.fn()}
        onDelete={vi.fn()}
        onEditName={vi.fn()}
        onSplit={vi.fn()}
        onRegenSoap={vi.fn()}
      />
    );

    expect(screen.getByText('Select sessions to manage')).toBeInTheDocument();
  });

  it('shows single-select actions when 1 session selected', () => {
    render(
      <CleanupActionBar
        selectedCount={1}
        onMerge={vi.fn()}
        onDelete={vi.fn()}
        onEditName={vi.fn()}
        onSplit={vi.fn()}
        onRegenSoap={vi.fn()}
      />
    );

    expect(screen.getByText('1 selected')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Delete' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Edit Name' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Split' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Regen SOAP' })).toBeInTheDocument();
    // Merge should NOT be available for single select
    expect(screen.queryByRole('button', { name: 'Merge' })).not.toBeInTheDocument();
  });

  it('shows multi-select actions when 2+ sessions selected', () => {
    render(
      <CleanupActionBar
        selectedCount={2}
        onMerge={vi.fn()}
        onDelete={vi.fn()}
        onEditName={vi.fn()}
        onSplit={vi.fn()}
        onRegenSoap={vi.fn()}
      />
    );

    expect(screen.getByText('2 selected')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Merge' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Delete' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Regen SOAP' })).toBeInTheDocument();
    // Single-select only actions should NOT appear
    expect(screen.queryByRole('button', { name: 'Edit Name' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Split' })).not.toBeInTheDocument();
  });

  it('calls onDelete when Delete clicked', async () => {
    const onDelete = vi.fn();
    const user = userEvent.setup();

    render(
      <CleanupActionBar
        selectedCount={1}
        onMerge={vi.fn()}
        onDelete={onDelete}
        onEditName={vi.fn()}
        onSplit={vi.fn()}
        onRegenSoap={vi.fn()}
      />
    );

    await user.click(screen.getByRole('button', { name: 'Delete' }));
    expect(onDelete).toHaveBeenCalledOnce();
  });

  it('calls onMerge when Merge clicked', async () => {
    const onMerge = vi.fn();
    const user = userEvent.setup();

    render(
      <CleanupActionBar
        selectedCount={3}
        onMerge={onMerge}
        onDelete={vi.fn()}
        onEditName={vi.fn()}
        onSplit={vi.fn()}
        onRegenSoap={vi.fn()}
      />
    );

    await user.click(screen.getByRole('button', { name: 'Merge' }));
    expect(onMerge).toHaveBeenCalledOnce();
  });

  it('calls onEditName when Edit Name clicked', async () => {
    const onEditName = vi.fn();
    const user = userEvent.setup();

    render(
      <CleanupActionBar
        selectedCount={1}
        onMerge={vi.fn()}
        onDelete={vi.fn()}
        onEditName={onEditName}
        onSplit={vi.fn()}
        onRegenSoap={vi.fn()}
      />
    );

    await user.click(screen.getByRole('button', { name: 'Edit Name' }));
    expect(onEditName).toHaveBeenCalledOnce();
  });

  it('calls onSplit when Split clicked', async () => {
    const onSplit = vi.fn();
    const user = userEvent.setup();

    render(
      <CleanupActionBar
        selectedCount={1}
        onMerge={vi.fn()}
        onDelete={vi.fn()}
        onEditName={vi.fn()}
        onSplit={onSplit}
        onRegenSoap={vi.fn()}
      />
    );

    await user.click(screen.getByRole('button', { name: 'Split' }));
    expect(onSplit).toHaveBeenCalledOnce();
  });

  it('calls onRegenSoap when Regen SOAP clicked', async () => {
    const onRegenSoap = vi.fn();
    const user = userEvent.setup();

    render(
      <CleanupActionBar
        selectedCount={1}
        onMerge={vi.fn()}
        onDelete={vi.fn()}
        onEditName={vi.fn()}
        onSplit={vi.fn()}
        onRegenSoap={onRegenSoap}
      />
    );

    await user.click(screen.getByRole('button', { name: 'Regen SOAP' }));
    expect(onRegenSoap).toHaveBeenCalledOnce();
  });

  it('shows correct count for 5 selected', () => {
    render(
      <CleanupActionBar
        selectedCount={5}
        onMerge={vi.fn()}
        onDelete={vi.fn()}
        onEditName={vi.fn()}
        onSplit={vi.fn()}
        onRegenSoap={vi.fn()}
      />
    );

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
      />
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
      />
    );

    expect(screen.getByText('Delete 2 Sessions?')).toBeInTheDocument();
  });

  it('shows encounter info for continuous mode sessions', () => {
    render(
      <DeleteConfirmDialog
        sessions={[mockLocalSessions[0]]}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
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
      />
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
      />
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
      />
    );

    expect(screen.getByRole('button', { name: 'Delete Sessions' })).toBeInTheDocument();
  });
});

describe('EditNameDialog', () => {
  it('renders with current name pre-filled', () => {
    render(
      <EditNameDialog
        currentName="Alice Johnson"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    expect(screen.getByText('Edit Patient Name')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Alice Johnson')).toBeInTheDocument();
  });

  it('renders with empty input when no current name', () => {
    render(
      <EditNameDialog
        currentName={null}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    expect(screen.getByPlaceholderText('Patient name (leave empty to clear)')).toBeInTheDocument();
    expect(screen.getByDisplayValue('')).toBeInTheDocument();
  });

  it('calls onConfirm with updated name on submit', async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();

    render(
      <EditNameDialog
        currentName="Alice"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />
    );

    const input = screen.getByDisplayValue('Alice');
    await user.clear(input);
    await user.type(input, 'Jane Doe');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    expect(onConfirm).toHaveBeenCalledWith('Jane Doe');
  });

  it('calls onCancel when cancel clicked', async () => {
    const onCancel = vi.fn();
    const user = userEvent.setup();

    render(
      <EditNameDialog
        currentName="Alice"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />
    );

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
      />
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
      />
    );

    // 150 + 250 = 400 total words
    expect(screen.getByText(/~400 words/)).toBeInTheDocument();
  });

  it('marks earliest session as keeper', () => {
    render(
      <MergeConfirmDialog
        sessions={mockLocalSessions.slice(0, 2)}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    expect(screen.getByText('keeps')).toBeInTheDocument();
  });

  it('warns about SOAP invalidation when sessions have SOAP notes', () => {
    render(
      <MergeConfirmDialog
        sessions={mockLocalSessions.slice(0, 2)}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    // mockLocalSessions[0] has has_soap_note: true
    expect(screen.getByText(/SOAP notes will be invalidated/)).toBeInTheDocument();
  });

  it('does not warn about SOAP when no sessions have notes', () => {
    const sessionsWithoutSoap = mockLocalSessions.slice(1, 3); // sessions 2 and 3 have no SOAP
    render(
      <MergeConfirmDialog
        sessions={sessionsWithoutSoap}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
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
      />
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
      />
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
      />
    );

    expect(screen.getByText('Merge 3 Sessions')).toBeInTheDocument();
    // 150 + 250 + 80 = 480 words
    expect(screen.getByText(/~480 words/)).toBeInTheDocument();
  });
});
