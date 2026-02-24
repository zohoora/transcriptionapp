//! LLM Router / SOAP Note generation commands

use crate::activity_log;
use crate::config::Config;
use crate::debug_storage;
use crate::ollama::{AudioEvent, LLMClient, LLMStatus, MultiPatientSoapResult, SoapNote, SoapOptions, SpeakerContext, SpeakerInfo};
use crate::screenshot;
use crate::vision_experiment::{
    self, ExperimentParams, ExperimentResult, PromptStrategy,
    run_experiment, save_result, load_results, generate_summary_report,
};
use super::SharedScreenCaptureState;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};


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

            // Shadow on-device SOAP generation (fire-and-forget)
            if config.on_device_soap_shadow_enabled {
                let transcript_clone = transcript.clone();
                let session_id_clone = session_id.clone();
                let primary_latency = generation_time_ms;
                let primary_content = result.notes.iter()
                    .map(|n| n.content.clone()).collect::<Vec<_>>().join("\n\n---\n\n");
                let detail_level = options.as_ref().map(|o| o.detail_level).unwrap_or(5);

                std::thread::spawn(move || {
                    let start = std::time::Instant::now();
                    match crate::on_device_llm::OnDeviceLLMClient::new() {
                        Ok(client) => match client.generate_soap(&transcript_clone, detail_level, "problem_based") {
                            Ok(shadow_soap) => {
                                let ondevice_latency = start.elapsed().as_millis() as u64;
                                // Save to archive
                                if let Some(ref sid) = session_id_clone {
                                    let now = chrono::Utc::now();
                                    let _ = crate::local_archive::add_shadow_soap_note(sid, &now, &shadow_soap);
                                }
                                // Log to CSV
                                let primary_wc = primary_content.split_whitespace().count();
                                let ondevice_wc = shadow_soap.split_whitespace().count();
                                let primary_sections = crate::on_device_llm::count_soap_sections(&primary_content);
                                let ondevice_sections = crate::on_device_llm::count_soap_sections(&shadow_soap);
                                let _ = crate::on_device_soap_shadow::OnDeviceSoapCsvLogger::new()
                                    .map(|mut logger| logger.log(&crate::on_device_soap_shadow::SoapComparisonMetrics {
                                        session_id: session_id_clone.unwrap_or_default(),
                                        primary_word_count: primary_wc,
                                        ondevice_word_count: ondevice_wc,
                                        primary_latency_ms: primary_latency,
                                        ondevice_latency_ms: ondevice_latency,
                                        primary_section_count: primary_sections,
                                        ondevice_section_count: ondevice_sections,
                                    }));
                                info!("Shadow on-device SOAP generated in {}ms ({} words)", ondevice_latency, ondevice_wc);
                            }
                            Err(e) => warn!("Shadow on-device SOAP failed: {}", e),
                        },
                        Err(e) => warn!("On-device LLM not available for shadow SOAP: {}", e),
                    }
                });
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

/// Generate a vision SOAP note using transcript + stitched screenshots.
///
/// Sends the transcript and a composite image (stitched thumbnails) to the
/// vision-model LLM alias. This is an experimental parallel code path
/// that does not affect existing SOAP generation.
///
/// # Arguments
/// * `transcript` - The clinical transcript text
/// * `audio_events` - Optional audio events
/// * `options` - Optional SOAP generation options
/// * `session_id` - Optional session ID for logging
/// * `screenshot_state` - Shared screen capture state containing screenshot paths
#[tauri::command]
pub async fn generate_vision_soap_note(
    transcript: String,
    audio_events: Option<Vec<AudioEvent>>,
    options: Option<SoapOptions>,
    session_id: Option<String>,
    image_path: Option<String>,
    screenshot_state: tauri::State<'_, SharedScreenCaptureState>,
) -> Result<SoapNote, String> {
    info!(
        "Generating vision SOAP note for transcript of {} chars, image_path: {:?}",
        transcript.len(),
        image_path,
    );

    if transcript.trim().is_empty() {
        return Err("Cannot generate vision SOAP note from empty transcript".to_string());
    }

    // Build base64 image: either from a user-selected path or the legacy stitch flow
    let image_base64 = if let Some(ref path) = image_path {
        // Single image mode: read and base64-encode the selected file
        let data = std::fs::read(path)
            .map_err(|e| format!("Failed to read image at {:?}: {}", path, e))?;
        info!("Using single image for vision SOAP: {} ({} bytes)", path, data.len());
        base64::engine::general_purpose::STANDARD.encode(&data)
    } else {
        // Legacy stitch mode: select and stitch thumbnails
        let paths = {
            let capture = screenshot_state.lock().map_err(|e| e.to_string())?;
            capture.screenshot_paths()
        };

        if paths.is_empty() {
            return Err("No screenshots available for vision SOAP generation".to_string());
        }

        let selected = screenshot::select_thumbnails(&paths, 3);
        if selected.is_empty() {
            return Err("No thumbnail screenshots found".to_string());
        }

        info!("Selected {} thumbnails for vision SOAP (legacy stitch)", selected.len());
        screenshot::stitch_thumbnails_to_base64(&selected)?
    };

    // Create LLM client and generate
    let config = Config::load_or_default();
    let client = LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &config.fast_model,
    )?;

    let start_time = std::time::Instant::now();
    let word_count = transcript.split_whitespace().count();
    let model = "vision-model";

    match client
        .generate_vision_soap_note(
            model,
            &transcript,
            &image_base64,
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
                model,
                true,
                None,
            );
            info!(
                "Vision SOAP note generated in {}ms ({} chars)",
                generation_time_ms,
                soap_note.content.len()
            );
            Ok(soap_note)
        }
        Err(e) => {
            let generation_time_ms = start_time.elapsed().as_millis() as u64;
            activity_log::log_soap_generation(
                session_id.as_deref().unwrap_or(""),
                word_count,
                generation_time_ms,
                model,
                false,
                Some(&e),
            );
            Err(e)
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

// ============================================================================
// Vision SOAP Prompt Experimentation Commands
// ============================================================================

/// Request to run vision prompt experiments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionExperimentRequest {
    /// Path to transcript file
    pub transcript_path: String,
    /// Path to EHR screenshot image
    pub image_path: String,
    /// Which prompt strategies to test (empty = all)
    pub strategies: Vec<String>,
    /// Temperature values to test (empty = [0.3])
    pub temperatures: Vec<f32>,
    /// Whether to test image-first ordering
    pub test_image_order: bool,
}

