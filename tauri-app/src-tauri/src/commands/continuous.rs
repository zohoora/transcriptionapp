//! Commands for continuous charting mode (start/stop/status)

use super::CommandError;
use crate::commands::physicians::{SharedActivePhysician, SharedProfileClient, SharedRoomConfig};
use crate::config::Config;
use crate::continuous_mode::{ContinuousModeHandle, ContinuousModeStats};
use crate::server_sync::ServerSyncContext;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use tracing::{info, warn};

/// Shared state for the continuous mode handle
pub type SharedContinuousModeState = Arc<Mutex<Option<Arc<ContinuousModeHandle>>>>;

/// Start continuous charting mode
///
/// Starts the audio pipeline and encounter detector loop.
/// Recording runs indefinitely until stop_continuous_mode is called.
#[tauri::command]
pub async fn start_continuous_mode(
    app: AppHandle,
    continuous_state: State<'_, SharedContinuousModeState>,
    active_physician: State<'_, SharedActivePhysician>,
    room_config_state: State<'_, SharedRoomConfig>,
    profile_client_state: State<'_, SharedProfileClient>,
) -> Result<(), CommandError> {
    info!("Starting continuous charting mode");

    // Check if already running
    {
        let state = continuous_state
            .lock()
            .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
        if state.is_some() {
            return Err(CommandError::AlreadyRunning("continuous mode".into()));
        }
    }

    // Build server sync context from current physician/room state
    let sync_ctx = ServerSyncContext::from_state(
        &active_physician, &room_config_state, &profile_client_state,
    ).await;

    // Create handle
    let handle = Arc::new(ContinuousModeHandle::new());

    // Store handle in shared state
    {
        let mut state = continuous_state
            .lock()
            .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
        *state = Some(handle.clone());
    }

    // Load config
    let config = Config::load_or_default();

    // Spawn the continuous mode loop
    let handle_for_task = handle.clone();
    let continuous_state_for_cleanup = continuous_state.inner().clone();

    tokio::spawn(async move {
        if let Err(e) = crate::continuous_mode::run_continuous_mode(
            app,
            handle_for_task,
            config,
            sync_ctx,
        )
        .await
        {
            warn!("Continuous mode exited with error: {}", e);
        }

        // Clean up shared state when done
        if let Ok(mut state) = continuous_state_for_cleanup.lock() {
            *state = None;
        }
    });

    Ok(())
}

