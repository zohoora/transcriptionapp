//! Permission-related Tauri commands.

use crate::permissions::{self, MicrophoneAuthStatus};
use serde::{Deserialize, Serialize};

/// Microphone permission status for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrophonePermissionStatus {
    pub status: String,
    pub authorized: bool,
    pub message: String,
}

/// Check microphone permission status
#[tauri::command]
pub fn check_microphone_permission() -> MicrophonePermissionStatus {
    let status = permissions::check_microphone_permission();

    match status {
        MicrophoneAuthStatus::Authorized => MicrophonePermissionStatus {
            status: "authorized".to_string(),
            authorized: true,
            message: "Microphone access granted".to_string(),
        },
        MicrophoneAuthStatus::Denied => MicrophonePermissionStatus {
            status: "denied".to_string(),
            authorized: false,
            message: "Microphone access denied. Please grant permission in System Settings → Privacy & Security → Microphone".to_string(),
        },
        MicrophoneAuthStatus::NotDetermined => MicrophonePermissionStatus {
            status: "not_determined".to_string(),
            authorized: false,
            message: "Microphone permission not yet requested".to_string(),
        },
        MicrophoneAuthStatus::Restricted => MicrophonePermissionStatus {
            status: "restricted".to_string(),
            authorized: false,
            message: "Microphone access is restricted by system policy".to_string(),
        },
        MicrophoneAuthStatus::Unknown => MicrophonePermissionStatus {
            status: "unknown".to_string(),
            authorized: false,
            message: "Could not determine microphone permission status".to_string(),
        },
    }
}

/// Request microphone permission from the user
/// Returns true if the permission request was initiated
#[tauri::command]
pub fn request_microphone_permission() -> bool {
    permissions::request_microphone_permission()
}

/// Open system settings to the microphone privacy section
#[tauri::command]
pub fn open_microphone_settings() -> Result<(), String> {
    permissions::open_microphone_settings().map_err(|e| e.to_string())
}
