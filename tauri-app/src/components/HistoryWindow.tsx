import React, { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useAuth } from './AuthProvider';
import { useSoapNote } from '../hooks/useSoapNote';
import { useOllamaConnection } from '../hooks/useOllamaConnection';
import Calendar from './Calendar';
import AudioPlayer from './AudioPlayer';
import {
  CleanupActionBar,
  DeleteConfirmDialog,
  EditNameDialog,
  MergeConfirmDialog,
} from './cleanup';
import { formatDateForApi, formatLocalTime, formatLocalDateTime, formatDurationShort } from '../utils';
import type {
  LocalArchiveSummary,
  LocalArchiveDetails,
  LocalArchiveMetadata,
  MultiPatientSoapResult,
  Settings,
  EncounterSummary,
  EncounterDetails,
  SoapOptions,
} from '../types';
import { DETAIL_LEVEL_LABELS } from '../types';

type View = 'list' | 'detail';
type DetailTab = 'transcript' | 'soap' | 'insights';
type DataSource = 'local' | 'medplum';
type CleanupDialog = 'none' | 'delete' | 'merge' | 'editName';

function formatDateForDisplay(date: Date): string {
  return date.toLocaleDateString('en-US', {
    weekday: 'long',
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });
}

function formatTime(dateString: string): string {
  return formatLocalTime(dateString);
}

// Format duration as M:SS (uses shared utility)
const formatDuration = formatDurationShort;

