//! LLM Router / SOAP Note generation commands

use crate::activity_log;
use crate::config::Config;
use crate::debug_storage;
use crate::ollama::{AudioEvent, LLMClient, LLMStatus, MultiPatientSoapResult, SoapNote, SoapOptions, SpeakerContext, SpeakerInfo};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// Re-export LLMStatus as OllamaStatus for backward compatibility with frontend
pub use crate::ollama::OllamaStatus;

/// Select the SOAP model for generation.
/// Uses soap_model for all transcript lengths.
fn select_soap_model(config: &Config, word_count: usize) -> &str {
    info!(
        word_count = word_count,
        model = %config.soap_model,
        "Using SOAP model"
    );
    &config.soap_model
}

/// Check LLM router status and list available models
#[tauri::command]
pub async fn check_ollama_status() -> LLMStatus {
    let config = Config::load_or_default();
    let client = match LLMClient::new(&config.llm_router_url, &config.llm_api_key, &config.llm_client_id, &config.fast_model) {
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
    let client = LLMClient::new(&config.llm_router_url, &config.llm_api_key, &config.llm_client_id, &config.fast_model)?;
    client.list_models().await
}

/// Pre-warm the LLM model to reduce latency on first request
///
/// This is especially useful for auto-session detection where speed is critical.
/// Should be called on app startup or when LLM settings change.
#[tauri::command]
pub async fn prewarm_ollama_model() -> Result<(), String> {
    let config = Config::load_or_default();
    let client = LLMClient::new(&config.llm_router_url, &config.llm_api_key, &config.llm_client_id, &config.fast_model)?;
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
/// * `speaker_context` - Optional speaker identification context for better SOAP generation
#[tauri::command]
pub async fn generate_soap_note(
    transcript: String,
    audio_events: Option<Vec<AudioEvent>>,
    options: Option<SoapOptions>,
    session_id: Option<String>,
    speaker_context: Option<Vec<SpeakerInfo>>,
) -> Result<SoapNote, String> {
    info!(
        "Generating SOAP note for transcript of {} chars, {} audio events, {} speakers, options: {:?}",
        transcript.len(),
        audio_events.as_ref().map(|e| e.len()).unwrap_or(0),
        speaker_context.as_ref().map(|s| s.len()).unwrap_or(0),
        options
    );

    if transcript.trim().is_empty() {
        return Err("Cannot generate SOAP note from empty transcript".to_string());
    }

    let config = Config::load_or_default();
    let client = LLMClient::new(&config.llm_router_url, &config.llm_api_key, &config.llm_client_id, &config.fast_model)?;

    // Count words for logging and model selection
    let word_count = transcript.split_whitespace().count();

    // Build speaker context if provided
    let ctx = speaker_context.map(|speakers| {
        let mut ctx = SpeakerContext::new();
        ctx.speakers = speakers;
        ctx
    });

    // Select appropriate model based on transcript length
    let selected_model = select_soap_model(&config, word_count);
    let start_time = std::time::Instant::now();

    match client
        .generate_soap_note(
            selected_model,
            &transcript,
            audio_events.as_deref(),
            options.as_ref(),
            ctx.as_ref(),
        )
        .await
    {
        Ok(soap_note) => {
            let generation_time_ms = start_time.elapsed().as_millis() as u64;
            activity_log::log_soap_generation(
                session_id.as_deref().unwrap_or(""),
                word_count,
                generation_time_ms,
                selected_model,
                true,
                None,
            );

            // Save SOAP note to debug storage if enabled and session_id provided
            if config.debug_storage_enabled {
                if let Some(ref sid) = session_id {
                    if let Err(e) = debug_storage::save_soap_note_standalone(
                        sid,
                        &soap_note.content,
                        selected_model,
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
                selected_model,
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
/// * `speaker_context` - Optional speaker identification context for better SOAP generation
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
    speaker_context: Option<Vec<SpeakerInfo>>,
) -> Result<MultiPatientSoapResult, String> {
    info!(
        "Generating multi-patient SOAP note for transcript of {} chars, {} audio events, {} speakers",
        transcript.len(),
        audio_events.as_ref().map(|e| e.len()).unwrap_or(0),
        speaker_context.as_ref().map(|s| s.len()).unwrap_or(0),
    );

    if transcript.trim().is_empty() {
        return Err("Cannot generate SOAP note from empty transcript".to_string());
    }

    let config = Config::load_or_default();
    let client = LLMClient::new(&config.llm_router_url, &config.llm_api_key, &config.llm_client_id, &config.fast_model)?;

    // Count words for logging and model selection
    let word_count = transcript.split_whitespace().count();

    // Build speaker context if provided
    let ctx = speaker_context.map(|speakers| {
        let mut ctx = SpeakerContext::new();
        ctx.speakers = speakers;
        ctx
    });

    // Select appropriate model based on transcript length
    let selected_model = select_soap_model(&config, word_count);
    let start_time = std::time::Instant::now();

    match client
        .generate_multi_patient_soap_note(
            selected_model,
            &transcript,
            audio_events.as_deref(),
            options.as_ref(),
            ctx.as_ref(),
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
                selected_model,
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
                        selected_model,
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
                selected_model,
                false,
                Some(&e),
            );
            Err(e)
        }
    }
}

/// Response from predictive hint generation including MIIS image concepts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictiveHintResponse {
    /// Brief clinical hint for the physician (max 60 chars)
    pub hint: String,
    /// Medical concepts for MIIS image search (de-identified)
    pub concepts: Vec<ImageConcept>,
}

/// A medical concept for MIIS image search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConcept {
    /// The concept text (e.g., "knee anatomy", "rotator cuff")
    pub text: String,
    /// Relevance weight 0.0-1.0
    pub weight: f64,
}

/// Generate a predictive hint and image concepts based on the current transcript
/// Returns both a clinical hint and MIIS image search concepts
#[tauri::command]
pub async fn generate_predictive_hint(transcript: String) -> Result<PredictiveHintResponse, String> {
    let empty_response = PredictiveHintResponse {
        hint: String::new(),
        concepts: Vec::new(),
    };

    if transcript.trim().is_empty() || transcript.split_whitespace().count() < 20 {
        return Ok(empty_response); // Not enough content yet
    }

    let config = Config::load_or_default();
    let client = LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &config.soap_model,
    )?;

    let system_prompt = r#"You are a clinical assistant analyzing a medical transcript. Provide TWO things:

1. HINT: A brief clinical fact the physician might need right now (max 60 chars, shorthand style)
2. CONCEPTS: 1-5 medical image search terms for relevant anatomical diagrams or illustrations

Respond ONLY with this JSON format:
{"hint":"brief clinical fact here","concepts":[{"text":"anatomy term","weight":0.9},{"text":"condition","weight":0.7}]}

RULES for hint:
- Maximum 60 characters
- Use shorthand, not full sentences
- Focus on: dosages, red flags, key values, quick reminders

RULES for concepts:
- Use SIMPLE, common anatomical terms (1-3 words max)
- Prefer basic anatomy: "knee anatomy", "heart anatomy", "liver", "spine"
- Avoid complex phrases like "iron metabolism pathway" - use "blood cells" or "bone marrow" instead
- NO patient names, NO PII, NO protocols/procedures
- Focus on: body parts, organs, basic conditions that have anatomical diagrams
- Weight 0.0-1.0 based on relevance to current discussion
- GOOD examples: "knee anatomy", "rotator cuff", "heart valves", "lumbar spine", "anemia", "thyroid"
- BAD examples: "iron metabolism pathway", "ferritin levels reference ranges", "McMurray test positioning"

If no relevant image concepts, return empty array: "concepts":[]
"#;

    // Truncate transcript if too long (keep last ~2000 words for context)
    let words: Vec<&str> = transcript.split_whitespace().collect();
    let truncated = if words.len() > 2000 {
        words[words.len() - 2000..].join(" ")
    } else {
        transcript.clone()
    };

    let user_content = format!("Current transcript:\n\n{}", truncated);

    match client
        .generate(&config.soap_model, system_prompt, &user_content, "predictive_hint")
        .await
    {
        Ok(response) => {
            // Try to parse as JSON first
            if let Some(parsed) = parse_hint_response(&response) {
                info!("Predictive hint parsed: hint={} chars, concepts={}",
                      parsed.hint.len(), parsed.concepts.len());
                return Ok(parsed);
            }

            // Fallback: treat as plain text hint (backwards compatibility)
            let cleaned: String = response
                .trim()
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .replace("**", "")
                .replace("*", "")
                .to_string();

            Ok(PredictiveHintResponse {
                hint: cleaned,
                concepts: Vec::new(),
            })
        }
        Err(e) => {
            warn!("Failed to generate predictive hint: {}", e);
            Ok(empty_response) // Return empty on error, don't fail the whole thing
        }
    }
}

/// Parse the JSON response from the LLM for hint + concepts
fn parse_hint_response(response: &str) -> Option<PredictiveHintResponse> {
    // Find JSON in response (may have markdown or other text around it)
    let text = response.replace("```json", "").replace("```", "");

    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            let json_str = &text[start..=end];

            #[derive(Deserialize)]
            struct RawResponse {
                hint: Option<String>,
                concepts: Option<Vec<RawConcept>>,
            }

            #[derive(Deserialize)]
            struct RawConcept {
                text: String,
                weight: Option<f64>,
            }

            if let Ok(raw) = serde_json::from_str::<RawResponse>(json_str) {
                let hint = raw.hint.unwrap_or_default()
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();

                let concepts: Vec<ImageConcept> = raw.concepts
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|c| !c.text.trim().is_empty())
                    .map(|c| ImageConcept {
                        text: c.text.trim().to_string(),
                        weight: c.weight.unwrap_or(1.0).clamp(0.0, 1.0),
                    })
                    .take(5) // Max 5 concepts
                    .collect();

                return Some(PredictiveHintResponse { hint, concepts });
            }
        }
    }

    None
}
