use crate::audio;
use crate::config::{Config, Settings};
use crate::session::{SessionError, SessionManager};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};
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
    session_state: State<'_, Mutex<SessionManager>>,
    device_id: Option<String>,
) -> Result<(), String> {
    info!("Starting session with device: {:?}", device_id);

    // Transition to preparing
    {
        let mut session = session_state.lock().map_err(|e| e.to_string())?;
        session.start_preparing().map_err(|e| e.to_string())?;
    }

    // Emit initial status
    emit_status(&app, &session_state)?;

    // Check model availability
    let model_status = check_model_status_internal();
    if !model_status.available {
        let mut session = session_state.lock().map_err(|e| e.to_string())?;
        session.set_error(SessionError::ModelNotFound(
            model_status.error.unwrap_or_else(|| "Model not found".to_string()),
        ));
        drop(session); // Release lock before emitting
        emit_status(&app, &session_state)?;
        return Err("Model not found".to_string());
    }

    // Start recording
    {
        let mut session = session_state.lock().map_err(|e| e.to_string())?;
        session.start_recording("whisper");
    }

    emit_status(&app, &session_state)?;

    // TODO: Start the actual audio capture and transcription pipeline
    // This is where the M2 integration work happens
    // For now, we just transition states to demonstrate the UI

    Ok(())
}

/// Stop the current transcription session
#[tauri::command]
pub async fn stop_session(
    app: AppHandle,
    session_state: State<'_, Mutex<SessionManager>>,
) -> Result<(), String> {
    info!("Stopping session");

    // Transition to stopping
    {
        let mut session = session_state.lock().map_err(|e| e.to_string())?;
        session.start_stopping().map_err(|e| e.to_string())?;
    }

    emit_status(&app, &session_state)?;

    // TODO: Wait for processing to complete
    // The stop flag is already set, the processing thread should stop

    // For now, just complete immediately
    {
        let mut session = session_state.lock().map_err(|e| e.to_string())?;
        session.complete();
    }

    emit_status(&app, &session_state)?;
    emit_transcript(&app, &session_state)?;

    Ok(())
}

/// Reset the session to idle
#[tauri::command]
pub fn reset_session(
    app: AppHandle,
    session_state: State<'_, Mutex<SessionManager>>,
) -> Result<(), String> {
    info!("Resetting session");

    {
        let mut session = session_state.lock().map_err(|e| e.to_string())?;
        session.reset();
    }

    emit_status(&app, &session_state)?;
    emit_transcript(&app, &session_state)?;

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

fn emit_status(
    app: &AppHandle,
    session_state: &State<'_, Mutex<SessionManager>>,
) -> Result<(), String> {
    let status = {
        let session = session_state.lock().map_err(|e| e.to_string())?;
        session.status()
    };
    app.emit("session_status", status).map_err(|e| e.to_string())
}

fn emit_transcript(
    app: &AppHandle,
    session_state: &State<'_, Mutex<SessionManager>>,
) -> Result<(), String> {
    let transcript = {
        let session = session_state.lock().map_err(|e| e.to_string())?;
        session.transcript_update()
    };
    app.emit("transcript_update", transcript).map_err(|e| e.to_string())
}
