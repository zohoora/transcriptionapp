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
        r#"You are a medical scribe assistant. Based on the following clinical transcript, generate a SOAP note.

TRANSCRIPT:
{}

Generate a structured SOAP note with these sections:
- Subjective: Patient's reported symptoms, history, and concerns
- Objective: Observable findings, vital signs, examination results mentioned
- Assessment: Clinical impression and potential diagnoses
- Plan: Recommended treatments, tests, follow-ups

Format your response exactly as:
SUBJECTIVE:
[content]

OBJECTIVE:
[content]

ASSESSMENT:
[content]

PLAN:
[content]

Important: Only include information that is explicitly mentioned or can be directly inferred from the transcript. If a section has no relevant information, write "No information available in transcript.""#,
        transcript
    )
}

/// Parse the LLM response into a structured SOAP note
fn parse_soap_response(response: &str, model: &str) -> Result<SoapNote, String> {
    // Clean up the response - handle both /think and /no_think variants from Qwen
    let clean_response = response
        .trim()
        // Remove thinking blocks if present
        .split("</think>")
        .last()
        .unwrap_or(response)
        .trim();

    // Find each section
    let subjective = extract_section(clean_response, "SUBJECTIVE:", &["OBJECTIVE:"]);
    let objective = extract_section(clean_response, "OBJECTIVE:", &["ASSESSMENT:"]);
    let assessment = extract_section(clean_response, "ASSESSMENT:", &["PLAN:"]);
    let plan = extract_section(clean_response, "PLAN:", &[]);

    // Validate that we got at least some content
    if subjective.is_empty() && objective.is_empty() && assessment.is_empty() && plan.is_empty() {
        return Err("Could not parse SOAP note sections from response".to_string());
    }

    Ok(SoapNote {
        subjective: if subjective.is_empty() {
            "No information available.".to_string()
        } else {
            subjective
        },
        objective: if objective.is_empty() {
            "No information available.".to_string()
        } else {
            objective
        },
        assessment: if assessment.is_empty() {
            "No information available.".to_string()
        } else {
            assessment
        },
        plan: if plan.is_empty() {
            "No information available.".to_string()
        } else {
            plan
        },
        generated_at: Utc::now().to_rfc3339(),
        model_used: model.to_string(),
    })
}

/// Extract a section from the response text
fn extract_section(text: &str, start_marker: &str, end_markers: &[&str]) -> String {
    // Case-insensitive search for start marker
    let text_upper = text.to_uppercase();
    let start_marker_upper = start_marker.to_uppercase();

    let start_pos = match text_upper.find(&start_marker_upper) {
        Some(pos) => pos + start_marker.len(),
        None => return String::new(),
    };

    let remaining = &text[start_pos..];

    // Find the earliest end marker
    let mut end_pos = remaining.len();
    for marker in end_markers {
        let marker_upper = marker.to_uppercase();
        if let Some(pos) = remaining.to_uppercase().find(&marker_upper) {
            if pos < end_pos {
                end_pos = pos;
            }
        }
    }

    remaining[..end_pos].trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_section() {
        let response = r#"SUBJECTIVE:
Patient reports headache for 3 days.

OBJECTIVE:
Vital signs stable. Temperature 98.6F.

ASSESSMENT:
Tension headache.

PLAN:
OTC pain relief. Follow up if symptoms persist."#;

        let subjective = extract_section(response, "SUBJECTIVE:", &["OBJECTIVE:"]);
        assert!(subjective.contains("headache for 3 days"));

        let objective = extract_section(response, "OBJECTIVE:", &["ASSESSMENT:"]);
        assert!(objective.contains("Vital signs stable"));

        let assessment = extract_section(response, "ASSESSMENT:", &["PLAN:"]);
        assert!(assessment.contains("Tension headache"));

        let plan = extract_section(response, "PLAN:", &[]);
        assert!(plan.contains("OTC pain relief"));
    }

    #[test]
    fn test_extract_section_case_insensitive() {
        let response = r#"subjective:
Lower case test.

objective:
More content."#;

        let subjective = extract_section(response, "SUBJECTIVE:", &["OBJECTIVE:"]);
        assert!(subjective.contains("Lower case test"));
    }

    #[test]
    fn test_parse_soap_response() {
        let response = r#"SUBJECTIVE:
Patient complains of cough.

OBJECTIVE:
Lungs clear.

ASSESSMENT:
Viral URI.

PLAN:
Rest and fluids."#;

        let soap = parse_soap_response(response, "qwen3:4b").unwrap();
        assert!(soap.subjective.contains("cough"));
        assert!(soap.objective.contains("Lungs clear"));
        assert!(soap.assessment.contains("Viral URI"));
        assert!(soap.plan.contains("Rest"));
        assert_eq!(soap.model_used, "qwen3:4b");
    }

    #[test]
    fn test_parse_soap_response_with_think_block() {
        let response = r#"<think>
Let me analyze this transcript...
</think>

SUBJECTIVE:
Patient reports fever.

OBJECTIVE:
Temperature 101F.

ASSESSMENT:
Possible infection.

PLAN:
Monitor temperature."#;

        let soap = parse_soap_response(response, "qwen3:4b").unwrap();
        assert!(soap.subjective.contains("fever"));
        assert!(!soap.subjective.contains("think"));
    }

    #[test]
    fn test_parse_soap_response_empty() {
        let response = "Some random text without SOAP sections";
        let result = parse_soap_response(response, "qwen3:4b");
        assert!(result.is_err());
    }

    #[test]
    fn test_build_soap_prompt() {
        let prompt = build_soap_prompt("Doctor: How are you feeling?");
        assert!(prompt.contains("Doctor: How are you feeling?"));
        assert!(prompt.contains("SUBJECTIVE"));
        assert!(prompt.contains("OBJECTIVE"));
        assert!(prompt.contains("ASSESSMENT"));
        assert!(prompt.contains("PLAN"));
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
