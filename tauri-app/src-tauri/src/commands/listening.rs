//! Tauri commands for listening mode (auto-session detection)

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tracing::{error, info};

use crate::config::Config;
use crate::listening::{self, ListeningConfig, ListeningEvent, ListeningHandle, ListeningStatus};

/// Load config from disk (consistent with other commands)
fn load_config() -> Config {
    Config::load_or_default()
}

/// Shared state for the listening pipeline
pub struct ListeningState {
    pub handle: Option<ListeningHandle>,
    pub status: ListeningStatus,
    /// Initial audio buffer (16kHz mono) to prepend to recording session
    /// Set when StartRecording event is emitted, consumed by start_session
    pub initial_audio_buffer: Option<Vec<f32>>,
}

impl Default for ListeningState {
    fn default() -> Self {
        Self {
            handle: None,
            status: ListeningStatus::default(),
            initial_audio_buffer: None,
        }
    }
}

/// Type alias for shared listening state (Arc-wrapped for sharing with callbacks)
pub type SharedListeningState = Arc<Mutex<ListeningState>>;

/// Event payload sent to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListeningEventPayload {
    #[serde(flatten)]
    pub event: ListeningEvent,
}

/// Start listening mode (passive VAD + greeting detection)
#[tauri::command]
pub async fn start_listening(
    app: AppHandle,
    listening_state: State<'_, SharedListeningState>,
    device_id: Option<String>,
) -> Result<(), String> {
    info!("Starting listening mode");

    // Check if already listening
    {
        let state = listening_state.lock().map_err(|e| format!("Lock error: {}", e))?;
        if state.handle.is_some() {
            return Err("Already listening".to_string());
        }
    }

    // Load config from disk (consistent with other commands)
    let cfg = load_config();

    // Get speaker model path (if speaker verification is enabled)
    let speaker_model_path = if cfg.auto_start_require_enrolled {
        dirs::home_dir().map(|h| {
            h.join(".transcriptionapp/models/ecapa_tdnn.onnx")
                .to_string_lossy()
                .to_string()
        })
    } else {
        None
    };

    let config = ListeningConfig {
        vad_threshold: cfg.vad_threshold,
        min_speech_duration_ms: cfg.min_speech_duration_ms.unwrap_or(2000),
        max_buffer_ms: 5000,
        greeting_sensitivity: cfg.greeting_sensitivity.unwrap_or(0.7),
        cooldown_ms: 5000,
        whisper_server_url: cfg.whisper_server_url.clone(),
        whisper_server_model: cfg.whisper_server_model.clone(),
        llm_router_url: cfg.llm_router_url.clone(),
        llm_api_key: cfg.llm_api_key.clone(),
        llm_client_id: cfg.llm_client_id.clone(),
        fast_model: cfg.fast_model.clone(),
        language: cfg.language.clone(),
        require_enrolled: cfg.auto_start_require_enrolled,
        required_role: cfg.auto_start_required_role.clone(),
        speaker_model_path,
    };

    // Create event callback that emits to frontend
    let app_handle = app.clone();
    let listening_state_clone = listening_state.inner().clone();
    let event_callback = move |event: ListeningEvent| {
        // Update internal status based on event
        if let Ok(mut state) = listening_state_clone.lock() {
            match &event {
                ListeningEvent::Started => {
                    state.status.is_listening = true;
                    state.status.speech_detected = false;
                    state.status.analyzing = false;
                }
                ListeningEvent::SpeechDetected { duration_ms } => {
                    state.status.speech_detected = true;
                    state.status.speech_duration_ms = *duration_ms;
                }
                ListeningEvent::Analyzing => {
                    state.status.analyzing = true;
                }
                ListeningEvent::StartRecording { ref initial_audio, .. } => {
                    // Store initial audio buffer for start_session to consume
                    state.initial_audio_buffer = Some(initial_audio.clone());
                    state.status.analyzing = true;
                }
                ListeningEvent::GreetingConfirmed { .. } => {
                    state.status.analyzing = false;
                    state.status.speech_detected = false;
                    state.status.speech_duration_ms = 0;
                }
                ListeningEvent::GreetingRejected { .. } => {
                    state.status.analyzing = false;
                    state.status.speech_detected = false;
                    state.status.speech_duration_ms = 0;
                    // Clear the audio buffer since greeting was rejected
                    state.initial_audio_buffer = None;
                }
                ListeningEvent::GreetingDetected { .. } | ListeningEvent::NotGreeting { .. } => {
                    state.status.analyzing = false;
                    state.status.speech_detected = false;
                    state.status.speech_duration_ms = 0;
                }
                ListeningEvent::Stopped => {
                    state.status.is_listening = false;
                    state.status.speech_detected = false;
                    state.status.analyzing = false;
                    // Clear audio buffer to prevent memory leak
                    state.initial_audio_buffer = None;
                }
                ListeningEvent::SpeakerNotVerified { .. } => {
                    state.status.analyzing = false;
                    state.status.speech_detected = false;
                    state.status.speech_duration_ms = 0;
                    // Clear audio buffer since speaker was not verified
                    state.initial_audio_buffer = None;
                }
                ListeningEvent::Error { .. } => {
                    state.status.analyzing = false;
                    // Clear audio buffer to prevent memory leak
                    state.initial_audio_buffer = None;
                }
            }
        }

        // Emit event to frontend
        let payload = ListeningEventPayload { event };
        if let Err(e) = app_handle.emit("listening_event", &payload) {
            error!("Failed to emit listening event: {}", e);
        }
    };

    // Start listening
    let handle = listening::start_listening(config, device_id, event_callback)?;

    // Store handle
    {
        let mut state = listening_state.lock().map_err(|e| format!("Lock error: {}", e))?;
        state.handle = Some(handle);
        state.status.is_listening = true;
    }

    Ok(())
}

/// Stop listening mode
#[tauri::command]
pub fn stop_listening(
    listening_state: State<'_, SharedListeningState>,
) -> Result<(), String> {
    info!("Stopping listening mode");

    let mut state = listening_state.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(handle) = state.handle.take() {
        handle.stop();
        // Note: Don't call join() here as it would block the command
        // The handle will be dropped which also signals stop
    }

    state.status = ListeningStatus::default();

    Ok(())
}

/// Get current listening status
#[tauri::command]
pub fn get_listening_status(
    listening_state: State<'_, SharedListeningState>,
) -> Result<ListeningStatus, String> {
    let state = listening_state.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(state.status.clone())
}
