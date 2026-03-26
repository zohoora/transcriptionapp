//! Whisper server commands for remote transcription
//!
//! Commands for checking Whisper server status and listing available models.

use crate::config::Config;
use crate::whisper_server::{WhisperServerClient, WhisperServerStatus};
use tracing::info;

/// Check the Whisper server status and list available models
///
/// Optional URL override allows testing pending settings without persisting them.
#[tauri::command]
pub async fn check_whisper_server_status(url: Option<String>) -> WhisperServerStatus {
    let config = Config::load_or_default();
    let url = url.unwrap_or_else(|| config.whisper_server_url.clone());

    // Validate that we have a server URL
    if url.is_empty() {
        return WhisperServerStatus {
            connected: false,
            available_models: vec![],
            error: Some("Whisper server URL is not configured".to_string()),
        };
    }

    info!("Checking Whisper server status at {}", url);

    // Try to create client and check status
    match WhisperServerClient::new(&url, &config.whisper_server_model) {
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
pub async fn list_whisper_server_models() -> Result<Vec<String>, super::CommandError> {
    let config = Config::load_or_default();

    if config.whisper_server_url.is_empty() {
        return Err(super::CommandError::Config(
            "Whisper server URL is not configured".into(),
        ));
    }

    let client = WhisperServerClient::new(&config.whisper_server_url, &config.whisper_server_model)
        .map_err(|e| super::CommandError::Network(e))?;

    client
        .list_models()
        .await
        .map_err(|e| super::CommandError::Network(e))
}

