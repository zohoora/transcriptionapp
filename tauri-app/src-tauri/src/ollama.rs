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

/// Generated SOAP note
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoapNote {
    pub subjective: String,
    pub objective: String,
    pub assessment: String,
    pub plan: String,
    pub generated_at: String,
    pub model_used: String,
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

    /// Generate a SOAP note from a clinical transcript
    pub async fn generate_soap_note(
        &self,
        model: &str,
        transcript: &str,
    ) -> Result<SoapNote, String> {
        // Validate transcript
        if transcript.trim().is_empty() {
            return Err("Transcript cannot be empty".to_string());
        }
        if transcript.len() > Self::MAX_TRANSCRIPT_SIZE {
            return Err(format!(
                "Transcript too large ({} bytes). Maximum size is {} bytes",
                transcript.len(),
                Self::MAX_TRANSCRIPT_SIZE
            ));
        }

        info!(
            "Generating SOAP note with model {} for transcript of {} chars",
            model,
            transcript.len()
        );

        let prompt = build_soap_prompt(transcript);
        let response = self.generate(model, &prompt).await?;

        let soap_note = parse_soap_response(&response, model)?;
        info!("Successfully generated SOAP note");

        Ok(soap_note)
    }
}

/// Build the prompt for SOAP note generation
fn build_soap_prompt(transcript: &str) -> String {
    format!(
        r#"/no_think You are a medical scribe assistant. Based on the following clinical transcript, generate a SOAP note.

TRANSCRIPT:
{}

Respond with ONLY valid JSON in this exact format:
{{
  "subjective": "Patient's reported symptoms, history, and concerns from the visit",
  "objective": "Observable findings, vital signs, examination results mentioned",
  "assessment": "Clinical impression and potential diagnoses",
  "plan": "Recommended treatments, tests, follow-ups"
}}

Rules:
- Only include information explicitly mentioned or directly inferable from the transcript
- Use "No information available" if a section has no relevant content
- Output ONLY the JSON object, no markdown, no explanation, no other text"#,
        transcript
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
    fn test_build_soap_prompt() {
        let prompt = build_soap_prompt("Doctor: How are you feeling?");
        assert!(prompt.contains("Doctor: How are you feeling?"));
        assert!(prompt.contains("subjective"));
        assert!(prompt.contains("objective"));
        assert!(prompt.contains("assessment"));
        assert!(prompt.contains("plan"));
        assert!(prompt.contains("JSON"));
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
    }
}