/// Summary of experiment results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionExperimentSummary {
    pub total_experiments: usize,
    pub best_strategy: String,
    pub best_score: i32,
    pub results: Vec<ExperimentResultSummary>,
    pub report_path: String,
}

/// Condensed result for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResultSummary {
    pub strategy: String,
    pub temperature: f32,
    pub score: i32,
    pub has_patient_name: bool,
    pub has_medication_name: bool,
    pub has_weight_issue: bool,
    pub irrelevant_count: usize,
    pub generation_time_ms: u64,
    pub result_path: String,
}

/// Run vision prompt experiments
///
/// Tests different prompt strategies for vision SOAP generation to find
/// the optimal approach for using EHR screenshots appropriately.
///
/// # Arguments
/// * `request` - Experiment configuration including paths and strategies to test
#[tauri::command]
pub async fn run_vision_experiments(
    request: VisionExperimentRequest,
) -> Result<VisionExperimentSummary, String> {
    let strategy_count = if request.strategies.is_empty() {
        "all".to_string()
    } else {
        request.strategies.len().to_string()
    };
    let temp_count = if request.temperatures.is_empty() {
        "default".to_string()
    } else {
        request.temperatures.len().to_string()
    };
    info!(
        "Running vision experiments: {} strategies, {} temperatures",
        strategy_count,
        temp_count
    );

    // Load transcript
    let transcript = std::fs::read_to_string(&request.transcript_path)
        .map_err(|e| format!("Failed to read transcript: {}", e))?;

    // Load and encode image
    let image_data = std::fs::read(&request.image_path)
        .map_err(|e| format!("Failed to read image: {}", e))?;
    let image_base64 = base64::engine::general_purpose::STANDARD.encode(&image_data);

    // Create LLM client
    let config = Config::load_or_default();
    let client = LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &config.fast_model,
    )?;

    // Determine strategies to test
    let strategies: Vec<PromptStrategy> = if request.strategies.is_empty() {
        PromptStrategy::all()
    } else {
        request.strategies.iter()
            .filter_map(|s| match s.as_str() {
                "current" | "p0" => Some(PromptStrategy::Current),
                "negative" | "p1" => Some(PromptStrategy::NegativeFraming),
                "flip" | "p2" => Some(PromptStrategy::FlipDefault),
                "twostep" | "p3" => Some(PromptStrategy::TwoStepReasoning),
                "prominent" | "p4" => Some(PromptStrategy::ProminentPlacement),
                "examples" | "p5" => Some(PromptStrategy::ConcreteExamples),
                "minimal" | "p6" => Some(PromptStrategy::MinimalImage),
                _ => None,
            })
            .collect()
    };

    // Determine temperatures to test
    let temperatures: Vec<f32> = if request.temperatures.is_empty() {
        vec![0.3]
    } else {
        request.temperatures.clone()
    };

    // Run experiments
    let mut results: Vec<ExperimentResult> = Vec::new();
    let mut result_summaries: Vec<ExperimentResultSummary> = Vec::new();

    for strategy in &strategies {
        for &temp in &temperatures {
            let orderings = if request.test_image_order {
                vec![false, true] // text-first, then image-first
            } else {
                vec![false] // text-first only
            };

            for image_first in orderings {
                let params = ExperimentParams {
                    strategy: *strategy,
                    temperature: temp,
                    max_tokens: 2000,
                    image_first,
                };

                match run_experiment(&client, "vision-model", &transcript, &image_base64, &params).await {
                    Ok(result) => {
                        // Save result to disk
                        let result_path = match save_result(&result) {
                            Ok(p) => p.to_string_lossy().to_string(),
                            Err(e) => {
                                warn!("Failed to save result: {}", e);
                                String::new()
                            }
                        };

                        // Create summary
                        let summary = ExperimentResultSummary {
                            strategy: strategy.name().to_string(),
                            temperature: temp,
                            score: result.score.total_score,
                            has_patient_name: result.score.has_patient_name,
                            has_medication_name: result.score.has_medication_name,
                            has_weight_issue: result.score.has_weight_issue,
                            irrelevant_count: result.score.irrelevant_inclusions.len(),
                            generation_time_ms: result.generation_time_ms,
                            result_path,
                        };

                        info!(
                            "Experiment complete: {} temp={} score={} (patient={}, med={}, weight={}, irrelevant={})",
                            strategy.name(),
                            temp,
                            result.score.total_score,
                            result.score.has_patient_name,
                            result.score.has_medication_name,
                            result.score.has_weight_issue,
                            result.score.irrelevant_inclusions.len()
                        );

                        result_summaries.push(summary);
                        results.push(result);
                    }
                    Err(e) => {
                        error!("Experiment failed: {} temp={} - {}", strategy.name(), temp, e);
                    }
                }
            }
        }
    }

    // Generate and save summary report
    let report = generate_summary_report(&results);
    let report_path = vision_experiment::experiments_dir().join("summary.md");
    if let Err(e) = std::fs::write(&report_path, &report) {
        warn!("Failed to save summary report: {}", e);
    }

    // Find best result
    let best = results.iter()
        .max_by_key(|r| r.score.total_score)
        .map(|r| (r.params.strategy.name().to_string(), r.score.total_score))
        .unwrap_or_else(|| ("None".to_string(), 0));

    Ok(VisionExperimentSummary {
        total_experiments: results.len(),
        best_strategy: best.0,
        best_score: best.1,
        results: result_summaries,
        report_path: report_path.to_string_lossy().to_string(),
    })
}

/// Get all saved experiment results
#[tauri::command]
pub async fn get_vision_experiment_results() -> Result<Vec<ExperimentResult>, String> {
    load_results()
}

/// Get the summary report for all experiments
#[tauri::command]
pub async fn get_vision_experiment_report() -> Result<String, String> {
    let results = load_results()?;
    Ok(generate_summary_report(&results))
}

/// List available prompt strategies
#[tauri::command]
pub async fn list_vision_experiment_strategies() -> Vec<(String, String)> {
    PromptStrategy::all()
        .iter()
        .map(|s| (s.id().to_string(), s.name().to_string()))
        .collect()
}
