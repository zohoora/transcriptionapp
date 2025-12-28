use crate::audio;
use crate::checklist::{self, ChecklistResult};
use crate::config::{Config, Settings};
use crate::models::{self, ModelInfo, WhisperModel};
use crate::pipeline::{start_pipeline, PipelineConfig, PipelineHandle, PipelineMessage};
use crate::session::{SessionError, SessionManager};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use tracing::{error, info};

/// Device information for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Model status for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStatus {
    pub available: bool,
    pub path: Option<String>,
    pub error: Option<String>,
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

    let pipeline_config = PipelineConfig {
        device_id: if device_id.as_deref() == Some("default") {
            None
        } else {
            device_id
        },
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

    // Transition to recording
    {
        let mut session = session_arc.lock().map_err(|e| e.to_string())?;
        session.start_recording("whisper");
    }

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
                    info!("Received segment: '{}' ({}ms - {}ms)", segment.text, segment.start_ms, segment.end_ms);
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

    if let Some(h) = handle {
        h.stop();

        // Wait for pipeline to finish in a separate task
        let app_clone = app.clone();
        let session_clone = session_arc.clone();

        tokio::task::spawn_blocking(move || {
            h.join();

            // Transition to completed
            if let Ok(mut session) = session_clone.lock() {
                session.complete();
                let status = session.status();
                let transcript = session.transcript_update();
                let _ = app_clone.emit("session_status", status);
                let _ = app_clone.emit("transcript_update", transcript);
            }
        });
    } else {
        // No pipeline running, just complete
        {
            let mut session = session_arc.lock().map_err(|e| e.to_string())?;
            session.complete();
        }
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

    emit_status_arc(&app, session_state.inner())?;
    emit_transcript_arc(&app, session_state.inner())?;

    Ok(())
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
    checklist::run_all_checks(&config)
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
