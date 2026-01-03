//! Ollama API client for SOAP note generation
//!
//! This module provides integration with Ollama LLM servers for generating
//! structured SOAP (Subjective, Objective, Assessment, Plan) notes from
//! clinical transcripts.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info};

/// Default timeout for Ollama API requests (2 minutes for generation)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Request body for Ollama generate endpoint
#[derive(Debug, Clone, Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

/// Options for Ollama generation
#[derive(Debug, Clone, Serialize)]
struct OllamaOptions {
    /// Disable thinking/reasoning mode for faster generation (Qwen, DeepSeek, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u32>,
}

/// Response from Ollama generate endpoint
#[derive(Debug, Clone, Deserialize)]
struct OllamaGenerateResponse {
    response: String,
    #[allow(dead_code)]
    done: bool,
}

/// Model info from Ollama tags endpoint
#[derive(Debug, Clone, Deserialize)]
struct OllamaModelInfo {
    name: String,
}

/// Response from Ollama tags endpoint (list models)
#[derive(Debug, Clone, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelInfo>,
}

/// Status of the Ollama connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaStatus {
    pub connected: bool,
    pub available_models: Vec<String>,
    pub error: Option<String>,
}

/// Audio event detected during recording (cough, laugh, sneeze, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioEvent {
    /// Timestamp in milliseconds from start of recording
    pub timestamp_ms: u64,
    /// Duration of the event in milliseconds
    pub duration_ms: u32,
    /// Model confidence score (raw logit value, higher = more confident)
    pub confidence: f32,
    /// Event label (e.g., "Cough", "Laughter", "Sneeze", "Throat clearing")
    pub label: String,
}

/// Generated SOAP note
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoapNote {
    pub subjective: String,
    pub objective: String,
    pub assessment: String,
    pub plan: String,
    pub generated_at: String,
    pub model_used: String,
    /// Raw response from the model (for debugging)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_response: Option<String>,
}

/// Ollama API client
#[derive(Debug)]
pub struct OllamaClient {
    client: reqwest::Client,
    base_url: String,
}

impl OllamaClient {
    /// Create a new Ollama client with URL validation
    pub fn new(base_url: &str) -> Result<Self, String> {
        let cleaned_url = base_url.trim_end_matches('/');

        // Validate URL format and scheme
        let parsed = reqwest::Url::parse(cleaned_url)
            .map_err(|e| format!("Invalid Ollama URL '{}': {}", cleaned_url, e))?;

        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(format!(
                "Ollama URL must use http or https scheme, got: {}",
                parsed.scheme()
            ));
        }

