import { useState, useEffect, useCallback, useRef } from 'react';
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

interface Settings {
  whisper_model: string;
  language: string;
  input_device_id: string | null;
  output_format: string;
  vad_threshold: number;
  silence_to_flush_ms: number;
  max_utterance_ms: number;
  diarization_enabled: boolean;
  max_speakers: number;
}

// Checklist types
type CheckCategory = 'audio' | 'model' | 'permission' | 'configuration' | 'network';
type CheckStatus = 'pass' | 'fail' | 'warning' | 'pending' | 'skipped';

interface CheckAction {
  download_model?: { model_name: string };
  open_settings?: { settings_type: string };
  retry?: boolean;
  none?: boolean;
}

interface CheckResult {
  id: string;
  name: string;
  category: CheckCategory;
  status: CheckStatus;
  message: string | null;
  action: CheckAction | null;
}

interface ChecklistResult {
  checks: CheckResult[];
  all_passed: boolean;
  can_start: boolean;
  summary: string;
}

const WHISPER_MODELS = [
  { value: 'tiny', label: 'Tiny (fastest)' },
  { value: 'base', label: 'Base' },
  { value: 'small', label: 'Small (recommended)' },
  { value: 'medium', label: 'Medium' },
  { value: 'large', label: 'Large (best)' },
];

