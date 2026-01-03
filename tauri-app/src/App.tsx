import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useAuth } from './components/AuthProvider';
import {
  Header,
  SettingsDrawer,
  ReadyMode,
  RecordingMode,
  ReviewMode,
} from './components';
import {
  useSessionState,
  useChecklist,
  useSoapNote,
  useMedplumSync,
  useSettings,
  useDevices,
  useOllamaConnection,
} from './hooks';
import type { Settings } from './types';

// UI Mode type
type UIMode = 'ready' | 'recording' | 'review';

function App() {
  // Medplum auth from context
  const { authState, login: medplumLogin, logout: medplumLogout, cancelLogin: medplumCancelLogin, isLoading: authLoading } = useAuth();

  // Session state from hook (includes timer)
  const {
    status,
    transcript,
    biomarkers,
    audioQuality,
    editedTranscript,
    setEditedTranscript,
    soapNote,
    setSoapNote,
    localElapsedMs,
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

  // Settings from hook
  const {
    settings,
    pendingSettings,
    setPendingSettings,
    saveSettings,
  } = useSettings();

  // Devices from hook
  const { devices } = useDevices();

  // Ollama connection from hook
  const { status: ollamaConnectionStatus, checkConnection: checkOllamaConnection } = useOllamaConnection();

  // UI state
  const [showSettings, setShowSettings] = useState(false);
  const [showBiomarkers, setShowBiomarkers] = useState(true);

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

  // Sync Ollama status from connection hook to SOAP hook
  useEffect(() => {
    if (ollamaConnectionStatus) {
      setOllamaStatus(ollamaConnectionStatus);
      if (ollamaConnectionStatus.connected) {
        setOllamaModels(ollamaConnectionStatus.available_models);
      }
    }
  }, [ollamaConnectionStatus, setOllamaStatus, setOllamaModels]);

  // Check Medplum connection and restore session on mount
  useEffect(() => {
    let mounted = true;

    async function initMedplum() {
      try {
        // Try restore session
        await invoke('medplum_try_restore_session');
        if (!mounted) return;

        // Check Medplum connection
        const connected = await invoke<boolean>('medplum_check_connection');
        if (mounted) {
          setMedplumConnected(connected);
        }
      } catch (e) {
        if (mounted) {
          setMedplumConnected(false);
          setMedplumError(String(e));
        }
      }
    }
    initMedplum();

    return () => {
      mounted = false;
    };
  }, [setMedplumConnected, setMedplumError]);

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
    const success = await saveSettings();
    if (success) {
      setShowSettings(false);
      // Refresh model status after saving
      try {
        const modelResult = await invoke<typeof modelStatus>('check_model_status');
        setModelStatus(modelResult);
      } catch (e) {
        console.error('Failed to refresh model status:', e);
      }
    }
  }, [saveSettings, setModelStatus]);

  // Test Ollama connection
  const handleTestOllama = useCallback(async () => {
    if (!pendingSettings || !settings) return;
    try {
      // Save settings first, then re-check connection
      await invoke('set_settings', {
        settings: {
          ...settings,
          ollama_server_url: pendingSettings.ollama_server_url,
          ollama_model: pendingSettings.ollama_model,
        },
      });
      await checkOllamaConnection();
    } catch (e) {
      console.error('Failed to test Ollama:', e);
      setOllamaStatus({ connected: false, available_models: [], error: String(e) });
    }
  }, [settings, pendingSettings, checkOllamaConnection, setOllamaStatus]);

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

  // Generate SOAP note (includes audio events like coughs, laughs for clinical context)
  const handleGenerateSoap = useCallback(async () => {
    const result = await generateSoapNote(editedTranscript, biomarkers?.recent_events);
    if (result) {
      setSoapNote(result);
    }
  }, [editedTranscript, biomarkers?.recent_events, generateSoapNote, setSoapNote]);

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
