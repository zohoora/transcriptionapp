use crate::activity_log;
use crate::audio;
use crate::checklist::{self, ChecklistResult};
use crate::config::{Config, ModelStatus, Settings};
use crate::medplum::{
    AuthState, AuthUrl, Encounter, EncounterDetails, EncounterSummary, MedplumClient,
    Patient, SyncResult, SyncStatus,
};
use crate::models::{self, ModelInfo, WhisperModel};
use crate::ollama::{OllamaClient, OllamaStatus, SoapNote};
use crate::pipeline::{start_pipeline, PipelineConfig, PipelineHandle, PipelineMessage};
use crate::session::{SessionError, SessionManager};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info};

/// Device information for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// State for the running pipeline
#[derive(Default)]
pub struct PipelineState {
    pub handle: Option<PipelineHandle>,
    /// Generation counter to detect stale pipeline messages after reset
    pub generation: u64,
}


impl PipelineState {
    /// Increment generation and return the new value
    pub fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }
}

/// Shared session manager type for use in async contexts
pub type SharedSessionManager = Arc<Mutex<SessionManager>>;

/// Shared pipeline state type for use in async contexts
pub type SharedPipelineState = Arc<Mutex<PipelineState>>;

/// Shared Medplum client for EMR integration
pub type SharedMedplumClient = Arc<RwLock<Option<MedplumClient>>>;

/// Create a shared Medplum client from config
pub fn create_medplum_client() -> SharedMedplumClient {
    Arc::new(RwLock::new(None))
}

/// List available input devices
#[tauri::command]
pub fn list_input_devices() -> Result<Vec<Device>, String> {
    match audio::list_input_devices() {
        Ok(devices) => Ok(devices
            .into_iter()
            .map(|d| Device {
                id: d.id,
                name: d.name,
                is_default: d.is_default,
            })
            .collect()),
        Err(e) => {
            error!("Failed to list devices: {}", e);
            Err(e.to_string())
        }
    }
}

/// Get current settings
#[tauri::command]
pub fn get_settings() -> Result<Settings, String> {
    let config = Config::load_or_default();
    Ok(config.to_settings())
}

/// Update settings
#[tauri::command]
pub fn set_settings(settings: Settings) -> Result<Settings, String> {
    // Validate settings before saving
    let validation_errors = settings.validate();
    if !validation_errors.is_empty() {
        let error_messages: Vec<String> = validation_errors.iter().map(|e| e.to_string()).collect();
        return Err(format!("Invalid settings: {}", error_messages.join("; ")));
    }

    let mut config = Config::load_or_default();
    config.update_from_settings(&settings);
    config.save().map_err(|e| e.to_string())?;
    Ok(config.to_settings())
}