        // Reject URLs with credentials (security risk)
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("Ollama URL must not contain credentials".to_string());
        }

        let client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            client,
            base_url: cleaned_url.to_string(),
        })
    }

    /// Check connection status and list available models
    pub async fn check_status(&self) -> OllamaStatus {
        match self.list_models().await {
            Ok(models) => OllamaStatus {
                connected: true,
                available_models: models,
                error: None,
            },
            Err(e) => OllamaStatus {
                connected: false,
                available_models: vec![],
                error: Some(e),
            },
        }
    }

    /// List available models from Ollama
    pub async fn list_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/api/tags", self.base_url);
        debug!("Listing Ollama models from {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to Ollama: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Ollama returned error status: {}",
                response.status()
            ));
        }

        let tags: OllamaTagsResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

        let models: Vec<String> = tags.models.into_iter().map(|m| m.name).collect();
        info!("Found {} Ollama models", models.len());

        Ok(models)
    }

    /// Generate text using Ollama
    async fn generate(&self, model: &str, prompt: &str) -> Result<String, String> {
        // Input validation
        if model.trim().is_empty() {
            return Err("Model name cannot be empty".to_string());
        }
        if prompt.trim().is_empty() {
            return Err("Prompt cannot be empty".to_string());
        }

        let url = format!("{}/api/generate", self.base_url);
        debug!("Generating with Ollama model {} at {}", model, url);

        let request = OllamaGenerateRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            stream: false,
            options: None,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to Ollama: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Ollama generate failed: {} - {}", status, body);
            return Err(format!("Ollama returned error: {} - {}", status, body));
        }

        let gen_response: OllamaGenerateResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

        Ok(gen_response.response)
    }

    /// Maximum transcript size (100KB) to prevent memory issues
    const MAX_TRANSCRIPT_SIZE: usize = 100_000;

    /// Minimum transcript length (50 chars) to ensure meaningful SOAP generation
    const MIN_TRANSCRIPT_LENGTH: usize = 50;

    /// Minimum word count for meaningful SOAP generation
    /// Set to 5 to allow short clinical notes like "Patient reports symptoms resolved."
    const MIN_WORD_COUNT: usize = 5;

    /// Generate a SOAP note from a clinical transcript
    ///
    /// # Arguments
    /// * `model` - The Ollama model to use
    /// * `transcript` - The clinical transcript text
    /// * `audio_events` - Optional audio events (coughs, laughs, etc.) detected during recording
    pub async fn generate_soap_note(
        &self,
        model: &str,
        transcript: &str,
        audio_events: Option<&[AudioEvent]>,
    ) -> Result<SoapNote, String> {
        let trimmed = transcript.trim();

        // Validate transcript is not empty
        if trimmed.is_empty() {
            return Err("Transcript cannot be empty".to_string());
        }

        // Validate minimum length
        if trimmed.len() < Self::MIN_TRANSCRIPT_LENGTH {
            return Err(format!(
                "Transcript too short ({} characters). Minimum {} characters required for meaningful SOAP note generation.",
                trimmed.len(),
                Self::MIN_TRANSCRIPT_LENGTH
            ));
        }

        // Validate minimum word count
        let word_count = trimmed.split_whitespace().count();
        if word_count < Self::MIN_WORD_COUNT {
            return Err(format!(
                "Transcript has too few words ({} words). Minimum {} words required for meaningful SOAP note generation.",
                word_count,
                Self::MIN_WORD_COUNT
            ));
        }

        // Validate maximum size
        if transcript.len() > Self::MAX_TRANSCRIPT_SIZE {
            return Err(format!(
                "Transcript too large ({} bytes). Maximum size is {} bytes",
                transcript.len(),
                Self::MAX_TRANSCRIPT_SIZE
            ));
        }

        info!(
            "Generating SOAP note with model {} for transcript of {} chars, {} audio events",
            model,
            transcript.len(),
            audio_events.map(|e| e.len()).unwrap_or(0)
        );

        // First attempt
        let prompt = build_soap_prompt(transcript, audio_events);
        let response = self.generate(model, &prompt).await?;

        match parse_soap_response(&response, model) {
            Ok(mut soap_note) => {
                soap_note.raw_response = Some(response);
                info!("Successfully generated SOAP note on first attempt");
                Ok(soap_note)
            }
            Err(first_error) => {
                // Retry with more explicit prompt
                info!("First SOAP generation attempt failed, retrying with stricter prompt...");

                let retry_prompt = build_soap_retry_prompt(transcript, audio_events, &response);
                match self.generate(model, &retry_prompt).await {
                    Ok(retry_response) => {
                        match parse_soap_response(&retry_response, model) {
                            Ok(mut soap_note) => {
                                soap_note.raw_response = Some(retry_response);
                                info!("Successfully generated SOAP note on retry");
                                Ok(soap_note)
                            }
                            Err(retry_error) => {
                                // Both attempts failed - return error with raw responses
                                error!("SOAP generation failed after retry. First response: {}, Retry response: {}",
                                    &response.chars().take(200).collect::<String>(),
                                    &retry_response.chars().take(200).collect::<String>()
                                );
                                Err(format!(
                                    "Failed to parse SOAP note after retry.\n\nFirst error: {}\n\nRetry error: {}\n\nRaw response: {}",
                                    first_error,
                                    retry_error,
                                    retry_response
                                ))
                            }
                        }
                    }
                    Err(gen_error) => {
                        // Retry generation itself failed
                        Err(format!(
                            "SOAP generation failed: {}. Retry also failed: {}. Raw response from first attempt: {}",
                            first_error,
                            gen_error,
                            response
                        ))
                    }
                }
            }
        }
    }
}

