/**
 * HistoryView - Encounter history browser
 *
 * Shows:
 * - List of past encounters
 * - Filter by date range
 * - Click to view details
 * - View transcript/SOAP note
 */

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { EncounterSummary, EncounterDetails } from '../types';

interface HistoryViewProps {
  onClose?: () => void;
  onSelectEncounter?: (details: EncounterDetails) => void;
}

export function HistoryView({ onClose, onSelectEncounter }: HistoryViewProps) {
  const [encounters, setEncounters] = useState<EncounterSummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedEncounter, setSelectedEncounter] = useState<EncounterDetails | null>(null);
  const [isLoadingDetails, setIsLoadingDetails] = useState(false);

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
      const results = await invoke<EncounterSummary[]>('medplum_get_encounter_history', {
        startDate: startDate || null,
        endDate: endDate || null,
      });
      setEncounters(results);
    } catch (e) {
      const errorMsg = e instanceof Error ? e.message : String(e);
      setError(errorMsg);
      console.error('Failed to load encounters:', e);
    } finally {
      setIsLoading(false);
    }
  };

  const loadEncounterDetails = async (encounterId: string) => {
    try {
      setIsLoadingDetails(true);
      const details = await invoke<EncounterDetails>('medplum_get_encounter_details', {
        encounterId,
      });
      setSelectedEncounter(details);
      if (onSelectEncounter) {
        onSelectEncounter(details);
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
      const date = new Date(isoString);
      return date.toLocaleDateString([], {
        month: 'short',
        day: 'numeric',
        year: 'numeric',
      });
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
        </div>

        <div className="history-detail-content">
          {selectedEncounter.transcript && (
            <div className="history-section">
              <h4>Transcript</h4>
              <div className="history-transcript">
                {selectedEncounter.transcript}
              </div>
            </div>
          )}

          {selectedEncounter.soapNote && (
            <div className="history-section">
              <h4>SOAP Note</h4>
              <div className="history-soap-note">
                {selectedEncounter.soapNote}
              </div>
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
        {onClose && (
          <button className="close-button" onClick={onClose}>
            &times;
          </button>
        )}
      </div>

      <div className="history-filters">
        <label>
          From
          <input
            type="date"
            value={startDate}
            onChange={(e) => setStartDate(e.target.value)}
          />
        </label>
        <label>
          To
          <input
            type="date"
            value={endDate}
            onChange={(e) => setEndDate(e.target.value)}
          />
        </label>
      </div>

      {error && (
        <div className="history-error">
          {error}
        </div>
      )}

      {isLoading ? (
        <div className="history-loading">Loading encounters...</div>
      ) : encounters.length === 0 ? (
        <div className="history-empty">No encounters found</div>
      ) : (
        <div className="history-list">
          {encounters.map((encounter) => (
            <button
              key={encounter.id}
              className="history-item"
              onClick={() => loadEncounterDetails(encounter.id)}
              disabled={isLoadingDetails}
            >
              <div className="history-item-header">
                <span className="history-patient">{encounter.patientName}</span>
                <span className="history-date">{formatDate(encounter.date)}</span>
              </div>
              <div className="history-item-meta">
                <span className="history-duration">
                  {formatDuration(encounter.durationMinutes)}
                </span>
                <div className="history-badges">
                  {encounter.hasSoapNote && (
                    <span className="history-badge soap">SOAP</span>
                  )}
                  {encounter.hasAudio && (
                    <span className="history-badge audio">Audio</span>
                  )}
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
