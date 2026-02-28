import { useState, useEffect, useCallback, useMemo } from 'react';
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
  ContinuousMode,
} from './components';
import type { SyncStatus } from './components';
import {
  useSessionState,
  useSoapNote,
  useMedplumSync,
  useSettings,
  useDevices,
  useChecklist,
  useAutoDetection,
  useClinicalChat,
  useSessionLifecycle,
  usePredictiveHint,
  useMiisImages,
  useAiImages,
  useScreenCapture,
  useContinuousModeOrchestrator,
  useConnectionTests,
} from './hooks';
import type { Settings, ChartingMode } from './types';

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
    setOllamaModels,
    soapOptions,
    setSoapError,
    generateSoapNote,
    generateVisionSoapNote,
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

  // Connection tests composite hook (LLM, Medplum init, Whisper server)
  const {
    whisperServerStatus,
    whisperServerModels,
    handleTestLLM,
    handleTestMedplum,
    handleTestWhisperServer,
  } = useConnectionTests({
    settings,
    pendingSettings,
    setOllamaStatus,
    setOllamaModels,
    setMedplumConnected,
    setMedplumError,
  });

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

  // Centralized session lifecycle coordination
  const {
    sessionNotes,
    setSessionNotes,
    startSession: lifecycleStartSession,
    startSessionAutoDetect,
    confirmAutoStart,
    handleGreetingRejected: lifecycleGreetingRejected,
    resetSession: lifecycleResetSession,
  } = useSessionLifecycle({
    sessionStart: sessionStart,
    sessionReset: sessionReset,
    resetSyncState,
    clearChat,
    clearSoapError: () => setSoapError(null),
  });

  // Derive charting mode from settings
  const chartingMode: ChartingMode = settings?.charting_mode || 'session';
  const isContinuousMode = chartingMode === 'continuous';

  // Continuous mode orchestrator (groups useContinuousMode, usePatientBiomarkers,
  // usePredictiveHint, useMiisImages, and related state)
  const continuous = useContinuousModeOrchestrator({ settings });

  // Auto-detection callbacks — delegate to lifecycle hook for coordinated resets
  const handleAutoStartRecording = useCallback(async () => {
    console.log('Auto-detection: Starting recording immediately (optimistic)');
    const device = pendingSettings?.device === 'default' ? null : pendingSettings?.device ?? null;
    await startSessionAutoDetect(device);
  }, [pendingSettings?.device, startSessionAutoDetect]);

  const handleGreetingConfirmed = useCallback((transcript: string, confidence: number) => {
    console.log(`Greeting confirmed: "${transcript}" (confidence: ${confidence.toFixed(2)})`);
    confirmAutoStart();
  }, [confirmAutoStart]);

  const handleGreetingRejected = useCallback(async (reason: string) => {
    console.log(`Greeting rejected: ${reason}`);
    const didReset = await lifecycleGreetingRejected();
    if (didReset) {
      console.log('Session still pending confirmation - aborted recording');
    } else {
      console.log('Session already confirmed or user-initiated - keeping recording');
    }
  }, [lifecycleGreetingRejected]);

  // Auto-detection from hook
  const {
    isListening,
    isPendingConfirmation: _isPendingConfirmation, // Available for future UI indicators
    listeningStatus,
    error: listeningError,
    startListening,
    stopListening,
  } = useAutoDetection(
    !isContinuousMode && (pendingSettings?.auto_start_enabled ?? false),
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

  // Sync indicator dismissed state (for hiding after user dismisses)
  const [syncDismissed, setSyncDismissed] = useState(false);

  // Derive sync status for header indicator
  const syncStatus: SyncStatus = useMemo(() => {
    if (syncDismissed) return 'idle';
    if (isSyncing || isAddingSoap) return 'syncing';
    if (syncError) return 'error';
    if (syncSuccess) return 'success';
    return 'idle';
  }, [syncDismissed, isSyncing, isAddingSoap, syncError, syncSuccess]);

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

  // Determine current UI mode based on session state
  const uiMode: UIMode = useMemo(() => {
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
  }, [status.state]);

  // Predictive hints and image concepts during recording
  const {
    hint: predictiveHint,
    concepts: imageConcepts,
    imagePrompt,
    isLoading: predictiveHintLoading,
  } = usePredictiveHint({
    transcript: transcript.finalized_text,
    isRecording: uiMode === 'recording',
  });

  const imageSource = (settings?.image_source ?? 'off') as 'off' | 'miis' | 'ai';

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
    enabled: imageSource === 'miis',
    serverUrl: settings?.miis_server_url ?? '',
  });

  // AI-generated image suggestions (uses image_prompt from predictive hints)
  const {
    images: aiImages,
    isLoading: aiLoading,
    error: aiError,
    dismissImage: aiDismiss,
  } = useAiImages({
    imagePrompt,
    enabled: imageSource === 'ai',
    sessionId: status.session_id ?? null,
  });

  // Screen capture tied to recording lifecycle
  useScreenCapture(isRecording, settings);

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
    if (uiMode === 'ready' && autoStartEnabled && !isListening && !isContinuousMode) {
      startListening(deviceId);
    }

    // Stop listening when:
    // 1. Not in ready mode (recording started)
    // 2. OR auto-start is disabled
    // 3. OR continuous mode is active
    if ((uiMode !== 'ready' || !autoStartEnabled || isContinuousMode) && isListening) {
      stopListening();
    }
  }, [uiMode, pendingSettings?.auto_start_enabled, pendingSettings?.device, isListening, isContinuousMode, startListening, stopListening]);

  // Handle start recording with permission check
  const handleStart = useCallback(async () => {
    const permStatus = await checkMicrophonePermission();
    if (!permStatus.authorized) {
      setPermissionError(permStatus.message);
      return;
    }

    setPermissionError(null);
    const device = pendingSettings?.device === 'default' ? null : pendingSettings?.device ?? null;
    await lifecycleStartSession(device);
  }, [pendingSettings?.device, lifecycleStartSession, checkMicrophonePermission]);

  // Handle reset/new session — lifecycle hook coordinates all resets
  const handleReset = useCallback(async () => {
    await lifecycleResetSession();
  }, [lifecycleResetSession]);

  // SettingsDrawer handlers
  const handleSaveSettings = useCallback(async () => {
    // Prevent switching from continuous to session mode while continuous recording is active
    if (
      continuous.isActive &&
      settings?.charting_mode === 'continuous' &&
      pendingSettings?.charting_mode === 'session'
    ) {
      alert('Cannot switch charting mode while continuous recording is active. Please stop recording first.');
      return;
    }

    const success = await saveSettings();
    if (success) {
      setShowSettings(false);
    }
  }, [saveSettings, continuous.isActive, settings?.charting_mode, pendingSettings?.charting_mode]);

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

  // Generate Vision SOAP note (experimental — uses transcript + screenshots)
  const handleGenerateVisionSoap = useCallback(async (imagePath: string) => {
    const optionsWithNotes = sessionNotes.trim()
      ? { ...soapOptions, session_notes: sessionNotes }
      : soapOptions;
    const result = await generateVisionSoapNote(editedTranscript, biomarkers?.recent_events, optionsWithNotes, status.session_id, imagePath);
    if (result) {
      // Wrap as MultiPatientSoapResult so ReviewMode can display it
      setSoapResult({
        notes: [{ patient_label: 'Vision', speaker_id: 'All', content: result.content }],
        physician_speaker: null,
        generated_at: result.generated_at,
        model_used: result.model_used,
      });
    }
  }, [
    editedTranscript,
    biomarkers?.recent_events,
    generateVisionSoapNote,
    setSoapResult,
    status.session_id,
    sessionNotes,
    soapOptions,
  ]);

  // Screen capture screenshot count for UI
  const [screenshotCount, setScreenshotCount] = useState(0);
  useEffect(() => {
    if (uiMode !== 'review') return;
    let cancelled = false;
    invoke<{ running: boolean; screenshot_count: number }>('get_screen_capture_status')
      .then(s => { if (!cancelled) setScreenshotCount(s.screenshot_count); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [uiMode]);

  // Auto-generate SOAP note when session completes (if Ollama is connected)
  // Disabled in continuous mode — encounter detector handles SOAP generation
  useEffect(() => {
    if (isContinuousMode) return;
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
    isContinuousMode,
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

  // Status dot class for header
  const statusDotClass = useMemo(() => {
    if (isRecording) return 'recording';
    if (isStopping) return 'stopping';
    if (status.state === 'preparing') return 'preparing';
    if (isIdle) return 'idle';
    return '';
  }, [isRecording, isStopping, status.state, isIdle]);

  return (
    <div className={`sidebar mode-${uiMode}`}>
      {/* Header - always visible */}
      <Header
        statusDotClass={statusDotClass}
        showSettings={showSettings}
        disabled={isRecording || isStopping}
        onHistoryClick={openHistoryWindow}
        onSettingsClick={() => setShowSettings(!showSettings)}
        syncStatus={syncStatus}
        syncError={syncError}
        onDismissSync={handleDismissSync}
      />

      {/* Mode-based content wrapped in ErrorBoundary */}
      <ErrorBoundary>
        {/* Continuous charting mode */}
        {isContinuousMode && (
          <ContinuousMode
            isActive={continuous.isActive}
            isStopping={continuous.isStopping}
            stats={continuous.stats}
            liveTranscript={continuous.liveTranscript}
            error={continuous.error}
            predictiveHint={continuous.predictiveHint}
            predictiveHintLoading={continuous.predictiveHintLoading}
            audioQuality={continuous.audioQuality}
            biomarkers={continuous.biomarkers}
            biomarkerTrends={continuous.biomarkerTrends}
            encounterNotes={continuous.encounterNotes}
            onEncounterNotesChange={continuous.onEncounterNotesChange}
            // Image suggestions (MIIS or AI)
            miisSuggestions={continuous.miisSuggestions}
            miisLoading={continuous.miisLoading}
            miisError={continuous.miisError}
            miisEnabled={continuous.miisEnabled}
            onMiisImpression={continuous.onMiisImpression}
            onMiisClick={continuous.onMiisClick}
            onMiisDismiss={continuous.onMiisDismiss}
            miisGetImageUrl={continuous.miisGetImageUrl}
            aiImages={continuous.aiImages}
            aiLoading={continuous.aiLoading}
            aiError={continuous.aiError}
            onAiDismiss={continuous.onAiDismiss}
            imageSource={continuous.imageSource}
            onStart={continuous.onStart}
            onStop={continuous.onStop}
            onNewPatient={continuous.onNewPatient}
            onViewHistory={openHistoryWindow}
          />
        )}

        {/* Session-based mode (original flow) */}
        {!isContinuousMode && uiMode === 'ready' && (
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

        {!isContinuousMode && uiMode === 'recording' && (
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
            // Image suggestions (MIIS or AI)
            miisSuggestions={miisSuggestions}
            miisLoading={miisLoading}
            miisError={miisError}
            miisEnabled={imageSource !== 'off'}
            onMiisImpression={miisRecordImpression}
            onMiisClick={miisRecordClick}
            onMiisDismiss={miisRecordDismiss}
            miisGetImageUrl={miisGetImageUrl}
            aiImages={aiImages}
            aiLoading={aiLoading}
            aiError={aiError}
            onAiDismiss={aiDismiss}
            imageSource={imageSource}
            autoEndEnabled={pendingSettings?.auto_end_enabled ?? true}
            onAutoEndToggle={handleAutoEndToggle}
            onStop={handleStop}
            onCancelAutoEnd={() => invoke('reset_silence_timer').catch(console.error)}
          />
        )}

        {!isContinuousMode && uiMode === 'review' && (
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
            onGenerateVisionSoap={handleGenerateVisionSoap}
            screenshotCount={screenshotCount}
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
