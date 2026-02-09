//! Commands for continuous charting mode (start/stop/status)

use crate::config::Config;
use crate::continuous_mode::{ContinuousModeHandle, ContinuousModeStats};
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
) -> Result<(), String> {
    info!("Starting continuous charting mode");

    // Check if already running
    {
        let state = continuous_state.lock().map_err(|e| e.to_string())?;
        if state.is_some() {
            return Err("Continuous mode is already running".to_string());
        }
    }

    // Create handle
    let handle = Arc::new(ContinuousModeHandle::new());

    // Store handle in shared state
    {
        let mut state = continuous_state.lock().map_err(|e| e.to_string())?;
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
) -> Result<(), String> {
    info!("Stopping continuous charting mode");

    let state = continuous_state.lock().map_err(|e| e.to_string())?;
    if let Some(ref handle) = *state {
        handle.stop();
        Ok(())
    } else {
        Err("Continuous mode is not running".to_string())
    }
}

/// Get the current status of continuous charting mode
#[tauri::command]
pub fn get_continuous_mode_status(
    continuous_state: State<'_, SharedContinuousModeState>,
) -> Result<ContinuousModeStats, String> {
    let state = continuous_state.lock().map_err(|e| e.to_string())?;
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
        })
    }
}
