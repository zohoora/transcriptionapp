import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
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
import { useSessionState } from './hooks/useSessionState';
import { useChecklist } from './hooks/useChecklist';
import { useSoapNote } from './hooks/useSoapNote';
import { useMedplumSync } from './hooks/useMedplumSync';
import type {
  Device,
  Settings,
  OllamaStatus,
} from './types';

// UI Mode type
type UIMode = 'ready' | 'recording' | 'review';

function App() {
  // Medplum auth from context
  const { authState, login: medplumLogin, logout: medplumLogout, cancelLogin: medplumCancelLogin, isLoading: authLoading } = useAuth();

  // Session state from hook
  const {
    status,
    transcript,
    biomarkers,
    audioQuality,
    editedTranscript,
    setEditedTranscript,
    soapNote,
    setSoapNote,
    isIdle,
    isRecording,
    handleStart: sessionStart,
    handleStop,
    handleReset: sessionReset,
  } = useSessionState();

  // Checklist state from hook
  const {
    checklistResult,
    checklistRunning,
    downloadingModel,
    modelStatus,
    setModelStatus,
    runChecklist,
    handleDownloadModel,
  } = useChecklist();

  // SOAP note generation from hook
  const {
    isGeneratingSoap,
    soapError,
    ollamaStatus,
    setOllamaStatus,
    ollamaModels,
    setOllamaModels,
    generateSoapNote,
  } = useSoapNote();

  // Medplum sync from hook
  const {
    medplumConnected,
    setMedplumConnected,
    medplumError,
    setMedplumError,
    isSyncing,
    syncError,
    setSyncError,
    syncSuccess,
    resetSyncState,
    syncToMedplum,
  } = useMedplumSync();

  // UI state (not in hooks)
  const [devices, setDevices] = useState<Device[]>([]);
  const [showSettings, setShowSettings] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [pendingSettings, setPendingSettings] = useState<PendingSettings | null>(null);
  const [showBiomarkers, setShowBiomarkers] = useState(true);

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
  }, [status.state, transcript.finalized_text, editedTranscript, setEditedTranscript]);

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

  // Load devices, settings, and check connections on mount
  useEffect(() => {
    async function init() {
      try {
        const deviceList = await invoke<Device[]>('list_input_devices');
        setDevices(deviceList);

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
  }, [setOllamaStatus, setOllamaModels, setMedplumConnected, setMedplumError]);

  // Handle start recording with reset
  const handleStart = useCallback(async () => {
    const device = pendingSettings?.device === 'default' ? null : pendingSettings?.device ?? null;
    resetSyncState();
    await sessionStart(device);
  }, [pendingSettings?.device, resetSyncState, sessionStart]);

  // Handle reset/new session with cleanup
  const handleReset = useCallback(async () => {
    resetSyncState();
    await sessionReset();
  }, [resetSyncState, sessionReset]);

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
      const modelResult = await invoke<typeof modelStatus>('check_model_status');
      setModelStatus(modelResult);
    } catch (e) {
      console.error('Failed to save settings:', e);
    }
  }, [pendingSettings, settings, setModelStatus]);

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
  }, [settings, pendingSettings, setOllamaStatus, setOllamaModels]);

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
  }, [settings, pendingSettings, setMedplumConnected, setMedplumError]);

  // Generate SOAP note
  const handleGenerateSoap = useCallback(async () => {
    const result = await generateSoapNote(editedTranscript);
    if (result) {
      setSoapNote(result);
    }
  }, [editedTranscript, generateSoapNote, setSoapNote]);

  // Sync to Medplum
  const handleSyncToMedplum = useCallback(async () => {
    await syncToMedplum({
      authState,
      transcript: editedTranscript,
      soapNote,
      elapsedMs: status.elapsed_ms,
    });
  }, [authState, editedTranscript, soapNote, status.elapsed_ms, syncToMedplum]);

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
  const isStopping = status.state === 'stopping';
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
          onSync={handleSyncToMedplum}
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
