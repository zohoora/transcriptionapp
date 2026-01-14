//! LLM Router / SOAP Note generation commands

use crate::activity_log;
use crate::config::Config;
use crate::debug_storage;
use crate::ollama::{AudioEvent, LLMClient, LLMStatus, MultiPatientSoapResult, SoapNote, SoapOptions};
use tracing::{info, warn};

// Re-export LLMStatus as OllamaStatus for backward compatibility with frontend
pub use crate::ollama::OllamaStatus;

/// Check LLM router status and list available models
#[tauri::command]
pub async fn check_ollama_status() -> LLMStatus {
    let config = Config::load_or_default();
    let client = match LLMClient::new(&config.llm_router_url, &config.llm_api_key, &config.llm_client_id) {
        Ok(c) => c,
        Err(e) => {
            return LLMStatus {
                connected: false,
                available_models: vec![],
                error: Some(e),
            }
        }
    };
    client.check_status().await
}

/// List available models from LLM router
#[tauri::command]
pub async fn list_ollama_models() -> Result<Vec<String>, String> {
    let config = Config::load_or_default();
    let client = LLMClient::new(&config.llm_router_url, &config.llm_api_key, &config.llm_client_id)?;
    client.list_models().await
}

/// Pre-warm the LLM model to reduce latency on first request
///
/// This is especially useful for auto-session detection where speed is critical.
/// Should be called on app startup or when LLM settings change.
#[tauri::command]
pub async fn prewarm_ollama_model() -> Result<(), String> {
    let config = Config::load_or_default();
    let client = LLMClient::new(&config.llm_router_url, &config.llm_api_key, &config.llm_client_id)?;
    // Pre-warm the fast model (used for greeting detection)
    client.prewarm_model(&config.fast_model).await
}

/// Generate a SOAP note from the given transcript
///
/// # Arguments
/// * `transcript` - The clinical transcript text
/// * `audio_events` - Optional audio events (coughs, laughs, etc.) detected during recording
/// * `options` - Optional SOAP generation options (detail level, format, custom instructions)
/// * `session_id` - Optional session ID for debug storage correlation
#[tauri::command]
pub async fn generate_soap_note(
    transcript: String,
    audio_events: Option<Vec<AudioEvent>>,
    options: Option<SoapOptions>,
    session_id: Option<String>,
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
    let client = LLMClient::new(&config.llm_router_url, &config.llm_api_key, &config.llm_client_id)?;

    // Count words for logging (not content)
    let word_count = transcript.split_whitespace().count();
    let start_time = std::time::Instant::now();

    match client
        .generate_soap_note(
            &config.soap_model,
            &transcript,
            audio_events.as_deref(),
            options.as_ref(),
        )
        .await
    {
        Ok(soap_note) => {
            let generation_time_ms = start_time.elapsed().as_millis() as u64;
            activity_log::log_soap_generation(
                session_id.as_deref().unwrap_or(""),
                word_count,
                generation_time_ms,
                &config.soap_model,
                true,
                None,
            );

            // Save SOAP note to debug storage if enabled and session_id provided
            if config.debug_storage_enabled {
                if let Some(ref sid) = session_id {
                    if let Err(e) = debug_storage::save_soap_note_standalone(
                        sid,
                        &soap_note.content,
                        &config.soap_model,
                        true,
                    ) {
                        warn!("Failed to save SOAP note to debug storage: {}", e);
                    } else {
                        info!(session_id = %sid, "SOAP note saved to debug storage");
                    }
                }
            }

            Ok(soap_note)
        }
        Err(e) => {
            let generation_time_ms = start_time.elapsed().as_millis() as u64;
            activity_log::log_soap_generation(
                session_id.as_deref().unwrap_or(""),
                word_count,
                generation_time_ms,
                &config.soap_model,
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
/// * `session_id` - Optional session ID for debug storage correlation
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
    session_id: Option<String>,
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
    let client = LLMClient::new(&config.llm_router_url, &config.llm_api_key, &config.llm_client_id)?;

    // Count words for logging (not content)
    let word_count = transcript.split_whitespace().count();
    let start_time = std::time::Instant::now();

    match client
        .generate_multi_patient_soap_note(
            &config.soap_model,
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
                session_id.as_deref().unwrap_or(""),
                word_count,
                generation_time_ms,
                &config.soap_model,
                true,
                None,
            );
            info!(
                "Multi-patient SOAP generation complete: {} patients, physician: {:?}",
                result.notes.len(),
                result.physician_speaker
            );

            // Save SOAP notes to debug storage if enabled and session_id provided
            if config.debug_storage_enabled {
                if let Some(ref sid) = session_id {
                    // Combine all patient SOAP notes into one file for debug storage
                    let combined_soap = result.notes.iter()
                        .map(|note| format!("=== {} ({}) ===\n\n{}", note.patient_label, note.speaker_id, note.content))
                        .collect::<Vec<_>>()
                        .join("\n\n---\n\n");

                    let soap_with_header = format!(
                        "Multi-Patient SOAP Notes\nPhysician: {}\nPatients: {}\n\n{}",
                        result.physician_speaker.as_deref().unwrap_or("Unknown"),
                        result.notes.len(),
                        combined_soap
                    );

                    if let Err(e) = debug_storage::save_soap_note_standalone(
                        sid,
                        &soap_with_header,
                        &config.soap_model,
                        true,
                    ) {
                        warn!("Failed to save multi-patient SOAP notes to debug storage: {}", e);
                    } else {
                        info!(session_id = %sid, patients = result.notes.len(), "Multi-patient SOAP notes saved to debug storage");
                    }
                }
            }

            Ok(result)
        }
        Err(e) => {
            let generation_time_ms = start_time.elapsed().as_millis() as u64;
            activity_log::log_soap_generation(
                session_id.as_deref().unwrap_or(""),
                word_count,
                generation_time_ms,
                &config.soap_model,
                false,
                Some(&e),
            );
            Err(e)
        }
    }
}
