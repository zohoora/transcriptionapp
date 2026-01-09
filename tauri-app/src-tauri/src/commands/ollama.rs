//! Ollama / SOAP Note generation commands

use crate::activity_log;
use crate::config::Config;
use crate::ollama::{AudioEvent, MultiPatientSoapResult, OllamaClient, OllamaStatus, SoapNote, SoapOptions};
use tracing::info;

/// Check Ollama server status and list available models
#[tauri::command]
pub async fn check_ollama_status() -> OllamaStatus {
    let config = Config::load_or_default();
    let client = match OllamaClient::new(&config.ollama_server_url, config.ollama_keep_alive) {
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
    let client = OllamaClient::new(&config.ollama_server_url, config.ollama_keep_alive)?;
    client.list_models().await
}

/// Pre-warm the Ollama model to reduce latency on first request
///
/// This is especially useful for auto-session detection where speed is critical.
/// Should be called on app startup or when Ollama settings change.
#[tauri::command]
pub async fn prewarm_ollama_model() -> Result<(), String> {
    let config = Config::load_or_default();
    let client = OllamaClient::new(&config.ollama_server_url, config.ollama_keep_alive)?;
    client.prewarm_model(&config.ollama_model).await
}

/// Generate a SOAP note from the given transcript
///
/// # Arguments
/// * `transcript` - The clinical transcript text
/// * `audio_events` - Optional audio events (coughs, laughs, etc.) detected during recording
/// * `options` - Optional SOAP generation options (detail level, format, custom instructions)
#[tauri::command]
pub async fn generate_soap_note(
    transcript: String,
    audio_events: Option<Vec<AudioEvent>>,
    options: Option<SoapOptions>,
) -> Result<SoapNote, String> {
    info!(
        "Generating SOAP note for transcript of {} chars, {} audio events, options: {:?}",
        transcript.len(),
        audio_events.as_ref().map(|e| e.len()).unwrap_or(0),
        options
    );

    if transcript.trim().is_empty() {
        return Err("Cannot generate SOAP note from empty transcript".to_string());
    }

    let config = Config::load_or_default();
    let client = OllamaClient::new(&config.ollama_server_url, config.ollama_keep_alive)?;

    // Count words for logging (not content)
    let word_count = transcript.split_whitespace().count();
    let start_time = std::time::Instant::now();

    match client
        .generate_soap_note(
            &config.ollama_model,
            &transcript,
            audio_events.as_deref(),
            options.as_ref(),
        )
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

/// Generate SOAP notes with automatic patient/physician detection
///
/// The LLM analyzes the transcript to identify which speakers are patients
/// vs the physician, then generates separate SOAP notes for each patient.
/// This supports visits with multiple patients (e.g., couples, families).
///
/// # Arguments
/// * `transcript` - The clinical transcript text (with speaker labels)
/// * `audio_events` - Optional audio events (coughs, laughs, etc.) detected during recording
/// * `options` - Optional SOAP generation options (detail level, custom instructions)
///
/// # Returns
/// A `MultiPatientSoapResult` containing:
/// - `notes`: Vec of PatientSoapNote (one per patient, 1-4 patients)
/// - `physician_speaker`: Which speaker was identified as the physician
/// - `generated_at`: Timestamp
/// - `model_used`: Model name
#[tauri::command]
pub async fn generate_soap_note_auto_detect(
    transcript: String,
    audio_events: Option<Vec<AudioEvent>>,
    options: Option<SoapOptions>,
) -> Result<MultiPatientSoapResult, String> {
    info!(
        "Generating multi-patient SOAP note for transcript of {} chars, {} audio events",
        transcript.len(),
        audio_events.as_ref().map(|e| e.len()).unwrap_or(0),
    );

    if transcript.trim().is_empty() {
        return Err("Cannot generate SOAP note from empty transcript".to_string());
    }

    let config = Config::load_or_default();
    let client = OllamaClient::new(&config.ollama_server_url, config.ollama_keep_alive)?;

    // Count words for logging (not content)
    let word_count = transcript.split_whitespace().count();
    let start_time = std::time::Instant::now();

    match client
        .generate_multi_patient_soap_note(
            &config.ollama_model,
            &transcript,
            audio_events.as_deref(),
            options.as_ref(),
        )
        .await
    {
        Ok(result) => {
            let generation_time_ms = start_time.elapsed().as_millis() as u64;
            // Log as multi-patient generation
            activity_log::log_soap_generation(
                "",
                word_count,
                generation_time_ms,
                &config.ollama_model,
                true,
                None,
            );
            info!(
                "Multi-patient SOAP generation complete: {} patients, physician: {:?}",
                result.notes.len(),
                result.physician_speaker
            );
            Ok(result)
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
