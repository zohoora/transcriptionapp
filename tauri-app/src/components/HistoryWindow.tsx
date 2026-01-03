import React, { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { useAuth } from './AuthProvider';
import Calendar from './Calendar';
import AudioPlayer from './AudioPlayer';
import { formatDateForApi, formatLocalTime } from '../utils';
import type { EncounterSummary, EncounterDetails } from '../types';

type View = 'list' | 'detail';

function formatDateForDisplay(date: Date): string {
  return date.toLocaleDateString('en-US', {
    weekday: 'long',
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });
}

// Use formatLocalTime from utils for time display
function formatTime(dateString: string): string {
  return formatLocalTime(dateString);
}

const HistoryWindow: React.FC = () => {
  const { authState, isLoading: authLoading, login } = useAuth();
  const [selectedDate, setSelectedDate] = useState(new Date());
  const [sessions, setSessions] = useState<EncounterSummary[]>([]);
  const [selectedSession, setSelectedSession] = useState<EncounterDetails | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<View>('list');
  const [copiedField, setCopiedField] = useState<string | null>(null);

  // Fetch sessions for selected date
  const fetchSessions = useCallback(async () => {
    if (!authState.is_authenticated) return;

    setLoading(true);
    setError(null);

    try {
      const dateStr = formatDateForApi(selectedDate);
      const result = await invoke<EncounterSummary[]>('medplum_get_encounter_history', {
        startDate: dateStr,
        endDate: dateStr,
      });
      setSessions(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setSessions([]);
    } finally {
      setLoading(false);
    }
  }, [selectedDate, authState.is_authenticated]);

  useEffect(() => {
    fetchSessions();
  }, [fetchSessions]);

  // Fetch session details
  const fetchSessionDetails = async (session: EncounterSummary) => {
    setLoading(true);
    setError(null);

    try {
      const details = await invoke<EncounterDetails>('medplum_get_encounter_details', {
        encounterId: session.fhirId,
      });
      setSelectedSession(details);
      setView('detail');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleBackToList = () => {
    setView('list');
    setSelectedSession(null);
  };

  const handleCopy = async (text: string, field: string) => {
    try {
      await writeText(text);
      setCopiedField(field);
      setTimeout(() => setCopiedField(null), 2000);
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

  // Not authenticated
  if (!authLoading && !authState.is_authenticated) {
    return (
      <div className="history-window">
        <div className="history-header">
          <button className="close-btn" onClick={handleClose} aria-label="Close">
            &times;
          </button>
          <h1>Session History</h1>
        </div>
        <div className="history-content auth-required">
          <p>Sign in to Medplum to view your session history.</p>
          <button className="primary-btn" onClick={login}>
            Sign In
          </button>
        </div>
      </div>
    );
  }

  // Loading auth
  if (authLoading) {
    return (
      <div className="history-window">
        <div className="history-header">
          <button className="close-btn" onClick={handleClose} aria-label="Close">
            &times;
          </button>
          <h1>Session History</h1>
        </div>
        <div className="history-content loading">
          <div className="spinner" />
          <p>Loading...</p>
        </div>
      </div>
    );
  }

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

        <div className="history-content">
          <div className="session-detail">
            <div className="detail-meta">
              <span className="detail-time">{formatTime(selectedSession.date)}</span>
              {selectedSession.durationMinutes && (
                <span className="detail-duration">
                  {selectedSession.durationMinutes} min
                </span>
              )}
            </div>

            {/* Transcript Section */}
            <div className="detail-section">
              <div className="section-header">
                <h3>Transcript</h3>
                {selectedSession.transcript && (
                  <button
                    className="copy-btn"
                    onClick={() => handleCopy(selectedSession.transcript!, 'transcript')}
                  >
                    {copiedField === 'transcript' ? 'Copied!' : 'Copy'}
                  </button>
                )}
              </div>
              <div className="section-content transcript-content">
                {selectedSession.transcript ? (
                  <pre>{selectedSession.transcript}</pre>
                ) : (
                  <p className="empty-message">No transcript available</p>
                )}
              </div>
            </div>

            {/* SOAP Note Section */}
            <div className="detail-section">
              <div className="section-header">
                <h3>SOAP Note</h3>
                {selectedSession.soapNote && (
                  <button
                    className="copy-btn"
                    onClick={() => handleCopy(selectedSession.soapNote!, 'soap')}
                  >
                    {copiedField === 'soap' ? 'Copied!' : 'Copy'}
                  </button>
                )}
              </div>
              <div className="section-content soap-content">
                {selectedSession.soapNote ? (
                  <pre>{selectedSession.soapNote}</pre>
                ) : (
                  <p className="empty-message">No SOAP note available</p>
                )}
              </div>
            </div>

            {/* Audio Section */}
            {selectedSession.audioUrl && (
              <div className="detail-section">
                <div className="section-header">
                  <h3>Audio Recording</h3>
                </div>
                <div className="section-content audio-content">
                  <AudioPlayer audioUrl={selectedSession.audioUrl} />
                </div>
              </div>
            )}
          </div>
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
      </div>

      <div className="history-content">
        <Calendar
          selectedDate={selectedDate}
          onDateSelect={setSelectedDate}
        />

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
              {sessions.map((session) => (
                <button
                  key={session.id}
                  className="session-item"
                  onClick={() => fetchSessionDetails(session)}
                >
                  <div className="session-info">
                    <span className="session-time">{formatTime(session.date)}</span>
                    <span className="session-name">Scribe Session</span>
                  </div>
                  <div className="session-meta">
                    {session.durationMinutes && (
                      <span className="session-duration">
                        {session.durationMinutes} min
                      </span>
                    )}
                    <div className="session-badges">
                      {session.hasSoapNote && (
                        <span className="badge soap-badge">SOAP</span>
                      )}
                      {session.hasAudio && (
                        <span className="badge audio-badge">Audio</span>
                      )}
                    </div>
                  </div>
                </button>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default HistoryWindow;