/// Format audio events for inclusion in the prompt
fn format_audio_events(events: &[AudioEvent]) -> String {
    if events.is_empty() {
        return String::new();
    }

    let mut output = String::from("\nAUDIO EVENTS DETECTED:\n");
    for event in events {
        // Format timestamp as MM:SS
        let total_seconds = event.timestamp_ms / 1000;
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;

        // Format confidence as percentage-like (logit 1.5 → ~75%, 2.0 → ~88%, 3.0 → ~95%)
        // Using sigmoid-like mapping: conf_pct = 100 / (1 + e^(-confidence))
        let conf_pct = 100.0 / (1.0 + (-event.confidence).exp());

        output.push_str(&format!(
            "- {} at {}:{:02} (confidence: {:.0}%)\n",
            event.label, minutes, seconds, conf_pct
        ));
    }
    output
}

/// Build the prompt for SOAP note generation
fn build_soap_prompt(transcript: &str, audio_events: Option<&[AudioEvent]>) -> String {
    let audio_section = audio_events
        .filter(|e| !e.is_empty())
        .map(format_audio_events)
        .unwrap_or_default();

    let audio_instruction = if audio_section.is_empty() {
        String::new()
    } else {
        String::from("\n- Consider audio events (coughs, laughs, etc.) when relevant to the clinical picture")
    };

    format!(
        r#"/no_think You are a medical scribe assistant. Based on the following clinical transcript, generate a SOAP note.

TRANSCRIPT:
{}{}

Respond with ONLY valid JSON in this exact format:
{{
  "subjective": "Patient's reported symptoms, history, and concerns from the visit",
  "objective": "Observable findings, vital signs, examination results mentioned",
  "assessment": "Clinical impression and potential diagnoses",
  "plan": "Recommended treatments, tests, follow-ups"
}}

Rules:
- Only include information explicitly mentioned or directly inferable from the transcript
- Use "No information available" if a section has no relevant content{}
- Output ONLY the JSON object, no markdown, no explanation, no other text"#,
        transcript, audio_section, audio_instruction
    )
}

/// Build a stricter retry prompt when the first attempt fails
fn build_soap_retry_prompt(
    transcript: &str,
    audio_events: Option<&[AudioEvent]>,
    _previous_response: &str,
) -> String {
    let audio_section = audio_events
        .filter(|e| !e.is_empty())
        .map(format_audio_events)
        .unwrap_or_default();

    format!(
        r#"/no_think IMPORTANT: You MUST respond with ONLY a JSON object. No conversation, no questions, no explanations.

Even if the transcript is short, unclear, or incomplete, you MUST still output valid JSON.

TRANSCRIPT:
{}{}

OUTPUT FORMAT (respond with ONLY this JSON structure):
{{"subjective":"...","objective":"...","assessment":"...","plan":"..."}}

If any section lacks information, use "No information available" as the value.
DO NOT ask questions. DO NOT explain. ONLY output the JSON object."#,
        transcript, audio_section
    )
}

/// JSON structure for LLM response (without metadata fields)
#[derive(Debug, Clone, Deserialize)]
struct SoapNoteJson {
    subjective: String,
    objective: String,
    assessment: String,
    plan: String,
}

/// Parse the LLM response into a structured SOAP note
/// Note: raw_response field is set to None here; caller should set it if needed
fn parse_soap_response(response: &str, model: &str) -> Result<SoapNote, String> {
    // Clean up the response
    let clean_response = response
        .trim()
        // Remove thinking blocks if present (Qwen models)
        .split("</think>")
        .last()
        .unwrap_or(response)
        .trim();

    // Extract JSON from markdown code blocks if present
    let json_str = extract_json(clean_response);

    // Parse as JSON
    match serde_json::from_str::<SoapNoteJson>(&json_str) {
        Ok(parsed) => {
            // Check if all fields are empty (LLM returned no useful content)
            let has_subjective = !parsed.subjective.trim().is_empty();
            let has_objective = !parsed.objective.trim().is_empty();
            let has_assessment = !parsed.assessment.trim().is_empty();
            let has_plan = !parsed.plan.trim().is_empty();

            if !has_subjective && !has_objective && !has_assessment && !has_plan {
                return Err(
                    "SOAP note generation returned empty content for all sections. \
                     The transcript may not contain enough clinical information."
                        .to_string(),
                );
            }

            info!("Successfully parsed SOAP note JSON");
            Ok(SoapNote {
                subjective: if parsed.subjective.is_empty() {
                    "No information available.".to_string()
                } else {
                    parsed.subjective
                },
                objective: if parsed.objective.is_empty() {
                    "No information available.".to_string()
                } else {
                    parsed.objective
                },
                assessment: if parsed.assessment.is_empty() {
                    "No information available.".to_string()
                } else {
                    parsed.assessment
                },
                plan: if parsed.plan.is_empty() {
                    "No information available.".to_string()
                } else {
                    parsed.plan
                },
                generated_at: Utc::now().to_rfc3339(),
                model_used: model.to_string(),
                raw_response: None, // Set by caller if needed
            })
        }
        Err(e) => {
            // Log the response for debugging
            tracing::warn!(
                "Failed to parse SOAP note JSON: {}. Response preview: {}",
                e,
                &json_str.chars().take(500).collect::<String>()
            );
            Err(format!(
                "Could not parse SOAP note JSON: {}. Response started with: {}...",
                e,
                &json_str.chars().take(100).collect::<String>()
            ))
        }
    }
}

