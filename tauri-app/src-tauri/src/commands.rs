use crate::audio;
use crate::config::{Config, Settings};
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
pub struct PipelineState {
    pub handle: Option<PipelineHandle>,
}

impl Default for PipelineState {
    fn default() -> Self {
        Self { handle: None }
    }
}

/// Shared session manager type for use in async contexts
pub type SharedSessionManager = Arc<Mutex<SessionManager>>;

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
    pipeline_state: State<'_, Mutex<PipelineState>>,
    device_id: Option<String>,
) -> Result<(), String> {
    info!("Starting session with device: {:?}", device_id);

    // Clone the Arc for use in async context
    let session_arc = session_state.inner().clone();

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

    // Store the pipeline handle
    {
        let mut ps = pipeline_state.lock().map_err(|e| e.to_string())?;
        ps.handle = Some(handle);
    }

    // Transition to recording
    {
        let mut session = session_arc.lock().map_err(|e| e.to_string())?;
        session.start_recording("whisper");
    }

    emit_status_arc(&app, &session_arc)?;

    // Spawn task to handle pipeline messages
    let app_clone = app.clone();
    let session_clone = session_arc.clone();

    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match msg {
                PipelineMessage::Segment(segment) => {
                    let mut session = match session_clone.lock() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    session.add_segment(segment);
                    drop(session);

                    // Emit transcript update
                    if let Ok(session) = session_clone.lock() {
                        let transcript = session.transcript_update();
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
    pipeline_state: State<'_, Mutex<PipelineState>>,
) -> Result<(), String> {
    info!("Stopping session");

    // Clone the Arc for use in async context
    let session_arc = session_state.inner().clone();

    // Transition to stopping
    {
        let mut session = session_arc.lock().map_err(|e| e.to_string())?;
        session.start_stopping().map_err(|e| e.to_string())?;
    }

    emit_status_arc(&app, &session_arc)?;

    // Stop the pipeline
    let handle = {
        let mut ps = pipeline_state.lock().map_err(|e| e.to_string())?;
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
    pipeline_state: State<'_, Mutex<PipelineState>>,
) -> Result<(), String> {
    info!("Resetting session");

    // Stop any running pipeline
    {
        let mut ps = pipeline_state.lock().map_err(|e| e.to_string())?;
        if let Some(h) = ps.handle.take() {
            h.stop();
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
