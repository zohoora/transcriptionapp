import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
// Clipboard is used by ReviewMode component
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useAuth } from './components/AuthProvider';
import {
  Header,
  SettingsDrawer,
  ReadyMode,
  RecordingMode,
  ReviewMode,
  type PendingSettings,
} from './components';
import type {
  SessionStatus,
  TranscriptUpdate,
  Device,
  ModelStatus,
  Settings,
  OllamaStatus,
  SoapNote,
  ChecklistResult,
  BiomarkerUpdate,
  AudioQualitySnapshot,
  SyncResult,
} from './types';

// UI Mode type
type UIMode = 'ready' | 'recording' | 'review';

function App() {
  // Medplum auth from context
  const { authState, login: medplumLogin, logout: medplumLogout, cancelLogin: medplumCancelLogin, isLoading: authLoading } = useAuth();

  // Session state
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
  const [editedTranscript, setEditedTranscript] = useState('');
  const [devices, setDevices] = useState<Device[]>([]);
  const [modelStatus, setModelStatus] = useState<ModelStatus | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [pendingSettings, setPendingSettings] = useState<PendingSettings | null>(null);

  // Medplum state
  const [medplumConnected, setMedplumConnected] = useState(false);
  const [medplumError, setMedplumError] = useState<string | null>(null);

  // Checklist state
  const [checklistResult, setChecklistResult] = useState<ChecklistResult | null>(null);
  const [checklistRunning, setChecklistRunning] = useState(true);
  const [downloadingModel, setDownloadingModel] = useState<string | null>(null);

  // Biomarker state
  const [biomarkers, setBiomarkers] = useState<BiomarkerUpdate | null>(null);
  const [showBiomarkers, setShowBiomarkers] = useState(true);

  // Audio quality state
  const [audioQuality, setAudioQuality] = useState<AudioQualitySnapshot | null>(null);

  // SOAP note state
  const [soapNote, setSoapNote] = useState<SoapNote | null>(null);
  const [isGeneratingSoap, setIsGeneratingSoap] = useState(false);
  const [soapError, setSoapError] = useState<string | null>(null);
  const [ollamaStatus, setOllamaStatus] = useState<OllamaStatus | null>(null);
  const [ollamaModels, setOllamaModels] = useState<string[]>([]);

  // Medplum sync state
  const [isSyncing, setIsSyncing] = useState(false);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [syncSuccess, setSyncSuccess] = useState(false);

  // Timer state
  const [localElapsedMs, setLocalElapsedMs] = useState(0);
  const recordingStartRef = useRef<number | null>(null);

  // Determine current UI mode based on session state
  const getUIMode = (): UIMode => {
    switch (status.state) {
      case 'recording':
      case 'preparing':
      case 'stopping':
        return 'recording';
      case 'completed':
        return 'review';
      case 'idle':
      case 'error':
      default:
        return 'ready';
    }
  };

  const uiMode = getUIMode();

  // Sync edited transcript with original when recording completes
  useEffect(() => {
    if (status.state === 'completed' && transcript.finalized_text && !editedTranscript) {
      setEditedTranscript(transcript.finalized_text);
    }
  }, [status.state, transcript.finalized_text, editedTranscript]);

  // Local timer that runs during recording/preparing
  useEffect(() => {
    if (status.state === 'preparing' || status.state === 'recording') {
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
      recordingStartRef.current = null;
      if (status.state === 'idle') {
        setLocalElapsedMs(0);
      }
    }
  }, [status.state]);

  // Run checklist function
  const runChecklist = useCallback(async () => {
    setChecklistRunning(true);
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
        command = 'download_whisper_model';
      }
      // Pass model name for Whisper downloads
      if (command === 'download_whisper_model') {
        await invoke(command, { modelName });
      } else {
        await invoke(command);
      }
      await runChecklist();
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

        // Try restore session
        await invoke('medplum_try_restore_session');

        // Check Ollama status
        try {
          const ollamaResult = await invoke<OllamaStatus>('check_ollama_status');
          setOllamaStatus(ollamaResult);
          if (ollamaResult.connected) {
            setOllamaModels(ollamaResult.available_models);
          }
        } catch (e) {
          setOllamaStatus({ connected: false, available_models: [], error: String(e) });
        }

        // Check Medplum connection
        try {
          const connected = await invoke<boolean>('medplum_check_connection');
          setMedplumConnected(connected);
        } catch (e) {
          setMedplumConnected(false);
          setMedplumError(String(e));
        }
      } catch (e) {
        console.error('Init error:', e);
      }
    }
    init();
  }, []);

  // Subscribe to events
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    listen<SessionStatus>('session_status', (event) => {
      setStatus(event.payload);
    }).then((fn) => unlisteners.push(fn));

    listen<TranscriptUpdate>('transcript_update', (event) => {
      setTranscript(event.payload);
    }).then((fn) => unlisteners.push(fn));

    listen<BiomarkerUpdate>('biomarker_update', (event) => {
      setBiomarkers(event.payload);
    }).then((fn) => unlisteners.push(fn));

    listen<AudioQualitySnapshot>('audio_quality', (event) => {
      setAudioQuality(event.payload);
    }).then((fn) => unlisteners.push(fn));

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, []);

  // Handle start recording
  const handleStart = useCallback(async () => {
    try {
      // Reset state for new session
      setTranscript({ finalized_text: '', draft_text: null, segment_count: 0 });
      setEditedTranscript('');
      setBiomarkers(null);
      setAudioQuality(null);
      setSoapNote(null);
      setSoapError(null);
      setSyncSuccess(false);
      setSyncError(null);

      const device = pendingSettings?.device === 'default' ? null : pendingSettings?.device;
      await invoke('start_session', { deviceId: device });
    } catch (e) {
      console.error('Failed to start session:', e);
    }
  }, [pendingSettings?.device]);

  // Handle stop recording
  const handleStop = useCallback(async () => {
    try {
      await invoke('stop_session');
    } catch (e) {
      console.error('Failed to stop session:', e);
    }
  }, []);

  // Handle reset/new session
  const handleReset = useCallback(async () => {
    try {
      await invoke('reset_session');
      setTranscript({ finalized_text: '', draft_text: null, segment_count: 0 });
      setEditedTranscript('');
      setBiomarkers(null);
      setAudioQuality(null);
      setSoapNote(null);
      setSoapError(null);
      setSyncSuccess(false);
      setSyncError(null);
    } catch (e) {
      console.error('Failed to reset session:', e);
    }
  }, []);

  // Save settings
  const handleSaveSettings = useCallback(async () => {
    if (!pendingSettings || !settings) return;
    try {
      const newSettings: Settings = {
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
      await invoke('set_settings', { settings: newSettings });
      setSettings(newSettings);
      setShowSettings(false);

      // Refresh model status
      const modelResult = await invoke<ModelStatus>('check_model_status');
      setModelStatus(modelResult);
    } catch (e) {
      console.error('Failed to save settings:', e);
    }
  }, [pendingSettings, settings]);

  // Test Ollama connection
  const handleTestOllama = useCallback(async () => {
    if (!pendingSettings) return;
    try {
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
    if (!editedTranscript.trim()) return;

    setIsGeneratingSoap(true);
    setSoapError(null);

    try {
      const result = await invoke<SoapNote>('generate_soap_note', {
        transcript: editedTranscript,
      });
      setSoapNote(result);
    } catch (e) {
      console.error('Failed to generate SOAP note:', e);
      setSoapError(String(e));
    } finally {
      setIsGeneratingSoap(false);
    }
  }, [editedTranscript]);

  // Sync to Medplum
  const syncToMedplum = useCallback(async () => {
    if (!authState.is_authenticated) return;

    setIsSyncing(true);
    setSyncError(null);
    setSyncSuccess(false);

    try {
      const audioFilePath = await invoke<string | null>('get_audio_file_path');

      const soapText = soapNote
        ? `SUBJECTIVE:\n${soapNote.subjective}\n\nOBJECTIVE:\n${soapNote.objective}\n\nASSESSMENT:\n${soapNote.assessment}\n\nPLAN:\n${soapNote.plan}`
        : null;

      const result = await invoke<SyncResult>('medplum_quick_sync', {
        transcript: editedTranscript,
        soapNote: soapText,
        audioFilePath: audioFilePath,
        sessionDurationMs: status.elapsed_ms,
      });

      if (result.success) {
        setSyncSuccess(true);
      } else {
        setSyncError(result.error || 'Sync failed');
      }
    } catch (e) {
      console.error('Failed to sync to Medplum:', e);
      setSyncError(String(e));
    } finally {
      setIsSyncing(false);
    }
  }, [authState.is_authenticated, editedTranscript, soapNote, status.elapsed_ms]);

  // Open history window
  const openHistoryWindow = useCallback(async () => {
    try {
      const existing = await WebviewWindow.getByLabel('history');
      if (existing) {
        await existing.setFocus();
        return;
      }

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

  // Derived state
  const isRecording = status.state === 'recording';
  const isStopping = status.state === 'stopping';
  const isIdle = status.state === 'idle';

  const canStart = isIdle && modelStatus?.available && checklistResult?.can_start;

  // Get status dot class for header
  const getStatusDotClass = (): string => {
    if (isRecording) return 'recording';
    if (isStopping) return 'stopping';
    if (status.state === 'preparing') return 'preparing';
    if (isIdle) return 'idle';
    return '';
  };

  return (
    <div className={`sidebar mode-${uiMode}`}>
      {/* Header - always visible */}
      <Header
        statusDotClass={getStatusDotClass()}
        showSettings={showSettings}
        disabled={isRecording || isStopping}
        onHistoryClick={openHistoryWindow}
        onSettingsClick={() => setShowSettings(!showSettings)}
      />

      {/* Mode-based content */}
      {uiMode === 'ready' && (
        <ReadyMode
          modelStatus={modelStatus}
          modelName={pendingSettings?.model || 'small'}
          checklistRunning={checklistRunning}
          checklistResult={checklistResult}
          onRunChecklist={runChecklist}
          onDownloadModel={handleDownloadModel}
          downloadingModel={downloadingModel}
          audioLevel={audioQuality ? Math.min(100, (audioQuality.rms_db + 60) / 0.6) : 0}
          errorMessage={status.state === 'error' ? status.error_message : null}
          canStart={!!canStart}
          onStart={handleStart}
        />
      )}

      {uiMode === 'recording' && (
        <RecordingMode
          elapsedMs={localElapsedMs}
          audioQuality={audioQuality}
          biomarkers={biomarkers}
          transcriptText={transcript.finalized_text}
          draftText={transcript.draft_text}
          isStopping={isStopping}
          onStop={handleStop}
        />
      )}

      {uiMode === 'review' && (
        <ReviewMode
          elapsedMs={status.elapsed_ms || localElapsedMs}
          audioQuality={audioQuality}
          originalTranscript={transcript.finalized_text}
          editedTranscript={editedTranscript}
          onTranscriptEdit={setEditedTranscript}
          soapNote={soapNote}
          isGeneratingSoap={isGeneratingSoap}
          soapError={soapError}
          ollamaConnected={ollamaStatus?.connected || false}
          onGenerateSoap={handleGenerateSoap}
          biomarkers={biomarkers}
          authState={authState}
          isSyncing={isSyncing}
          syncSuccess={syncSuccess}
          syncError={syncError}
          onSync={syncToMedplum}
          onClearSyncError={() => setSyncError(null)}
          onNewSession={handleReset}
          onLogin={medplumLogin}
          onCancelLogin={medplumCancelLogin}
          authLoading={authLoading}
          autoSyncEnabled={pendingSettings?.medplum_auto_sync || false}
        />
      )}

      {/* Settings Drawer */}
      <SettingsDrawer
        isOpen={showSettings}
        onClose={() => setShowSettings(false)}
        pendingSettings={pendingSettings}
        onSettingsChange={setPendingSettings}
        onSave={handleSaveSettings}
        devices={devices}
        showBiomarkers={showBiomarkers}
        onShowBiomarkersChange={setShowBiomarkers}
        ollamaStatus={ollamaStatus}
        ollamaModels={ollamaModels}
        onTestOllama={handleTestOllama}
        medplumConnected={medplumConnected}
        medplumError={medplumError}
        onTestMedplum={handleTestMedplum}
        authState={authState}
        authLoading={authLoading}
        onLogin={medplumLogin}
        onLogout={medplumLogout}
        onCancelLogin={medplumCancelLogin}
      />
    </div>
  );
}

export default App;
