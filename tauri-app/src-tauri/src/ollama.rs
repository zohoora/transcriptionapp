//! Ollama API client for SOAP note generation
//!
//! This module provides integration with Ollama LLM servers for generating
//! structured SOAP (Subjective, Objective, Assessment, Plan) notes from
//! clinical transcripts.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Default timeout for Ollama API requests (2 minutes for generation)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Default number of retry attempts for transient failures
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Initial backoff delay for retries
const INITIAL_BACKOFF_MS: u64 = 500;

/// Maximum backoff delay
const MAX_BACKOFF_MS: u64 = 5000;

/// Request body for Ollama generate endpoint
#[derive(Debug, Clone, Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    /// How long to keep the model loaded in memory after request
    /// -1 = keep loaded indefinitely (recommended for frequent use)
    /// 0 = unload immediately
    /// Positive values = seconds to keep loaded
    #[serde(skip_serializing_if = "Option::is_none")]
    keep_alive: Option<i32>,
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

/// Multi-patient SOAP result (returned by LLM auto-detection)
/// Contains separate SOAP notes for each patient identified in the transcript
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPatientSoapResult {
    /// Individual SOAP notes for each patient detected
    pub notes: Vec<PatientSoapNote>,
    /// Which speaker was identified as the physician (e.g., "Speaker 2")
    pub physician_speaker: Option<String>,
    /// When the result was generated
    pub generated_at: String,
    /// Which LLM model was used
    pub model_used: String,
}

/// Per-patient SOAP note with speaker identification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatientSoapNote {
    /// Label for this patient (e.g., "Patient 1", "Patient 2", or custom name)
    pub patient_label: String,
    /// Which speaker this patient was identified as (e.g., "Speaker 1", "Speaker 3")
    pub speaker_id: String,
    /// The SOAP note for this patient
    pub soap: SoapNote,
}

/// Result of greeting detection check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreetingResult {
    /// Whether this appears to be a greeting starting a session
    pub is_greeting: bool,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// The detected greeting phrase, if any
    pub detected_phrase: Option<String>,
}

/// SOAP note format style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SoapFormat {
    /// Organize by problem - separate S/O/A/P for each medical problem
    #[default]
    ProblemBased,
    /// Single unified SOAP covering all problems together
    Comprehensive,
}

/// Options for SOAP note generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoapOptions {
    /// Detail level (1-10, where 5 is standard)
    #[serde(default = "default_detail_level")]
    pub detail_level: u8,
    /// SOAP format style
    #[serde(default)]
    pub format: SoapFormat,
    /// Custom instructions from the physician
    #[serde(default)]
    pub custom_instructions: String,
}

fn default_detail_level() -> u8 {
    5
}

impl Default for SoapOptions {
    fn default() -> Self {
        Self {
            detail_level: 5,
            format: SoapFormat::ProblemBased,
            custom_instructions: String::new(),
        }
    }
}

/// Ollama API client
#[derive(Debug)]
pub struct OllamaClient {
    client: reqwest::Client,
    base_url: String,
    /// How long to keep model loaded (-1 = forever, 0 = unload immediately)
    keep_alive: i32,
}

/// Check if a reqwest error is retryable (transient network issues)
fn is_retryable_error(err: &reqwest::Error) -> bool {
    // Retry on connection errors, timeouts, and certain status codes
    if err.is_connect() || err.is_timeout() {
        return true;
    }
    // Retry on 5xx server errors
    if let Some(status) = err.status() {
        return status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS;
    }
    false
}

/// Check if an HTTP status code is retryable
fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
}

/// Calculate backoff delay with exponential increase and jitter
fn calculate_backoff(attempt: u32) -> Duration {
    let base_delay = INITIAL_BACKOFF_MS * 2u64.pow(attempt);
    let capped_delay = base_delay.min(MAX_BACKOFF_MS);
    // Add small random jitter (0-100ms) to prevent thundering herd
    let jitter = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis() as u64)
        % 100;
    Duration::from_millis(capped_delay + jitter)
}

impl OllamaClient {
    /// Create a new Ollama client with URL validation
    ///
    /// # Arguments
    /// * `base_url` - The Ollama server URL
    /// * `keep_alive` - How long to keep the model loaded in seconds (-1 = forever, 0 = unload immediately)
    pub fn new(base_url: &str, keep_alive: i32) -> Result<Self, String> {
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
            .connect_timeout(Duration::from_secs(30))
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        info!("OllamaClient created for {} with keep_alive={}", cleaned_url, keep_alive);

        Ok(Self {
            client,
            base_url: cleaned_url.to_string(),
            keep_alive,
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

    /// Pre-warm the model by sending a minimal request to load it into memory
    ///
    /// This is useful for reducing latency on the first real request, especially
    /// for auto-session detection where speed is critical.
    ///
    /// # Arguments
    /// * `model` - The model name to pre-warm
    ///
    /// # Returns
    /// * `Ok(())` if the model was successfully loaded
    /// * `Err(String)` if the model could not be loaded
    pub async fn prewarm_model(&self, model: &str) -> Result<(), String> {
        info!("Pre-warming Ollama model: {}", model);
        let start = std::time::Instant::now();

        let url = format!("{}/api/generate", self.base_url);

        // Use a minimal prompt that generates a very short response
        let request = OllamaGenerateRequest {
            model: model.to_string(),
            prompt: "Say OK".to_string(),
            stream: false,
            options: Some(OllamaOptions {
                num_ctx: Some(128), // Minimal context for faster loading
            }),
            keep_alive: Some(self.keep_alive),
        };

        match self.client.post(&url).json(&request).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let elapsed = start.elapsed();
                    info!("Model {} pre-warmed successfully in {:?}", model, elapsed);
                    Ok(())
                } else {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    error!("Failed to pre-warm model {}: {} - {}", model, status, body);
                    Err(format!("Failed to pre-warm model: {} - {}", status, body))
                }
            }
            Err(e) => {
                error!("Failed to pre-warm model {}: {}", model, e);
                Err(format!("Failed to connect to Ollama: {}", e))
            }
        }
    }

