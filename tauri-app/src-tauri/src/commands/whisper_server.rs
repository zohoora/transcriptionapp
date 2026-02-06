//! Whisper server commands for remote transcription
//!
//! Commands for checking Whisper server status and listing available models.

use crate::config::Config;
use crate::whisper_server::{WhisperServerClient, WhisperServerStatus};
use tracing::info;

/// Check the Whisper server status and list available models
#[tauri::command]
pub async fn check_whisper_server_status() -> WhisperServerStatus {
    let config = Config::load_or_default();

    // Validate that we have a server URL
    if config.whisper_server_url.is_empty() {
        return WhisperServerStatus {
            connected: false,
            available_models: vec![],
            error: Some("Whisper server URL is not configured".to_string()),
        };
    }

    info!(
        "Checking Whisper server status at {}",
        config.whisper_server_url
    );

    // Try to create client and check status
    match WhisperServerClient::new(&config.whisper_server_url, &config.whisper_server_model) {
        Ok(client) => client.check_status().await,
        Err(e) => WhisperServerStatus {
            connected: false,
            available_models: vec![],
            error: Some(e),
        },
    }
}

/// List available models from the Whisper server
#[tauri::command]
pub async fn list_whisper_server_models() -> Result<Vec<String>, String> {
    let config = Config::load_or_default();

    if config.whisper_server_url.is_empty() {
        return Err("Whisper server URL is not configured".to_string());
    }

    let client =
        WhisperServerClient::new(&config.whisper_server_url, &config.whisper_server_model)?;

    client.list_models().await
}

