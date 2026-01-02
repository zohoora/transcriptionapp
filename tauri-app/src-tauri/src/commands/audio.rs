//! Audio device commands

use super::Device;
use crate::audio;
use tracing::error;

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