/// Extract JSON from response, handling markdown code blocks
fn extract_json(text: &str) -> String {
    let trimmed = text.trim();

    // Check for markdown code blocks: ```json ... ``` or ``` ... ```
    if trimmed.starts_with("```") {
        // Find the end of the opening fence line
        if let Some(start_idx) = trimmed.find('\n') {
            let after_fence = &trimmed[start_idx + 1..];
            // Find the closing fence
            if let Some(end_idx) = after_fence.rfind("```") {
                return after_fence[..end_idx].trim().to_string();
            }
        }
    }

    // No code block, return as-is
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_soap_response_json() {
        let response = r#"{
            "subjective": "Patient complains of cough for 3 days.",
            "objective": "Lungs clear on auscultation. No fever.",
            "assessment": "Viral upper respiratory infection.",
            "plan": "Rest, fluids, and OTC cough suppressant."
        }"#;

        let soap = parse_soap_response(response, "qwen3:4b").unwrap();
        assert!(soap.subjective.contains("cough"));
        assert!(soap.objective.contains("Lungs clear"));
        assert!(soap.assessment.contains("Viral"));
        assert!(soap.plan.contains("Rest"));
        assert_eq!(soap.model_used, "qwen3:4b");
    }

    #[test]
    fn test_parse_soap_response_with_markdown_code_block() {
        let response = r#"```json
{
    "subjective": "Patient reports headache.",
    "objective": "Vital signs normal.",
    "assessment": "Tension headache.",
    "plan": "Ibuprofen as needed."
}
```"#;

        let soap = parse_soap_response(response, "llama3:8b").unwrap();
        assert!(soap.subjective.contains("headache"));
        assert!(soap.objective.contains("Vital signs"));
        assert!(soap.assessment.contains("Tension"));
        assert!(soap.plan.contains("Ibuprofen"));
    }

    #[test]
    fn test_parse_soap_response_with_think_block() {
        let response = r#"<think>
Let me analyze this transcript...
</think>

{
    "subjective": "Patient reports fever and chills.",
    "objective": "Temperature 101.5F.",
    "assessment": "Possible viral infection.",
    "plan": "Monitor temperature, hydrate."
}"#;

        let soap = parse_soap_response(response, "qwen3:4b").unwrap();
        assert!(soap.subjective.contains("fever"));
        assert!(!soap.subjective.contains("think"));
    }

    #[test]
    fn test_parse_soap_response_empty_sections() {
        let response = r#"{
            "subjective": "",
            "objective": "Blood pressure 120/80.",
            "assessment": "",
            "plan": "Continue current medications."
        }"#;

        let soap = parse_soap_response(response, "qwen3:4b").unwrap();
        assert_eq!(soap.subjective, "No information available.");
        assert!(soap.objective.contains("Blood pressure"));
        assert_eq!(soap.assessment, "No information available.");
        assert!(soap.plan.contains("Continue"));
    }

    #[test]
    fn test_parse_soap_response_invalid_json() {
        let response = "Some random text without JSON";
        let result = parse_soap_response(response, "qwen3:4b");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_soap_response_all_empty_sections() {
        // When all sections are empty, should return an error
        let response = r#"{
            "subjective": "",
            "objective": "",
            "assessment": "",
            "plan": ""
        }"#;

        let result = parse_soap_response(response, "qwen3:4b");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("empty content for all sections"));
    }

    #[test]
    fn test_parse_soap_response_whitespace_only_sections() {
        // Whitespace-only sections should also be considered empty
        let response = r#"{
            "subjective": "   ",
            "objective": "\t\n",
            "assessment": "  ",
            "plan": ""
        }"#;

        let result = parse_soap_response(response, "qwen3:4b");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("empty content for all sections"));
    }

    #[test]
    fn test_parse_soap_response_missing_fields() {
        let response = r#"{ "subjective": "Patient has cough." }"#;
        let result = parse_soap_response(response, "qwen3:4b");
        assert!(result.is_err()); // Missing required fields
    }

    #[test]
    fn test_extract_json_plain() {
        let input = r#"{"key": "value"}"#;
        assert_eq!(extract_json(input), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_from_code_block() {
        let input = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json(input), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_from_plain_code_block() {
        let input = "```\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json(input), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_build_soap_prompt_without_audio_events() {
        let prompt = build_soap_prompt("Doctor: How are you feeling?", None);
        assert!(prompt.contains("Doctor: How are you feeling?"));
        assert!(prompt.contains("subjective"));
        assert!(prompt.contains("objective"));
        assert!(prompt.contains("assessment"));
        assert!(prompt.contains("plan"));
        assert!(prompt.contains("JSON"));
        assert!(!prompt.contains("AUDIO EVENTS"));
    }

    #[test]
    fn test_build_soap_prompt_with_audio_events() {
        let events = vec![
            AudioEvent {
                timestamp_ms: 30000, // 0:30
                duration_ms: 500,
                confidence: 2.0, // ~88%
                label: "Cough".to_string(),
            },
            AudioEvent {
                timestamp_ms: 65000, // 1:05
                duration_ms: 800,
                confidence: 1.5, // ~82%
                label: "Laughter".to_string(),
            },
        ];
        let prompt = build_soap_prompt("Doctor: How are you feeling?", Some(&events));
        assert!(prompt.contains("Doctor: How are you feeling?"));
        assert!(prompt.contains("AUDIO EVENTS DETECTED"));
        assert!(prompt.contains("Cough at 0:30"));
        assert!(prompt.contains("Laughter at 1:05"));
        assert!(prompt.contains("Consider audio events"));
    }

    #[test]
    fn test_format_audio_events() {
        let events = vec![AudioEvent {
            timestamp_ms: 125000, // 2:05
            duration_ms: 300,
            confidence: 3.0, // ~95%
            label: "Sneeze".to_string(),
        }];
        let formatted = format_audio_events(&events);
        assert!(formatted.contains("Sneeze at 2:05"));
        assert!(formatted.contains("95%"));
    }

    #[test]
    fn test_format_audio_events_empty() {
        let events: Vec<AudioEvent> = vec![];
        let formatted = format_audio_events(&events);
        assert!(formatted.is_empty());
    }

    #[test]
    fn test_format_audio_events_multiple() {
        let events = vec![
            AudioEvent {
                timestamp_ms: 0,
                duration_ms: 500,
                confidence: 2.5,
                label: "Cough".to_string(),
            },
            AudioEvent {
                timestamp_ms: 60000, // 1:00
                duration_ms: 300,
                confidence: 1.5,
                label: "Sneeze".to_string(),
            },
            AudioEvent {
                timestamp_ms: 3661000, // 61:01 (over an hour)
                duration_ms: 200,
                confidence: 3.0,
                label: "Laughter".to_string(),
            },
        ];
        let formatted = format_audio_events(&events);
        assert!(formatted.contains("Cough at 0:00"));
        assert!(formatted.contains("Sneeze at 1:00"));
        assert!(formatted.contains("Laughter at 61:01"));
    }

    #[test]
    fn test_format_audio_events_confidence_conversion() {
        // Test sigmoid-like confidence conversion
        // logit 0 → 50%, logit 1.0 → 73%, logit 2.0 → 88%, logit 3.0 → 95%
        let events = vec![
            AudioEvent {
                timestamp_ms: 0,
                duration_ms: 100,
                confidence: 0.0,
                label: "Low".to_string(),
            },
            AudioEvent {
                timestamp_ms: 1000,
                duration_ms: 100,
                confidence: 1.0,
                label: "Medium".to_string(),
            },
            AudioEvent {
                timestamp_ms: 2000,
                duration_ms: 100,
                confidence: 2.0,
                label: "High".to_string(),
            },
        ];
        let formatted = format_audio_events(&events);
        assert!(formatted.contains("50%")); // logit 0
        assert!(formatted.contains("73%")); // logit 1.0
        assert!(formatted.contains("88%")); // logit 2.0
    }

    #[test]
    fn test_format_audio_events_negative_confidence() {
        // Negative confidence (unlikely but should handle gracefully)
        let events = vec![AudioEvent {
            timestamp_ms: 5000,
            duration_ms: 100,
            confidence: -1.0,
            label: "Uncertain".to_string(),
        }];
        let formatted = format_audio_events(&events);
        assert!(formatted.contains("Uncertain at 0:05"));
        assert!(formatted.contains("27%")); // sigmoid(-1) ≈ 0.27
    }

    #[test]
    fn test_build_soap_prompt_empty_audio_events_slice() {
        // Empty slice should be treated the same as None
        let events: Vec<AudioEvent> = vec![];
        let prompt = build_soap_prompt("Test transcript", Some(&events));
        assert!(!prompt.contains("AUDIO EVENTS"));
        assert!(!prompt.contains("Consider audio events"));
    }

    #[test]
    fn test_audio_event_serialization() {
        // Verify AudioEvent can be serialized/deserialized (for IPC)
        let event = AudioEvent {
            timestamp_ms: 12345,
            duration_ms: 500,
            confidence: 2.5,
            label: "Cough".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AudioEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.timestamp_ms, 12345);
        assert_eq!(parsed.duration_ms, 500);
        assert!((parsed.confidence - 2.5).abs() < 0.001);
        assert_eq!(parsed.label, "Cough");
    }

    #[test]
    fn test_ollama_client_new() {
        let client = OllamaClient::new("http://localhost:11434").unwrap();
        assert_eq!(client.base_url, "http://localhost:11434");

        // Test trailing slash removal
        let client2 = OllamaClient::new("http://localhost:11434/").unwrap();
        assert_eq!(client2.base_url, "http://localhost:11434");

        // Test https scheme
        let client3 = OllamaClient::new("https://ollama.example.com").unwrap();
        assert_eq!(client3.base_url, "https://ollama.example.com");
    }

    #[test]
    fn test_ollama_client_new_invalid_url() {
        // Test invalid URL format
        let result = OllamaClient::new("not-a-valid-url");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid Ollama URL"));

        // Test invalid scheme
        let result2 = OllamaClient::new("ftp://localhost:11434");
        assert!(result2.is_err());
        assert!(result2.unwrap_err().contains("http or https"));

        // Test URL with credentials (security risk)
        let result3 = OllamaClient::new("http://user:pass@localhost:11434");
        assert!(result3.is_err());
        assert!(result3.unwrap_err().contains("must not contain credentials"));

        // Test URL with username only
        let result4 = OllamaClient::new("http://admin@localhost:11434");
        assert!(result4.is_err());
        assert!(result4.unwrap_err().contains("must not contain credentials"));
    }

    #[test]
    fn test_build_soap_retry_prompt() {
        let prompt = build_soap_retry_prompt("Patient says hello", None, "Previous invalid response");
        assert!(prompt.contains("Patient says hello"));
        assert!(prompt.contains("MUST respond with ONLY a JSON object"));
        assert!(prompt.contains("DO NOT ask questions"));
        assert!(!prompt.contains("AUDIO EVENTS"));
    }

    #[test]
    fn test_build_soap_retry_prompt_with_audio_events() {
        let events = vec![AudioEvent {
            timestamp_ms: 45000,
            duration_ms: 400,
            confidence: 1.8,
            label: "Throat clearing".to_string(),
        }];
        let prompt = build_soap_retry_prompt("Patient says hello", Some(&events), "Previous response");
        assert!(prompt.contains("Patient says hello"));
        assert!(prompt.contains("AUDIO EVENTS DETECTED"));
        assert!(prompt.contains("Throat clearing at 0:45"));
    }

    #[test]
    fn test_transcript_validation_constants() {
        // Ensure validation constants are reasonable
        assert!(OllamaClient::MIN_TRANSCRIPT_LENGTH > 0);
        assert!(OllamaClient::MIN_WORD_COUNT > 0);
        assert!(OllamaClient::MAX_TRANSCRIPT_SIZE > OllamaClient::MIN_TRANSCRIPT_LENGTH);
    }
}
