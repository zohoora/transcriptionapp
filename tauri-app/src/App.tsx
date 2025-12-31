import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useAuth } from './components/AuthProvider';
import type {
  SessionStatus,
  TranscriptUpdate,
  Device,
  ModelStatus,
  Settings,
  OllamaStatus,
  SoapNote,
  CheckStatus,
  ChecklistResult,
  BiomarkerUpdate,
  AudioQualitySnapshot,
  Patient,
  SyncResult,
} from './types';

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

// Biomarker interpretation helpers
// Vitality: F0 std dev in Hz. Normal speech: 30-80 Hz, low vitality: <20 Hz
function getVitalityPercent(value: number | null): number {
  if (value === null) return 0;
  return Math.min(100, (value / 60) * 100); // 60 Hz = 100%
}

function getVitalityClass(value: number | null): string {
  if (value === null) return '';
  if (value >= 30) return 'metric-good';
  if (value >= 15) return 'metric-warning';
  return 'metric-low';
}

// Stability: CPP in dB. Normal: 8-15 dB, concerning: <6 dB
function getStabilityPercent(value: number | null): number {
  if (value === null) return 0;
  return Math.min(100, (value / 12) * 100); // 12 dB = 100%
}

function getStabilityClass(value: number | null): string {
  if (value === null) return '';
  if (value >= 8) return 'metric-good';
  if (value >= 5) return 'metric-warning';
  return 'metric-low';
}

// Audio quality interpretation helpers
// Level: -40 to -6 dBFS is good range
function getLevelPercent(value: number): number {
  // Map -60 to 0 dBFS to 0-100%
  return Math.min(100, Math.max(0, ((value + 60) / 60) * 100));
}

function getLevelClass(value: number): string {
  if (value < -40) return 'metric-low';      // Too quiet
  if (value > -6) return 'metric-warning';   // Too hot
  return 'metric-good';
}

// SNR: >10 dB is good
function getSnrPercent(value: number): number {
  // Map 0-30 dB to 0-100%
  return Math.min(100, Math.max(0, (value / 30) * 100));
}

function getSnrClass(value: number): string {
  if (value >= 15) return 'metric-good';
  if (value >= 10) return 'metric-warning';
  return 'metric-low';
}

function getQualityStatus(quality: AudioQualitySnapshot): { label: string; class: string } {
  const levelOk = quality.rms_db >= -40 && quality.rms_db <= -6;
  const snrOk = quality.snr_db >= 10;
  const clippingOk = quality.clipped_ratio < 0.001;
  const dropoutOk = quality.dropout_count === 0;

  if (levelOk && snrOk && clippingOk && dropoutOk) {
    return { label: 'Good', class: 'quality-good' };
  }
  if (!clippingOk || !dropoutOk) {
    return { label: 'Poor', class: 'quality-poor' };
  }
  return { label: 'Fair', class: 'quality-fair' };
}

function getQualitySuggestion(quality: AudioQualitySnapshot): string | null {
  // Priority order: most actionable issues first
  if (quality.clipped_ratio >= 0.001) {
    return 'Speak softer or move mic further away';
  }
  if (quality.dropout_count > 0) {
    return 'Audio gaps detected - check connection';
  }
  if (quality.rms_db < -40) {
    return 'Move microphone closer';
  }
  if (quality.rms_db > -6) {
    return 'Move microphone further away';
  }
  if (quality.snr_db < 10) {
    return 'Reduce background noise';
  }
  return null; // All good, no suggestion needed
}

// Conversation dynamics interpretation helpers
// Response latency: <500ms = great, 500-1500ms = ok, >1500ms = slow
function getResponseLatencyClass(value: number): string {
  if (value < 500) return 'metric-good';
  if (value < 1500) return 'metric-warning';
  return 'metric-low';
}

// Engagement score: 0-100, higher is better
function getEngagementPercent(value: number | null): number {
  if (value === null) return 0;
  return Math.min(100, Math.max(0, value));
}

