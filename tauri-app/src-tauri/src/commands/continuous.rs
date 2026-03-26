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

/// Set STT language dynamically (takes effect on the next utterance, no pipeline restart)
#[tauri::command]
pub fn set_stt_language(
    language: String,
    pipeline_state: State<'_, super::SharedPipelineState>,
) -> Result<(), CommandError> {
    let stt_name = crate::config::iso_to_stt_language(&language).to_string();
    let state = pipeline_state
        .lock()
        .map_err(|_| CommandError::lock_poisoned("pipeline_state"))?;
    if let Some(ref handle) = state.handle {
        handle.set_stt_language(stt_name);
        Ok(())
    } else {
        Err(CommandError::NotRunning("pipeline".into()))
    }
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