    /// List available models from Ollama with retry logic
    pub async fn list_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/api/tags", self.base_url);
        debug!("Listing Ollama models from {}", url);

        let mut last_error = String::new();

        for attempt in 0..DEFAULT_MAX_RETRIES {
            if attempt > 0 {
                let backoff = calculate_backoff(attempt - 1);
                warn!(
                    "Ollama list_models attempt {} failed, retrying in {:?}",
                    attempt, backoff
                );
                tokio::time::sleep(backoff).await;
            }

            match self.client.get(&url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<OllamaTagsResponse>().await {
                            Ok(tags) => {
                                let models: Vec<String> =
                                    tags.models.into_iter().map(|m| m.name).collect();
                                info!("Found {} Ollama models", models.len());
                                return Ok(models);
                            }
                            Err(e) => {
                                last_error = format!("Failed to parse Ollama response: {}", e);
                                // Parse errors are not retryable
                                break;
                            }
                        }
                    } else if is_retryable_status(response.status()) {
                        last_error = format!(
                            "Ollama returned error status: {}",
                            response.status()
                        );
                        continue;
                    } else {
                        // Non-retryable status code
                        return Err(format!(
                            "Ollama returned error status: {}",
                            response.status()
                        ));
                    }
                }
                Err(e) => {
                    if is_retryable_error(&e) {
                        last_error = format!("Failed to connect to Ollama: {}", e);
                        continue;
                    } else {
                        return Err(format!("Failed to connect to Ollama: {}", e));
                    }
                }
            }
        }

        error!(
            "Ollama list_models failed after {} attempts: {}",
            DEFAULT_MAX_RETRIES, last_error
        );
        Err(last_error)
    }

    /// Generate text using Ollama with retry logic
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
            // Use configured keep_alive setting
            keep_alive: Some(self.keep_alive),
        };

        let mut last_error = String::new();

        for attempt in 0..DEFAULT_MAX_RETRIES {
            if attempt > 0 {
                let backoff = calculate_backoff(attempt - 1);
                warn!(
                    "Ollama generate attempt {} failed, retrying in {:?}",
                    attempt, backoff
                );
                tokio::time::sleep(backoff).await;
            }

            match self.client.post(&url).json(&request).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<OllamaGenerateResponse>().await {
                            Ok(gen_response) => {
                                return Ok(gen_response.response);
                            }
                            Err(e) => {
                                last_error = format!("Failed to parse Ollama response: {}", e);
                                // Parse errors are not retryable
                                break;
                            }
                        }
                    } else if is_retryable_status(response.status()) {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        last_error = format!("Ollama returned error: {} - {}", status, body);
                        continue;
                    } else {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        error!("Ollama generate failed: {} - {}", status, body);
                        return Err(format!("Ollama returned error: {} - {}", status, body));
                    }
                }
                Err(e) => {
                    if is_retryable_error(&e) {
                        last_error = format!("Failed to connect to Ollama: {}", e);
                        continue;
                    } else {
                        return Err(format!("Failed to connect to Ollama: {}", e));
                    }
                }
            }
        }

        error!(
            "Ollama generate failed after {} attempts: {}",
            DEFAULT_MAX_RETRIES, last_error
        );
        Err(last_error)
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
    /// * `options` - Optional SOAP generation options (detail level, format, custom instructions)
    pub async fn generate_soap_note(
        &self,
        model: &str,
        transcript: &str,
        audio_events: Option<&[AudioEvent]>,
        options: Option<&SoapOptions>,
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

        let opts = options.cloned().unwrap_or_default();
        info!(
            "Generating SOAP note with model {} for transcript of {} chars, {} audio events, detail_level={}, format={:?}",
            model,
            transcript.len(),
            audio_events.map(|e| e.len()).unwrap_or(0),
            opts.detail_level,
            opts.format
        );

        // First attempt with enhanced prompt
        let prompt = build_soap_prompt_with_options(transcript, audio_events, &opts);
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

    /// Generate multi-patient SOAP notes from a clinical transcript
    ///
    /// The LLM auto-detects which speakers are patients vs the physician,
    /// and generates separate SOAP notes for each patient identified.
    ///
    /// # Arguments
    /// * `model` - The Ollama model to use
    /// * `transcript` - The clinical transcript text (with speaker labels)
    /// * `audio_events` - Optional audio events (coughs, laughs, etc.) detected during recording
    /// * `options` - Optional SOAP generation options (detail level, custom instructions)
    pub async fn generate_multi_patient_soap_note(
        &self,
        model: &str,
        transcript: &str,
        audio_events: Option<&[AudioEvent]>,
        options: Option<&SoapOptions>,
    ) -> Result<MultiPatientSoapResult, String> {
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

        let opts = options.cloned().unwrap_or_default();
        info!(
            "Generating multi-patient SOAP note with model {} for transcript of {} chars, {} audio events",
            model,
            transcript.len(),
            audio_events.map(|e| e.len()).unwrap_or(0)
        );

        // First attempt with multi-patient prompt
        let prompt = build_multi_patient_soap_prompt(transcript, audio_events, &opts);
        let response = self.generate(model, &prompt).await?;

        match parse_multi_patient_soap_response(&response, model) {
            Ok(result) => {
                info!(
                    "Successfully generated multi-patient SOAP note on first attempt ({} patients)",
                    result.notes.len()
                );
                Ok(result)
            }
            Err(first_error) => {
                // Retry with stricter prompt
                info!("First multi-patient SOAP generation attempt failed, retrying with stricter prompt...");

                let retry_prompt = build_multi_patient_retry_prompt(transcript, audio_events, &response);
                match self.generate(model, &retry_prompt).await {
                    Ok(retry_response) => {
                        match parse_multi_patient_soap_response(&retry_response, model) {
                            Ok(result) => {
                                info!(
                                    "Successfully generated multi-patient SOAP note on retry ({} patients)",
                                    result.notes.len()
                                );
                                Ok(result)
                            }
                            Err(retry_error) => {
                                // Both attempts failed
                                error!("Multi-patient SOAP generation failed after retry. First response: {}, Retry response: {}",
                                    &response.chars().take(200).collect::<String>(),
                                    &retry_response.chars().take(200).collect::<String>()
                                );
                                Err(format!(
                                    "Failed to parse multi-patient SOAP note after retry.\n\nFirst error: {}\n\nRetry error: {}\n\nRaw response: {}",
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
                            "Multi-patient SOAP generation failed: {}. Retry also failed: {}. Raw response from first attempt: {}",
                            first_error,
                            gen_error,
                            response
                        ))
                    }
                }
            }
        }
    }

    /// Timeout for greeting detection (includes model loading time)
    /// Note: qwen3:4b with /no_think typically takes 20-30 seconds on first request
    const GREETING_TIMEOUT: Duration = Duration::from_secs(45);

    /// Check if a transcript contains a greeting that should start a session
    ///
    /// Uses a lightweight prompt with shorter timeout for real-time detection.
    pub async fn check_greeting(
        &self,
        transcript: &str,
        sensitivity: f32,
    ) -> Result<GreetingResult, String> {
        let trimmed = transcript.trim();

        // Empty or very short transcripts are not greetings
        if trimmed.is_empty() || trimmed.len() < 3 {
            return Ok(GreetingResult {
                is_greeting: false,
                confidence: 0.0,
                detected_phrase: None,
            });
        }

        let prompt = build_greeting_prompt(trimmed);

        let url = format!("{}/api/generate", self.base_url);
        info!("Checking greeting with Ollama at {} (timeout={}s)", url, Self::GREETING_TIMEOUT.as_secs());

        let request = OllamaGenerateRequest {
            model: "qwen3:4b".to_string(), // Use fast model for greeting detection
            prompt,
            stream: false,
            options: None,
            // Use configured keep_alive setting
            keep_alive: Some(self.keep_alive),
        };

        // Use the existing client with a per-request timeout
        let response = self.client
            .post(&url)
            .timeout(Self::GREETING_TIMEOUT)  // Per-request timeout
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    error!("Ollama greeting check timed out after {}s", Self::GREETING_TIMEOUT.as_secs());
                    format!("Ollama request timed out after {}s - is the server responding?", Self::GREETING_TIMEOUT.as_secs())
                } else if e.is_connect() {
                    error!("Ollama greeting check failed to connect: {}", e);
                    format!("Failed to connect to Ollama at {}: {}", self.base_url, e)
                } else {
                    error!("Ollama greeting check error: {}", e);
                    format!("Failed to connect to Ollama: {}", e)
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Ollama greeting check failed: {} - {}", status, body);
            return Err(format!("Ollama returned error: {} - {}", status, body));
        }

        let gen_response: OllamaGenerateResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Ollama response: {}", e))?;

        info!("Ollama greeting check completed, parsing response");
        debug!("Greeting check raw response: {}", &gen_response.response.chars().take(200).collect::<String>());

        // Parse the response
        parse_greeting_response(&gen_response.response, sensitivity)
    }
}

/// Build the prompt for greeting detection
fn build_greeting_prompt(transcript: &str) -> String {
    // Use /no_think to disable Qwen's thinking mode for faster responses
    format!(
        r#"/no_think
Analyze if this speech is a greeting that would START a medical consultation.

TRANSCRIPT: "{}"

Common greeting patterns that START consultations:
- "Hello" / "Hi" / "Good morning" / "Good afternoon"
- "How are you today?" / "How are you feeling?"
- "What brings you in today?"
- Patient introductions or names
- "Nice to meet you" / "Come on in"

NOT greetings (ongoing conversation):
- Medical symptoms discussion
- Treatment discussions
- Background noise or partial words
- Mid-conversation phrases

Respond with ONLY valid JSON:
{{"is_greeting": true/false, "confidence": 0.0-1.0, "detected_phrase": "the greeting phrase if found or null"}}

Rules:
- is_greeting = true ONLY if this sounds like the START of a conversation
- confidence > 0.8 for clear greetings like "Hello, how are you?"
- confidence 0.5-0.8 for partial or uncertain greetings
- confidence < 0.5 for unlikely greetings
- Do NOT mark ongoing medical discussion as greetings
- Output ONLY the JSON object, nothing else"#,
        transcript
    )
}

/// Parse greeting detection response from the LLM
fn parse_greeting_response(response: &str, _sensitivity: f32) -> Result<GreetingResult, String> {
    let cleaned = response.trim();

    // Remove thinking blocks (Qwen models)
    let without_think = if let Some(start) = cleaned.find("<think>") {
        if let Some(end) = cleaned.find("</think>") {
            format!("{}{}", &cleaned[..start], &cleaned[end + 8..])
        } else {
            cleaned.to_string()
        }
    } else {
        cleaned.to_string()
    };

    // Try to find JSON in the response
    let json_str = if let Some(start) = without_think.find('{') {
        if let Some(end) = without_think.rfind('}') {
            &without_think[start..=end]
        } else {
            return Err("No valid JSON found in response".to_string());
        }
    } else {
        return Err("No JSON object found in response".to_string());
    };

    // Parse JSON
    #[derive(Deserialize)]
    struct GreetingResponse {
        is_greeting: bool,
        confidence: f32,
        detected_phrase: Option<String>,
    }

    let parsed: GreetingResponse = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse greeting JSON: {} - Response: {}", e, json_str))?;

    Ok(GreetingResult {
        is_greeting: parsed.is_greeting,
        confidence: parsed.confidence.clamp(0.0, 1.0),
        detected_phrase: parsed.detected_phrase,
    })
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

/// Build the prompt for SOAP note generation (legacy - uses default options)
fn build_soap_prompt(transcript: &str, audio_events: Option<&[AudioEvent]>) -> String {
    build_soap_prompt_with_options(transcript, audio_events, &SoapOptions::default())
}

/// Build the enhanced prompt for SOAP note generation with options
fn build_soap_prompt_with_options(
    transcript: &str,
    audio_events: Option<&[AudioEvent]>,
    options: &SoapOptions,
) -> String {
    let audio_section = audio_events
        .filter(|e| !e.is_empty())
        .map(format_audio_events)
        .unwrap_or_default();

    let audio_instruction = if audio_section.is_empty() {
        String::new()
    } else {
        String::from("\n- Consider audio events (coughs, laughs, etc.) when relevant to the clinical picture")
    };

    // Build the base prompt with strong anti-hallucination instructions
    let base_prompt = format!(
        r#"You are a medical scribe assistant. Extract information from the transcript below to create a SOAP note.

**CRITICAL: DO NOT HALLUCINATE OR INVENT INFORMATION**
- ONLY include information that is EXPLICITLY stated in the transcript
- If something is not mentioned, DO NOT include it - use "No information available" instead
- DO NOT assume, infer, or add typical/common medical details that weren't stated
- DO NOT add vital signs, exam findings, or diagnoses unless explicitly mentioned
- When in doubt, LEAVE IT OUT

**LANGUAGE:** Write entirely in English, regardless of transcript language.

**STYLE:** Concise bullet points only, no paragraphs.

TRANSCRIPT:
{}{}

Respond with ONLY valid JSON:
{{
  "subjective": "...",
  "objective": "...",
  "assessment": "...",
  "plan": "..."
}}

**STRICT RULES:**
1. ONLY quote or paraphrase what was actually said in the transcript
2. Use "No information available" for ANY section without explicit content
3. DO NOT add standard medical phrases, typical findings, or boilerplate text
4. DO NOT invent physical exam findings, vital signs, or test results
5. DO NOT assume diagnoses - only include if explicitly discussed{}
6. No markdown formatting (no *, #, etc.) - plain text only
7. Output ONLY the JSON object, nothing else

Example of what NOT to do:
- BAD: "Vital signs stable" (if not mentioned)
- BAD: "Physical exam unremarkable" (if no exam described)
- BAD: "Continue current medications" (if not discussed)
- GOOD: "No information available" (when nothing was said)"#,
        transcript, audio_section, audio_instruction
    );

    // Add format modifier
    let format_modifier = build_format_modifier(options.format);

    // Add detail level modifier
    let detail_modifier = build_detail_modifier(options.detail_level);

    // Add custom instructions if provided
    let custom_section = if options.custom_instructions.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\n\n**ADDITIONAL INSTRUCTIONS FROM PHYSICIAN:**\n{}\n\nPlease incorporate these instructions when generating the SOAP note.",
            options.custom_instructions.trim()
        )
    };

    format!("{}{}{}{}", base_prompt, format_modifier, detail_modifier, custom_section)
}

/// Build the format modifier based on SOAP format style
fn build_format_modifier(format: SoapFormat) -> String {
    match format {
        SoapFormat::ProblemBased => String::from(r#"

**FORMAT: PROBLEM-BASED**
If the patient has multiple distinct medical problems, organize each problem separately within the JSON sections. Use problem headers like "PROBLEM 1: [title]" within each section."#),
        SoapFormat::Comprehensive => String::from(r#"

**FORMAT: COMPREHENSIVE (SINGLE NOTE)**
Create ONE unified SOAP note covering ALL problems together. Combine all findings into cohesive sections."#),
    }
}

/// Build the detail level modifier (1-10 scale)
fn build_detail_modifier(level: u8) -> String {
    let clamped = level.clamp(1, 10);

    match clamped {
        1 => String::from(r#"

**DETAIL LEVEL: 1/10 (ULTRA-BRIEF)**
- Maximum 1-2 bullet points per section
- Only the single most important finding/symptom
- One-word or very short phrase bullet points
- Skip sections entirely if minimal relevant info"#),
        2 => String::from(r#"

**DETAIL LEVEL: 2/10 (MINIMAL)**
- Maximum 2-3 bullet points per section
- Only key symptoms and findings
- Brief, telegraphic style"#),
        3 => String::from(r#"

**DETAIL LEVEL: 3/10 (BRIEF)**
- 3-4 bullet points per section maximum
- Focus on primary complaint
- Essential findings only"#),
        4 => String::from(r#"

**DETAIL LEVEL: 4/10 (SHORT)**
- Fewer bullet points than usual
- Combine related items where possible
- Focus on main issues"#),
        5 => String::new(), // Standard - no modifier needed
        6 => String::from(r#"

**DETAIL LEVEL: 6/10 (EXPANDED)**
- Include additional symptom descriptors
- Add relevant context
- More thorough but still concise"#),
        7 => String::from(r#"

**DETAIL LEVEL: 7/10 (DETAILED)**
- Include symptom timing, severity, quality
- Add relevant history context
- More complete objective findings"#),
        8 => String::from(r#"

**DETAIL LEVEL: 8/10 (THOROUGH)**
- Comprehensive symptom description with OPQRST elements where relevant
- Include pertinent negatives
- Detailed physical exam findings
- Full differential consideration"#),
        9 => String::from(r#"

**DETAIL LEVEL: 9/10 (COMPREHENSIVE)**
- Extensive history with full context
- All mentioned symptoms with complete descriptors
- Thorough assessment with reasoning
- Complete plan with patient education points"#),
        10 => String::from(r#"

**DETAIL LEVEL: 10/10 (MAXIMUM)**
- Include every detail mentioned in the transcript
- Full symptom characterization
- Complete history including all context
- Extensive differential diagnosis discussion
- Nothing should be omitted"#),
        _ => String::new(),
    }
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
        r#"IMPORTANT: You MUST respond with ONLY a JSON object. No conversation, no questions, no explanations.

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

/// Build the prompt for multi-patient SOAP note generation with auto-detection
/// The LLM analyzes the transcript to identify the physician and each patient,
/// then generates separate SOAP notes for each patient.
fn build_multi_patient_soap_prompt(
    transcript: &str,
    audio_events: Option<&[AudioEvent]>,
    options: &SoapOptions,
) -> String {
    let audio_section = audio_events
        .filter(|e| !e.is_empty())
        .map(format_audio_events)
        .unwrap_or_default();

    let audio_instruction = if audio_section.is_empty() {
        String::new()
    } else {
        String::from("\n- Consider audio events (coughs, laughs, etc.) when relevant to the clinical picture for each patient")
    };

    // Build the detail level instruction
    let detail_instruction = match options.detail_level {
        1..=3 => "Use brief, concise bullet points.",
        4..=6 => "Use standard clinical detail.",
        7..=10 => "Include thorough clinical detail with all mentioned symptoms and findings.",
        _ => "Use standard clinical detail.",
    };

    // Custom instructions section
    let custom_section = if options.custom_instructions.trim().is_empty() {
        String::new()
    } else {
        format!(
            "\n\n**ADDITIONAL INSTRUCTIONS FROM PHYSICIAN:**\n{}\n",
            options.custom_instructions.trim()
        )
    };

    format!(
        r#"You are a medical scribe assistant. Analyze the transcript below to identify patients and generate SOAP notes.

**CRITICAL: PATIENT & PHYSICIAN IDENTIFICATION**
- Analyze the conversation to identify who is the PHYSICIAN and who are the PATIENTS
- The physician asks questions, examines, diagnoses, and prescribes
- Patients describe symptoms, answer questions, receive instructions
- DO NOT assume Speaker 1 is the physician - determine from context
- There may be 1-4 patients in this visit (e.g., couple, family)

**ANTI-HALLUCINATION RULES:**
- ONLY include information EXPLICITLY stated in the transcript
- DO NOT assume, infer, or add typical/common medical details that weren't stated
- DO NOT add vital signs, exam findings, or diagnoses unless explicitly mentioned
- Use "No information available" if a section lacks patient-specific content
- When in doubt, LEAVE IT OUT

**LANGUAGE:** Write entirely in English, regardless of transcript language.

**STYLE:** {} No markdown formatting (no *, #, etc.) - plain text only.

TRANSCRIPT:
{}{}

**OUTPUT FORMAT - Respond with ONLY valid JSON:**
{{
  "physician_speaker": "Speaker X",
  "patients": [
    {{
      "patient_label": "Patient 1",
      "speaker_id": "Speaker Y",
      "subjective": "...",
      "objective": "...",
      "assessment": "...",
      "plan": "..."
    }}
  ]
}}

**STRICT RULES:**
1. First identify the physician from conversation context (NOT by speaker number)
2. Generate ONE SOAP note per patient identified (1-4 patients maximum)
3. Each patient's SOAP contains ONLY information about THAT patient
4. Use "No information available" for ANY section without explicit content for that patient
5. DO NOT generate SOAP for the physician
6. DO NOT mix information between patients
7. If only 1 patient detected, return single-item patients array
8. Output ONLY the JSON object, nothing else{}{}

Example output for 2 patients:
{{
  "physician_speaker": "Speaker 2",
  "patients": [
    {{
      "patient_label": "Patient 1",
      "speaker_id": "Speaker 1",
      "subjective": "Headaches for 2 weeks...",
      "objective": "No information available",
      "assessment": "Possible tension headache",
      "plan": "OTC pain relief..."
    }},
    {{
      "patient_label": "Patient 2",
      "speaker_id": "Speaker 3",
      "subjective": "Persistent cough for 1 week...",
      "objective": "No information available",
      "assessment": "Upper respiratory symptoms",
      "plan": "Rest, fluids..."
    }}
  ]
}}"#,
        detail_instruction,
        transcript,
        audio_section,
        audio_instruction,
        custom_section
    )
}

/// Build a stricter retry prompt for multi-patient SOAP when first attempt fails
fn build_multi_patient_retry_prompt(
    transcript: &str,
    audio_events: Option<&[AudioEvent]>,
    _previous_response: &str,
) -> String {
    let audio_section = audio_events
        .filter(|e| !e.is_empty())
        .map(format_audio_events)
        .unwrap_or_default();

    format!(
        r#"IMPORTANT: You MUST respond with ONLY a JSON object. No conversation, no questions, no explanations.

Analyze this transcript to identify patients and physician. Generate a SOAP note for EACH patient.

TRANSCRIPT:
{}{}

OUTPUT FORMAT (respond with ONLY this JSON structure):
{{"physician_speaker":"Speaker X","patients":[{{"patient_label":"Patient 1","speaker_id":"Speaker Y","subjective":"...","objective":"...","assessment":"...","plan":"..."}}]}}

RULES:
- Identify physician by who asks questions and examines
- Generate one SOAP per patient (NOT for physician)
- Use "No information available" for empty sections
- DO NOT ask questions. DO NOT explain. ONLY output the JSON object."#,
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

/// JSON structure for multi-patient LLM response
#[derive(Debug, Clone, Deserialize)]
struct MultiPatientSoapJson {
    physician_speaker: Option<String>,
    patients: Vec<PatientSoapJson>,
}

/// JSON structure for individual patient SOAP in multi-patient response
#[derive(Debug, Clone, Deserialize)]
struct PatientSoapJson {
    patient_label: String,
    speaker_id: String,
    subjective: String,
    objective: String,
    assessment: String,
    plan: String,
}

/// Parse the multi-patient LLM response into a structured result
fn parse_multi_patient_soap_response(
    response: &str,
    model: &str,
) -> Result<MultiPatientSoapResult, String> {
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
    match serde_json::from_str::<MultiPatientSoapJson>(&json_str) {
        Ok(parsed) => {
            // Validate we have at least one patient
            if parsed.patients.is_empty() {
                return Err("No patients identified in transcript".to_string());
            }

            // Limit to 4 patients maximum
            if parsed.patients.len() > 4 {
                warn!(
                    "LLM detected {} patients, limiting to 4",
                    parsed.patients.len()
                );
            }

            let generated_at = Utc::now().to_rfc3339();

            // Convert to our public types
            let notes: Vec<PatientSoapNote> = parsed
                .patients
                .into_iter()
                .take(4) // Limit to 4 patients
                .map(|p| PatientSoapNote {
                    patient_label: p.patient_label,
                    speaker_id: p.speaker_id,
                    soap: SoapNote {
                        subjective: if p.subjective.trim().is_empty() {
                            "No information available.".to_string()
                        } else {
                            p.subjective
                        },
                        objective: if p.objective.trim().is_empty() {
                            "No information available.".to_string()
                        } else {
                            p.objective
                        },
                        assessment: if p.assessment.trim().is_empty() {
                            "No information available.".to_string()
                        } else {
                            p.assessment
                        },
                        plan: if p.plan.trim().is_empty() {
                            "No information available.".to_string()
                        } else {
                            p.plan
                        },
                        generated_at: generated_at.clone(),
                        model_used: model.to_string(),
                        raw_response: None,
                    },
                })
                .collect();

            // Check if all patients have completely empty SOAP notes
            let any_has_content = notes.iter().any(|n| {
                n.soap.subjective != "No information available."
                    || n.soap.objective != "No information available."
                    || n.soap.assessment != "No information available."
                    || n.soap.plan != "No information available."
            });

            if !any_has_content {
                return Err(
                    "SOAP note generation returned empty content for all patients. \
                     The transcript may not contain enough clinical information."
                        .to_string(),
                );
            }

            info!(
                "Successfully parsed multi-patient SOAP: {} patients, physician: {:?}",
                notes.len(),
                parsed.physician_speaker
            );

            Ok(MultiPatientSoapResult {
                notes,
                physician_speaker: parsed.physician_speaker,
                generated_at,
                model_used: model.to_string(),
            })
        }
        Err(e) => {
            // Log the response for debugging
            warn!(
                "Failed to parse multi-patient SOAP JSON: {}. Response preview: {}",
                e,
                &json_str.chars().take(500).collect::<String>()
            );
            Err(format!(
                "Could not parse multi-patient SOAP JSON: {}. Response started with: {}...",
                e,
                &json_str.chars().take(100).collect::<String>()
            ))
        }
    }
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
        let client = OllamaClient::new("http://localhost:11434", -1).unwrap();
        assert_eq!(client.base_url, "http://localhost:11434");

        // Test trailing slash removal
        let client2 = OllamaClient::new("http://localhost:11434/", -1).unwrap();
        assert_eq!(client2.base_url, "http://localhost:11434");

        // Test https scheme
        let client3 = OllamaClient::new("https://ollama.example.com", -1).unwrap();
        assert_eq!(client3.base_url, "https://ollama.example.com");
    }

    #[test]
    fn test_ollama_client_new_invalid_url() {
        // Test invalid URL format
        let result = OllamaClient::new("not-a-valid-url", -1);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid Ollama URL"));

        // Test invalid scheme
        let result2 = OllamaClient::new("ftp://localhost:11434", -1);
        assert!(result2.is_err());
        assert!(result2.unwrap_err().contains("http or https"));

        // Test URL with credentials (security risk)
        let result3 = OllamaClient::new("http://user:pass@localhost:11434", -1);
        assert!(result3.is_err());
        assert!(result3.unwrap_err().contains("must not contain credentials"));

        // Test URL with username only
        let result4 = OllamaClient::new("http://admin@localhost:11434", -1);
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

    // SOAP Options tests
    #[test]
    fn test_soap_options_default() {
        let opts = SoapOptions::default();
        assert_eq!(opts.detail_level, 5);
        assert_eq!(opts.format, SoapFormat::ProblemBased);
        assert!(opts.custom_instructions.is_empty());
    }

    #[test]
    fn test_soap_format_default() {
        let format = SoapFormat::default();
        assert_eq!(format, SoapFormat::ProblemBased);
    }

    #[test]
    fn test_soap_format_serialization() {
        let problem_based = SoapFormat::ProblemBased;
        let json = serde_json::to_string(&problem_based).unwrap();
        assert_eq!(json, "\"problem_based\"");

        let comprehensive = SoapFormat::Comprehensive;
        let json = serde_json::to_string(&comprehensive).unwrap();
        assert_eq!(json, "\"comprehensive\"");
    }

    #[test]
    fn test_soap_format_deserialization() {
        let pb: SoapFormat = serde_json::from_str("\"problem_based\"").unwrap();
        assert_eq!(pb, SoapFormat::ProblemBased);

        let comp: SoapFormat = serde_json::from_str("\"comprehensive\"").unwrap();
        assert_eq!(comp, SoapFormat::Comprehensive);
    }

    #[test]
    fn test_build_format_modifier_problem_based() {
        let modifier = build_format_modifier(SoapFormat::ProblemBased);
        assert!(modifier.contains("PROBLEM-BASED"));
        assert!(modifier.contains("multiple distinct medical problems"));
    }

    #[test]
    fn test_build_format_modifier_comprehensive() {
        let modifier = build_format_modifier(SoapFormat::Comprehensive);
        assert!(modifier.contains("COMPREHENSIVE"));
        assert!(modifier.contains("ONE unified SOAP note"));
    }

    #[test]
    fn test_build_detail_modifier_level_5() {
        // Level 5 is standard, should return empty
        let modifier = build_detail_modifier(5);
        assert!(modifier.is_empty());
    }

    #[test]
    fn test_build_detail_modifier_level_1() {
        let modifier = build_detail_modifier(1);
        assert!(modifier.contains("ULTRA-BRIEF"));
        assert!(modifier.contains("1-2 bullet points"));
    }

    #[test]
    fn test_build_detail_modifier_level_10() {
        let modifier = build_detail_modifier(10);
        assert!(modifier.contains("MAXIMUM"));
        assert!(modifier.contains("every detail"));
    }

    #[test]
    fn test_build_detail_modifier_clamping() {
        // Test that values are clamped to 1-10
        let modifier_0 = build_detail_modifier(0);
        let modifier_1 = build_detail_modifier(1);
        assert_eq!(modifier_0, modifier_1); // 0 should be clamped to 1

        let modifier_11 = build_detail_modifier(11);
        let modifier_10 = build_detail_modifier(10);
        assert_eq!(modifier_11, modifier_10); // 11 should be clamped to 10
    }

    #[test]
    fn test_build_detail_modifier_all_levels() {
        // Ensure all levels 1-10 produce valid modifiers (or empty for level 5)
        for level in 1..=10 {
            let modifier = build_detail_modifier(level);
            if level == 5 {
                assert!(modifier.is_empty());
            } else {
                assert!(modifier.contains("DETAIL LEVEL:"));
            }
        }
    }

    #[test]
    fn test_build_soap_prompt_with_options_custom_instructions() {
        let opts = SoapOptions {
            detail_level: 5,
            format: SoapFormat::ProblemBased,
            custom_instructions: "Please include medication allergies.".to_string(),
        };
        let prompt = build_soap_prompt_with_options("Test transcript", None, &opts);
        assert!(prompt.contains("ADDITIONAL INSTRUCTIONS FROM PHYSICIAN"));
        assert!(prompt.contains("medication allergies"));
    }

    #[test]
    fn test_build_soap_prompt_with_options_empty_custom_instructions() {
        let opts = SoapOptions {
            detail_level: 5,
            format: SoapFormat::ProblemBased,
            custom_instructions: "   ".to_string(), // whitespace only
        };
        let prompt = build_soap_prompt_with_options("Test transcript", None, &opts);
        assert!(!prompt.contains("ADDITIONAL INSTRUCTIONS"));
    }

    #[test]
    fn test_build_soap_prompt_with_options_all_combined() {
        let opts = SoapOptions {
            detail_level: 8,
            format: SoapFormat::Comprehensive,
            custom_instructions: "Focus on respiratory symptoms.".to_string(),
        };
        let events = vec![AudioEvent {
            timestamp_ms: 5000,
            duration_ms: 300,
            confidence: 2.0,
            label: "Cough".to_string(),
        }];
        let prompt = build_soap_prompt_with_options("Patient coughing", Some(&events), &opts);

        // Check all components are present
        assert!(prompt.contains("Patient coughing"));
        assert!(prompt.contains("AUDIO EVENTS DETECTED"));
        assert!(prompt.contains("COMPREHENSIVE"));
        assert!(prompt.contains("DETAIL LEVEL: 8/10"));
        assert!(prompt.contains("respiratory symptoms"));
    }

    #[test]
    fn test_soap_options_serialization() {
        let opts = SoapOptions {
            detail_level: 7,
            format: SoapFormat::Comprehensive,
            custom_instructions: "Include vital signs.".to_string(),
        };
        let json = serde_json::to_string(&opts).unwrap();
        let parsed: SoapOptions = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.detail_level, 7);
        assert_eq!(parsed.format, SoapFormat::Comprehensive);
        assert_eq!(parsed.custom_instructions, "Include vital signs.");
    }

    #[test]
    fn test_soap_note_serialization() {
        let note = SoapNote {
            subjective: "Patient reports pain.".to_string(),
            objective: "BP 120/80.".to_string(),
            assessment: "Hypertension.".to_string(),
            plan: "Continue medication.".to_string(),
            generated_at: "2025-01-07T10:00:00Z".to_string(),
            model_used: "qwen3:4b".to_string(),
            raw_response: Some("raw".to_string()),
        };
        let json = serde_json::to_string(&note).unwrap();
        let parsed: SoapNote = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.subjective, "Patient reports pain.");
        assert_eq!(parsed.raw_response, Some("raw".to_string()));
    }

    #[test]
    fn test_soap_note_raw_response_skip_serializing() {
        let note = SoapNote {
            subjective: "Test".to_string(),
            objective: "Test".to_string(),
            assessment: "Test".to_string(),
            plan: "Test".to_string(),
            generated_at: "2025-01-07T10:00:00Z".to_string(),
            model_used: "test".to_string(),
            raw_response: None,
        };
        let json = serde_json::to_string(&note).unwrap();
        assert!(!json.contains("raw_response"));
    }

    #[test]
    fn test_ollama_status_serialization() {
        let status = OllamaStatus {
            connected: true,
            available_models: vec!["model1".to_string(), "model2".to_string()],
            error: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: OllamaStatus = serde_json::from_str(&json).unwrap();

        assert!(parsed.connected);
        assert_eq!(parsed.available_models.len(), 2);
        assert!(parsed.error.is_none());
    }

    #[test]
    fn test_ollama_status_with_error() {
        let status = OllamaStatus {
            connected: false,
            available_models: vec![],
            error: Some("Connection refused".to_string()),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Connection refused"));
    }

    #[test]
    fn test_extract_json_whitespace() {
        let input = "  \n\n{\"key\": \"value\"}  \n";
        assert_eq!(extract_json(input), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_code_block_with_extra_text() {
        // Code block with text after
        let input = "```json\n{\"key\": \"value\"}\n```\nSome extra text";
        assert_eq!(extract_json(input), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_parse_soap_response_with_extra_whitespace() {
        let response = r#"

        {
            "subjective": "  Patient feels okay.  ",
            "objective": "Normal exam.",
            "assessment": "Healthy.",
            "plan": "Annual checkup."
        }

        "#;

        let soap = parse_soap_response(response, "test").unwrap();
        assert!(soap.subjective.contains("Patient feels okay"));
    }

    #[test]
    fn test_detail_level_2() {
        let modifier = build_detail_modifier(2);
        assert!(modifier.contains("MINIMAL"));
        assert!(modifier.contains("2-3 bullet points"));
    }

    #[test]
    fn test_detail_level_3() {
        let modifier = build_detail_modifier(3);
        assert!(modifier.contains("BRIEF"));
        assert!(modifier.contains("3-4 bullet points"));
    }

    #[test]
    fn test_detail_level_4() {
        let modifier = build_detail_modifier(4);
        assert!(modifier.contains("SHORT"));
    }

    #[test]
    fn test_detail_level_6() {
        let modifier = build_detail_modifier(6);
        assert!(modifier.contains("EXPANDED"));
    }

    #[test]
    fn test_detail_level_7() {
        let modifier = build_detail_modifier(7);
        assert!(modifier.contains("DETAILED"));
        assert!(modifier.contains("timing, severity, quality"));
    }

    #[test]
    fn test_detail_level_8() {
        let modifier = build_detail_modifier(8);
        assert!(modifier.contains("THOROUGH"));
        assert!(modifier.contains("OPQRST"));
    }

    #[test]
    fn test_detail_level_9() {
        let modifier = build_detail_modifier(9);
        assert!(modifier.contains("COMPREHENSIVE"));
        assert!(modifier.contains("patient education"));
    }
}
