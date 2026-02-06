//! Screen capture Tauri commands.

use super::CommandError;
use crate::screenshot::{self, ScreenCaptureState};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tracing::warn;

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
pub fn open_screen_recording_settings() -> Result<(), CommandError> {
    screenshot::open_screen_recording_settings().map_err(CommandError::Other)
}

/// Start periodic screen capture
#[tauri::command]
pub fn start_screen_capture(
    state: tauri::State<'_, SharedScreenCaptureState>,
    interval_secs: u32,
) -> Result<(), CommandError> {
    let mut capture = state.lock().map_err(|e| CommandError::LockPoisoned {
        context: format!("screen capture state: {}", e),
    })?;
    capture.start(interval_secs).map_err(CommandError::Other)
}

/// Stop screen capture and clean up
#[tauri::command]
pub fn stop_screen_capture(
    state: tauri::State<'_, SharedScreenCaptureState>,
) -> Result<(), CommandError> {
    let mut capture = state.lock().map_err(|e| CommandError::LockPoisoned {
        context: format!("screen capture state: {}", e),
    })?;
    capture.stop();
    Ok(())
}

/// Get paths of all captured screenshots
#[tauri::command]
pub fn get_screenshot_paths(
    state: tauri::State<'_, SharedScreenCaptureState>,
) -> Result<Vec<String>, CommandError> {
    let capture = state.lock().map_err(|e| CommandError::LockPoisoned {
        context: format!("screen capture state: {}", e),
    })?;
    Ok(capture.screenshot_paths().iter().map(|p| p.to_string_lossy().to_string()).collect())
}

/// Get current screen capture status
#[tauri::command]
pub fn get_screen_capture_status(
    state: tauri::State<'_, SharedScreenCaptureState>,
) -> Result<ScreenCaptureStatus, CommandError> {
    let capture = state.lock().map_err(|e| CommandError::LockPoisoned {
        context: format!("screen capture state: {}", e),
    })?;
    Ok(ScreenCaptureStatus {
        running: capture.is_running(),
        screenshot_count: capture.screenshot_count(),
    })
}

/// A thumbnail with its file path and base64-encoded data URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotThumbnail {
    /// Absolute file path to the thumbnail
    pub path: String,
    /// Base64-encoded data URL (data:image/jpeg;base64,...)
    pub data_url: String,
    /// Human-readable label extracted from filename timestamp
    pub label: String,
}

/// Get all thumbnail screenshots as base64 data URLs for the picker UI
#[tauri::command]
pub fn get_screenshot_thumbnails(
    state: tauri::State<'_, SharedScreenCaptureState>,
) -> Result<Vec<ScreenshotThumbnail>, CommandError> {
    let paths = {
        let capture = state.lock().map_err(|e| CommandError::LockPoisoned {
            context: format!("screen capture state: {}", e),
        })?;
        capture.screenshot_paths()
    };

    let thumbs: Vec<_> = paths
        .iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.contains("-thumb.jpg"))
                .unwrap_or(false)
        })
        .collect();

    let mut result = Vec::with_capacity(thumbs.len());
    for path in thumbs {
        let data = match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to read thumbnail {:?}: {}", path, e);
                continue;
            }
        };

        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
        let data_url = format!("data:image/jpeg;base64,{}", b64);

        // Extract timestamp from filename like "capture-143025-123-thumb.jpg"
        let label = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|name| {
                // Pattern: capture-HHMMSS-mmm-thumb.jpg
                let stripped = name.strip_prefix("capture-")?;
                let ts_part = stripped.split('-').next()?;
                if ts_part.len() >= 6 {
                    let h = &ts_part[0..2];
                    let m = &ts_part[2..4];
                    let s = &ts_part[4..6];
                    Some(format!("{}:{}:{}", h, m, s))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "Screenshot".to_string());

        result.push(ScreenshotThumbnail {
            path: path.to_string_lossy().to_string(),
            data_url,
            label,
        });
    }

    Ok(result)
}