function getEngagementClass(value: number | null): string {
  if (value === null) return '';
  if (value >= 70) return 'metric-good';
  if (value >= 40) return 'metric-warning';
  return 'metric-low';
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

function App() {
  // Medplum auth from context
  const { authState, login: medplumLogin, logout: medplumLogout, isLoading: authLoading } = useAuth();

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
    ollama_server_url: string;
    ollama_model: string;
    medplum_server_url: string;
    medplum_client_id: string;
    medplum_auto_sync: boolean;
  } | null>(null);

  // Medplum state
  const [medplumConnected, setMedplumConnected] = useState(false);
  const [medplumError, setMedplumError] = useState<string | null>(null);

  // Checklist state
  const [checklistResult, setChecklistResult] = useState<ChecklistResult | null>(null);
  const [checklistRunning, setChecklistRunning] = useState(true);
  const [checklistDismissed, setChecklistDismissed] = useState(false);
  const [downloadingModel, setDownloadingModel] = useState<string | null>(null);

  // Biomarker state
  const [biomarkers, setBiomarkers] = useState<BiomarkerUpdate | null>(null);
  const [biomarkersExpanded, setBiomarkersExpanded] = useState(true);
  const [showBiomarkers, setShowBiomarkers] = useState(true);

  // Conversation dynamics state
  const [dynamicsExpanded, setDynamicsExpanded] = useState(true);

  // Audio quality state
  const [audioQuality, setAudioQuality] = useState<AudioQualitySnapshot | null>(null);
  const [audioQualityExpanded, setAudioQualityExpanded] = useState(true);

  // SOAP note state
  const [soapNote, setSoapNote] = useState<SoapNote | null>(null);
  const [isGeneratingSoap, setIsGeneratingSoap] = useState(false);
  const [soapError, setSoapError] = useState<string | null>(null);
  const [soapExpanded, setSoapExpanded] = useState(true);
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus | null>(null);
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);

  // Medplum sync state
  const [showSyncModal, setShowSyncModal] = useState(false);
  const [patientSearchQuery, setPatientSearchQuery] = useState('');
  const [patientSearchResults, setPatientSearchResults] = useState<Patient[]>([]);
  const [isSearchingPatients, setIsSearchingPatients] = useState(false);
  const [selectedPatient, setSelectedPatient] = useState<Patient | null>(null);
  const [isSyncing, setIsSyncing] = useState(false);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [syncSuccess, setSyncSuccess] = useState(false);

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
      } else if (modelName === 'yamnet') {
        command = 'download_yamnet_model';
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
          ollama_server_url: settingsResult.ollama_server_url,
          ollama_model: settingsResult.ollama_model,
          medplum_server_url: settingsResult.medplum_server_url,
          medplum_client_id: settingsResult.medplum_client_id,
          medplum_auto_sync: settingsResult.medplum_auto_sync,
        });

        // Check Medplum connection
        try {
          const medplumResult = await invoke<boolean>('medplum_check_connection');
          setMedplumConnected(medplumResult);
        } catch {
          setMedplumConnected(false);
        }
        if (settingsResult.input_device_id) {
          setSelectedDevice(settingsResult.input_device_id);
        }

        // Check Ollama status
        try {
          const ollamaStatusResult = await invoke<OllamaStatus>('check_ollama_status');
          setOllamaStatus(ollamaStatusResult);
          if (ollamaStatusResult.connected) {
            setOllamaModels(ollamaStatusResult.available_models);
          }
        } catch {
          // Ollama not available - that's okay
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
    let unlistenBiomarkers: UnlistenFn | undefined;
    let unlistenAudioQuality: UnlistenFn | undefined;

    async function setupListeners() {
      unlistenStatus = await listen<SessionStatus>('session_status', (event) => {
        setStatus(event.payload);
      });

      unlistenTranscript = await listen<TranscriptUpdate>('transcript_update', (event) => {
        setTranscript(event.payload);
      });

      unlistenBiomarkers = await listen<BiomarkerUpdate>('biomarker_update', (event) => {
        setBiomarkers(event.payload);
      });

      unlistenAudioQuality = await listen<AudioQualitySnapshot>('audio_quality', (event) => {
        setAudioQuality(event.payload);
      });
    }

    setupListeners();

    return () => {
      unlistenStatus?.();
      unlistenTranscript?.();
      unlistenBiomarkers?.();
      unlistenAudioQuality?.();
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
      setBiomarkers(null);
      setAudioQuality(null);
      setSoapNote(null);
      setSoapError(null);
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
        ollama_server_url: pendingSettings.ollama_server_url,
        ollama_model: pendingSettings.ollama_model,
        medplum_server_url: pendingSettings.medplum_server_url,
        medplum_client_id: pendingSettings.medplum_client_id,
        medplum_auto_sync: pendingSettings.medplum_auto_sync,
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

  // Test Ollama connection
  const handleTestOllama = useCallback(async () => {
    if (!pendingSettings) return;
    try {
      // Temporarily save settings to test with new URL
      const testSettings: Settings = {
        ...settings!,
        ollama_server_url: pendingSettings.ollama_server_url,
        ollama_model: pendingSettings.ollama_model,
      };
      await invoke('set_settings', { settings: testSettings });

      const statusResult = await invoke<OllamaStatus>('check_ollama_status');
      setOllamaStatus(statusResult);
      if (statusResult.connected) {
        setOllamaModels(statusResult.available_models);
      }
    } catch (e) {
      console.error('Failed to test Ollama:', e);
      setOllamaStatus({ connected: false, available_models: [], error: String(e) });
    }
  }, [settings, pendingSettings]);

  // Test Medplum connection
  const handleTestMedplum = useCallback(async () => {
    if (!pendingSettings) return;
    setMedplumError(null);
    try {
      // Temporarily save settings to test with new URL
      const testSettings: Settings = {
        ...settings!,
        medplum_server_url: pendingSettings.medplum_server_url,
        medplum_client_id: pendingSettings.medplum_client_id,
        medplum_auto_sync: pendingSettings.medplum_auto_sync,
      };
      await invoke('set_settings', { settings: testSettings });

      const result = await invoke<boolean>('medplum_check_connection');
      setMedplumConnected(result);
      if (!result) {
        setMedplumError('Could not connect to server');
      }
    } catch (e) {
      console.error('Failed to test Medplum:', e);
      setMedplumConnected(false);
      setMedplumError(String(e));
    }
  }, [settings, pendingSettings]);

  // Generate SOAP note
  const handleGenerateSoap = useCallback(async () => {
    if (!transcript.finalized_text.trim()) return;

    setIsGeneratingSoap(true);
    setSoapError(null);

    try {
      const result = await invoke<SoapNote>('generate_soap_note', {
        transcript: transcript.finalized_text,
      });
      setSoapNote(result);
    } catch (e) {
      console.error('Failed to generate SOAP note:', e);
      setSoapError(String(e));
    } finally {
      setIsGeneratingSoap(false);
    }
  }, [transcript.finalized_text]);

  // Search patients in Medplum
  const searchPatients = useCallback(async (query: string) => {
    if (!query.trim() || !authState.is_authenticated) return;

    setIsSearchingPatients(true);
    try {
      const results = await invoke<Patient[]>('medplum_search_patients', { query });
      setPatientSearchResults(results);
    } catch (e) {
      console.error('Failed to search patients:', e);
      setPatientSearchResults([]);
    } finally {
      setIsSearchingPatients(false);
    }
  }, [authState.is_authenticated]);

  // Quick sync session to Medplum (auto-creates placeholder patient)
  const syncToMedplum = useCallback(async () => {
    if (!authState.is_authenticated) return;

    setIsSyncing(true);
    setSyncError(null);
    setSyncSuccess(false);

    try {
      // Get audio file path if available
      const audioFilePath = await invoke<string | null>('get_audio_file_path');

      // Format SOAP note if available
      const soapText = soapNote
        ? `SUBJECTIVE:\n${soapNote.subjective}\n\nOBJECTIVE:\n${soapNote.objective}\n\nASSESSMENT:\n${soapNote.assessment}\n\nPLAN:\n${soapNote.plan}`
        : null;

      // Quick sync - creates placeholder patient and encounter automatically
      const result = await invoke<SyncResult>('medplum_quick_sync', {
        transcript: transcript.finalized_text,
        soapNote: soapText,
        audioFilePath: audioFilePath,
        sessionDurationMs: status.elapsed_ms,
      });

      if (result.success) {
        setSyncSuccess(true);
        console.log('Sync successful:', result.status);
      } else {
        setSyncError(result.error || 'Sync failed');
      }
    } catch (e) {
      console.error('Failed to sync to Medplum:', e);
      setSyncError(String(e));
    } finally {
      setIsSyncing(false);
    }
  }, [authState.is_authenticated, transcript.finalized_text, soapNote, status.elapsed_ms]);

  // Open the session history window
  const openHistoryWindow = useCallback(async () => {
    try {
      // Check if window already exists
      const existing = await WebviewWindow.getByLabel('history');
      if (existing) {
        await existing.setFocus();
        return;
      }

      // Create new history window
      const historyWindow = new WebviewWindow('history', {
        url: 'history.html',
        title: 'Session History',
        width: 500,
        height: 700,
        minWidth: 400,
        minHeight: 500,
        resizable: true,
      });

      historyWindow.once('tauri://error', (e) => {
        console.error('Failed to open history window:', e);
      });
    } catch (e) {
      console.error('Error opening history window:', e);
    }
  }, []);

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
        <div className="header-buttons">
          <button
            className="history-btn"
            onClick={openHistoryWindow}
            aria-label="Session History"
            disabled={isRecording || isStopping}
            title={isRecording || isStopping ? 'History disabled during recording' : 'Session History'}
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <rect x="3" y="4" width="18" height="18" rx="2" ry="2" />
              <line x1="16" y1="2" x2="16" y2="6" />
              <line x1="8" y1="2" x2="8" y2="6" />
              <line x1="3" y1="10" x2="21" y2="10" />
            </svg>
          </button>
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
        </div>
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

      {/* Audio Quality Section */}
      {!showChecklist && (isRecording || isCompleted) && audioQuality && (
        <section className="audio-quality-section">
          <div
            className="audio-quality-header"
            onClick={() => setAudioQualityExpanded(!audioQualityExpanded)}
            onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setAudioQualityExpanded(!audioQualityExpanded); }}}
            role="button"
            tabIndex={0}
            aria-expanded={audioQualityExpanded}
          >
            <div className="audio-quality-header-left">
              <span className={`chevron ${audioQualityExpanded ? '' : 'collapsed'}`}>
                &#9660;
              </span>
              <span className="audio-quality-title">Audio Quality</span>
            </div>
            <span className={`quality-badge ${getQualityStatus(audioQuality).class}`}>
              {getQualityStatus(audioQuality).label}
            </span>
          </div>

          {audioQualityExpanded && (
            <div className="audio-quality-content">
              {getQualitySuggestion(audioQuality) && (
                <div className="quality-suggestion">
                  {getQualitySuggestion(audioQuality)}
                </div>
              )}
              <div className="metric-row">
                <span className="metric-label">Level</span>
                <div className="metric-bar-container">
                  <div
                    className={`metric-bar ${getLevelClass(audioQuality.rms_db)}`}
                    style={{ width: `${getLevelPercent(audioQuality.rms_db)}%` }}
                  />
                </div>
                <span className="metric-value">
                  {audioQuality.rms_db.toFixed(0)} dB
                </span>
              </div>
              <div className="metric-row">
                <span className="metric-label">SNR</span>
                <div className="metric-bar-container">
                  <div
                    className={`metric-bar ${getSnrClass(audioQuality.snr_db)}`}
                    style={{ width: `${getSnrPercent(audioQuality.snr_db)}%` }}
                  />
                </div>
                <span className="metric-value">
                  {audioQuality.snr_db.toFixed(0)} dB
                </span>
              </div>
              {audioQuality.total_clipped > 0 && (
                <div className="metric-row quality-warning">
                  <span className="metric-label">Clips</span>
                  <span className="metric-value-wide">{audioQuality.total_clipped}</span>
                </div>
              )}
              {audioQuality.dropout_count > 0 && (
                <div className="metric-row quality-warning">
                  <span className="metric-label">Drops</span>
                  <span className="metric-value-wide">{audioQuality.dropout_count}</span>
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {/* Biomarkers Section */}
      {!showChecklist && showBiomarkers && (isRecording || isCompleted) && (
        <section className="biomarkers-section">
          <div
            className="biomarkers-header"
            onClick={() => setBiomarkersExpanded(!biomarkersExpanded)}
            onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setBiomarkersExpanded(!biomarkersExpanded); }}}
            role="button"
            tabIndex={0}
            aria-expanded={biomarkersExpanded}
          >
            <div className="biomarkers-header-left">
              <span className={`chevron ${biomarkersExpanded ? '' : 'collapsed'}`}>
                &#9660;
              </span>
              <span className="biomarkers-title">Biomarkers</span>
            </div>
            {biomarkers && biomarkers.cough_count > 0 && (
              <span className="cough-badge">{biomarkers.cough_count}</span>
            )}
          </div>

          {biomarkersExpanded && (
            <div className="biomarkers-content">
              {biomarkers ? (
                <>
                  {/* Per-Speaker Biomarkers (when diarization enabled) */}
                  {biomarkers.speaker_metrics.length > 0 ? (
                    <div className="speaker-biomarkers">
                      {biomarkers.speaker_metrics.map((speaker) => (
                        <div key={speaker.speaker_id} className="speaker-metrics-group">
                          <div className="speaker-label">{speaker.speaker_id}</div>
                          <div className="metric-row">
                            <span className="metric-label">Vitality</span>
                            <div className="metric-bar-container">
                              <div
                                className={`metric-bar ${getVitalityClass(speaker.vitality_mean)}`}
                                style={{ width: `${getVitalityPercent(speaker.vitality_mean)}%` }}
                              />
                            </div>
                            <span className="metric-value">
                              {speaker.vitality_mean?.toFixed(1) ?? '--'} Hz
                            </span>
                          </div>
                          <div className="metric-row">
                            <span className="metric-label">Stability</span>
                            <div className="metric-bar-container">
                              <div
                                className={`metric-bar ${getStabilityClass(speaker.stability_mean)}`}
                                style={{ width: `${getStabilityPercent(speaker.stability_mean)}%` }}
                              />
                            </div>
                            <span className="metric-value">
                              {speaker.stability_mean?.toFixed(1) ?? '--'} dB
                            </span>
                          </div>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <>
                      {/* Combined Session Metrics (fallback when no per-speaker data) */}
                      <div className="metric-row">
                        <span className="metric-label">Vitality</span>
                        <div className="metric-bar-container">
                          <div
                            className={`metric-bar ${getVitalityClass(biomarkers.vitality_session_mean)}`}
                            style={{ width: `${getVitalityPercent(biomarkers.vitality_session_mean)}%` }}
                          />
                        </div>
                        <span className="metric-value">
                          {biomarkers.vitality_session_mean?.toFixed(1) ?? '--'} Hz
                        </span>
                      </div>
                      <div className="metric-row">
                        <span className="metric-label">Stability</span>
                        <div className="metric-bar-container">
                          <div
                            className={`metric-bar ${getStabilityClass(biomarkers.stability_session_mean)}`}
                            style={{ width: `${getStabilityPercent(biomarkers.stability_session_mean)}%` }}
                          />
                        </div>
                        <span className="metric-value">
                          {biomarkers.stability_session_mean?.toFixed(1) ?? '--'} dB
                        </span>
                      </div>
                    </>
                  )}

                  {/* Cough Stats */}
                  {biomarkers.cough_count > 0 && (
                    <div className="metric-row cough-stats">
                      <span className="metric-label">Coughs</span>
                      <span className="metric-value-wide">
                        {biomarkers.cough_count} ({biomarkers.cough_rate_per_min.toFixed(1)}/min)
                      </span>
                    </div>
                  )}

                  {/* Session Metrics (if diarization enabled with multiple speakers) */}
                  {biomarkers.turn_count > 1 && (
                    <div className="session-metrics">
                      <div className="metric-row">
                        <span className="metric-label">Turns</span>
                        <span className="metric-value-wide">{biomarkers.turn_count}</span>
                      </div>
                      {biomarkers.talk_time_ratio !== null && (
                        <div className="metric-row">
                          <span className="metric-label">Balance</span>
                          <span className="metric-value-wide">
                            {(biomarkers.talk_time_ratio * 100).toFixed(0)}%
                          </span>
                        </div>
                      )}
                    </div>
                  )}
                </>
              ) : (
                <div className="biomarkers-placeholder">
                  {isPreparing ? 'Initializing...' : 'Listening...'}
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {/* Conversation Dynamics Section */}
      {!showChecklist && showBiomarkers && (isRecording || isCompleted) && biomarkers?.conversation_dynamics && biomarkers.turn_count > 1 && (
        <section className="dynamics-section">
          <div
            className="dynamics-header"
            onClick={() => setDynamicsExpanded(!dynamicsExpanded)}
            onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setDynamicsExpanded(!dynamicsExpanded); }}}
            role="button"
            tabIndex={0}
            aria-expanded={dynamicsExpanded}
          >
            <div className="dynamics-header-left">
              <span className={`chevron ${dynamicsExpanded ? '' : 'collapsed'}`}>
                &#9660;
              </span>
              <span className="dynamics-title">Conversation</span>
            </div>
            {biomarkers.conversation_dynamics.engagement_score !== null && (
              <span className={`engagement-badge ${getEngagementClass(biomarkers.conversation_dynamics.engagement_score)}`}>
                {Math.round(biomarkers.conversation_dynamics.engagement_score)}
              </span>
            )}
          </div>

          {dynamicsExpanded && (
            <div className="dynamics-content">
              {/* Response Latency */}
              <div className="metric-row">
                <span className="metric-label">Response</span>
                <span className={`metric-value-wide ${getResponseLatencyClass(biomarkers.conversation_dynamics.mean_response_latency_ms)}`}>
                  {formatDuration(biomarkers.conversation_dynamics.mean_response_latency_ms)} avg
                </span>
              </div>

              {/* Overlaps & Interruptions */}
              {(biomarkers.conversation_dynamics.total_overlap_count > 0 || biomarkers.conversation_dynamics.total_interruption_count > 0) && (
                <div className="metric-row">
                  <span className="metric-label">Overlaps</span>
                  <span className="metric-value-wide">
                    {biomarkers.conversation_dynamics.total_overlap_count}
                    {biomarkers.conversation_dynamics.total_interruption_count > 0 && (
                      <span className="interruption-count"> ({biomarkers.conversation_dynamics.total_interruption_count} interr.)</span>
                    )}
                  </span>
                </div>
              )}

              {/* Long Pauses */}
              {biomarkers.conversation_dynamics.silence.long_pause_count > 0 && (
                <div className="metric-row">
                  <span className="metric-label">Long Pauses</span>
                  <span className="metric-value-wide">{biomarkers.conversation_dynamics.silence.long_pause_count}</span>
                </div>
              )}

              {/* Engagement Score with bar */}
              {biomarkers.conversation_dynamics.engagement_score !== null && (
                <div className="metric-row">
                  <span className="metric-label">Engagement</span>
                  <div className="metric-bar-container">
                    <div
                      className={`metric-bar ${getEngagementClass(biomarkers.conversation_dynamics.engagement_score)}`}
                      style={{ width: `${getEngagementPercent(biomarkers.conversation_dynamics.engagement_score)}%` }}
                    />
                  </div>
                  <span className="metric-value">
                    {Math.round(biomarkers.conversation_dynamics.engagement_score)}
                  </span>
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {/* Transcript Section */}
      {!showChecklist && (
        <section className="transcript-section">
          <div className="transcript-header">
            <div
              className="transcript-header-left"
              onClick={() => setTranscriptExpanded(!transcriptExpanded)}
              onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setTranscriptExpanded(!transcriptExpanded); }}}
              role="button"
              tabIndex={0}
              aria-expanded={transcriptExpanded}
            >
              <span className={`chevron ${transcriptExpanded ? '' : 'collapsed'}`} aria-hidden="true">
                &#9660;
              </span>
              <span className="transcript-title">Transcript</span>
            </div>
            <button
              className={`copy-btn ${copySuccess ? 'success' : ''}`}
              onClick={handleCopy}
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

      {/* SOAP Note Section - only show when completed with transcript */}
      {!showChecklist && isCompleted && transcript.finalized_text.trim() && (
        <section className="soap-section">
          <div
            className="soap-header"
            onClick={() => setSoapExpanded(!soapExpanded)}
            onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setSoapExpanded(!soapExpanded); }}}
            role="button"
            tabIndex={0}
            aria-expanded={soapExpanded}
          >
            <div className="soap-header-left">
              <span className={`chevron ${soapExpanded ? '' : 'collapsed'}`}>
                &#9660;
              </span>
              <span className="soap-title">SOAP Note</span>
            </div>
            {soapNote && (
              <button
                className="copy-btn"
                onClick={(e) => {
                  e.stopPropagation();
                  const fullNote = `SUBJECTIVE:\n${soapNote.subjective}\n\nOBJECTIVE:\n${soapNote.objective}\n\nASSESSMENT:\n${soapNote.assessment}\n\nPLAN:\n${soapNote.plan}`;
                  writeText(fullNote);
                }}
              >
                Copy
              </button>
            )}
          </div>

          {soapExpanded && (
            <div className="soap-content">
              {!soapNote && !isGeneratingSoap && !soapError && (
                <button
                  className="btn btn-generate"
                  onClick={handleGenerateSoap}
                  disabled={!ollamaStatus?.connected}
                >
                  {ollamaStatus?.connected ? 'Generate SOAP Note' : 'Ollama not connected'}
                </button>
              )}

              {isGeneratingSoap && (
                <div className="soap-loading">
                  <div className="spinner" />
                  <span>Generating SOAP note...</span>
                </div>
              )}

              {soapError && (
                <div className="soap-error">
                  <span>{soapError}</span>
                  <button className="btn-retry-small" onClick={handleGenerateSoap}>
                    Retry
                  </button>
                </div>
              )}

              {soapNote && (
                <div className="soap-note-content">
                  <div className="soap-section-item">
                    <div className="soap-section-label">SUBJECTIVE</div>
                    <div className="soap-section-text">{soapNote.subjective}</div>
                  </div>
                  <div className="soap-section-item">
                    <div className="soap-section-label">OBJECTIVE</div>
                    <div className="soap-section-text">{soapNote.objective}</div>
                  </div>
                  <div className="soap-section-item">
                    <div className="soap-section-label">ASSESSMENT</div>
                    <div className="soap-section-text">{soapNote.assessment}</div>
                  </div>
                  <div className="soap-section-item">
                    <div className="soap-section-label">PLAN</div>
                    <div className="soap-section-text">{soapNote.plan}</div>
                  </div>
                  <div className="soap-footer">
                    Generated {new Date(soapNote.generated_at).toLocaleString()} ({soapNote.model_used})
                  </div>
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {/* Sync Banner - show when completed, auto-sync enabled, but not authenticated */}
      {!showChecklist && isCompleted && pendingSettings?.medplum_auto_sync && !authState.is_authenticated && (
        <div className="sync-login-banner">
          <span className="sync-login-message">Sign in to sync this session to Medplum</span>
          <div className="sync-login-actions">
            <button
              className="btn btn-signin-small"
              onClick={medplumLogin}
              disabled={authLoading || !medplumConnected}
            >
              {authLoading ? 'Signing in...' : 'Sign In'}
            </button>
          </div>
        </div>
      )}

      {/* Action Bar - only show when completed */}
      {!showChecklist && isCompleted && (
        <div className="action-bar">
          {authState.is_authenticated && transcript.finalized_text.trim() && (
            <button
              className="btn btn-secondary"
              onClick={syncToMedplum}
              disabled={isSyncing || syncSuccess}
            >
              {isSyncing ? 'Syncing...' : syncSuccess ? '✓ Synced' : 'Sync to Medplum'}
            </button>
          )}
          <button className="btn btn-primary" onClick={handleReset}>
            New Session
          </button>
        </div>
      )}

      {/* Sync Error Toast */}
      {syncError && (
        <div className="sync-error-toast">
          <span>{syncError}</span>
          <button onClick={() => setSyncError(null)}>&times;</button>
        </div>
      )}

      {/* Sync Modal */}
      {showSyncModal && (
        <>
          <div className="settings-overlay" onClick={() => setShowSyncModal(false)} />
          <div className="sync-modal">
            <div className="sync-modal-header">
              <span className="sync-modal-title">Sync to Medplum</span>
              <button className="close-btn" onClick={() => setShowSyncModal(false)}>
                &times;
              </button>
            </div>
            <div className="sync-modal-content">
              {/* Patient Search */}
              <div className="patient-search">
                <label className="settings-label">Search Patient</label>
                <div className="patient-search-input">
                  <input
                    type="text"
                    placeholder="Enter patient name or MRN..."
                    value={patientSearchQuery}
                    onChange={(e) => setPatientSearchQuery(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') searchPatients(patientSearchQuery);
                    }}
                  />
                  <button
                    className="btn btn-small"
                    onClick={() => searchPatients(patientSearchQuery)}
                    disabled={isSearchingPatients || !patientSearchQuery.trim()}
                  >
                    {isSearchingPatients ? '...' : 'Search'}
                  </button>
                </div>
              </div>

              {/* Patient Results */}
              {patientSearchResults.length > 0 && (
                <div className="patient-results">
                  {patientSearchResults.map((patient) => (
                    <div
                      key={patient.id}
                      className={`patient-item ${selectedPatient?.id === patient.id ? 'selected' : ''}`}
                      onClick={() => setSelectedPatient(patient)}
                    >
                      <span className="patient-name">{patient.name}</span>
                      {patient.mrn && <span className="patient-mrn">MRN: {patient.mrn}</span>}
                      {patient.birthDate && <span className="patient-dob">DOB: {patient.birthDate}</span>}
                    </div>
                  ))}
                </div>
              )}

              {/* Selected Patient */}
              {selectedPatient && (
                <div className="selected-patient">
                  <span className="selected-label">Selected:</span>
                  <span className="selected-name">{selectedPatient.name}</span>
                </div>
              )}

              {/* Sync Error */}
              {syncError && (
                <div className="sync-error">{syncError}</div>
              )}

              {/* Sync Button */}
              <div className="sync-actions">
                <button
                  className="btn btn-secondary"
                  onClick={() => setShowSyncModal(false)}
                >
                  Cancel
                </button>
                <button
                  className="btn btn-primary"
                  onClick={syncToMedplum}
                  disabled={!selectedPatient || isSyncing}
                >
                  {isSyncing ? 'Syncing...' : 'Sync Encounter'}
                </button>
              </div>
            </div>
          </div>
        </>
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

                  <div className="settings-group">
                    <div className="settings-toggle">
                      <span className="settings-label" style={{ marginBottom: 0 }}>Show Biomarkers</span>
                      <label className="toggle-switch">
                        <input
                          type="checkbox"
                          checked={showBiomarkers}
                          onChange={(e) => setShowBiomarkers(e.target.checked)}
                          aria-label="Show biomarkers panel"
                        />
                        <span className="toggle-slider"></span>
                      </label>
                    </div>
                  </div>

                  {/* Ollama / SOAP Note Settings */}
                  <div className="settings-divider" />
                  <div className="settings-section-title">SOAP Note Generation</div>

                  <div className="settings-group">
                    <label className="settings-label" htmlFor="ollama-url">Ollama Server</label>
                    <input
                      id="ollama-url"
                      type="text"
                      className="settings-input"
                      value={pendingSettings.ollama_server_url}
                      onChange={(e) => setPendingSettings({ ...pendingSettings, ollama_server_url: e.target.value })}
                      placeholder="http://localhost:11434"
                    />
                  </div>

                  <div className="settings-group">
                    <label className="settings-label" htmlFor="ollama-model">Model</label>
                    <select
                      id="ollama-model"
                      className="settings-select"
                      value={pendingSettings.ollama_model}
                      onChange={(e) => setPendingSettings({ ...pendingSettings, ollama_model: e.target.value })}
                    >
                      <option value={pendingSettings.ollama_model}>{pendingSettings.ollama_model}</option>
                      {ollamaModels
                        .filter((m) => m !== pendingSettings.ollama_model)
                        .map((m) => (
                          <option key={m} value={m}>{m}</option>
                        ))}
                    </select>
                  </div>

                  <div className="settings-group ollama-status-group">
                    <div className="ollama-status">
                      <span className={`status-indicator ${ollamaStatus?.connected ? 'connected' : 'disconnected'}`} />
                      <span className="status-text">
                        {ollamaStatus?.connected
                          ? `Connected (${ollamaModels.length} models)`
                          : ollamaStatus?.error || 'Not connected'}
                      </span>
                    </div>
                    <button className="btn-test" onClick={handleTestOllama}>
                      Test
                    </button>
                  </div>

                  {/* Medplum EMR Settings */}
                  <div className="settings-divider" />
                  <div className="settings-section-title">Medplum EMR</div>

                  <div className="settings-group">
                    <label className="settings-label" htmlFor="medplum-url">Server URL</label>
                    <input
                      id="medplum-url"
                      type="text"
                      className="settings-input"
                      value={pendingSettings.medplum_server_url}
                      onChange={(e) => setPendingSettings({ ...pendingSettings, medplum_server_url: e.target.value })}
                      placeholder="http://localhost:8103"
                      disabled={authState.is_authenticated}
                    />
                  </div>

                  <div className="settings-group">
                    <label className="settings-label" htmlFor="medplum-client-id">Client ID</label>
                    <input
                      id="medplum-client-id"
                      type="text"
                      className="settings-input"
                      value={pendingSettings.medplum_client_id}
                      onChange={(e) => setPendingSettings({ ...pendingSettings, medplum_client_id: e.target.value })}
                      placeholder="Enter client ID from Medplum"
                      disabled={authState.is_authenticated}
                    />
                  </div>

                  <div className="settings-group">
                    <div className="settings-toggle">
                      <span className="settings-label" style={{ marginBottom: 0 }}>Auto-sync encounters</span>
                      <label className="toggle-switch">
                        <input
                          type="checkbox"
                          checked={pendingSettings.medplum_auto_sync}
                          onChange={(e) =>
                            setPendingSettings({ ...pendingSettings, medplum_auto_sync: e.target.checked })
                          }
                          aria-label="Auto-sync encounters to Medplum"
                        />
                        <span className="toggle-slider"></span>
                      </label>
                    </div>
                  </div>

                  <div className="settings-group ollama-status-group">
                    <div className="ollama-status">
                      <span className={`status-indicator ${medplumConnected ? 'connected' : 'disconnected'}`} />
                      <span className="status-text">
                        {medplumConnected
                          ? 'Connected'
                          : medplumError || 'Not connected'}
                      </span>
                    </div>
                    <button className="btn-test" onClick={handleTestMedplum}>
                      Test
                    </button>
                  </div>

                  {/* Medplum Authentication */}
                  <div className="settings-group medplum-auth-group">
                    {authState.is_authenticated ? (
                      <div className="medplum-auth-status">
                        <div className="auth-user-info">
                          <span className="status-indicator connected" />
                          <span className="auth-user-name">
                            {authState.practitioner_name || 'Signed in'}
                          </span>
                        </div>
                        <button
                          className="btn-signout"
                          onClick={medplumLogout}
                          disabled={authLoading}
                        >
                          {authLoading ? 'Signing out...' : 'Sign Out'}
                        </button>
                      </div>
                    ) : (
                      <button
                        className="btn-signin"
                        onClick={medplumLogin}
                        disabled={authLoading || !medplumConnected}
                        title={!medplumConnected ? 'Connect to server first' : ''}
                      >
                        {authLoading ? 'Signing in...' : 'Sign In with Medplum'}
                      </button>
                    )}
                  </div>
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
