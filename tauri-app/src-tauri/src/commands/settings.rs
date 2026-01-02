//! Settings commands

use crate::config::{Config, Settings};

/// Get current settings
#[tauri::command]
pub fn get_settings() -> Result<Settings, String> {
    let config = Config::load_or_default();
    Ok(config.to_settings())
}

/// Update settings
#[tauri::command]
pub fn set_settings(settings: Settings) -> Result<Settings, String> {
    // Validate settings before saving
    let validation_errors = settings.validate();
    if !validation_errors.is_empty() {
        let error_messages: Vec<String> =
            validation_errors.iter().map(|e| e.to_string()).collect();
        return Err(format!(
            "Invalid settings: {}",
            error_messages.join("; ")
        ));
    }

    let mut config = Config::load_or_default();
    config.update_from_settings(&settings);
    config.save().map_err(|e| e.to_string())?;
    Ok(config.to_settings())
}