/// Start a transcription session
#[tauri::command]
pub async fn start_session(
    app: AppHandle,
    session_state: State<'_, SharedSessionManager>,
    pipeline_state: State<'_, SharedPipelineState>,
    device_id: Option<String>,
) -> Result<(), String> {
    info!("Starting session with device: {:?}", device_id);

    // Clone the Arcs for use in async context
    let session_arc = session_state.inner().clone();
    let pipeline_arc = pipeline_state.inner().clone();

    // Transition to preparing
    {
        let mut session = session_arc.lock().map_err(|e| e.to_string())?;
        session.start_preparing().map_err(|e| e.to_string())?;
    }

    // Emit initial status
    emit_status_arc(&app, &session_arc)?;

    // Check model availability
    let config = Config::load_or_default();
    let model_path = match config.get_model_path() {
        Ok(path) => {
            if !path.exists() {
                let mut session = session_arc.lock().map_err(|e| e.to_string())?;
                session.set_error(SessionError::ModelNotFound(format!(
                    "Model not found at {:?}",
                    path
                )));
                drop(session);
                emit_status_arc(&app, &session_arc)?;
                return Err("Model not found".to_string());
            }
            path
        }
        Err(e) => {
            let mut session = session_arc.lock().map_err(|e| e.to_string())?;
            session.set_error(SessionError::ModelNotFound(e.to_string()));
            drop(session);
            emit_status_arc(&app, &session_arc)?;
            return Err(e.to_string());
        }
    };

    // Create pipeline config
    let diarization_model_path = if config.diarization_enabled {
        config.get_diarization_model_path().ok()
    } else {
        None
    };

    let enhancement_model_path = if config.enhancement_enabled {
        config.get_enhancement_model_path().ok()
    } else {
        None
    };

    let emotion_model_path = if config.emotion_enabled {
        config.get_emotion_model_path().ok()
    } else {
        None
    };

    let yamnet_model_path = if config.biomarkers_enabled {
        config.get_yamnet_model_path().ok()
    } else {
        None
    };

    // Generate audio file path for recording
    let audio_output_path = {
        let recordings_dir = config.get_recordings_dir();
        if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
            info!("Could not create recordings directory: {}, audio won't be saved", e);
            None
        } else {
            let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
            let audio_path = recordings_dir.join(format!("session_{}.wav", timestamp));
            Some(audio_path)
        }
    };

    // Store audio path in session
    if let Some(ref path) = audio_output_path {
        let mut session = session_arc.lock().map_err(|e| e.to_string())?;
        session.set_audio_file_path(path.clone());
    }

    // Prepare device ID for logging before moving into pipeline_config
    let device_id_for_config = if device_id.as_deref() == Some("default") {
        None
    } else {
        device_id
    };
    let device_name_for_log = device_id_for_config.clone();

    let pipeline_config = PipelineConfig {
        device_id: device_id_for_config,
        model_path,
        language: config.language.clone(),
        vad_threshold: config.vad_threshold,
        silence_to_flush_ms: config.silence_to_flush_ms,
        max_utterance_ms: config.max_utterance_ms,
        n_threads: 4,
        diarization_enabled: config.diarization_enabled,
        diarization_model_path,
        speaker_similarity_threshold: config.speaker_similarity_threshold,
        max_speakers: config.max_speakers,
        enhancement_enabled: config.enhancement_enabled,
        enhancement_model_path,
        emotion_enabled: config.emotion_enabled,
        emotion_model_path,
        biomarkers_enabled: config.biomarkers_enabled,
        yamnet_model_path,
        audio_output_path,
    };

    // Create message channel
    let (tx, mut rx) = mpsc::channel::<PipelineMessage>(32);

    // Start the pipeline
    let handle = match start_pipeline(pipeline_config, tx) {
        Ok(h) => h,
        Err(e) => {
            error!("Failed to start pipeline: {}", e);
            let mut session = session_arc.lock().map_err(|e| e.to_string())?;
            session.set_error(SessionError::AudioDeviceError(e.to_string()));
            drop(session);
            emit_status_arc(&app, &session_arc)?;
            return Err(e.to_string());
        }
    };

    // Store the pipeline handle and get generation for this pipeline instance
    let expected_generation = {
        let mut ps = pipeline_arc.lock().map_err(|e| e.to_string())?;
        ps.handle = Some(handle);
        ps.next_generation()
    };

    // Transition to recording and get session ID for logging
    let session_id = {
        let mut session = session_arc.lock().map_err(|e| e.to_string())?;
        session.start_recording("whisper");
        // Use the session's ID (generated in start_preparing) for log correlation
        session.session_id().unwrap_or("unknown").to_string()
    };

    // Log session start (no PHI - just IDs and metadata)
    activity_log::log_session_start(
        &session_id,
        device_name_for_log.as_deref(),
        &config.whisper_model,
    );

    emit_status_arc(&app, &session_arc)?;

    // Spawn task to handle pipeline messages
    let app_clone = app.clone();
    let session_clone = session_arc.clone();
    let pipeline_clone = pipeline_arc.clone();

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // Check if this pipeline instance is still current
            // If generation has changed (due to reset), discard messages
            let current_generation = match pipeline_clone.lock() {
                Ok(ps) => ps.generation,
                Err(_) => break, // Poisoned lock, exit
            };
            if current_generation != expected_generation {
                info!("Discarding stale pipeline message (generation {} != {})", expected_generation, current_generation);
                continue;
            }

            match msg {
                PipelineMessage::Segment(segment) => {
                    // Log segment metadata only - no transcript text (PHI)
                    info!("Received segment: {} words ({}ms - {}ms)", segment.text.split_whitespace().count(), segment.start_ms, segment.end_ms);
                    let mut session = match session_clone.lock() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    session.add_segment(segment);
                    drop(session);

                    // Emit transcript update
                    if let Ok(session) = session_clone.lock() {
                        let transcript = session.transcript_update();
                        info!("Emitting transcript_update: {} chars", transcript.finalized_text.len());
                        let _ = app_clone.emit("transcript_update", transcript);
                    }
                }
                PipelineMessage::Status {
                    audio_clock_ms: _,
                    pending_count,
                    is_speech_active: _,
                } => {
                    if let Ok(mut session) = session_clone.lock() {
                        session.set_pending_count(pending_count);
                        let status = session.status();
                        let _ = app_clone.emit("session_status", status);
                    }
                }
                PipelineMessage::Biomarker(update) => {
                    // Emit biomarker update to frontend
                    let _ = app_clone.emit("biomarker_update", update);
                }
                PipelineMessage::AudioQuality(snapshot) => {
                    // Emit audio quality update to frontend
                    let _ = app_clone.emit("audio_quality", snapshot);
                }
                PipelineMessage::Stopped => {
                    info!("Pipeline stopped message received");
                    break;
                }
                PipelineMessage::Error(e) => {
                    error!("Pipeline error: {}", e);
                    if let Ok(mut session) = session_clone.lock() {
                        session.set_error(SessionError::TranscriptionError(e));
                        let status = session.status();
                        let _ = app_clone.emit("session_status", status);
                    }
                    break;
                }
            }
        }
    });

    Ok(())
}