const LANGUAGES = [
  { value: 'en', label: 'English' },
  { value: 'fa', label: 'Persian' },
  { value: 'ar', label: 'Arabic' },
  { value: 'es', label: 'Spanish' },
  { value: 'fr', label: 'French' },
  { value: 'de', label: 'German' },
  { value: 'zh', label: 'Chinese' },
  { value: 'ja', label: 'Japanese' },
  { value: 'ko', label: 'Korean' },
  { value: 'ru', label: 'Russian' },
  { value: 'pt', label: 'Portuguese' },
  { value: 'it', label: 'Italian' },
  { value: 'auto', label: 'Auto-detect' },
];

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
  const [showSettings, setShowSettings] = useState(false);
  const [transcriptExpanded, setTranscriptExpanded] = useState(true);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [pendingSettings, setPendingSettings] = useState<{
    model: string;
    language: string;
    device: string;
    diarization_enabled: boolean;
    max_speakers: number;
  } | null>(null);

  // Checklist state
  const [checklistResult, setChecklistResult] = useState<ChecklistResult | null>(null);
  const [checklistRunning, setChecklistRunning] = useState(true);
  const [checklistDismissed, setChecklistDismissed] = useState(false);
  const [downloadingModel, setDownloadingModel] = useState<string | null>(null);

  const transcriptRef = useRef<HTMLDivElement>(null);
  const [localElapsedMs, setLocalElapsedMs] = useState(0);
  const recordingStartRef = useRef<number | null>(null);

  // Local timer that runs during recording/preparing
  useEffect(() => {
    if (status.state === 'preparing' || status.state === 'recording') {
      // Start local timer when recording begins
      if (recordingStartRef.current === null) {
        recordingStartRef.current = Date.now();
      }
      
      const interval = setInterval(() => {
        if (recordingStartRef.current) {
          setLocalElapsedMs(Date.now() - recordingStartRef.current);
        }
      }, 100);
      
      return () => clearInterval(interval);
    } else {
      // Reset when not recording
      recordingStartRef.current = null;
      if (status.state === 'idle') {
        setLocalElapsedMs(0);
      }
    }
  }, [status.state]);

  // Auto-scroll transcript during recording
  useEffect(() => {
    if (status.state === 'recording' && transcriptRef.current) {
      transcriptRef.current.scrollTop = transcriptRef.current.scrollHeight;
    }
  }, [transcript.finalized_text, status.state]);

  // Run checklist function
  const runChecklist = useCallback(async () => {
    setChecklistRunning(true);
    setChecklistDismissed(false); // Reset dismissed when re-running
    try {
      const result = await invoke<ChecklistResult>('run_checklist');
      setChecklistResult(result);
    } catch (e) {
      console.error('Failed to run checklist:', e);
      setChecklistResult({
        checks: [],
        all_passed: false,
        can_start: false,
        summary: 'Failed to run checklist',
      });
    } finally {
      setChecklistRunning(false);
    }
  }, []);

  // Handle model download
  const handleDownloadModel = useCallback(async (modelName: string) => {
    setDownloadingModel(modelName);
    try {
      let command = '';
      if (modelName === 'speaker_embedding') {
        command = 'download_speaker_model';
      } else if (modelName === 'gtcrn_simple') {
        command = 'download_enhancement_model';
      } else if (modelName === 'wav2small') {
        command = 'download_emotion_model';
      } else {
        // Whisper model
        command = 'download_whisper_model';
      }
      await invoke(command);
      // Re-run checklist after download
      await runChecklist();
      // Also refresh model status
      const modelResult = await invoke<ModelStatus>('check_model_status');
      setModelStatus(modelResult);
    } catch (e) {
      console.error('Failed to download model:', e);
    } finally {
      setDownloadingModel(null);
    }
  }, [runChecklist]);

  // Load devices, model status, settings, and run checklist on mount
  useEffect(() => {
    async function init() {
      try {
        // Run checklist first
        const checklistResultData = await invoke<ChecklistResult>('run_checklist');
        setChecklistResult(checklistResultData);
        setChecklistRunning(false);

        const deviceList = await invoke<Device[]>('list_input_devices');
        setDevices(deviceList);

        const modelResult = await invoke<ModelStatus>('check_model_status');
        setModelStatus(modelResult);

        const settingsResult = await invoke<Settings>('get_settings');
        setSettings(settingsResult);
        setPendingSettings({
          model: settingsResult.whisper_model,
          language: settingsResult.language,
          device: settingsResult.input_device_id || 'default',
          diarization_enabled: settingsResult.diarization_enabled,
          max_speakers: settingsResult.max_speakers,
        });
        if (settingsResult.input_device_id) {
          setSelectedDevice(settingsResult.input_device_id);
        }
      } catch (e) {
        console.error('Failed to initialize:', e);
        setChecklistRunning(false);
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
        console.log('Session status update:', event.payload);
        setStatus(event.payload);
      });

      unlistenTranscript = await listen<TranscriptUpdate>('transcript_update', (event) => {
        console.log('Transcript update received:', event.payload);
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

  const handleSaveSettings = useCallback(async () => {
    if (!settings || !pendingSettings) return;

    try {
      const updatedSettings: Settings = {
        ...settings,
        whisper_model: pendingSettings.model,
        language: pendingSettings.language,
        input_device_id: pendingSettings.device === 'default' ? null : pendingSettings.device,
        diarization_enabled: pendingSettings.diarization_enabled,
        max_speakers: pendingSettings.max_speakers,
      };
      const result = await invoke<Settings>('set_settings', { settings: updatedSettings });
      setSettings(result);
      setSelectedDevice(pendingSettings.device);

      // Refresh model status in case model changed
      const modelResult = await invoke<ModelStatus>('check_model_status');
      setModelStatus(modelResult);

      setShowSettings(false);
    } catch (e) {
      console.error('Failed to save settings:', e);
    }
  }, [settings, pendingSettings]);

  const isRecording = status.state === 'recording';
  const isStopping = status.state === 'stopping';
  const isCompleted = status.state === 'completed';
  const isPreparing = status.state === 'preparing';
  const isIdle = status.state === 'idle';
  const hasError = status.state === 'error';

  // Show checklist if:
  // - Still running checks
  // - Has failures (can_start=false) - must be shown until fixed
  // - Has results but not yet dismissed by user (for pass/warning states)
  const showChecklist = checklistRunning ||
    (checklistResult && !checklistResult.can_start) ||
    (checklistResult && !checklistDismissed);

  const canStart = isIdle && modelStatus?.available && checklistResult?.can_start;
  const canCopy = (isCompleted || isRecording) && transcript.finalized_text.length > 0;

  // Get status icon for checklist items
  const getCheckIcon = (checkStatus: CheckStatus) => {
    switch (checkStatus) {
      case 'pass': return '✓';
      case 'fail': return '✗';
      case 'warning': return '⚠';
      case 'pending': return '○';
      case 'skipped': return '−';
    }
  };

  const getCheckClass = (checkStatus: CheckStatus) => {
    switch (checkStatus) {
      case 'pass': return 'check-pass';
      case 'fail': return 'check-fail';
      case 'warning': return 'check-warning';
      case 'pending': return 'check-pending';
      case 'skipped': return 'check-skipped';
    }
  };

  // Determine button state
  const getButtonState = () => {
    if (isRecording) return 'recording';
    if (isStopping) return 'stopping';
    if (isPreparing) return 'preparing';
    return 'idle';
  };

  const getStatusDotClass = () => {
    if (isRecording) return 'recording';
    if (isStopping) return 'stopping';
    if (isPreparing) return 'preparing';
    if (isIdle) return 'idle';
    return '';
  };

  const handleRecordClick = () => {
    if (isRecording) {
      handleStop();
    } else if (canStart) {
      handleStart();
    }
  };

  return (
    <div className="sidebar">
      {/* Header */}
      <header className="header">
        <div className="header-left">
          <span className={`status-dot ${getStatusDotClass()}`} />
          <span className="app-title">Scribe</span>
        </div>
        <button
          className={`settings-btn ${showSettings ? 'active' : ''}`}
          onClick={() => setShowSettings(!showSettings)}
          aria-label="Settings"
          disabled={isRecording || isStopping}
          title={isRecording || isStopping ? 'Settings disabled during recording' : 'Settings'}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="3" />
            <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
          </svg>
        </button>
      </header>

      {/* Pre-launch Checklist */}
      {showChecklist && (
        <div className="checklist-overlay">
          <div className="checklist-container">
            <h2 className="checklist-title">Pre-Launch Checklist</h2>
            {checklistRunning ? (
              <div className="checklist-loading">
                <div className="spinner" />
                <span>Running checks...</span>
              </div>
            ) : (
              <>
                <div className="checklist-items">
                  {checklistResult?.checks.map((check) => (
                    <div key={check.id} className={`checklist-item ${getCheckClass(check.status)}`}>
                      <span className="check-icon">{getCheckIcon(check.status)}</span>
                      <div className="check-content">
                        <span className="check-name">{check.name}</span>
                        {check.message && (
                          <span className="check-message">{check.message}</span>
                        )}
                      </div>
                      {check.status === 'fail' && check.action?.download_model && (
                        <button
                          className="download-btn"
                          onClick={() => handleDownloadModel(check.action!.download_model!.model_name)}
                          disabled={downloadingModel !== null}
                        >
                          {downloadingModel === check.action.download_model.model_name ? 'Downloading...' : 'Download'}
                        </button>
                      )}
                      {check.status === 'warning' && check.action?.download_model && (
                        <button
                          className="download-btn secondary"
                          onClick={() => handleDownloadModel(check.action!.download_model!.model_name)}
                          disabled={downloadingModel !== null}
                        >
                          {downloadingModel === check.action.download_model.model_name ? 'Downloading...' : 'Download'}
                        </button>
                      )}
                    </div>
                  ))}
                </div>
                <div className="checklist-summary">
                  <span className={checklistResult?.can_start ? 'summary-pass' : 'summary-fail'}>
                    {checklistResult?.summary}
                  </span>
                </div>
                {checklistResult?.can_start && (
                  <button
                    className="btn btn-primary checklist-continue"
                    onClick={() => setChecklistDismissed(true)}
                  >
                    Continue
                  </button>
                )}
                <button
                  className="btn-retry"
                  onClick={runChecklist}
                  disabled={checklistRunning || downloadingModel !== null}
                >
                  Re-run Checks
                </button>
              </>
            )}
          </div>
        </div>
      )}

      {/* Model warning */}
      {!showChecklist && !modelStatus?.available && (
        <div className="warning-banner">
          Model not found. Check settings.
        </div>
      )}

      {/* Error message */}
      {!showChecklist && hasError && status.error_message && (
        <div className="error-banner">
          {status.error_message}
        </div>
      )}

      {/* Record Section */}
      {!showChecklist && (
        <section className="record-section">
          <button
            className={`record-button ${getButtonState()}`}
            onClick={handleRecordClick}
            disabled={isPreparing || isStopping || !modelStatus?.available}
          >
            <span className="icon" />
            <span className="label">
              {isRecording ? 'Stop' : isPreparing ? '...' : isStopping ? '...' : 'Start'}
            </span>
          </button>
          <div className={`timer ${isRecording || isStopping || isPreparing ? 'active' : ''}`}>
            {formatTime(localElapsedMs)}
          </div>
        </section>
      )}

      {/* Transcript Section */}
      {!showChecklist && (
        <section className="transcript-section">
          <div
            className="transcript-header"
            onClick={() => setTranscriptExpanded(!transcriptExpanded)}
          >
            <div className="transcript-header-left">
              <span className={`chevron ${transcriptExpanded ? '' : 'collapsed'}`}>
                &#9660;
              </span>
              <span className="transcript-title">Transcript</span>
            </div>
            <button
              className={`copy-btn ${copySuccess ? 'success' : ''}`}
              onClick={(e) => {
                e.stopPropagation();
                handleCopy();
              }}
              disabled={!canCopy}
            >
              {copySuccess ? 'Copied!' : 'Copy'}
            </button>
          </div>
          <div
            ref={transcriptRef}
            className={`transcript-content ${transcriptExpanded ? '' : 'collapsed'}`}
          >
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
                {isIdle && 'Tap Start to begin'}
                {isPreparing && 'Initializing...'}
                {isRecording && 'Listening...'}
                {isStopping && 'Processing...'}
                {isCompleted && 'No transcript'}
              </div>
            )}
          </div>
        </section>
      )}

      {/* Action Bar - only show when completed */}
      {!showChecklist && isCompleted && (
        <div className="action-bar">
          <button className="btn btn-primary" onClick={handleReset}>
            New Session
          </button>
        </div>
      )}

      {/* Settings Drawer */}
      {showSettings && (
        <>
          <div className="settings-overlay" onClick={() => setShowSettings(false)} />
          <div className="settings-drawer">
            <div className="settings-drawer-header">
              <span className="settings-drawer-title">Settings</span>
              <button className="close-btn" onClick={() => setShowSettings(false)}>
                &times;
              </button>
            </div>
            <div className="settings-drawer-content">
              {pendingSettings && (
                <>
                  <div className="settings-group">
                    <label className="settings-label" htmlFor="model-select">Model</label>
                    <select
                      id="model-select"
                      className="settings-select"
                      value={pendingSettings.model}
                      onChange={(e) => setPendingSettings({ ...pendingSettings, model: e.target.value })}
                    >
                      {WHISPER_MODELS.map((m) => (
                        <option key={m.value} value={m.value}>
                          {m.label}
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="settings-group">
                    <label className="settings-label" htmlFor="language-select">Language</label>
                    <select
                      id="language-select"
                      className="settings-select"
                      value={pendingSettings.language}
                      onChange={(e) => setPendingSettings({ ...pendingSettings, language: e.target.value })}
                    >
                      {LANGUAGES.map((l) => (
                        <option key={l.value} value={l.value}>
                          {l.label}
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="settings-group">
                    <label className="settings-label" htmlFor="microphone-select">Microphone</label>
                    <select
                      id="microphone-select"
                      className="settings-select"
                      value={pendingSettings.device}
                      onChange={(e) => setPendingSettings({ ...pendingSettings, device: e.target.value })}
                    >
                      <option value="default">Default</option>
                      {devices.map((d) => (
                        <option key={d.id} value={d.id}>
                          {d.name}
                        </option>
                      ))}
                    </select>
                  </div>

                  <div className="settings-group">
                    <div className="settings-toggle">
                      <span className="settings-label" style={{ marginBottom: 0 }}>Speaker Detection</span>
                      <label className="toggle-switch">
                        <input
                          type="checkbox"
                          checked={pendingSettings.diarization_enabled}
                          onChange={(e) =>
                            setPendingSettings({ ...pendingSettings, diarization_enabled: e.target.checked })
                          }
                          aria-label="Enable speaker detection"
                        />
                        <span className="toggle-slider"></span>
                      </label>
                    </div>
                  </div>

                  {pendingSettings.diarization_enabled && (
                    <div className="settings-group">
                      <label className="settings-label" htmlFor="max-speakers-slider">Max Speakers</label>
                      <div className="settings-slider">
                        <input
                          id="max-speakers-slider"
                          type="range"
                          min="2"
                          max="10"
                          value={pendingSettings.max_speakers}
                          onChange={(e) =>
                            setPendingSettings({ ...pendingSettings, max_speakers: parseInt(e.target.value) })
                          }
                        />
                        <span className="slider-value">{pendingSettings.max_speakers}</span>
                      </div>
                    </div>
                  )}
                </>
              )}
            </div>
            <div className="settings-drawer-footer">
              <p className="settings-note">Changes apply on next recording</p>
              <button className="btn-save" onClick={handleSaveSettings}>
                Save Settings
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

export default App;
