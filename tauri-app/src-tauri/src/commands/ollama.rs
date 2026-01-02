//! Ollama / SOAP Note generation commands

use crate::activity_log;
use crate::config::Config;
use crate::ollama::{OllamaClient, OllamaStatus, SoapNote};
use tracing::info;

/// Check Ollama server status and list available models
#[tauri::command]
pub async fn check_ollama_status() -> OllamaStatus {
    let config = Config::load_or_default();
    let client = match OllamaClient::new(&config.ollama_server_url) {
        Ok(c) => c,
        Err(e) => {
            return OllamaStatus {
                connected: false,
                available_models: vec![],
                error: Some(e),
            }
        }
    };
    client.check_status().await
}

/// List available models from Ollama server
#[tauri::command]
pub async fn list_ollama_models() -> Result<Vec<String>, String> {
    let config = Config::load_or_default();
    let client = OllamaClient::new(&config.ollama_server_url)?;
    client.list_models().await
}

/// Generate a SOAP note from the given transcript
#[tauri::command]
pub async fn generate_soap_note(transcript: String) -> Result<SoapNote, String> {
    info!(
        "Generating SOAP note for transcript of {} chars",
        transcript.len()
    );

    if transcript.trim().is_empty() {
        return Err("Cannot generate SOAP note from empty transcript".to_string());
    }

    let config = Config::load_or_default();
    let client = OllamaClient::new(&config.ollama_server_url)?;

    // Count words for logging (not content)
    let word_count = transcript.split_whitespace().count();
    let start_time = std::time::Instant::now();

    match client
        .generate_soap_note(&config.ollama_model, &transcript)
        .await
    {
        Ok(soap_note) => {
            let generation_time_ms = start_time.elapsed().as_millis() as u64;
            activity_log::log_soap_generation(
                "", // session_id not available here
                word_count,
                generation_time_ms,
                &config.ollama_model,
                true,
                None,
            );
            Ok(soap_note)
        }
        Err(e) => {
            let generation_time_ms = start_time.elapsed().as_millis() as u64;
            activity_log::log_soap_generation(
                "",
                word_count,
                generation_time_ms,
                &config.ollama_model,
                false,
                Some(&e),
            );
            Err(e)
        }
    }
}