/// Stop the current transcription session
#[tauri::command]
pub async fn stop_session(
    app: AppHandle,
    session_state: State<'_, SharedSessionManager>,
    pipeline_state: State<'_, SharedPipelineState>,
) -> Result<(), String> {
    info!("Stopping session");

    // Clone the Arcs for use in async context
    let session_arc = session_state.inner().clone();
    let pipeline_arc = pipeline_state.inner().clone();

    // Get session info for logging before stopping
    let (session_id, elapsed_ms, segment_count) = {
        let session = session_arc.lock().map_err(|e| e.to_string())?;
        let status = session.status();
        (
            // Reuse the session's ID for log correlation (same ID as start)
            session.session_id().unwrap_or("unknown").to_string(),
            status.elapsed_ms,
            session.segments().len(),
        )
    };

    // Transition to stopping
    {
        let mut session = session_arc.lock().map_err(|e| e.to_string())?;
        session.start_stopping().map_err(|e| e.to_string())?;
    }

    emit_status_arc(&app, &session_arc)?;

    // Stop the pipeline
    let handle = {
        let mut ps = pipeline_arc.lock().map_err(|e| e.to_string())?;
        ps.handle.take()
    };

    // Get audio file size if available
    let audio_file_size = {
        let session = session_arc.lock().map_err(|e| e.to_string())?;
        session.audio_file_path().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len())
    };

    if let Some(h) = handle {
        h.stop();

        // Wait for pipeline to finish in a separate task
        let app_clone = app.clone();
        let session_clone = session_arc.clone();
        let session_id_for_log = session_id.clone();

        tokio::task::spawn_blocking(move || {
            h.join();

            // Transition to completed
            if let Ok(mut session) = session_clone.lock() {
                session.complete();
                let status = session.status();
                let transcript = session.transcript_update();
                let _ = app_clone.emit("session_status", status.clone());
                let _ = app_clone.emit("transcript_update", transcript);

                // Log session stop (no PHI - just metrics)
                activity_log::log_session_stop(
                    &session_id_for_log,
                    status.elapsed_ms,
                    session.segments().len(),
                    session.audio_file_path().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()),
                );
            }
        });
    } else {
        // No pipeline running, just complete
        {
            let mut session = session_arc.lock().map_err(|e| e.to_string())?;
            session.complete();
        }

        // Log session stop
        activity_log::log_session_stop(&session_id, elapsed_ms, segment_count, audio_file_size);

        emit_status_arc(&app, &session_arc)?;
        emit_transcript_arc(&app, &session_arc)?;
    }

    Ok(())
}

/// Reset the session to idle
#[tauri::command]
pub fn reset_session(
    app: AppHandle,
    session_state: State<'_, SharedSessionManager>,
    pipeline_state: State<'_, SharedPipelineState>,
) -> Result<(), String> {
    info!("Resetting session");

    // Generate session ID for logging
    let session_id = Some(uuid::Uuid::new_v4().to_string());

    // Stop any running pipeline and increment generation
    // The generation increment ensures any in-flight pipeline messages are discarded
    {
        let mut ps = pipeline_state.lock().map_err(|e| e.to_string())?;
        // Increment generation first so receiver task will discard any pending messages
        ps.next_generation();
        if let Some(h) = ps.handle.take() {
            h.stop();
            // Note: We don't join here because it would block the main thread.
            // The receiver task will exit when it sees the Stopped message or
            // when it notices the generation has changed.
        }
    }

    // Reset session
    {
        let mut session = session_state.lock().map_err(|e| e.to_string())?;
        session.reset();
    }

    // Log session reset
    activity_log::log_session_reset(session_id.as_deref());

    emit_status_arc(&app, session_state.inner())?;
    emit_transcript_arc(&app, session_state.inner())?;

    Ok(())
}

/// Get the audio file path for the current session
#[tauri::command]
pub fn get_audio_file_path(
    session_state: State<'_, SharedSessionManager>,
) -> Result<Option<String>, String> {
    let session = session_state.lock().map_err(|e| e.to_string())?;
    Ok(session.audio_file_path().map(|p| p.to_string_lossy().to_string()))
}

/// Check if the Whisper model is available
#[tauri::command]
pub fn check_model_status() -> ModelStatus {
    check_model_status_internal()
}

