import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { useAuth } from './components/AuthProvider';
import {
  ErrorBoundary,
  Header,
  SettingsDrawer,
  ReadyMode,
  RecordingMode,
  ReviewMode,
} from './components';
import type { SyncStatus } from './components';
import {
  useSessionState,
  useSoapNote,
  useMedplumSync,
  useSettings,
  useDevices,
  useOllamaConnection,
  useChecklist,
  useAutoDetection,
  useClinicalChat,
} from './hooks';
import { usePredictiveHint } from './hooks/usePredictiveHint';
import { useMiisImages } from './hooks/useMiisImages';
import { useScreenCapture } from './hooks/useScreenCapture';
import type { Settings, WhisperServerStatus } from './types';

// UI Mode type
type UIMode = 'ready' | 'recording' | 'review';

// Helper to get the current whisper model name based on mode
function getWhisperModelName(
  whisperMode: 'local' | 'remote' | undefined,
  whisperServerModel: string | undefined,
  localModel: string | undefined
): string {
  if (whisperMode === 'remote') {
    return whisperServerModel || 'unknown';
  }
  return localModel || 'unknown';
}

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
    soapResult,
    setSoapResult,
    localElapsedMs,
    silenceWarning,
    isIdle,
    isRecording,
    handleStart: sessionStart,
    handleStop,
    handleReset: sessionReset,
  } = useSessionState();

  // SOAP note generation from hook
  const {
    isGeneratingSoap,
    soapError,
    ollamaStatus,
    setOllamaStatus,
    ollamaModels,
    setOllamaModels,  // Still needed for connection sync
    soapOptions,
    generateSoapNote,
    updateSoapDetailLevel,
    updateSoapFormat,
    updateSoapCustomInstructions,
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
    syncedEncounter,
    isAddingSoap,
    resetSyncState,
    syncToMedplum,
    syncMultiPatientToMedplum,
    addSoapToEncounter,
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

  // Checklist from hook (for permission checks)
  const { checkMicrophonePermission, openMicrophoneSettings } = useChecklist();

  // Clinical chat for during-appointment Q&A
  const {
    messages: chatMessages,
    isLoading: chatIsLoading,
    error: chatError,
    sendMessage: chatSendMessage,
    clearChat,
  } = useClinicalChat(
    settings?.llm_router_url || '',
    settings?.llm_api_key || '',
    settings?.llm_client_id || 'ai-scribe'
  );

  // Ref to track if an auto-started session is still pending greeting confirmation
  const autoStartPendingRef = useRef(false);

  // Auto-detection callbacks for optimistic recording flow
  // 1. onStartRecording: Start recording immediately when speech detected
  const handleAutoStartRecording = useCallback(async () => {
    console.log('Auto-detection: Starting recording immediately (optimistic)');
    autoStartPendingRef.current = true; // Mark as pending confirmation
    setSessionNotes(''); // Reset session notes on auto-start
    const device = pendingSettings?.device === 'default' ? null : pendingSettings?.device ?? null;
    resetSyncState();
    await sessionStart(device);
  }, [pendingSettings?.device, resetSyncState, sessionStart]);

  // 2. onGreetingConfirmed: Greeting check passed - recording continues
  const handleGreetingConfirmed = useCallback((transcript: string, confidence: number) => {
    console.log(`Greeting confirmed: "${transcript}" (confidence: ${confidence.toFixed(2)})`);
    autoStartPendingRef.current = false; // No longer pending - session is confirmed
    // Recording is already in progress, just log confirmation
  }, []);

  // 3. onGreetingRejected: Not a greeting - abort recording only if session is still new
  const handleGreetingRejected = useCallback(async (reason: string) => {
    console.log(`Greeting rejected: ${reason}`);
    // Only reset if we're still pending confirmation (session just started)
    // If the session has been running for a while, the user may have started speaking
    // and we shouldn't discard their recording
    if (autoStartPendingRef.current) {
      console.log('Session still pending confirmation - aborting recording');
      autoStartPendingRef.current = false;
      await sessionReset();
    } else {
      console.log('Session already confirmed or user-initiated - keeping recording');
    }
  }, [sessionReset]);

  // Auto-detection from hook
  const {
    isListening,
    isPendingConfirmation: _isPendingConfirmation, // Available for future UI indicators
    listeningStatus,
    error: listeningError,
    startListening,
    stopListening,
  } = useAutoDetection(
    pendingSettings?.auto_start_enabled ?? false,
    {
      onStartRecording: handleAutoStartRecording,
      onGreetingConfirmed: handleGreetingConfirmed,
      onGreetingRejected: handleGreetingRejected,
    }
  );

  // Handle auto-start toggle - updates settings and saves immediately
  const handleAutoStartToggle = useCallback(async (enabled: boolean) => {
    if (!settings || !pendingSettings) return;

    // Update pending settings first (for UI)
    const newPendingSettings = { ...pendingSettings, auto_start_enabled: enabled };
    setPendingSettings(newPendingSettings);

    // Build full settings object and save directly (avoids async state issue)
    const fullSettings: Settings = {
      ...settings,
      auto_start_enabled: enabled,
    };

    try {
      await invoke('set_settings', { settings: fullSettings });
      console.log(`Auto-start ${enabled ? 'enabled' : 'disabled'} and saved`);
    } catch (e) {
      console.error('Failed to save auto-start setting:', e);
    }
  }, [settings, pendingSettings, setPendingSettings]);

  // Handle auto-end toggle - updates settings and saves immediately
  const handleAutoEndToggle = useCallback(async (enabled: boolean) => {
    if (!settings || !pendingSettings) return;

    // Update pending settings first (for UI)
    const newPendingSettings = { ...pendingSettings, auto_end_enabled: enabled };
    setPendingSettings(newPendingSettings);

    // Build full settings object and save directly (avoids async state issue)
    const fullSettings: Settings = {
      ...settings,
      auto_end_enabled: enabled,
    };

    try {
      await invoke('set_settings', { settings: fullSettings });
      console.log(`Auto-end ${enabled ? 'enabled' : 'disabled'} and saved`);
    } catch (e) {
      console.error('Failed to save auto-end setting:', e);
    }
  }, [settings, pendingSettings, setPendingSettings]);

  // Permission error state
  const [permissionError, setPermissionError] = useState<string | null>(null);

  // Session notes state (clinician observations during recording)
  const [sessionNotes, setSessionNotes] = useState('');

  // Sync indicator dismissed state (for hiding after user dismisses)
  const [syncDismissed, setSyncDismissed] = useState(false);

  // Derive sync status for header indicator
  const getSyncStatus = (): SyncStatus => {
    if (syncDismissed) return 'idle';
    if (isSyncing || isAddingSoap) return 'syncing';
    if (syncError) return 'error';
    if (syncSuccess) return 'success';
    return 'idle';
  };

  // Reset dismissed state when sync starts
  useEffect(() => {
    if (isSyncing || isAddingSoap) {
      setSyncDismissed(false);
    }
  }, [isSyncing, isAddingSoap]);

  // Handle dismiss sync indicator
  const handleDismissSync = useCallback(() => {
    setSyncDismissed(true);
  }, []);

  // Note: Local Whisper models removed - using remote server only

  // UI state
  const [showSettings, setShowSettings] = useState(false);
  const [showBiomarkers, setShowBiomarkers] = useState(false);

  // Whisper server state
  const [whisperServerStatus, setWhisperServerStatus] = useState<WhisperServerStatus | null>(null);
  const [whisperServerModels, setWhisperServerModels] = useState<string[]>([]);

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

  // Predictive hints and image concepts during recording
  const {
    hint: predictiveHint,
    concepts: imageConcepts,
    isLoading: predictiveHintLoading,
  } = usePredictiveHint({
    transcript: transcript.finalized_text,
    isRecording: uiMode === 'recording',
  });

  // MIIS image suggestions (uses concepts from predictive hints)
  const {
    suggestions: miisSuggestions,
    isLoading: miisLoading,
    error: miisError,
    recordImpression: miisRecordImpression,
    recordClick: miisRecordClick,
    recordDismiss: miisRecordDismiss,
    getImageUrl: miisGetImageUrl,
  } = useMiisImages({
    sessionId: status.session_id ?? null,
    concepts: imageConcepts,
    enabled: settings?.miis_enabled ?? false,
    serverUrl: settings?.miis_server_url ?? '',
  });

  // Screen capture tied to recording lifecycle
  useScreenCapture(isRecording, settings);

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

  // Auto-sync to Medplum when session completes (if authenticated and auto-sync enabled)
  useEffect(() => {
    // Only auto-sync when:
    // 1. Session just completed
    // 2. User is authenticated
    // 3. Auto-sync is enabled
    // 4. Not already syncing or synced
    // 5. There's a transcript to sync
    if (
      status.state === 'completed' &&
      authState.is_authenticated &&
      pendingSettings?.medplum_auto_sync &&
      !syncSuccess &&
      !isSyncing &&
      !syncedEncounter &&
      transcript.finalized_text
    ) {
      // Auto-sync without SOAP (SOAP will be added later if generated)
      syncToMedplum({
        authState,
        transcript: transcript.finalized_text,
        soapNote: null,
        elapsedMs: status.elapsed_ms,
      });
    }
  }, [
    status.state,
    status.elapsed_ms,
    authState,
    pendingSettings?.medplum_auto_sync,
    syncSuccess,
    isSyncing,
    syncedEncounter,
    transcript.finalized_text,
    syncToMedplum,
  ]);

  // Start/stop listening mode based on UI state and settings
  useEffect(() => {
    const autoStartEnabled = pendingSettings?.auto_start_enabled ?? false;
    const deviceId = pendingSettings?.device === 'default' ? null : pendingSettings?.device ?? null;

    // Start listening when:
    // 1. In ready mode (idle or error state)
    // 2. Auto-start is enabled
    // 3. Not already listening
    if (uiMode === 'ready' && autoStartEnabled && !isListening) {
      startListening(deviceId);
    }

    // Stop listening when:
    // 1. Not in ready mode (recording started)
    // 2. OR auto-start is disabled
    if ((uiMode !== 'ready' || !autoStartEnabled) && isListening) {
      stopListening();
    }
  }, [uiMode, pendingSettings?.auto_start_enabled, pendingSettings?.device, isListening, startListening, stopListening]);

  // Handle start recording with permission check
  const handleStart = useCallback(async () => {
    // Check microphone permission first
    const permStatus = await checkMicrophonePermission();
    if (!permStatus.authorized) {
      setPermissionError(permStatus.message);
      return;
    }

    setPermissionError(null);
    setSessionNotes(''); // Reset session notes on new recording
    autoStartPendingRef.current = false; // Manual start, not pending auto-confirmation
    const device = pendingSettings?.device === 'default' ? null : pendingSettings?.device ?? null;
    resetSyncState();
    await sessionStart(device);
  }, [pendingSettings?.device, resetSyncState, sessionStart, checkMicrophonePermission]);

  // Handle reset/new session with cleanup
  const handleReset = useCallback(async () => {
    autoStartPendingRef.current = false; // Clear pending state on reset
    setSessionNotes(''); // Reset session notes
    resetSyncState();
    await sessionReset();
  }, [resetSyncState, sessionReset]);

  // SettingsDrawer handlers
  const handleSaveSettings = useCallback(async () => {
    const success = await saveSettings();
    if (success) {
      setShowSettings(false);
    }
  }, [saveSettings]);

  const handleTestLLM = useCallback(async () => {
    if (!pendingSettings || !settings) return;
    try {
      await invoke('set_settings', {
        settings: {
          ...settings,
          llm_router_url: pendingSettings.llm_router_url,
          llm_api_key: pendingSettings.llm_api_key,
          llm_client_id: pendingSettings.llm_client_id,
          soap_model: pendingSettings.soap_model,
          fast_model: pendingSettings.fast_model,
        },
      });
      await checkOllamaConnection();
    } catch (e) {
      console.error('Failed to test LLM router:', e);
      setOllamaStatus({ connected: false, available_models: [], error: String(e) });
    }
  }, [settings, pendingSettings, checkOllamaConnection, setOllamaStatus]);

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

  const handleTestWhisperServer = useCallback(async () => {
    if (!pendingSettings || !settings) return;
    try {
      await invoke('set_settings', {
        settings: {
          ...settings,
          whisper_server_url: pendingSettings.whisper_server_url,
          whisper_server_model: pendingSettings.whisper_server_model,
        },
      });

      const status = await invoke<WhisperServerStatus>('check_whisper_server_status');
      setWhisperServerStatus(status);
      if (status.connected) {
        setWhisperServerModels(status.available_models);
      }
    } catch (e) {
      console.error('Failed to test Whisper server:', e);
      setWhisperServerStatus({ connected: false, available_models: [], error: String(e) });
    }
  }, [settings, pendingSettings]);

  // Generate SOAP note (includes audio events like coughs, laughs for clinical context)
  // If already synced to Medplum, auto-add SOAP to the encounter
  // For multi-patient sessions, use multi-patient sync
  const handleGenerateSoap = useCallback(async () => {
    // Include session notes in the SOAP options
    const optionsWithNotes = sessionNotes.trim()
      ? { ...soapOptions, session_notes: sessionNotes }
      : soapOptions;
    // Pass session_id for debug storage correlation
    const result = await generateSoapNote(editedTranscript, biomarkers?.recent_events, optionsWithNotes, status.session_id);
    if (result) {
      setSoapResult(result);

      // If authenticated and have SOAP notes, sync them
      if (authState.is_authenticated && result.notes.length > 0) {
        const isMultiPatient = result.notes.length > 1;

        if (isMultiPatient) {
          // Multi-patient: Use multi-patient sync (creates patients/encounters for each)
          await syncMultiPatientToMedplum({
            authState,
            transcript: editedTranscript,
            soapResult: result,
            elapsedMs: status.elapsed_ms,
          });
        } else if (syncedEncounter && !syncedEncounter.hasSoap) {
          // Single patient, already synced: Add SOAP to existing encounter
          // Construct SoapNote from PatientSoapNote content
          await addSoapToEncounter({
            content: result.notes[0].content,
            generated_at: result.generated_at,
            model_used: result.model_used,
          });
        } else if (!syncedEncounter) {
          // Single patient, not yet synced: Sync with SOAP
          await syncToMedplum({
            authState,
            transcript: editedTranscript,
            soapNote: {
              content: result.notes[0].content,
              generated_at: result.generated_at,
              model_used: result.model_used,
            },
            elapsedMs: status.elapsed_ms,
          });
        }
      }
    }
  }, [
    editedTranscript,
    biomarkers?.recent_events,
    generateSoapNote,
    setSoapResult,
    authState,
    status.session_id,
    status.elapsed_ms,
    syncedEncounter,
    syncToMedplum,
    syncMultiPatientToMedplum,
    addSoapToEncounter,
    sessionNotes,
    soapOptions,
  ]);

  // Auto-generate SOAP note when session completes (if Ollama is connected)
  useEffect(() => {
    // Only auto-generate when:
    // 1. Session just completed
    // 2. Ollama is connected
    // 3. Not already generating or generated
    // 4. There's a transcript
    if (
      status.state === 'completed' &&
      ollamaStatus?.connected &&
      !isGeneratingSoap &&
      !soapResult &&
      transcript.finalized_text
    ) {
      // Auto-generate SOAP note
      handleGenerateSoap();
    }
  }, [
    status.state,
    ollamaStatus?.connected,
    isGeneratingSoap,
    soapResult,
    transcript.finalized_text,
    handleGenerateSoap,
  ]);

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
        syncStatus={getSyncStatus()}
        syncError={syncError}
        onDismissSync={handleDismissSync}
      />

      {/* Mode-based content wrapped in ErrorBoundary */}
      <ErrorBoundary>
        {uiMode === 'ready' && (
          <ReadyMode
            audioLevel={audioQuality ? Math.min(100, (audioQuality.rms_db + 60) / 0.6) : 0}
            errorMessage={permissionError || listeningError || (status.state === 'error' ? status.error_message : null)}
            isPermissionError={!!permissionError}
            autoStartEnabled={pendingSettings?.auto_start_enabled ?? false}
            isListening={isListening}
            listeningStatus={listeningStatus}
            authState={authState}
            authLoading={authLoading}
            onLogin={medplumLogin}
            onCancelLogin={medplumCancelLogin}
            onStart={handleStart}
            onOpenSettings={openMicrophoneSettings}
            onAutoStartToggle={handleAutoStartToggle}
            autoEndEnabled={pendingSettings?.auto_end_enabled ?? true}
            onAutoEndToggle={handleAutoEndToggle}
          />
        )}

        {uiMode === 'recording' && (
          <RecordingMode
            elapsedMs={localElapsedMs}
            audioQuality={audioQuality}
            biomarkers={biomarkers}
            transcriptText={transcript.finalized_text}
            draftText={transcript.draft_text}
            whisperMode={pendingSettings?.whisper_mode || 'local'}
            whisperModel={getWhisperModelName(pendingSettings?.whisper_mode, pendingSettings?.whisper_server_model, pendingSettings?.model)}
            sessionNotes={sessionNotes}
            onSessionNotesChange={setSessionNotes}
            isStopping={isStopping}
            silenceWarning={silenceWarning}
            chatMessages={chatMessages}
            chatIsLoading={chatIsLoading}
            chatError={chatError}
            onChatSendMessage={chatSendMessage}
            onChatClear={clearChat}
            predictiveHint={predictiveHint}
            predictiveHintLoading={predictiveHintLoading}
            // MIIS image suggestions
            miisSuggestions={miisSuggestions}
            miisLoading={miisLoading}
            miisError={miisError}
            miisEnabled={settings?.miis_enabled ?? false}
            onMiisImpression={miisRecordImpression}
            onMiisClick={miisRecordClick}
            onMiisDismiss={miisRecordDismiss}
            miisGetImageUrl={miisGetImageUrl}
            autoEndEnabled={pendingSettings?.auto_end_enabled ?? true}
            onAutoEndToggle={handleAutoEndToggle}
            onStop={handleStop}
            onCancelAutoEnd={() => invoke('reset_silence_timer').catch(console.error)}
          />
        )}

        {uiMode === 'review' && (
          <ReviewMode
            elapsedMs={status.elapsed_ms || localElapsedMs}
            audioQuality={audioQuality}
            originalTranscript={transcript.finalized_text}
            editedTranscript={editedTranscript}
            onTranscriptEdit={setEditedTranscript}
            soapResult={soapResult}
            isGeneratingSoap={isGeneratingSoap}
            soapError={soapError}
            llmConnected={ollamaStatus?.connected || false}
            onGenerateSoap={handleGenerateSoap}
            soapOptions={soapOptions}
            onSoapDetailLevelChange={updateSoapDetailLevel}
            onSoapFormatChange={updateSoapFormat}
            onSoapCustomInstructionsChange={updateSoapCustomInstructions}
            biomarkers={biomarkers}
            whisperMode={pendingSettings?.whisper_mode || 'local'}
            whisperModel={getWhisperModelName(pendingSettings?.whisper_mode, pendingSettings?.whisper_server_model, pendingSettings?.model)}
            authState={authState}
            isSyncing={isSyncing}
            syncSuccess={syncSuccess}
            syncError={syncError}
            syncedEncounter={syncedEncounter}
            isAddingSoap={isAddingSoap}
            onClearSyncError={() => setSyncError(null)}
            onNewSession={handleReset}
            onLogin={medplumLogin}
            onCancelLogin={medplumCancelLogin}
            authLoading={authLoading}
            autoSyncEnabled={pendingSettings?.medplum_auto_sync || false}
          />
        )}
      </ErrorBoundary>

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
        whisperServerStatus={whisperServerStatus}
        whisperServerModels={whisperServerModels}
        onTestWhisperServer={handleTestWhisperServer}
        llmStatus={ollamaStatus}
        llmModels={ollamaModels}
        onTestLLM={handleTestLLM}
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
