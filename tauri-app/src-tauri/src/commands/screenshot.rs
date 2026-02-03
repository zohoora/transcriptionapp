//! Screen capture Tauri commands.

use crate::screenshot::{self, ScreenCaptureState};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Shared screen capture state type
pub type SharedScreenCaptureState = Arc<Mutex<ScreenCaptureState>>;

/// Screen recording permission status for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenRecordingPermissionStatus {
    pub authorized: bool,
    pub message: String,
}

/// Screen capture status for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenCaptureStatus {
    pub running: bool,
    pub screenshot_count: usize,
}

/// Check screen recording permission
#[tauri::command]
pub fn check_screen_recording_permission() -> ScreenRecordingPermissionStatus {
    let authorized = screenshot::check_screen_recording_permission();
    ScreenRecordingPermissionStatus {
        authorized,
        message: if authorized {
            "Screen recording access granted".to_string()
        } else {
            "Screen recording access not granted. Please enable in System Settings → Privacy & Security → Screen Recording".to_string()
        },
    }
}

/// Open system settings to the screen recording privacy section
#[tauri::command]
pub fn open_screen_recording_settings() -> Result<(), String> {
    screenshot::open_screen_recording_settings()
}

/// Start periodic screen capture
#[tauri::command]
pub fn start_screen_capture(
    state: tauri::State<'_, SharedScreenCaptureState>,
    interval_secs: u32,
) -> Result<(), String> {
    let mut capture = state.lock().map_err(|e| e.to_string())?;
    capture.start(interval_secs)
}

/// Stop screen capture and clean up
#[tauri::command]
pub fn stop_screen_capture(
    state: tauri::State<'_, SharedScreenCaptureState>,
) -> Result<(), String> {
    let mut capture = state.lock().map_err(|e| e.to_string())?;
    capture.stop();
    Ok(())
}

/// Get current screen capture status
#[tauri::command]
pub fn get_screen_capture_status(
    state: tauri::State<'_, SharedScreenCaptureState>,
) -> Result<ScreenCaptureStatus, String> {
    let capture = state.lock().map_err(|e| e.to_string())?;
    Ok(ScreenCaptureStatus {
        running: capture.is_running(),
        screenshot_count: capture.screenshot_count(),
    })
}