fn check_model_status_internal() -> ModelStatus {
    let config = Config::load_or_default();

    match config.get_model_path() {
        Ok(path) => {
            if path.exists() {
                ModelStatus {
                    available: true,
                    path: Some(path.to_string_lossy().to_string()),
                    error: None,
                }
            } else {
                ModelStatus {
                    available: false,
                    path: Some(path.to_string_lossy().to_string()),
                    error: Some(format!("Model file not found at {:?}", path)),
                }
            }
        }
        Err(e) => ModelStatus {
            available: false,
            path: None,
            error: Some(e.to_string()),
        },
    }
}

/// Get information about all required models
#[tauri::command]
pub fn get_model_info() -> Vec<ModelInfo> {
    let config = Config::load_or_default();
    models::get_model_info(&config.whisper_model)
}

/// Download a Whisper model
#[tauri::command]
pub async fn download_whisper_model(model_name: String) -> Result<String, String> {
    info!("Downloading Whisper model: {}", model_name);

    let model = WhisperModel::from_name(&model_name)
        .ok_or_else(|| format!("Invalid model name: {}", model_name))?;

    // Run download in blocking task to not block async runtime
    let result = tokio::task::spawn_blocking(move || {
        models::ensure_whisper_model(model)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Ok(path) => {
            info!("Whisper model downloaded to: {:?}", path);
            Ok(path.to_string_lossy().to_string())
        }
        Err(e) => {
            error!("Failed to download Whisper model: {}", e);
            Err(e.to_string())
        }
    }
}

/// Download the speaker embedding model for diarization
#[tauri::command]
pub async fn download_speaker_model() -> Result<String, String> {
    info!("Downloading speaker embedding model");

    // Run download in blocking task to not block async runtime
    let result = tokio::task::spawn_blocking(|| {
        models::ensure_speaker_model()
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Ok(path) => {
            info!("Speaker model downloaded to: {:?}", path);
            Ok(path.to_string_lossy().to_string())
        }
        Err(e) => {
            error!("Failed to download speaker model: {}", e);
            Err(e.to_string())
        }
    }
}

/// Ensure all required models are downloaded
#[tauri::command]
pub async fn ensure_models() -> Result<(), String> {
    let config = Config::load_or_default();
    let whisper_model = config.whisper_model.clone();
    let need_diarization = config.diarization_enabled;

    info!(
        "Ensuring models are available: whisper={}, diarization={}",
        whisper_model, need_diarization
    );

    // Run download in blocking task to not block async runtime
    let result = tokio::task::spawn_blocking(move || {
        models::ensure_all_models(&whisper_model, need_diarization)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Ok(()) => {
            info!("All required models are available");
            Ok(())
        }
        Err(e) => {
            error!("Failed to ensure models: {}", e);
            Err(e.to_string())
        }
    }
}

/// Run the launch sequence checklist
///
/// This verifies all requirements before starting a recording session:
/// - Audio input devices available
/// - Required models downloaded
/// - Configuration valid
#[tauri::command]
pub fn run_checklist() -> ChecklistResult {
    info!("Running launch sequence checklist");
    let config = Config::load_or_default();
    let result = checklist::run_all_checks(&config);

    // Log checklist result (counts only, no content)
    let (passed, failed, warnings) = result.checks.iter().fold((0, 0, 0), |(p, f, w), check| {
        match check.status {
            checklist::CheckStatus::Pass => (p + 1, f, w),
            checklist::CheckStatus::Fail => (p, f + 1, w),
            checklist::CheckStatus::Warning => (p, f, w + 1),
            checklist::CheckStatus::Skipped | checklist::CheckStatus::Pending => (p, f, w),
        }
    });
    activity_log::log_checklist_result(result.checks.len(), passed, failed, warnings);

    result
}

/// Download the speech enhancement model
#[tauri::command]
pub async fn download_enhancement_model() -> Result<String, String> {
    info!("Downloading speech enhancement model");

    let result = tokio::task::spawn_blocking(|| {
        models::ensure_enhancement_model()
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Ok(path) => {
            info!("Enhancement model downloaded to: {:?}", path);
            Ok(path.to_string_lossy().to_string())
        }
        Err(e) => {
            error!("Failed to download enhancement model: {}", e);
            Err(e.to_string())
        }
    }
}

/// Download the emotion detection model
#[tauri::command]
pub async fn download_emotion_model() -> Result<String, String> {
    info!("Downloading emotion detection model");

    let result = tokio::task::spawn_blocking(|| {
        models::ensure_emotion_model()
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Ok(path) => {
            info!("Emotion model downloaded to: {:?}", path);
            Ok(path.to_string_lossy().to_string())
        }
        Err(e) => {
            error!("Failed to download emotion model: {}", e);
            Err(e.to_string())
        }
    }
}

/// Download the YAMNet audio classification model (for cough detection)
#[tauri::command]
pub async fn download_yamnet_model() -> Result<String, String> {
    info!("Downloading YAMNet audio classification model");

    let result = tokio::task::spawn_blocking(|| {
        models::ensure_yamnet_model()
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Ok(path) => {
            info!("YAMNet model downloaded to: {:?}", path);
            Ok(path.to_string_lossy().to_string())
        }
        Err(e) => {
            error!("Failed to download YAMNet model: {}", e);
            Err(e.to_string())
        }
    }
}

fn emit_status_arc(
    app: &AppHandle,
    session_state: &SharedSessionManager,
) -> Result<(), String> {
    let status = {
        let session = session_state.lock().map_err(|e| e.to_string())?;
        session.status()
    };
    app.emit("session_status", status).map_err(|e| e.to_string())
}

fn emit_transcript_arc(
    app: &AppHandle,
    session_state: &SharedSessionManager,
) -> Result<(), String> {
    let transcript = {
        let session = session_state.lock().map_err(|e| e.to_string())?;
        session.transcript_update()
    };
    app.emit("transcript_update", transcript).map_err(|e| e.to_string())
}

// ============================================================================
// Ollama / SOAP Note Commands
// ============================================================================

/// Check Ollama server status and list available models
#[tauri::command]
pub async fn check_ollama_status() -> OllamaStatus {
    let config = Config::load_or_default();
    let client = match OllamaClient::new(&config.ollama_server_url) {
        Ok(c) => c,
        Err(e) => {
            return OllamaStatus {
                connected: false,
                available_models: vec![],
                error: Some(e),
            }
        }
    };
    client.check_status().await
}

/// List available models from Ollama server
#[tauri::command]
pub async fn list_ollama_models() -> Result<Vec<String>, String> {
    let config = Config::load_or_default();
    let client = OllamaClient::new(&config.ollama_server_url)?;
    client.list_models().await
}

/// Generate a SOAP note from the given transcript
#[tauri::command]
pub async fn generate_soap_note(transcript: String) -> Result<SoapNote, String> {
    info!("Generating SOAP note for transcript of {} chars", transcript.len());

    if transcript.trim().is_empty() {
        return Err("Cannot generate SOAP note from empty transcript".to_string());
    }

    let config = Config::load_or_default();
    let client = OllamaClient::new(&config.ollama_server_url)?;

    // Count words for logging (not content)
    let word_count = transcript.split_whitespace().count();
    let start_time = std::time::Instant::now();

    match client
        .generate_soap_note(&config.ollama_model, &transcript)
        .await
    {
        Ok(soap_note) => {
            let generation_time_ms = start_time.elapsed().as_millis() as u64;
            activity_log::log_soap_generation(
                "", // session_id not available here
                word_count,
                generation_time_ms,
                &config.ollama_model,
                true,
                None,
            );
            Ok(soap_note)
        }
        Err(e) => {
            let generation_time_ms = start_time.elapsed().as_millis() as u64;
            activity_log::log_soap_generation(
                "",
                word_count,
                generation_time_ms,
                &config.ollama_model,
                false,
                Some(&e),
            );
            Err(e)
        }
    }
}

// ============================================================================
// Medplum EMR Commands
// ============================================================================

/// Get or create the Medplum client, initializing if needed
async fn get_or_create_medplum_client(
    medplum_state: &SharedMedplumClient,
) -> Result<(), String> {
    let mut client_guard = medplum_state.write().await;
    if client_guard.is_none() {
        let config = Config::load_or_default();
        if config.medplum_client_id.is_empty() {
            return Err("Medplum client ID not configured. Please set it in settings.".to_string());
        }
        let client = MedplumClient::new(&config.medplum_server_url, &config.medplum_client_id)
            .map_err(|e| e.to_string())?;
        *client_guard = Some(client);
    }
    Ok(())
}

/// Get the current Medplum authentication state
#[tauri::command]
pub async fn medplum_get_auth_state(
    medplum_state: State<'_, SharedMedplumClient>,
) -> Result<AuthState, String> {
    let client_guard = medplum_state.read().await;
    if let Some(ref client) = *client_guard {
        Ok(client.get_auth_state().await)
    } else {
        Ok(AuthState::default())
    }
}

/// Try to restore a previous Medplum session (auto-refresh if needed)
/// Call this on app startup to check if user is already logged in
#[tauri::command]
pub async fn medplum_try_restore_session(
    medplum_state: State<'_, SharedMedplumClient>,
) -> Result<AuthState, String> {
    info!("Attempting to restore Medplum session...");

    get_or_create_medplum_client(medplum_state.inner()).await?;

    let client_guard = medplum_state.read().await;
    if let Some(ref client) = *client_guard {
        let auth_state = client.try_restore_session().await;
        activity_log::log_medplum_auth(
            "restore",
            auth_state.practitioner_id.as_deref(),
            auth_state.is_authenticated,
            None,
        );
        Ok(auth_state)
    } else {
        activity_log::log_medplum_auth("restore", None, false, Some("Client not initialized"));
        Ok(AuthState::default())
    }
}

/// Start the Medplum OAuth authorization flow
/// Returns the authorization URL to open in a browser
#[tauri::command]
pub async fn medplum_start_auth(
    medplum_state: State<'_, SharedMedplumClient>,
) -> Result<AuthUrl, String> {
    info!("Starting Medplum OAuth flow");

    activity_log::log_medplum_auth("login_start", None, true, None);

    get_or_create_medplum_client(medplum_state.inner()).await?;

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client.start_auth_flow().await.map_err(|e| {
        activity_log::log_medplum_auth("login_start", None, false, Some(&e.to_string()));
        e.to_string()
    })
}

/// Handle OAuth callback with authorization code
#[tauri::command]
pub async fn medplum_handle_callback(
    medplum_state: State<'_, SharedMedplumClient>,
    code: String,
    state: String,
) -> Result<AuthState, String> {
    info!("Handling Medplum OAuth callback");

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    match client.exchange_code(&code, &state).await {
        Ok(auth_state) => {
            activity_log::log_medplum_auth(
                "login_complete",
                auth_state.practitioner_id.as_deref(),
                true,
                None,
            );
            Ok(auth_state)
        }
        Err(e) => {
            activity_log::log_medplum_auth("login_complete", None, false, Some(&e.to_string()));
            Err(e.to_string())
        }
    }
}

/// Logout from Medplum
#[tauri::command]
pub async fn medplum_logout(
    medplum_state: State<'_, SharedMedplumClient>,
) -> Result<(), String> {
    info!("Logging out from Medplum");

    let client_guard = medplum_state.read().await;
    if let Some(ref client) = *client_guard {
        client.logout().await;
        activity_log::log_medplum_auth("logout", None, true, None);
    }
    Ok(())
}

/// Refresh the Medplum access token
#[tauri::command]
pub async fn medplum_refresh_token(
    medplum_state: State<'_, SharedMedplumClient>,
) -> Result<AuthState, String> {
    info!("Refreshing Medplum access token");

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    match client.refresh_token().await {
        Ok(auth_state) => {
            activity_log::log_medplum_auth(
                "refresh",
                auth_state.practitioner_id.as_deref(),
                true,
                None,
            );
            Ok(auth_state)
        }
        Err(e) => {
            activity_log::log_medplum_auth("refresh", None, false, Some(&e.to_string()));
            Err(e.to_string())
        }
    }
}

/// Search for patients by name or MRN
#[tauri::command]
pub async fn medplum_search_patients(
    medplum_state: State<'_, SharedMedplumClient>,
    query: String,
) -> Result<Vec<Patient>, String> {
    info!("Searching for patients: {}", query);

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client.search_patients(&query).await.map_err(|e| e.to_string())
}

/// Create a new encounter for a patient
#[tauri::command]
pub async fn medplum_create_encounter(
    medplum_state: State<'_, SharedMedplumClient>,
    patient_id: String,
) -> Result<Encounter, String> {
    info!("Creating encounter for patient: {}", patient_id);

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client.create_encounter(&patient_id).await.map_err(|e| e.to_string())
}

/// Complete an encounter with transcript, SOAP note, and optional audio
#[tauri::command]
pub async fn medplum_complete_encounter(
    medplum_state: State<'_, SharedMedplumClient>,
    encounter_id: String,
    encounter_fhir_id: String,
    patient_id: String,
    transcript: String,
    soap_note: Option<String>,
    audio_data: Option<Vec<u8>>,
) -> Result<SyncResult, String> {
    info!("Completing encounter: {}", encounter_id);

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    let mut sync_status = SyncStatus::default();
    let mut errors = Vec::new();

    // Upload transcript (log size, not content)
    let transcript_size = transcript.len();
    match client
        .upload_transcript(&encounter_id, &encounter_fhir_id, &patient_id, &transcript)
        .await
    {
        Ok(doc_id) => {
            sync_status.transcript_synced = true;
            activity_log::log_document_upload(
                &encounter_id,
                &encounter_fhir_id,
                "transcript",
                &doc_id,
                transcript_size,
                true,
                None,
            );
        }
        Err(e) => {
            activity_log::log_document_upload(
                &encounter_id,
                &encounter_fhir_id,
                "transcript",
                "",
                transcript_size,
                false,
                Some(&e.to_string()),
            );
            errors.push(format!("Transcript: {}", e));
        }
    }

    // Upload SOAP note if provided (log size, not content)
    if let Some(ref soap) = soap_note {
        let soap_size = soap.len();
        match client
            .upload_soap_note(&encounter_id, &encounter_fhir_id, &patient_id, soap)
            .await
        {
            Ok(doc_id) => {
                sync_status.soap_note_synced = true;
                activity_log::log_document_upload(
                    &encounter_id,
                    &encounter_fhir_id,
                    "soap_note",
                    &doc_id,
                    soap_size,
                    true,
                    None,
                );
            }
            Err(e) => {
                activity_log::log_document_upload(
                    &encounter_id,
                    &encounter_fhir_id,
                    "soap_note",
                    "",
                    soap_size,
                    false,
                    Some(&e.to_string()),
                );
                errors.push(format!("SOAP note: {}", e));
            }
        }
    } else {
        sync_status.soap_note_synced = true; // Not required
    }

    // Upload audio if provided
    if let Some(ref audio) = audio_data {
        let audio_size = audio.len();
        match client
            .upload_audio(
                &encounter_id,
                &encounter_fhir_id,
                &patient_id,
                audio,
                "audio/webm",
                None,
            )
            .await
        {
            Ok(media_id) => {
                sync_status.audio_synced = true;
                activity_log::log_audio_upload(
                    &encounter_id,
                    &encounter_fhir_id,
                    &media_id,
                    "", // binary_id not returned
                    audio_size,
                    None,
                    true,
                    None,
                );
            }
            Err(e) => {
                activity_log::log_audio_upload(
                    &encounter_id,
                    &encounter_fhir_id,
                    "",
                    "",
                    audio_size,
                    None,
                    false,
                    Some(&e.to_string()),
                );
                errors.push(format!("Audio: {}", e));
            }
        }
    } else {
        sync_status.audio_synced = true; // Not required
    }

    // Complete the encounter
    match client.complete_encounter(&encounter_fhir_id).await {
        Ok(_) => {
            sync_status.encounter_synced = true;
            activity_log::log_encounter_sync(
                &encounter_id,
                &encounter_id,
                &encounter_fhir_id,
                "complete",
                true,
                None,
            );
        }
        Err(e) => {
            activity_log::log_encounter_sync(
                &encounter_id,
                &encounter_id,
                &encounter_fhir_id,
                "complete",
                false,
                Some(&e.to_string()),
            );
            errors.push(format!("Encounter: {}", e));
        }
    }

    sync_status.last_sync_time = Some(chrono::Utc::now().to_rfc3339());

    let success = errors.is_empty();
    let error = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };

    Ok(SyncResult {
        success,
        status: sync_status,
        error,
    })
}

/// Get encounter history for the current practitioner
#[tauri::command]
pub async fn medplum_get_encounter_history(
    medplum_state: State<'_, SharedMedplumClient>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<Vec<EncounterSummary>, String> {
    info!("Getting encounter history");

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client
        .get_encounter_history(start_date.as_deref(), end_date.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Get detailed encounter data including documents
#[tauri::command]
pub async fn medplum_get_encounter_details(
    medplum_state: State<'_, SharedMedplumClient>,
    encounter_id: String,
) -> Result<EncounterDetails, String> {
    info!("Getting encounter details: {}", encounter_id);

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client
        .get_encounter_details(&encounter_id)
        .await
        .map_err(|e| e.to_string())
}

/// Get raw audio data from Medplum Binary resource
#[tauri::command]
pub async fn medplum_get_audio_data(
    medplum_state: State<'_, SharedMedplumClient>,
    binary_id: String,
) -> Result<Vec<u8>, String> {
    info!("Fetching audio data: {}", binary_id);

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    client
        .get_audio_data(&binary_id)
        .await
        .map_err(|e| e.to_string())
}

/// Manual sync of an encounter
#[tauri::command]
pub async fn medplum_sync_encounter(
    medplum_state: State<'_, SharedMedplumClient>,
    encounter_id: String,
    encounter_fhir_id: String,
    patient_id: String,
    transcript: String,
    soap_note: Option<String>,
    audio_data: Option<Vec<u8>>,
) -> Result<SyncResult, String> {
    info!("Manual sync for encounter: {}", encounter_id);

    // Reuse the complete_encounter logic
    medplum_complete_encounter(
        medplum_state,
        encounter_id,
        encounter_fhir_id,
        patient_id,
        transcript,
        soap_note,
        audio_data,
    )
    .await
}

/// Quick sync - creates placeholder patient, encounter, and uploads everything in one call
#[tauri::command]
pub async fn medplum_quick_sync(
    medplum_state: State<'_, SharedMedplumClient>,
    transcript: String,
    soap_note: Option<String>,
    audio_file_path: Option<String>,
    session_duration_ms: u64,
) -> Result<SyncResult, String> {
    info!("Quick sync: creating placeholder patient and encounter");

    let client_guard = medplum_state.read().await;
    let client = client_guard
        .as_ref()
        .ok_or_else(|| "Medplum client not initialized".to_string())?;

    // Step 1: Create placeholder patient
    let patient = client
        .create_placeholder_patient()
        .await
        .map_err(|e| format!("Failed to create placeholder patient: {}", e))?;

    info!("Created placeholder patient: {}", patient.id);

    // Step 2: Create encounter
    let encounter = client
        .create_encounter(&patient.id)
        .await
        .map_err(|e| format!("Failed to create encounter: {}", e))?;

    info!("Created encounter: {}", encounter.id);

    // Log encounter creation
    activity_log::log_encounter_sync(
        &encounter.id,
        &encounter.id,
        &encounter.id,
        "create",
        true,
        None,
    );

    // Step 3: Upload transcript and SOAP note
    let mut sync_status = SyncStatus::default();
    let mut errors = Vec::new();

    // Upload transcript (log size, not content)
    let transcript_size = transcript.len();
    match client
        .upload_transcript(&encounter.id, &encounter.id, &patient.id, &transcript)
        .await
    {
        Ok(doc_id) => {
            sync_status.transcript_synced = true;
            activity_log::log_document_upload(
                &encounter.id,
                &encounter.id,
                "transcript",
                &doc_id,
                transcript_size,
                true,
                None,
            );
            info!("Transcript uploaded successfully");
        }
        Err(e) => {
            activity_log::log_document_upload(
                &encounter.id,
                &encounter.id,
                "transcript",
                "",
                transcript_size,
                false,
                Some(&e.to_string()),
            );
            errors.push(format!("Transcript: {}", e));
        }
    }

    // Upload SOAP note if provided (log size, not content)
    if let Some(ref soap) = soap_note {
        let soap_size = soap.len();
        match client
            .upload_soap_note(&encounter.id, &encounter.id, &patient.id, soap)
            .await
        {
            Ok(doc_id) => {
                sync_status.soap_note_synced = true;
                activity_log::log_document_upload(
                    &encounter.id,
                    &encounter.id,
                    "soap_note",
                    &doc_id,
                    soap_size,
                    true,
                    None,
                );
                info!("SOAP note uploaded successfully");
            }
            Err(e) => {
                activity_log::log_document_upload(
                    &encounter.id,
                    &encounter.id,
                    "soap_note",
                    "",
                    soap_size,
                    false,
                    Some(&e.to_string()),
                );
                errors.push(format!("SOAP note: {}", e));
            }
        }
    } else {
        sync_status.soap_note_synced = true; // Not applicable
    }

    // Upload audio if provided
    if let Some(ref audio_path) = audio_file_path {
        let path = PathBuf::from(audio_path);
        if path.exists() {
            match std::fs::read(&path) {
                Ok(audio_data) => {
                    let audio_size = audio_data.len();
                    let duration_seconds = Some(session_duration_ms / 1000);
                    match client
                        .upload_audio(
                            &encounter.id,
                            &encounter.id,
                            &patient.id,
                            &audio_data,
                            "audio/wav",
                            duration_seconds,
                        )
                        .await
                    {
                        Ok(media_id) => {
                            sync_status.audio_synced = true;
                            activity_log::log_audio_upload(
                                &encounter.id,
                                &encounter.id,
                                &media_id,
                                "",
                                audio_size,
                                duration_seconds,
                                true,
                                None,
                            );
                            info!("Audio uploaded successfully");
                        }
                        Err(e) => {
                            activity_log::log_audio_upload(
                                &encounter.id,
                                &encounter.id,
                                "",
                                "",
                                audio_size,
                                duration_seconds,
                                false,
                                Some(&e.to_string()),
                            );
                            errors.push(format!("Audio: {}", e));
                        }
                    }
                }
                Err(e) => errors.push(format!("Audio read error: {}", e)),
            }
        } else {
            info!("Audio file not found at {:?}, skipping audio upload", path);
            sync_status.audio_synced = true; // Not available
        }
    } else {
        sync_status.audio_synced = true; // Not applicable
    }

    // Mark encounter as synced
    sync_status.encounter_synced = true;
    sync_status.last_sync_time = Some(chrono::Utc::now().to_rfc3339());

    let success = errors.is_empty();
    let error = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };

    info!(
        "Quick sync complete. Success: {}, Transcript: {}, SOAP: {}",
        success, sync_status.transcript_synced, sync_status.soap_note_synced
    );

    Ok(SyncResult {
        success,
        status: sync_status,
        error,
    })
}

/// Check if Medplum server is reachable (doesn't require authentication)
#[tauri::command]
pub async fn medplum_check_connection(
    medplum_state: State<'_, SharedMedplumClient>,
) -> Result<bool, String> {
    let config = Config::load_or_default();
    if config.medplum_server_url.is_empty() {
        return Ok(false);
    }

    // Try to create client if not exists
    if let Err(_) = get_or_create_medplum_client(medplum_state.inner()).await {
        return Ok(false);
    }

    let client_guard = medplum_state.read().await;
    if let Some(ref client) = *client_guard {
        // Check server connectivity (not authentication)
        Ok(client.check_server_connectivity().await)
    } else {
        Ok(false)
    }
}
