import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';

// Session state types
type SessionState =
  | 'idle'
  | 'preparing'
  | 'recording'
  | 'stopping'
  | 'completed'
  | 'error';

interface SessionStatus {
  state: SessionState;
  provider: 'whisper' | 'apple' | null;
  elapsed_ms: number;
  is_processing_behind: boolean;
  error_message?: string;
}

interface TranscriptUpdate {
  finalized_text: string;
  draft_text: string | null;
  segment_count: number;
}

interface Device {
  id: string;
  name: string;
  is_default: boolean;
}

interface ModelStatus {
  available: boolean;
  path: string | null;
  error: string | null;
}

function formatTime(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
  }
  return `${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
}

function App() {
  const [status, setStatus] = useState<SessionStatus>({
    state: 'idle',
    provider: null,
    elapsed_ms: 0,
    is_processing_behind: false,
  });
  const [transcript, setTranscript] = useState<TranscriptUpdate>({
    finalized_text: '',
    draft_text: null,
    segment_count: 0,
  });
  const [devices, setDevices] = useState<Device[]>([]);
  const [selectedDevice, setSelectedDevice] = useState<string>('default');
  const [modelStatus, setModelStatus] = useState<ModelStatus | null>(null);
  const [copySuccess, setCopySuccess] = useState(false);

  // Load devices and model status on mount
  useEffect(() => {
    async function init() {
      try {
        const deviceList = await invoke<Device[]>('list_input_devices');
        setDevices(deviceList);

        const status = await invoke<ModelStatus>('check_model_status');
        setModelStatus(status);
      } catch (e) {
        console.error('Failed to initialize:', e);
      }
    }
    init();
  }, []);

  // Listen for events from backend
  useEffect(() => {
    let unlistenStatus: UnlistenFn | undefined;
    let unlistenTranscript: UnlistenFn | undefined;

    async function setupListeners() {
      unlistenStatus = await listen<SessionStatus>('session_status', (event) => {
        setStatus(event.payload);
      });

      unlistenTranscript = await listen<TranscriptUpdate>('transcript_update', (event) => {
        setTranscript(event.payload);
      });
    }

    setupListeners();

    return () => {
      unlistenStatus?.();
      unlistenTranscript?.();
    };
  }, []);

  const handleStart = useCallback(async () => {
    try {
      await invoke('start_session', { deviceId: selectedDevice });
    } catch (e) {
      console.error('Failed to start session:', e);
    }
  }, [selectedDevice]);

  const handleStop = useCallback(async () => {
    try {
      await invoke('stop_session');
    } catch (e) {
      console.error('Failed to stop session:', e);
    }
  }, []);

  const handleCopy = useCallback(async () => {
    try {
      await writeText(transcript.finalized_text);
      setCopySuccess(true);
      setTimeout(() => setCopySuccess(false), 2000);
    } catch (e) {
      console.error('Failed to copy:', e);
    }
  }, [transcript.finalized_text]);

  const handleReset = useCallback(async () => {
    try {
      await invoke('reset_session');
      setTranscript({
        finalized_text: '',
        draft_text: null,
        segment_count: 0,
      });
    } catch (e) {
      console.error('Failed to reset:', e);
    }
  }, []);

  const isRecording = status.state === 'recording';
  const isStopping = status.state === 'stopping';
  const isCompleted = status.state === 'completed';
  const isPreparing = status.state === 'preparing';
  const isIdle = status.state === 'idle';
  const hasError = status.state === 'error';

  const canStart = isIdle && modelStatus?.available;
  const canStop = isRecording;
  const canCopy = (isCompleted || isRecording) && transcript.finalized_text.length > 0;

  return (
    <div className="app">
      {/* Header */}
      <header className="header">
        <div className="header-left">
          <span className="provider-badge">
            {status.provider === 'whisper' ? 'Whisper' : 'Ready'}
          </span>
          {isRecording && (
            <span className={`status-indicator ${status.is_processing_behind ? 'behind' : ''}`}>
              <span className="dot" />
              Recording
            </span>
          )}
          {isStopping && (
            <span className="status-indicator stopping">
              <span className="dot" />
              Finishing...
            </span>
          )}
          {isPreparing && (
            <span className="status-indicator preparing">
              <span className="dot" />
              Preparing...
            </span>
          )}
        </div>
        <div className="header-right">
          {(isRecording || isStopping) && (
            <span className="elapsed-time">{formatTime(status.elapsed_ms)}</span>
          )}
          {status.is_processing_behind && (
            <span className="processing-badge">Processing...</span>
          )}
        </div>
      </header>

      {/* Model warning */}
      {!modelStatus?.available && (
        <div className="warning-banner">
          <strong>Model not found:</strong> {modelStatus?.error || 'Please download a Whisper model'}
          <br />
          <small>Expected at: {modelStatus?.path}</small>
        </div>
      )}

      {/* Error message */}
      {hasError && status.error_message && (
        <div className="error-banner">
          <strong>Error:</strong> {status.error_message}
        </div>
      )}

      {/* Transcript area */}
      <main className="transcript-area">
        {transcript.finalized_text ? (
          <div className="transcript-text">
            {transcript.finalized_text.split('\n\n').map((paragraph, i) => (
              <p key={i}>{paragraph}</p>
            ))}
            {transcript.draft_text && (
              <p className="draft-text">{transcript.draft_text}</p>
            )}
          </div>
        ) : (
          <div className="transcript-placeholder">
            {isIdle && 'Click Start to begin recording'}
            {isPreparing && 'Initializing...'}
            {isRecording && 'Listening...'}
            {isStopping && 'Processing final audio...'}
          </div>
        )}
      </main>

      {/* Controls */}
      <footer className="controls">
        <div className="controls-left">
          {isIdle && (
            <select
              value={selectedDevice}
              onChange={(e) => setSelectedDevice(e.target.value)}
              className="device-select"
              aria-label="Select audio input device"
            >
              <option value="default">Default Device</option>
              {devices.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name} {d.is_default ? '(default)' : ''}
                </option>
              ))}
            </select>
          )}
        </div>

        <div className="controls-center">
          {canStart && (
            <button className="btn btn-primary" onClick={handleStart}>
              Start
            </button>
          )}
          {canStop && (
            <button className="btn btn-stop" onClick={handleStop}>
              Stop
            </button>
          )}
          {(isPreparing || isStopping) && (
            <button className="btn btn-disabled" disabled>
              {isPreparing ? 'Preparing...' : 'Stopping...'}
            </button>
          )}
          {isCompleted && (
            <button className="btn btn-secondary" onClick={handleReset}>
              New Recording
            </button>
          )}
        </div>

        <div className="controls-right">
          {canCopy && (
            <button
              className={`btn btn-copy ${copySuccess ? 'success' : ''}`}
              onClick={handleCopy}
            >
              {copySuccess ? 'Copied!' : 'Copy'}
            </button>
          )}
        </div>
      </footer>
    </div>
  );
}

export default App;