/// Stop continuous charting mode
///
/// Signals the pipeline and encounter detector to stop.
/// Any buffered transcript is flushed as a final encounter check.
#[tauri::command]
pub fn stop_continuous_mode(
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<(), CommandError> {
    info!("Stopping continuous charting mode");

    let state = continuous_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
    if let Some(ref handle) = *state {
        handle.stop();
        Ok(())
    } else {
        Err(CommandError::NotRunning("continuous mode".into()))
    }
}

/// Get the current status of continuous charting mode
#[tauri::command]
pub fn get_continuous_mode_status(
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<ContinuousModeStats, CommandError> {
    let state = continuous_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
    if let Some(ref handle) = *state {
        Ok(handle.get_stats())
    } else {
        // Return idle stats when not running
        Ok(ContinuousModeStats {
            state: "idle".to_string(),
            recording_since: String::new(),
            encounters_detected: 0,
            last_encounter_at: None,
            last_encounter_words: None,
            last_encounter_patient_name: None,
            last_error: None,
            buffer_word_count: 0,
            buffer_started_at: None,
            sensor_connected: None,
            sensor_state: None,
            shadow_mode_active: None,
            shadow_method: None,
            last_shadow_outcome: None,
            is_sleeping: false,
            sleep_resume_at: None,
        })
    }
}

/// Set per-encounter notes for the current continuous mode encounter
///
/// Notes are passed to SOAP generation and cleared when a new encounter starts.
#[tauri::command]
pub fn set_continuous_encounter_notes(
    notes: String,
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<(), CommandError> {
    let state = continuous_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
    if let Some(ref handle) = *state {
        if let Ok(mut encounter_notes) = handle.encounter_notes.lock() {
            *encounter_notes = notes;
        }
        Ok(())
    } else {
        Err(CommandError::NotRunning("continuous mode".into()))
    }
}

/// Core logic for setting STT language — checks both pipeline states.
/// Extracted from the Tauri command for testability.
pub(crate) fn set_stt_language_inner(
    stt_name: String,
    pipeline_state: &super::SharedPipelineState,
    continuous_state: &SharedContinuousModeState,
) -> Result<(), CommandError> {
    // Try session-mode pipeline first
    if let Ok(state) = pipeline_state.lock() {
        if let Some(ref handle) = state.handle {
            handle.set_stt_language(stt_name);
            return Ok(());
        }
    }

    // Try continuous-mode pipeline
    if let Ok(state) = continuous_state.lock() {
        if let Some(ref handle) = *state {
            if let Ok(lang_opt) = handle.stt_language.lock() {
                if let Some(ref lang) = *lang_opt {
                    if let Ok(mut l) = lang.lock() {
                        info!("STT language changed to: {} (continuous mode)", stt_name);
                        *l = stt_name;
                        return Ok(());
                    }
                }
            }
        }
    }

    Err(CommandError::NotRunning("pipeline".into()))
}

/// Set STT language dynamically (takes effect on the next utterance, no pipeline restart).
/// Checks both session-mode and continuous-mode pipelines.
#[tauri::command]
pub fn set_stt_language(
    language: String,
    pipeline_state: State<'_, super::SharedPipelineState>,
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<(), CommandError> {
    let stt_name = crate::config::iso_to_stt_language(&language).to_string();
    set_stt_language_inner(stt_name, &pipeline_state, &continuous_state)
}

/// List available serial ports (for presence sensor configuration)
///
/// Returns a list of port names (e.g. `/dev/cu.usbserial-2110`) that can be
/// used with the mmWave presence sensor.
#[tauri::command]
pub fn list_serial_ports() -> Result<Vec<String>, CommandError> {
    let ports = serialport::available_ports()
        .map_err(|e| CommandError::Io(e.to_string()))?;
    Ok(ports
        .into_iter()
        .map(|p| p.port_name)
        .collect())
}

/// Trigger a manual new patient encounter split
///
/// Wakes the encounter detector immediately, bypassing minimum duration and
/// word count guards. If the buffer has any content, it will be archived as
/// an encounter and a new SOAP note generated.
#[tauri::command]
pub fn trigger_new_patient(
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<(), CommandError> {
    info!("Manual new patient trigger received");
    let state = continuous_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("continuous_state"))?;
    if let Some(ref handle) = *state {
        handle.encounter_manual_trigger.notify_one();
        Ok(())
    } else {
        Err(CommandError::NotRunning("continuous mode".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::PipelineState;

    #[test]
    fn test_set_language_fails_when_no_pipeline_running() {
        let pipeline_state: super::super::SharedPipelineState = Arc::new(Mutex::new(PipelineState::default()));
        let continuous_state: SharedContinuousModeState = Arc::new(Mutex::new(None));

        let result = set_stt_language_inner("en".into(), &pipeline_state, &continuous_state);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_language_reaches_continuous_mode_handle() {
        let pipeline_state: super::super::SharedPipelineState = Arc::new(Mutex::new(PipelineState::default()));

        // Create a ContinuousModeHandle with an stt_language mutex
        let handle = ContinuousModeHandle::new();
        let lang_arc = Arc::new(std::sync::Mutex::new("en".to_string()));
        *handle.stt_language.lock().unwrap() = Some(lang_arc.clone());
        let continuous_state: SharedContinuousModeState = Arc::new(Mutex::new(Some(Arc::new(handle))));

        // Session-mode pipeline is not running — should fall through to continuous mode
        let result = set_stt_language_inner("fa".into(), &pipeline_state, &continuous_state);
        assert!(result.is_ok(), "Should succeed via continuous mode handle");

        // Verify the language was actually changed
        let current = lang_arc.lock().unwrap().clone();
        assert_eq!(current, "fa");
    }

    #[test]
    fn test_set_language_without_stt_language_populated() {
        let pipeline_state: super::super::SharedPipelineState = Arc::new(Mutex::new(PipelineState::default()));

        // Handle exists but stt_language not yet populated (pipeline not started)
        let handle = ContinuousModeHandle::new();
        let continuous_state: SharedContinuousModeState = Arc::new(Mutex::new(Some(Arc::new(handle))));

        let result = set_stt_language_inner("fa".into(), &pipeline_state, &continuous_state);
        assert!(result.is_err(), "Should fail when stt_language not populated");
    }

    #[test]
    fn test_set_language_multiple_switches() {
        let pipeline_state: super::super::SharedPipelineState = Arc::new(Mutex::new(PipelineState::default()));

        let handle = ContinuousModeHandle::new();
        let lang_arc = Arc::new(std::sync::Mutex::new("en".to_string()));
        *handle.stt_language.lock().unwrap() = Some(lang_arc.clone());
        let continuous_state: SharedContinuousModeState = Arc::new(Mutex::new(Some(Arc::new(handle))));

        set_stt_language_inner("fa".into(), &pipeline_state, &continuous_state).unwrap();
        assert_eq!(*lang_arc.lock().unwrap(), "fa");

        set_stt_language_inner("en".into(), &pipeline_state, &continuous_state).unwrap();
        assert_eq!(*lang_arc.lock().unwrap(), "en");

        set_stt_language_inner("ar".into(), &pipeline_state, &continuous_state).unwrap();
        assert_eq!(*lang_arc.lock().unwrap(), "ar");
    }
}