const HistoryWindow: React.FC = () => {
  const { authState, isLoading: authLoading, login } = useAuth();

  // Use shared SOAP hook - handles LLM status, options persistence, and generation
  const {
    isGeneratingSoap,
    soapError,
    setSoapError,
    ollamaStatus,
    setOllamaStatus,
    soapOptions,
    setSoapOptions,
    updateSoapDetailLevel,
    updateSoapFormat,
    updateSoapCustomInstructions,
    generateSoapNote,
  } = useSoapNote();

  const [selectedDate, setSelectedDate] = useState(new Date());
  const [sessions, setSessions] = useState<LocalArchiveSummary[]>([]);
  const [selectedSession, setSelectedSession] = useState<LocalArchiveDetails | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<View>('list');
  const [datesWithSessions, setDatesWithSessions] = useState<Set<string>>(new Set());

  // Data source based on debug_storage_enabled setting
  const [dataSource, setDataSource] = useState<DataSource>('local');
  const [settingsLoaded, setSettingsLoaded] = useState(false);

  // Global SOAP defaults (from settings, used as fallback for historical sessions)
  const [globalSoapDefaults, setGlobalSoapDefaults] = useState<SoapOptions>({
    detail_level: 5,
    format: 'problem_based',
    custom_instructions: '',
  });

  // Detail view state
  const [activeTab, setActiveTab] = useState<DetailTab>('transcript');
  const [isEditing, setIsEditing] = useState(false);
  const [editedTranscript, setEditedTranscript] = useState('');
  const [copySuccess, setCopySuccess] = useState<string | null>(null);

  // SOAP display state (result stored locally since hook doesn't track per-session)
  const [soapResult, setSoapResult] = useState<MultiPatientSoapResult | null>(null);
  const [customInstructionsExpanded, setCustomInstructionsExpanded] = useState(false);
  const [activePatient, setActivePatient] = useState(0);

  // Cleanup mode state
  const [isCleanupMode, setIsCleanupMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [cleanupDialog, setCleanupDialog] = useState<CleanupDialog>('none');
  const [cleanupMessage, setCleanupMessage] = useState<string | null>(null);

  // LLM connection check - sync to SOAP hook
  const { status: ollamaConnectionStatus } = useOllamaConnection();

  // Sync Ollama status from connection hook to SOAP hook
  useEffect(() => {
    if (ollamaConnectionStatus) {
      setOllamaStatus(ollamaConnectionStatus);
    }
  }, [ollamaConnectionStatus, setOllamaStatus]);

  // LLM connection status from hook
  const llmConnected = ollamaStatus?.connected ?? false;

  // Load settings to determine data source (SOAP options handled by hook)
  useEffect(() => {
    const loadSettings = async () => {
      try {
        const settings = await invoke<Settings>('get_settings');
        // If debug storage is disabled, use Medplum (when authenticated)
        // If debug storage is enabled, use local archive
        if (settings.debug_storage_enabled) {
          setDataSource('local');
        } else {
          setDataSource('medplum');
        }
      } catch (e) {
        console.error('Failed to load settings:', e);
        // Default to local on error
        setDataSource('local');
      } finally {
        setSettingsLoaded(true);
      }
    };
    loadSettings();
  }, []);

  // Sync globalSoapDefaults with hook's soapOptions (for session metadata fallback)
  useEffect(() => {
    setGlobalSoapDefaults(soapOptions);
  }, [soapOptions]);

  // Fetch all dates that have sessions (for calendar highlighting)
  useEffect(() => {
    if (!settingsLoaded) return;

    const fetchDates = async () => {
      try {
        if (dataSource === 'local') {
          // Fetch from local archive
          const dates = await invoke<string[]>('get_local_session_dates');
          setDatesWithSessions(new Set(dates));
        } else if (dataSource === 'medplum' && authState.is_authenticated) {
          // Fetch from Medplum - get all encounters and extract unique dates
          const encounters = await invoke<EncounterSummary[]>('medplum_get_encounter_history', {
            startDate: null,
            endDate: null,
          });
          const dates = new Set<string>();
          encounters.forEach((enc) => {
            // Extract date from encounter date string (YYYY-MM-DD format)
            const dateOnly = enc.date.split('T')[0];
            dates.add(dateOnly);
          });
          setDatesWithSessions(dates);
        } else {
          // Not authenticated for Medplum - show empty
          setDatesWithSessions(new Set());
        }
      } catch (e) {
        console.error('Failed to fetch session dates:', e);
        setDatesWithSessions(new Set());
      }
    };
    fetchDates();
  }, [settingsLoaded, dataSource, authState.is_authenticated]);

  // Fetch sessions for selected date from local archive or Medplum
  const fetchSessions = useCallback(async () => {
    if (!settingsLoaded) return;

    setLoading(true);
    setError(null);

    try {
      const dateStr = formatDateForApi(selectedDate);

      if (dataSource === 'local') {
        // Fetch from local archive
        const result = await invoke<LocalArchiveSummary[]>('get_local_sessions_by_date', {
          date: dateStr,
        });
        setSessions(result);
      } else if (dataSource === 'medplum') {
        if (!authState.is_authenticated) {
          setError('Sign in to Medplum to view session history');
          setSessions([]);
          return;
        }

        // Fetch from Medplum for the selected date
        const nextDay = new Date(selectedDate);
        nextDay.setDate(nextDay.getDate() + 1);
        const endDateStr = formatDateForApi(nextDay);

        const encounters = await invoke<EncounterSummary[]>('medplum_get_encounter_history', {
          startDate: dateStr,
          endDate: endDateStr,
        });

        // Convert EncounterSummary to LocalArchiveSummary format
        const converted: LocalArchiveSummary[] = encounters.map((enc) => ({
          session_id: enc.fhirId,
          date: enc.date,
          duration_ms: enc.durationMinutes ? enc.durationMinutes * 60 * 1000 : null,
          word_count: 0, // Not available from Medplum summary
          has_soap_note: enc.hasSoapNote,
          has_audio: enc.hasAudio,
          auto_ended: false, // Not tracked in Medplum
          charting_mode: null,
          encounter_number: null,
          patient_name: null,
        }));
        setSessions(converted);
      }
    } catch (e) {
      const errMsg = e instanceof Error ? e.message : String(e);
      if (!errMsg.includes('not found')) {
        setError(errMsg);
      }
      setSessions([]);
    } finally {
      setLoading(false);
    }
  }, [selectedDate, settingsLoaded, dataSource, authState.is_authenticated]);

  useEffect(() => {
    if (settingsLoaded) {
      fetchSessions();
    }
  }, [fetchSessions, settingsLoaded]);

  // Fetch session details from local archive or Medplum
  const fetchSessionDetails = async (session: LocalArchiveSummary) => {
    setLoading(true);
    setError(null);

    try {
      let details: LocalArchiveDetails;

      if (dataSource === 'local') {
        // Fetch from local archive
        const dateStr = formatDateForApi(selectedDate);
        details = await invoke<LocalArchiveDetails>('get_local_session_details', {
          sessionId: session.session_id,
          date: dateStr,
        });
      } else {
        // Fetch from Medplum
        const encDetails = await invoke<EncounterDetails>('medplum_get_encounter_details', {
          encounterId: session.session_id,
        });

        // Convert EncounterDetails to LocalArchiveDetails format
        const metadata: LocalArchiveMetadata = {
          session_id: encDetails.fhirId,
          started_at: encDetails.date,
          ended_at: null,
          duration_ms: encDetails.durationMinutes ? encDetails.durationMinutes * 60 * 1000 : null,
          segment_count: 0,
          word_count: encDetails.transcript ? encDetails.transcript.split(/\s+/).length : 0,
          has_soap_note: encDetails.hasSoapNote,
          has_audio: encDetails.hasAudio,
          auto_ended: false,
          auto_end_reason: null,
          soap_detail_level: null, // Not available from Medplum
          soap_format: null, // Not available from Medplum
          charting_mode: null,
          encounter_number: null,
          patient_name: null,
          likely_non_clinical: null,
        };

        details = {
          session_id: encDetails.fhirId,
          metadata,
          transcript: encDetails.transcript,
          soap_note: encDetails.soapNote,
          audio_path: encDetails.audioUrl, // This is a URL, not a local path
        };
      }

      setSelectedSession(details);
      setEditedTranscript(details.transcript || '');

      // Reset SOAP state
      setSoapResult(null);
      setSoapError(null);
      setActivePatient(0);

      // Load SOAP options from metadata if available, otherwise use global defaults
      if (details.metadata.soap_detail_level !== null || details.metadata.soap_format !== null) {
        // Session has saved SOAP options - use them for regeneration
        setSoapOptions({
          detail_level: details.metadata.soap_detail_level ?? globalSoapDefaults.detail_level,
          format: details.metadata.soap_format ?? globalSoapDefaults.format,
          custom_instructions: globalSoapDefaults.custom_instructions, // Custom instructions not stored per-session
        });
      } else {
        // No saved options (old session or Medplum) - use global defaults
        setSoapOptions(globalSoapDefaults);
      }

      // If session has SOAP note, create a simple result to display it
      if (details.soap_note) {
        setSoapResult({
          notes: [{
            speaker_id: 'Patient',
            patient_label: 'Patient',
            content: details.soap_note,
          }],
          physician_speaker: null,
          generated_at: details.metadata.ended_at || new Date().toISOString(),
          model_used: 'archived',
        });
        setActiveTab('soap');
      } else {
        setActiveTab('transcript');
      }

      setView('detail');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  // Generate SOAP note using shared hook
  const handleGenerateSoap = useCallback(async () => {
    if (!editedTranscript.trim()) return;

    // Use hook's generateSoapNote - handles LLM call, clipboard copy, settings persistence
    const result = await generateSoapNote(
      editedTranscript,
      undefined, // audioEvents
      soapOptions,
      selectedSession?.session_id
    );

    if (!result) return; // Hook handles error state

    setSoapResult(result);
    setActivePatient(0);

    // Save SOAP note to archive (hook doesn't know about session context)
    if (selectedSession) {
      const soapContent = result.notes.map(n =>
        result.notes.length > 1
          ? `=== ${n.patient_label} ===\n\n${n.content}`
          : n.content
      ).join('\n\n---\n\n');

      try {
        if (dataSource === 'local') {
          // Save to local archive with SOAP options
          const dateStr = formatDateForApi(selectedDate);
          await invoke('save_local_soap_note', {
            sessionId: selectedSession.session_id,
            date: dateStr,
            soapContent,
            detailLevel: soapOptions.detail_level,
            format: soapOptions.format,
          });
        } else if (dataSource === 'medplum' && authState.is_authenticated) {
          // Save to Medplum (no metadata support)
          await invoke('medplum_add_soap_to_encounter', {
            encounterFhirId: selectedSession.session_id,
            soapNote: soapContent,
          });
        }
      } catch (saveErr) {
        console.error('Failed to save SOAP to archive:', saveErr);
      }

      // Update local global defaults after successful generation
      setGlobalSoapDefaults(soapOptions);
    }
  }, [editedTranscript, soapOptions, selectedSession, selectedDate, dataSource, authState.is_authenticated, generateSoapNote]);

  const handleBackToList = () => {
    setView('list');
    setSelectedSession(null);
    setIsEditing(false);
    setSoapResult(null);
    setSoapError(null);
  };

  const handleCopy = async (text: string, field: string) => {
    try {
      await writeText(text);
      setCopySuccess(field);
      setTimeout(() => setCopySuccess(null), 2000);
    } catch (e) {
      console.error('Failed to copy:', e);
    }
  };

  const handleClose = async () => {
    try {
      const window = getCurrentWindow();
      await window.close();
    } catch (e) {
      console.error('Failed to close window:', e);
    }
  };

  // Cleanup mode helpers
  const toggleCleanupMode = useCallback(() => {
    setIsCleanupMode(prev => {
      if (prev) {
        // Exiting cleanup mode — clear selection
        setSelectedIds(new Set());
        setCleanupDialog('none');
      }
      return !prev;
    });
  }, []);

  const toggleSessionSelection = useCallback((sessionId: string) => {
    setSelectedIds(prev => {
      const next = new Set(prev);
      if (next.has(sessionId)) {
        next.delete(sessionId);
      } else {
        next.add(sessionId);
      }
      return next;
    });
  }, []);

  // Get selected sessions in display order (for dialogs)
  const getSelectedSessions = useCallback((): LocalArchiveSummary[] => {
    return sessions.filter(s => selectedIds.has(s.session_id));
  }, [sessions, selectedIds]);

  // Post-operation: refresh list, renumber, clear selection, show message
  const afterCleanupOp = useCallback(async (message: string) => {
    setCleanupDialog('none');
    setSelectedIds(new Set());
    setCleanupMessage(message);
    setTimeout(() => setCleanupMessage(null), 3000);

    // Renumber encounters and refresh
    const dateStr = formatDateForApi(selectedDate);
    try {
      await invoke('renumber_local_encounters', { date: dateStr });
    } catch (e) {
      console.error('Failed to renumber encounters:', e);
    }
    await fetchSessions();

    // Refresh calendar dates
    try {
      const dates = await invoke<string[]>('get_local_session_dates');
      setDatesWithSessions(new Set(dates));
    } catch (e) {
      console.error('Failed to refresh dates:', e);
    }
  }, [selectedDate, fetchSessions]);

  // Delete operation
  const handleDeleteConfirm = useCallback(async () => {
    const dateStr = formatDateForApi(selectedDate);
    const ids = Array.from(selectedIds);
    try {
      for (const id of ids) {
        await invoke('delete_local_session', { sessionId: id, date: dateStr });
      }
      await afterCleanupOp(`Deleted ${ids.length} session${ids.length > 1 ? 's' : ''}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setCleanupDialog('none');
    }
  }, [selectedDate, selectedIds, afterCleanupOp]);

  // Merge operation
  const handleMergeConfirm = useCallback(async () => {
    const dateStr = formatDateForApi(selectedDate);
    const ids = Array.from(selectedIds);
    try {
      await invoke<string>('merge_local_sessions', { sessionIds: ids, date: dateStr });
      await afterCleanupOp(`Merged ${ids.length} sessions`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setCleanupDialog('none');
    }
  }, [selectedDate, selectedIds, afterCleanupOp]);

  // Edit name operation
  const handleEditNameConfirm = useCallback(async (name: string) => {
    const dateStr = formatDateForApi(selectedDate);
    const id = Array.from(selectedIds)[0];
    try {
      await invoke('update_session_patient_name', { sessionId: id, date: dateStr, patientName: name });
      await afterCleanupOp('Patient name updated');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setCleanupDialog('none');
    }
  }, [selectedDate, selectedIds, afterCleanupOp]);

  // Open split window — passes context via URL query params
  const openSplitWindow = useCallback(async () => {
    const dateStr = formatDateForApi(selectedDate);
    const id = Array.from(selectedIds)[0];
    if (!id) return;

    try {
      // Close existing split window if any
      const existing = await WebviewWindow.getByLabel('split');
      if (existing) {
        await existing.close();
      }

      const splitWindow = new WebviewWindow('split', {
        url: `split.html?sessionId=${encodeURIComponent(id)}&date=${encodeURIComponent(dateStr)}`,
        title: 'Split Session',
        width: 700,
        height: 800,
        minWidth: 500,
        minHeight: 400,
        resizable: true,
      });

      splitWindow.once('tauri://error', (e) => {
        console.error('Failed to open split window:', e);
      });
    } catch (e) {
      console.error('Error opening split window:', e);
    }
  }, [selectedDate, selectedIds]);

  // Listen for split_complete from SplitWindow
  // Store callback in ref to keep listener stable (avoid re-subscription on callback identity change)
  const afterCleanupOpRef = useRef(afterCleanupOp);
  afterCleanupOpRef.current = afterCleanupOp;

  useEffect(() => {
    let mounted = true;
    let cleanup: (() => void) | undefined;

    const setup = async () => {
      const unlisten = await listen('split_complete', () => {
        if (mounted) {
          afterCleanupOpRef.current('Session split into two');
        }
      });

      if (!mounted) {
        unlisten();
        return;
      }

      cleanup = unlisten;
    };

    setup();

    return () => {
      mounted = false;
      cleanup?.();
    };
  }, []);

  // SOAP regeneration for selected sessions
  const handleRegenSoap = useCallback(async () => {
    const dateStr = formatDateForApi(selectedDate);
    const ids = Array.from(selectedIds);
    let regenCount = 0;
    for (const id of ids) {
      try {
        const details = await invoke<LocalArchiveDetails>('get_local_session_details', {
          sessionId: id,
          date: dateStr,
        });
        if (details.transcript?.trim()) {
          const result = await generateSoapNote(details.transcript, undefined, soapOptions, id);
          if (result) {
            const soapContent = result.notes.map(n =>
              result.notes.length > 1
                ? `=== ${n.patient_label} ===\n\n${n.content}`
                : n.content
            ).join('\n\n---\n\n');
            await invoke('save_local_soap_note', {
              sessionId: id,
              date: dateStr,
              soapContent,
              detailLevel: soapOptions.detail_level,
              format: soapOptions.format,
            });
            regenCount++;
          }
        }
      } catch (e) {
        console.error(`Failed to regen SOAP for ${id}:`, e);
      }
    }
    await afterCleanupOp(`Regenerated SOAP for ${regenCount} session${regenCount !== 1 ? 's' : ''}`);
  }, [selectedDate, selectedIds, soapOptions, generateSoapNote, afterCleanupOp]);

  // Cleanup mode only works with local data source
  const canCleanup = dataSource === 'local';

  // Derived values
  const hasTranscript = editedTranscript.trim().length > 0;
  const isModified = selectedSession?.transcript !== editedTranscript;
  const activeSoapContent = soapResult?.notes[activePatient]?.content ?? null;
  const isMultiPatient = (soapResult?.notes.length ?? 0) > 1;

  // Detail view
  if (view === 'detail' && selectedSession) {
    return (
      <div className="history-window">
        <div className="history-header">
          <button className="back-btn" onClick={handleBackToList}>
            &#8592; Back
          </button>
          <h1>Session Details</h1>
          <button className="close-btn" onClick={handleClose} aria-label="Close">
            &times;
          </button>
        </div>

        <div className="history-content detail-content">
          {/* Session Summary Bar */}
          <div className="session-summary">
            <span className="summary-time">{formatTime(selectedSession.metadata.started_at)}</span>
            {selectedSession.metadata.duration_ms && (
              <span className="summary-duration">{formatDuration(selectedSession.metadata.duration_ms)}</span>
            )}
            <span className="summary-words">{selectedSession.metadata.word_count} words</span>
            {selectedSession.metadata.charting_mode === 'continuous' && (
              <span className="summary-badge charted">
                Auto-charted{selectedSession.metadata.encounter_number != null ? ` #${selectedSession.metadata.encounter_number}` : ''}{selectedSession.metadata.patient_name ? ` \u2014 ${selectedSession.metadata.patient_name}` : ''}
              </span>
            )}
            {selectedSession.metadata.auto_ended && (
              <span className="summary-badge auto">Auto-ended</span>
            )}
          </div>

          {/* Tab Navigation */}
          <div className="review-tabs">
            <button
              className={`review-tab ${activeTab === 'transcript' ? 'active' : ''}`}
              onClick={() => setActiveTab('transcript')}
            >
              Transcript
              {isModified && <span className="tab-badge">edited</span>}
            </button>
            <button
              className={`review-tab ${activeTab === 'soap' ? 'active' : ''}`}
              onClick={() => setActiveTab('soap')}
              disabled={!hasTranscript}
            >
              SOAP
              {soapResult && <span className="tab-badge done">✓</span>}
            </button>
            <button
              className={`review-tab ${activeTab === 'insights' ? 'active' : ''}`}
              onClick={() => setActiveTab('insights')}
            >
              Insights
            </button>
          </div>

          {/* Tab Content */}
          <div className="review-tab-content">
            {/* Transcript Tab */}
            {activeTab === 'transcript' && (
              <div className="tab-panel transcript-panel">
                <div className="panel-header">
                  <div className="panel-actions">
                    {hasTranscript && (
                      <>
                        <button
                          className={`btn-small ${isEditing ? 'active' : ''}`}
                          onClick={() => setIsEditing(!isEditing)}
                        >
                          {isEditing ? 'Done' : 'Edit'}
                        </button>
                        <button
                          className={`btn-small copy-btn ${copySuccess === 'transcript' ? 'success' : ''}`}
                          onClick={() => handleCopy(editedTranscript, 'transcript')}
                        >
                          {copySuccess === 'transcript' ? 'Copied!' : 'Copy'}
                        </button>
                      </>
                    )}
                  </div>
                </div>

                <div className="panel-body">
                  {hasTranscript ? (
                    isEditing ? (
                      <textarea
                        className="transcript-editor"
                        value={editedTranscript}
                        onChange={(e) => setEditedTranscript(e.target.value)}
                        placeholder="Edit transcript..."
                      />
                    ) : (
                      <div className="transcript-display">
                        {editedTranscript.split('\n\n').map((paragraph, i) => (
                          <p key={i}>{paragraph}</p>
                        ))}
                      </div>
                    )
                  ) : (
                    <div className="panel-empty">No transcript recorded</div>
                  )}
                </div>
              </div>
            )}

            {/* SOAP Tab */}
            {activeTab === 'soap' && (
              <div className="tab-panel soap-panel">
                {/* SOAP Options */}
                {!isGeneratingSoap && (
                  <div className="soap-options">
                    {/* Detail Level Slider */}
                    <div className="soap-option-row">
                      <label className="soap-option-label">
                        Detail: {DETAIL_LEVEL_LABELS[soapOptions.detail_level]?.name || 'Standard'}
                      </label>
                      <div className="soap-detail-slider">
                        <input
                          type="range"
                          min="1"
                          max="10"
                          value={soapOptions.detail_level}
                          onChange={(e) => updateSoapDetailLevel(parseInt(e.target.value))}
                          className="detail-slider"
                          aria-label="SOAP note detail level"
                        />
                        <span className="detail-value">{soapOptions.detail_level}</span>
                      </div>
                    </div>

                    {/* Format Toggle */}
                    <div className="soap-option-row">
                      <label className="soap-option-label">Format</label>
                      <div className="soap-format-toggle">
                        <button
                          className={`format-btn ${soapOptions.format === 'problem_based' ? 'active' : ''}`}
                          onClick={() => updateSoapFormat('problem_based')}
                        >
                          Problem
                        </button>
                        <button
                          className={`format-btn ${soapOptions.format === 'comprehensive' ? 'active' : ''}`}
                          onClick={() => updateSoapFormat('comprehensive')}
                        >
                          Comprehensive
                        </button>
                      </div>
                    </div>

                    {/* Custom Instructions */}
                    <div className="soap-option-row custom-instructions">
                      <button
                        className="custom-instructions-toggle"
                        onClick={() => setCustomInstructionsExpanded(!customInstructionsExpanded)}
                      >
                        <span className={`chevron-small ${customInstructionsExpanded ? '' : 'collapsed'}`}>&#9660;</span>
                        Custom Instructions
                        {soapOptions.custom_instructions.trim() && (
                          <span className="custom-badge">Active</span>
                        )}
                      </button>
                      {customInstructionsExpanded && (
                        <textarea
                          className="custom-instructions-input"
                          value={soapOptions.custom_instructions}
                          onChange={(e) => updateSoapCustomInstructions(e.target.value)}
                          placeholder="Add specific instructions..."
                          rows={3}
                        />
                      )}
                    </div>
                  </div>
                )}

                {/* Generate Button */}
                {!soapResult && !isGeneratingSoap && !soapError && (
                  <button
                    className="btn-generate"
                    onClick={handleGenerateSoap}
                    disabled={!llmConnected || !hasTranscript}
                  >
                    {!hasTranscript ? 'No transcript' : llmConnected ? 'Generate SOAP Note' : 'LLM not connected'}
                  </button>
                )}

                {/* Loading State */}
                {isGeneratingSoap && (
                  <div className="soap-loading">
                    <div className="spinner-small" />
                    <span>Generating SOAP note...</span>
                  </div>
                )}

                {/* Error State */}
                {soapError && (
                  <div className="soap-error">
                    <span>{soapError}</span>
                    <button className="btn-retry-small" onClick={handleGenerateSoap}>
                      Retry
                    </button>
                  </div>
                )}

                {/* SOAP Display */}
                {soapResult && activeSoapContent && (
                  <div className="soap-display">
                    <div className="soap-header">
                      <span className="soap-timestamp">
                        {soapResult.model_used !== 'archived'
                          ? `Generated ${formatLocalDateTime(soapResult.generated_at)}`
                          : 'Previously generated'}
                      </span>
                      <div className="soap-actions">
                        <button
                          className={`btn-small copy-btn ${copySuccess === 'soap' ? 'success' : ''}`}
                          onClick={() => handleCopy(activeSoapContent, 'soap')}
                        >
                          {copySuccess === 'soap' ? 'Copied!' : 'Copy'}
                        </button>
                        <button
                          className="btn-small"
                          onClick={handleGenerateSoap}
                          disabled={isGeneratingSoap || !llmConnected}
                        >
                          Regenerate
                        </button>
                      </div>
                    </div>

                    {/* Multi-patient info and tabs */}
                    {isMultiPatient && (
                      <div className="multi-patient-soap">
                        <div className="patient-info">
                          <span className="physician-label">
                            Physician: {soapResult.physician_speaker || 'Not identified'}
                          </span>
                          <span className="patient-count">
                            {soapResult.notes.length} patients detected
                          </span>
                        </div>
                        <div className="patient-tabs">
                          {soapResult.notes.map((note, i) => (
                            <button
                              key={i}
                              className={`patient-tab ${activePatient === i ? 'active' : ''}`}
                              onClick={() => setActivePatient(i)}
                            >
                              {note.patient_label}
                              <span className="speaker-id">({note.speaker_id})</span>
                            </button>
                          ))}
                        </div>
                      </div>
                    )}

                    <div className="soap-content">
                      <pre className="soap-text-content">{activeSoapContent}</pre>
                    </div>

                    {soapResult.model_used !== 'archived' && (
                      <div className="soap-meta">
                        <span className="soap-model">Model: {soapResult.model_used}</span>
                      </div>
                    )}
                  </div>
                )}
              </div>
            )}

            {/* Insights Tab */}
            {activeTab === 'insights' && (
              <div className="tab-panel insights-panel">
                {/* Session Info */}
                <div className="insight-card">
                  <div className="insight-card-header">Session Info</div>
                  <div className="insight-card-body">
                    <div className="insight-metric">
                      <span className="metric-label">Started</span>
                      <span className="metric-value">{formatLocalDateTime(selectedSession.metadata.started_at)}</span>
                    </div>
                    {selectedSession.metadata.ended_at && (
                      <div className="insight-metric">
                        <span className="metric-label">Ended</span>
                        <span className="metric-value">{formatLocalDateTime(selectedSession.metadata.ended_at)}</span>
                      </div>
                    )}
                    <div className="insight-metric">
                      <span className="metric-label">Duration</span>
                      <span className="metric-value">{formatDuration(selectedSession.metadata.duration_ms)}</span>
                    </div>
                    <div className="insight-metric">
                      <span className="metric-label">Words</span>
                      <span className="metric-value">{selectedSession.metadata.word_count}</span>
                    </div>
                  </div>
                </div>

                {/* Storage Info */}
                <div className="insight-card">
                  <div className="insight-card-header">Storage</div>
                  <div className="insight-card-body">
                    <div className="insight-metric">
                      <span className="metric-label">Transcript</span>
                      <span className="metric-value">{selectedSession.transcript ? '✓ Saved' : '✗ None'}</span>
                    </div>
                    <div className="insight-metric">
                      <span className="metric-label">SOAP Note</span>
                      <span className="metric-value">{selectedSession.metadata.has_soap_note ? '✓ Saved' : '✗ None'}</span>
                    </div>
                    <div className="insight-metric">
                      <span className="metric-label">Audio</span>
                      <span className="metric-value">{selectedSession.audio_path ? '✓ Saved' : '✗ None'}</span>
                    </div>
                  </div>
                </div>

                {/* Auto-end Info */}
                {selectedSession.metadata.auto_ended && (
                  <div className="insight-card">
                    <div className="insight-card-header">Auto-End</div>
                    <div className="insight-card-body">
                      <div className="insight-metric">
                        <span className="metric-label">Reason</span>
                        <span className="metric-value">{selectedSession.metadata.auto_end_reason || 'Silence detected'}</span>
                      </div>
                    </div>
                  </div>
                )}

                {/* Audio Player */}
                {selectedSession.audio_path && (
                  <div className="insight-card">
                    <div className="insight-card-header">Audio Recording</div>
                    <div className="insight-card-body audio-player-container">
                      <AudioPlayer
                        audioUrl={
                          dataSource === 'medplum'
                            ? selectedSession.audio_path // Medplum provides a URL
                            : `file://${selectedSession.audio_path}` // Local file path
                        }
                      />
                    </div>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Data source and auth status footer */}
          {!authLoading && (
            <div className="history-footer">
              <span className="data-source-indicator">
                {dataSource === 'local' ? '💾 Local Storage' : '☁️ Medplum'}
              </span>
              {dataSource === 'medplum' && !authState.is_authenticated && (
                <button className="auth-status not-authenticated" onClick={login}>
                  Sign in to view history
                </button>
              )}
              {dataSource === 'local' && authState.is_authenticated && (
                <span className="auth-status authenticated">
                  ☁️ Also synced to Medplum
                </span>
              )}
            </div>
          )}
        </div>
      </div>
    );
  }

  // List view
  return (
    <div className="history-window">
      <div className="history-header">
        <button className="close-btn" onClick={handleClose} aria-label="Close">
          &times;
        </button>
        <h1>Session History</h1>
        {canCleanup && sessions.length > 0 && (
          <button
            className={`cleanup-toggle-btn ${isCleanupMode ? 'active' : ''}`}
            onClick={toggleCleanupMode}
            aria-label={isCleanupMode ? 'Exit cleanup mode' : 'Enter cleanup mode'}
            title={isCleanupMode ? 'Exit cleanup mode' : 'Manage sessions'}
          >
            &#9998;
          </button>
        )}
      </div>

      <div className="history-content">
        {/* Success message */}
        {cleanupMessage && (
          <div className="cleanup-success-message">{cleanupMessage}</div>
        )}

        <div className="calendar-with-today">
          <Calendar
            selectedDate={selectedDate}
            onDateSelect={setSelectedDate}
            datesWithSessions={Array.from(datesWithSessions)}
          />
          {selectedDate.toDateString() !== new Date().toDateString() && (
            <button
              className="btn-today"
              onClick={() => setSelectedDate(new Date())}
            >
              Today
            </button>
          )}
        </div>

        <div className="sessions-section">
          <h2 className="sessions-date-header">
            {formatDateForDisplay(selectedDate)}
          </h2>

          {loading ? (
            <div className="sessions-loading">
              <div className="spinner" />
            </div>
          ) : error ? (
            <div className="sessions-error">
              <p>{error}</p>
              <button onClick={fetchSessions}>Retry</button>
            </div>
          ) : sessions.length === 0 ? (
            <div className="sessions-empty">
              <p>No sessions recorded on this date</p>
            </div>
          ) : (
            <div className="sessions-list">
              {[...sessions].sort((a, b) => new Date(a.date).getTime() - new Date(b.date).getTime()).map((session) => (
                <div
                  key={session.session_id}
                  className={`session-item ${isCleanupMode && selectedIds.has(session.session_id) ? 'selected' : ''}`}
                >
                  {isCleanupMode && (
                    <label className="cleanup-checkbox" onClick={(e) => e.stopPropagation()}>
                      <input
                        type="checkbox"
                        checked={selectedIds.has(session.session_id)}
                        onChange={() => toggleSessionSelection(session.session_id)}
                      />
                    </label>
                  )}
                  <button
                    className="session-item-body"
                    onClick={() => {
                      if (isCleanupMode) {
                        toggleSessionSelection(session.session_id);
                      } else {
                        fetchSessionDetails(session);
                      }
                    }}
                  >
                    <div className="session-info">
                      <span className="session-time">{formatTime(session.date)}</span>
                      <span className="session-name">
                        {session.charting_mode === 'continuous' && session.encounter_number != null
                          ? `Encounter #${session.encounter_number}${session.patient_name ? ` \u2014 ${session.patient_name}` : ''}`
                          : session.word_count > 0
                            ? `${session.word_count} words`
                            : 'Scribe Session'}
                      </span>
                    </div>
                    <div className="session-meta">
                      {session.duration_ms && (
                        <span className="session-duration">
                          {formatDuration(session.duration_ms)}
                        </span>
                      )}
                      <div className="session-badges">
                        {session.charting_mode === 'continuous' && (
                          <span className="badge charted-badge">Auto-charted</span>
                        )}
                        {session.has_soap_note && (
                          <span className="badge soap-badge">SOAP</span>
                        )}
                        {session.has_audio && (
                          <span className="badge audio-badge">Audio</span>
                        )}
                        {session.auto_ended && (
                          <span className="badge auto-badge">Auto</span>
                        )}
                      </div>
                    </div>
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Cleanup action bar */}
        {isCleanupMode && (
          <CleanupActionBar
            selectedCount={selectedIds.size}
            onMerge={() => setCleanupDialog('merge')}
            onDelete={() => setCleanupDialog('delete')}
            onEditName={() => setCleanupDialog('editName')}
            onSplit={openSplitWindow}
            onRegenSoap={handleRegenSoap}
          />
        )}

        {/* Data source and auth status footer */}
        {!authLoading && !isCleanupMode && (
          <div className="history-footer">
            <span className="data-source-indicator">
              {dataSource === 'local' ? '💾 Local Storage' : '☁️ Medplum'}
            </span>
            {dataSource === 'medplum' && !authState.is_authenticated && (
              <button className="auth-status not-authenticated" onClick={login}>
                Sign in to view history
              </button>
            )}
            {dataSource === 'local' && authState.is_authenticated && (
              <span className="auth-status authenticated">
                ☁️ Also synced to Medplum
              </span>
            )}
          </div>
        )}
      </div>

      {/* Cleanup dialogs */}
      {cleanupDialog === 'delete' && (
        <DeleteConfirmDialog
          sessions={getSelectedSessions()}
          onConfirm={handleDeleteConfirm}
          onCancel={() => setCleanupDialog('none')}
        />
      )}
      {cleanupDialog === 'merge' && (
        <MergeConfirmDialog
          sessions={getSelectedSessions()}
          onConfirm={handleMergeConfirm}
          onCancel={() => setCleanupDialog('none')}
        />
      )}
      {cleanupDialog === 'editName' && selectedIds.size === 1 && (
        <EditNameDialog
          currentName={getSelectedSessions()[0]?.patient_name ?? null}
          onConfirm={handleEditNameConfirm}
          onCancel={() => setCleanupDialog('none')}
        />
      )}
      {/* Split is now handled in a separate window via openSplitWindow */}
    </div>
  );
};

export default HistoryWindow;
