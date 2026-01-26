/**
 * HistoryView - Encounter history browser
 *
 * Shows:
 * - List of past encounters from both Medplum and local archive
 * - Filter by date range
 * - Click to view details
 * - View transcript/SOAP note
 * - Works offline with local archive fallback
 */

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type {
  EncounterSummary,
  EncounterDetails,
  LocalArchiveSummary,
  LocalArchiveDetails,
} from '../types';
import { formatLocalDate } from '../utils';

// Unified encounter item that can come from either source
interface UnifiedEncounter {
  id: string;
  source: 'medplum' | 'local';
  patientName: string;
  date: string;
  durationMinutes: number | null;
  hasSoapNote: boolean;
  hasAudio: boolean;
  autoEnded?: boolean;
  // For local archive, we need the date string for detail lookup
  localDate?: string;
}

// Unified detail view
interface UnifiedDetails {
  source: 'medplum' | 'local';
  patientName: string;
  date: string;
  durationMinutes: number | null;
  transcript: string | null;
  soapNote: string | null;
  audioUrl: string | null;
  autoEnded?: boolean;
}

interface HistoryViewProps {
  onClose?: () => void;
  onSelectEncounter?: (details: EncounterDetails) => void;
}

export function HistoryView({ onClose, onSelectEncounter }: HistoryViewProps) {
  const [encounters, setEncounters] = useState<UnifiedEncounter[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedEncounter, setSelectedEncounter] = useState<UnifiedDetails | null>(null);
  const [isLoadingDetails, setIsLoadingDetails] = useState(false);
  const [dataSource, setDataSource] = useState<'both' | 'local' | 'medplum'>('both');

  // Date range filters
  const [startDate, setStartDate] = useState<string>('');
  const [endDate, setEndDate] = useState<string>('');

  useEffect(() => {
    loadEncounters();
  }, [startDate, endDate]);

  const loadEncounters = async () => {
    try {
      setIsLoading(true);
      setError(null);

      const unified: UnifiedEncounter[] = [];
      let medplumLoaded = false;
      let localLoaded = false;

      // Try to load from Medplum first
      try {
        const medplumResults = await invoke<EncounterSummary[]>('medplum_get_encounter_history', {
          startDate: startDate || null,
          endDate: endDate || null,
        });

        for (const enc of medplumResults) {
          unified.push({
            id: enc.id,
            source: 'medplum',
            patientName: enc.patientName,
            date: enc.date,
            durationMinutes: enc.durationMinutes,
            hasSoapNote: enc.hasSoapNote,
            hasAudio: enc.hasAudio,
          });
        }
        medplumLoaded = true;
      } catch (e) {
        console.log('Medplum unavailable, using local archive only:', e);
      }

      // Load from local archive
      try {
        // Get all dates with sessions
        const dates = await invoke<string[]>('get_local_session_dates');

        // Filter dates if range is specified
        const filteredDates = dates.filter((d) => {
          if (startDate && d < startDate) return false;
          if (endDate && d > endDate) return false;
          return true;
        });

        // Load sessions for each date
        for (const date of filteredDates) {
          const sessions = await invoke<LocalArchiveSummary[]>('get_local_sessions_by_date', {
            date,
          });

          for (const session of sessions) {
            // Check if this session is already in unified from Medplum
            // (by matching session_id - Medplum encounters would have same ID)
            const existingIdx = unified.findIndex(
              (u) => u.id === session.session_id || u.id === session.session_id.split('-')[0]
            );

            if (existingIdx === -1) {
              // Not in Medplum, add from local
              unified.push({
                id: session.session_id,
                source: 'local',
                patientName: 'Local Session',
                date: session.date,
                durationMinutes: session.duration_ms ? Math.round(session.duration_ms / 60000) : null,
                hasSoapNote: session.has_soap_note,
                hasAudio: session.has_audio,
                autoEnded: session.auto_ended,
                localDate: date,
              });
            }
          }
        }
        localLoaded = true;
      } catch (e) {
        console.log('Local archive unavailable:', e);
      }

      // Update data source indicator
      if (medplumLoaded && localLoaded) {
        setDataSource('both');
      } else if (medplumLoaded) {
        setDataSource('medplum');
      } else if (localLoaded) {
        setDataSource('local');
      }

      // Sort by date descending
      unified.sort((a, b) => b.date.localeCompare(a.date));

      setEncounters(unified);
    } catch (e) {
      const errorMsg = e instanceof Error ? e.message : String(e);
      setError(errorMsg);
      console.error('Failed to load encounters:', e);
    } finally {
      setIsLoading(false);
    }
  };

  const loadEncounterDetails = async (encounter: UnifiedEncounter) => {
    try {
      setIsLoadingDetails(true);

      if (encounter.source === 'medplum') {
        const details = await invoke<EncounterDetails>('medplum_get_encounter_details', {
          encounterId: encounter.id,
        });
        setSelectedEncounter({
          source: 'medplum',
          patientName: details.patientName,
          date: details.date,
          durationMinutes: details.durationMinutes,
          transcript: details.transcript,
          soapNote: details.soapNote,
          audioUrl: details.audioUrl,
        });
        if (onSelectEncounter) {
          onSelectEncounter(details);
        }
      } else {
        // Local archive
        const details = await invoke<LocalArchiveDetails>('get_local_session_details', {
          sessionId: encounter.id,
          date: encounter.localDate,
        });
        setSelectedEncounter({
          source: 'local',
          patientName: 'Local Session',
          date: details.metadata.started_at,
          durationMinutes: details.metadata.duration_ms
            ? Math.round(details.metadata.duration_ms / 60000)
            : null,
          transcript: details.transcript,
          soapNote: details.soap_note,
          audioUrl: details.audio_path ? `file://${details.audio_path}` : null,
          autoEnded: details.metadata.auto_ended,
        });
      }
    } catch (e) {
      const errorMsg = e instanceof Error ? e.message : String(e);
      setError(errorMsg);
      console.error('Failed to load encounter details:', e);
    } finally {
      setIsLoadingDetails(false);
    }
  };

  const formatDate = (isoString: string): string => {
    try {
      return formatLocalDate(isoString);
    } catch {
      return '--';
    }
  };

  const formatDuration = (minutes: number | null): string => {
    if (minutes === null) return '--';
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.floor(minutes / 60);
    const mins = minutes % 60;
    return `${hours}h ${mins}m`;
  };

  const handleBack = useCallback(() => {
    setSelectedEncounter(null);
  }, []);

  // Detail view
  if (selectedEncounter) {
    return (
      <div className="history-detail-view">
        <div className="history-detail-header">
          <button className="back-button" onClick={handleBack}>
            &larr; Back
          </button>
          <h3>{selectedEncounter.patientName}</h3>
          {onClose && (
            <button className="close-button" onClick={onClose}>
              &times;
            </button>
          )}
        </div>

        <div className="history-detail-meta">
          <span>{formatDate(selectedEncounter.date)}</span>
          <span>{formatDuration(selectedEncounter.durationMinutes)}</span>
          <span className={`source-badge ${selectedEncounter.source}`}>
            {selectedEncounter.source === 'medplum' ? 'Cloud' : 'Local'}
          </span>
          {selectedEncounter.autoEnded && (
            <span className="auto-end-badge">Auto-ended</span>
          )}
        </div>

        <div className="history-detail-content">
          {selectedEncounter.transcript && (
            <div className="history-section">
              <h4>Transcript</h4>
              <div className="history-transcript">{selectedEncounter.transcript}</div>
            </div>
          )}

          {selectedEncounter.soapNote && (
            <div className="history-section">
              <h4>SOAP Note</h4>
              <div className="history-soap-note">{selectedEncounter.soapNote}</div>
            </div>
          )}

          {selectedEncounter.audioUrl && (
            <div className="history-section">
              <h4>Audio Recording</h4>
              <audio controls src={selectedEncounter.audioUrl} />
            </div>
          )}
        </div>
      </div>
    );
  }

  // List view
  return (
    <div className="history-view">
      <div className="history-header">
        <h3>Encounter History</h3>
        <span className="data-source-indicator">
          {dataSource === 'both'
            ? 'Cloud + Local'
            : dataSource === 'medplum'
              ? 'Cloud'
              : 'Local Only'}
        </span>
        {onClose && (
          <button className="close-button" onClick={onClose}>
            &times;
          </button>
        )}
      </div>

      <div className="history-filters">
        <label>
          From
          <input type="date" value={startDate} onChange={(e) => setStartDate(e.target.value)} />
        </label>
        <label>
          To
          <input type="date" value={endDate} onChange={(e) => setEndDate(e.target.value)} />
        </label>
      </div>

      {error && <div className="history-error">{error}</div>}

      {isLoading ? (
        <div className="history-loading">Loading encounters...</div>
      ) : encounters.length === 0 ? (
        <div className="history-empty">No encounters found</div>
      ) : (
        <div className="history-list">
          {encounters.map((encounter) => (
            <button
              key={`${encounter.source}-${encounter.id}`}
              className="history-item"
              onClick={() => loadEncounterDetails(encounter)}
              disabled={isLoadingDetails}
            >
              <div className="history-item-header">
                <span className="history-patient">{encounter.patientName}</span>
                <span className="history-date">{formatDate(encounter.date)}</span>
              </div>
              <div className="history-item-meta">
                <span className="history-duration">{formatDuration(encounter.durationMinutes)}</span>
                <div className="history-badges">
                  <span className={`source-badge small ${encounter.source}`}>
                    {encounter.source === 'medplum' ? 'Cloud' : 'Local'}
                  </span>
                  {encounter.hasSoapNote && <span className="history-badge soap">SOAP</span>}
                  {encounter.hasAudio && <span className="history-badge audio">Audio</span>}
                  {encounter.autoEnded && <span className="history-badge auto-end">Auto</span>}
                </div>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export default HistoryView;
