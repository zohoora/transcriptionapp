//! LLM Router API client for SOAP note generation
//!
//! This module provides integration with an OpenAI-compatible LLM router for generating
//! structured SOAP (Subjective, Objective, Assessment, Plan) notes from clinical transcripts.

use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Default timeout for LLM API requests (2 minutes for generation)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Default number of retry attempts for transient failures
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Initial backoff delay for retries
const INITIAL_BACKOFF_MS: u64 = 500;

/// Maximum backoff delay
const MAX_BACKOFF_MS: u64 = 5000;

/// Task identifiers for the X-Clinic-Task header
pub mod tasks {
    pub const SOAP_NOTE: &str = "soap_note";
    pub const GREETING_DETECTION: &str = "greeting_detection";
    pub const HEALTH_CHECK: &str = "health_check";
}

/// OpenAI-compatible chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// OpenAI-compatible chat completion request
#[derive(Debug, Clone, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

/// OpenAI-compatible chat completion response
#[derive(Debug, Clone, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    #[allow(dead_code)]
    model: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

/// Model info from OpenAI-compatible /v1/models endpoint
#[derive(Debug, Clone, Deserialize)]
struct ModelInfo {
    id: String,
}

/// Response from /v1/models endpoint
#[derive(Debug, Clone, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

/// Status of the LLM router connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMStatus {
    pub connected: bool,
    pub available_models: Vec<String>,
    pub error: Option<String>,
}

// Re-export as OllamaStatus for backward compatibility with frontend types
pub type OllamaStatus = LLMStatus;

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

/// Speaker context for SOAP generation
/// Contains information about identified speakers in the transcript
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpeakerContext {
    /// List of identified speakers with their descriptions
    /// Key: speaker ID as it appears in transcript (e.g., "Dr. Smith", "Speaker 2")
    /// Value: description for LLM context (e.g., "Attending physician, internal medicine")
    pub speakers: Vec<SpeakerInfo>,
}

/// Information about a single speaker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerInfo {
    /// Speaker ID as it appears in transcript (e.g., "Dr. Smith", "Speaker 2")
    pub id: String,
    /// Description for LLM context (e.g., "Attending physician, internal medicine")
    pub description: String,
    /// Whether this speaker was enrolled (recognized) vs auto-detected
    pub is_enrolled: bool,
}

impl SpeakerContext {
    /// Create a new empty speaker context
    pub fn new() -> Self {
        Self { speakers: Vec::new() }
    }

    /// Add an enrolled speaker (recognized from profile)
    pub fn add_enrolled(&mut self, name: String, description: String) {
        self.speakers.push(SpeakerInfo {
            id: name,
            description,
            is_enrolled: true,
        });
    }

    /// Add an auto-detected speaker (not enrolled)
    pub fn add_auto_detected(&mut self, speaker_id: String) {
        self.speakers.push(SpeakerInfo {
            id: speaker_id,
            description: "Unidentified speaker".to_string(),
            is_enrolled: false,
        });
    }

    /// Check if there are any speakers
    pub fn has_speakers(&self) -> bool {
        !self.speakers.is_empty()
    }

    /// Format for LLM prompt
    pub fn format_for_prompt(&self) -> String {
        if self.speakers.is_empty() {
            return String::new();
        }

        let mut output = String::from("SPEAKER CONTEXT:\nThe following speakers have been identified in this encounter:\n");
        for speaker in &self.speakers {
            output.push_str(&format!("- {}: {}\n", speaker.id, speaker.description));
        }
        output
    }
}

/// Generated SOAP note - simplified to single text content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoapNote {
    /// The full SOAP note content as generated by the LLM
    pub content: String,
    pub generated_at: String,
    pub model_used: String,
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
    /// The SOAP note content for this patient
    pub content: String,
}

/// JSON structure for SOAP note from LLM
#[derive(Debug, Clone, Deserialize)]
struct SoapJsonResponse {
    #[serde(default)]
    subjective: Vec<String>,
    #[serde(default)]
    objective: Vec<String>,
    #[serde(default)]
    assessment: Vec<String>,
    #[serde(default)]
    plan: Vec<String>,
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
    /// Custom instructions from the physician (persisted in settings)
    #[serde(default)]
    pub custom_instructions: String,
    /// Session-specific notes from the clinician (entered during recording)
    #[serde(default)]
    pub session_notes: String,
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
            session_notes: String::new(),
        }
    }
}

/// LLM Router API client (OpenAI-compatible)
#[derive(Debug)]
pub struct LLMClient {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    client_id: String,
    fast_model: String,
}

// Re-export as OllamaClient for backward compatibility
pub type OllamaClient = LLMClient;

/// Check if a reqwest error is retryable (transient network issues)
fn is_retryable_error(err: &reqwest::Error) -> bool {
    if err.is_connect() || err.is_timeout() {
        return true;
    }
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
    let jitter = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis() as u64)
        % 100;
    Duration::from_millis(capped_delay + jitter)
}

impl LLMClient {
    /// Create a new LLM client with URL validation
    ///
    /// # Arguments
    /// * `base_url` - The LLM router URL (e.g., "http://localhost:4000")
    /// * `api_key` - API key for authentication
    /// * `client_id` - Client identifier for the X-Client-Id header
    /// * `fast_model` - Model to use for fast tasks like greeting detection
    pub fn new(base_url: &str, api_key: &str, client_id: &str, fast_model: &str) -> Result<Self, String> {
        let cleaned_url = base_url.trim_end_matches('/');

        // Validate URL format and scheme
        let parsed = reqwest::Url::parse(cleaned_url)
            .map_err(|e| format!("Invalid LLM router URL '{}': {}", cleaned_url, e))?;

        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(format!(
                "LLM router URL must use http or https scheme, got: {}",
                parsed.scheme()
            ));
        }

        // Reject URLs with credentials (security risk)
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("LLM router URL must not contain credentials".to_string());
        }

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        info!("LLMClient created for {}", cleaned_url);

        Ok(Self {
            client,
            base_url: cleaned_url.to_string(),
            api_key: api_key.to_string(),
            client_id: client_id.to_string(),
            fast_model: fast_model.to_string(),
        })
    }

    /// Build authentication headers for requests
    fn auth_headers(&self, task: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();

        if !self.api_key.is_empty() {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                    .unwrap_or_else(|_| HeaderValue::from_static("")),
            );
        }

        headers.insert(
            "X-Client-Id",
            HeaderValue::from_str(&self.client_id)
                .unwrap_or_else(|_| HeaderValue::from_static("ai-scribe")),
        );

        headers.insert(
            "X-Clinic-Task",
            HeaderValue::from_str(task)
                .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
        );

        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        headers
    }

    /// Check connection status and list available models
    pub async fn check_status(&self) -> LLMStatus {
        match self.list_models().await {
            Ok(models) => LLMStatus {
                connected: true,
                available_models: models,
                error: None,
            },
            Err(e) => LLMStatus {
                connected: false,
                available_models: vec![],
                error: Some(e),
            },
        }
    }

    /// Pre-warm the model by sending a minimal request to load it into memory
    pub async fn prewarm_model(&self, model: &str) -> Result<(), String> {
        info!("Pre-warming LLM model: {}", model);
        let start = std::time::Instant::now();

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Say OK".to_string(),
            }],
            stream: false,
            max_tokens: Some(10),
        };

        let url = format!("{}/v1/chat/completions", self.base_url);

        match self.client
            .post(&url)
            .headers(self.auth_headers(tasks::HEALTH_CHECK))
            .json(&request)
            .send()
            .await
        {
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
                Err(format!("Failed to connect to LLM router: {}", e))
            }
        }
    }

    /// List available models from the LLM router with retry logic
    pub async fn list_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/v1/models", self.base_url);
        debug!("Listing LLM models from {}", url);

        let mut last_error = String::new();

        for attempt in 0..DEFAULT_MAX_RETRIES {
            if attempt > 0 {
                let backoff = calculate_backoff(attempt - 1);
                warn!(
                    "LLM list_models attempt {} failed, retrying in {:?}",
                    attempt, backoff
                );
                tokio::time::sleep(backoff).await;
            }

            match self.client
                .get(&url)
                .headers(self.auth_headers(tasks::HEALTH_CHECK))
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<ModelsResponse>().await {
                            Ok(models_response) => {
                                let models: Vec<String> =
                                    models_response.data.into_iter().map(|m| m.id).collect();
                                info!("Found {} LLM models", models.len());
                                return Ok(models);
                            }
                            Err(e) => {
                                last_error = format!("Failed to parse LLM response: {}", e);
                                break;
                            }
                        }
                    } else if is_retryable_status(response.status()) {
                        last_error = format!(
                            "LLM router returned error status: {}",
                            response.status()
                        );
                        continue;
                    } else {
                        return Err(format!(
                            "LLM router returned error status: {}",
                            response.status()
                        ));
                    }
                }
                Err(e) => {
                    if is_retryable_error(&e) {
                        last_error = format!("Failed to connect to LLM router: {}", e);
                        continue;
                    } else {
                        return Err(format!("Failed to connect to LLM router: {}", e));
                    }
                }
            }
        }

        error!(
            "LLM list_models failed after {} attempts: {}",
            DEFAULT_MAX_RETRIES, last_error
        );
        Err(last_error)
    }

    /// Generate text using the LLM with retry logic
    pub async fn generate(
        &self,
        model: &str,
        system_prompt: &str,
        user_content: &str,
        task: &str,
    ) -> Result<String, String> {
        if model.trim().is_empty() {
            return Err("Model name cannot be empty".to_string());
        }
        if user_content.trim().is_empty() {
            return Err("User content cannot be empty".to_string());
        }

        let url = format!("{}/v1/chat/completions", self.base_url);
        debug!("Generating with LLM model {} at {}", model, url);

        let mut messages = Vec::new();

        if !system_prompt.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            });
        }

        messages.push(ChatMessage {
            role: "user".to_string(),
            content: user_content.to_string(),
        });

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages,
            stream: false,
            max_tokens: None,
        };

        let mut last_error = String::new();

        for attempt in 0..DEFAULT_MAX_RETRIES {
            if attempt > 0 {
                let backoff = calculate_backoff(attempt - 1);
                warn!(
                    "LLM generate attempt {} failed, retrying in {:?}",
                    attempt, backoff
                );
                tokio::time::sleep(backoff).await;
            }

            match self.client
                .post(&url)
                .headers(self.auth_headers(task))
                .json(&request)
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<ChatCompletionResponse>().await {
                            Ok(chat_response) => {
                                if let Some(choice) = chat_response.choices.first() {
                                    return Ok(choice.message.content.clone());
                                }
                                return Err("No response choices returned".to_string());
                            }
                            Err(e) => {
                                last_error = format!("Failed to parse LLM response: {}", e);
                                break;
                            }
                        }
                    } else if is_retryable_status(response.status()) {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        last_error = format!("LLM router returned error: {} - {}", status, body);
                        continue;
                    } else {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        error!("LLM generate failed: {} - {}", status, body);
                        return Err(format!("LLM router returned error: {} - {}", status, body));
                    }
                }
                Err(e) => {
                    if is_retryable_error(&e) {
                        last_error = format!("Failed to connect to LLM router: {}", e);
                        continue;
                    } else {
                        return Err(format!("Failed to connect to LLM router: {}", e));
                    }
                }
            }
        }

        error!(
            "LLM generate failed after {} attempts: {}",
            DEFAULT_MAX_RETRIES, last_error
        );
        Err(last_error)
    }

    /// Maximum transcript size (500KB) to prevent memory issues
    /// Allows sessions up to ~5 hours before hitting this limit
    const MAX_TRANSCRIPT_SIZE: usize = 500_000;

    /// Minimum transcript length (50 chars) to ensure meaningful SOAP generation
    const MIN_TRANSCRIPT_LENGTH: usize = 50;

    /// Minimum word count for meaningful SOAP generation
    const MIN_WORD_COUNT: usize = 5;

    /// Maximum words to send to LLM (keeps under typical 32K token context)
    /// ~10,000 words ≈ 13,000 tokens, leaving room for system prompt and response
    const MAX_WORDS_FOR_LLM: usize = 10_000;

    /// Validate and prepare transcript for SOAP generation
    /// Returns a (possibly truncated) transcript string
    fn prepare_transcript(transcript: &str) -> Result<String, String> {
        let trimmed = transcript.trim();

        if trimmed.is_empty() {
            return Err("Transcript cannot be empty".to_string());
        }

        if trimmed.len() < Self::MIN_TRANSCRIPT_LENGTH {
            return Err(format!(
                "Transcript too short ({} characters). Minimum {} characters required.",
                trimmed.len(),
                Self::MIN_TRANSCRIPT_LENGTH
            ));
        }

        let word_count = trimmed.split_whitespace().count();
        if word_count < Self::MIN_WORD_COUNT {
            return Err(format!(
                "Transcript has too few words ({} words). Minimum {} words required.",
                word_count,
                Self::MIN_WORD_COUNT
            ));
        }

        if transcript.len() > Self::MAX_TRANSCRIPT_SIZE {
            return Err(format!(
                "Transcript too large ({} bytes). Maximum size is {} bytes",
                transcript.len(),
                Self::MAX_TRANSCRIPT_SIZE
            ));
        }

        // Truncate if too long for LLM context
        if word_count > Self::MAX_WORDS_FOR_LLM {
            Ok(Self::truncate_transcript(trimmed, word_count))
        } else {
            Ok(trimmed.to_string())
        }
    }

    /// Truncate a long transcript while preserving context
    /// Keeps first 20% (greeting, chief complaint) and last 80% (recent, plan)
    fn truncate_transcript(transcript: &str, word_count: usize) -> String {
        let words: Vec<&str> = transcript.split_whitespace().collect();
        let target_words = Self::MAX_WORDS_FOR_LLM;

        // Keep 20% from start (greeting, initial complaint) and 80% from end (recent context, plan)
        let start_words = target_words / 5;  // 20%
        let end_words = target_words - start_words;  // 80%

        let start_portion: Vec<&str> = words.iter().take(start_words).copied().collect();
        let end_portion: Vec<&str> = words.iter().skip(word_count - end_words).copied().collect();

        let omitted = word_count - target_words;

        warn!(
            "Transcript truncated: {} words -> {} words ({} words omitted from middle)",
            word_count, target_words, omitted
        );

        format!(
            "{}\n\n[... {} words omitted from middle of transcript ...]\n\n{}",
            start_portion.join(" "),
            omitted,
            end_portion.join(" ")
        )
    }

    /// Generate a SOAP note from a clinical transcript
    /// Returns the raw LLM output as a single text block
    ///
    /// # Arguments
    /// * `model` - LLM model name to use
    /// * `transcript` - Clinical transcript text
    /// * `audio_events` - Optional detected audio events (coughs, etc.)
    /// * `options` - SOAP generation options
    /// * `speaker_context` - Optional speaker identification context
    pub async fn generate_soap_note(
        &self,
        model: &str,
        transcript: &str,
        audio_events: Option<&[AudioEvent]>,
        options: Option<&SoapOptions>,
        speaker_context: Option<&SpeakerContext>,
    ) -> Result<SoapNote, String> {
        let prepared_transcript = Self::prepare_transcript(transcript)?;
        let opts = options.cloned().unwrap_or_default();
        info!(
            "Generating SOAP note with model {} for transcript of {} chars ({} words), {} audio events, {} speakers",
            model,
            prepared_transcript.len(),
            prepared_transcript.split_whitespace().count(),
            audio_events.map(|e| e.len()).unwrap_or(0),
            speaker_context.map(|c| c.speakers.len()).unwrap_or(0)
        );

        let system_prompt = build_simple_soap_prompt(&opts);
        let session_notes = if opts.session_notes.trim().is_empty() { None } else { Some(opts.session_notes.as_str()) };
        let user_content = build_soap_user_content(&prepared_transcript, audio_events, session_notes, speaker_context);

        let response = self.generate(model, &system_prompt, &user_content, tasks::SOAP_NOTE).await?;

        // Parse JSON response and format as bullet-point text
        let content = parse_and_format_soap_json(&response);

        info!("Successfully generated SOAP note ({} chars)", content.len());
        Ok(SoapNote {
            content,
            generated_at: Utc::now().to_rfc3339(),
            model_used: model.to_string(),
        })
    }

    /// Generate multi-patient SOAP notes from a clinical transcript
    /// Returns a single combined note covering all patients
    ///
    /// # Arguments
    /// * `model` - LLM model name to use
    /// * `transcript` - Clinical transcript text
    /// * `audio_events` - Optional detected audio events (coughs, etc.)
    /// * `options` - SOAP generation options
    /// * `speaker_context` - Optional speaker identification context
    pub async fn generate_multi_patient_soap_note(
        &self,
        model: &str,
        transcript: &str,
        audio_events: Option<&[AudioEvent]>,
        options: Option<&SoapOptions>,
        speaker_context: Option<&SpeakerContext>,
    ) -> Result<MultiPatientSoapResult, String> {
        let prepared_transcript = Self::prepare_transcript(transcript)?;
        let opts = options.cloned().unwrap_or_default();
        info!(
            "Generating multi-patient SOAP note with model {} for transcript of {} chars ({} words), {} speakers",
            model,
            prepared_transcript.len(),
            prepared_transcript.split_whitespace().count(),
            speaker_context.map(|c| c.speakers.len()).unwrap_or(0)
        );

        let system_prompt = build_simple_multi_patient_prompt(&opts);
        let session_notes = if opts.session_notes.trim().is_empty() { None } else { Some(opts.session_notes.as_str()) };
        let user_content = build_soap_user_content(&prepared_transcript, audio_events, session_notes, speaker_context);

        let response = self.generate(model, &system_prompt, &user_content, tasks::SOAP_NOTE).await?;

        // Parse JSON response and format as bullet-point text
        let content = parse_and_format_soap_json(&response);

        info!("Successfully generated multi-patient SOAP note ({} chars)", content.len());
        Ok(MultiPatientSoapResult {
            notes: vec![PatientSoapNote {
                patient_label: "Combined".to_string(),
                speaker_id: "All".to_string(),
                content,
            }],
            physician_speaker: None,
            generated_at: Utc::now().to_rfc3339(),
            model_used: model.to_string(),
        })
    }

    /// Timeout for greeting detection
    const GREETING_TIMEOUT: Duration = Duration::from_secs(45);

    /// Check if a transcript contains a greeting that should start a session
    pub async fn check_greeting(
        &self,
        transcript: &str,
        sensitivity: f32,
    ) -> Result<GreetingResult, String> {
        let trimmed = transcript.trim();

        if trimmed.is_empty() || trimmed.len() < 3 {
            return Ok(GreetingResult {
                is_greeting: false,
                confidence: 0.0,
                detected_phrase: None,
            });
        }

        let system_prompt = format!(
            r#"You are a speech classifier. Analyze if the given speech is a greeting that would START a medical consultation.

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

Use a sensitivity threshold of {:.2} (higher = more likely to classify as greeting).

Respond ONLY with JSON: {{"is_greeting": true/false, "confidence": 0.0-1.0, "detected_phrase": "the greeting phrase if found or null"}}"#,
            sensitivity
        );

        let url = format!("{}/v1/chat/completions", self.base_url);
        info!("Checking greeting with LLM at {} (timeout={}s)", url, Self::GREETING_TIMEOUT.as_secs());

        let request = ChatCompletionRequest {
            model: self.fast_model.clone(), // Use configured fast model for greeting detection
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: trimmed.to_string(),
                },
            ],
            stream: false,
            max_tokens: Some(100),
        };

        let response = self.client
            .post(&url)
            .headers(self.auth_headers(tasks::GREETING_DETECTION))
            .timeout(Self::GREETING_TIMEOUT)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    format!("LLM request timed out after {}s", Self::GREETING_TIMEOUT.as_secs())
                } else if e.is_connect() {
                    format!("Failed to connect to LLM router at {}: {}", self.base_url, e)
                } else {
                    format!("Failed to connect to LLM router: {}", e)
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("LLM router returned error: {} - {}", status, body));
        }

        let chat_response: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        let content = chat_response
            .choices
            .first()
            .map(|c| c.message.content.as_str())
            .unwrap_or("");

        info!("LLM greeting check completed, parsing response");
        debug!("Greeting check raw response: {}", &content.chars().take(200).collect::<String>());

        parse_greeting_response(content, sensitivity)
    }
}

/// Build a simple system prompt for SOAP note generation (JSON output required)
fn build_simple_soap_prompt(options: &SoapOptions) -> String {
    let detail_instruction = match options.detail_level {
        1..=3 => "Be brief and concise.",
        4..=6 => "Use standard clinical detail.",
        7..=10 => "Include thorough clinical detail.",
        _ => "Use standard clinical detail.",
    };

    let format_instruction = match options.format {
        SoapFormat::ProblemBased => "ORGANIZATION: If multiple medical problems are discussed, organize by problem - label each problem (e.g., 'Problem 1: Hypertension') and include its relevant S/O/A/P items grouped together.",
        SoapFormat::Comprehensive => "ORGANIZATION: Create a single unified SOAP note covering all problems together in each section.",
    };

    let custom_section = if options.custom_instructions.trim().is_empty() {
        String::new()
    } else {
        format!("\n\nAdditional instructions: {}", options.custom_instructions.trim())
    };

    format!(
        r#"You are a medical scribe that outputs ONLY valid JSON. Extract clinical information from transcripts into SOAP notes.

The transcript is from speech-to-text and may contain errors. Interpret medical terms correctly:
- "human blade 1c" or "h b a 1 c" → HbA1c (hemoglobin A1c)
- "ekg" or "e k g" → EKG/ECG
- Homophones and phonetic errors are common - use clinical context

RESPOND WITH ONLY THIS JSON STRUCTURE - NO OTHER TEXT:
{{"subjective":["item"],"objective":["item"],"assessment":["item"],"plan":["item"]}}

Rules:
- Your entire response must be valid JSON - nothing else
- Use simple string arrays, no nested objects
- Do NOT use newlines inside JSON strings - keep each array item as a single line
- Use empty arrays [] for sections with no information
- Use correct medical terminology
- Do NOT use any markdown formatting (no **, no __, no #, no backticks) - output plain text only
- Do NOT include specific patient names or healthcare provider names - use "patient" or "the physician/provider" instead
- Do NOT hallucinate or embellish - only include what was explicitly stated
- PLAN SECTION: Include ONLY treatments, tests, and follow-ups the doctor actually mentioned. Do NOT add recommendations, monitoring suggestions, or instructions not stated in the transcript.
- CLINICIAN NOTES: If provided, incorporate clinician observations into the appropriate SOAP sections (usually Objective for physical observations, Subjective for reported symptoms).
- {detail_instruction}
- {format_instruction}{custom_section}"#
    )
}

/// Build a simple system prompt for multi-patient SOAP note generation (JSON output required)
fn build_simple_multi_patient_prompt(options: &SoapOptions) -> String {
    let detail_instruction = match options.detail_level {
        1..=3 => "Be brief and concise.",
        4..=6 => "Use standard clinical detail.",
        7..=10 => "Include thorough clinical detail.",
        _ => "Use standard clinical detail.",
    };

    let format_instruction = match options.format {
        SoapFormat::ProblemBased => "ORGANIZATION: If multiple medical problems are discussed, organize by problem - label each problem (e.g., 'Problem 1: Hypertension') and include its relevant S/O/A/P items grouped together.",
        SoapFormat::Comprehensive => "ORGANIZATION: Create a single unified SOAP note covering all problems together in each section.",
    };

    let custom_section = if options.custom_instructions.trim().is_empty() {
        String::new()
    } else {
        format!("\n\nAdditional instructions: {}", options.custom_instructions.trim())
    };

    format!(
        r#"You are a medical scribe that outputs ONLY valid JSON. Extract clinical information from transcripts into SOAP notes.

The transcript is from speech-to-text and may contain errors. Interpret medical terms correctly:
- "human blade 1c" or "h b a 1 c" → HbA1c (hemoglobin A1c)
- "ekg" or "e k g" → EKG/ECG
- Homophones and phonetic errors are common - use clinical context

RESPOND WITH ONLY THIS JSON STRUCTURE - NO OTHER TEXT:
{{"subjective":["item"],"objective":["item"],"assessment":["item"],"plan":["item"]}}

Rules:
- Your entire response must be valid JSON - nothing else
- Use simple string arrays, no nested objects
- Do NOT use newlines inside JSON strings - keep each array item as a single line
- Use empty arrays [] for sections with no information
- Use correct medical terminology
- Do NOT use any markdown formatting (no **, no __, no #, no backticks) - output plain text only
- Do NOT include specific patient names or healthcare provider names - use "patient" or "the physician/provider" instead
- Do NOT hallucinate or embellish - only include what was explicitly stated
- PLAN SECTION: Include ONLY treatments, tests, and follow-ups the doctor actually mentioned. Do NOT add recommendations, monitoring suggestions, or instructions not stated in the transcript.
- CLINICIAN NOTES: If provided, incorporate clinician observations into the appropriate SOAP sections (usually Objective for physical observations, Subjective for reported symptoms).
- {detail_instruction}
- {format_instruction}{custom_section}"#
    )
}

/// Clean up LLM response by removing channel markers, think tags, and markdown formatting
fn clean_llm_response(response: &str) -> String {
    let mut text = response.to_string();
    info!("clean_llm_response INPUT ({} chars): {:?}", response.len(), &response[..response.len().min(300)]);

    // Handle multi-channel LLM outputs - extract content after "final" channel
    if let Some(final_idx) = text.find("<|channel|>final<|message|>") {
        text = text[final_idx + "<|channel|>final<|message|>".len()..].to_string();
        debug!("After channel extraction: {} chars", text.len());
    }

    // Strip end markers
    if let Some(end_idx) = text.find("<|end|>") {
        text = text[..end_idx].to_string();
        debug!("After end marker strip: {} chars", text.len());
    }

    // Remove other common artifacts
    text = text
        .replace("<|start|>", "")
        .replace("<|assistant|>", "")
        .replace("<|user|>", "");

    // Remove <think>...</think> blocks (including multiline)
    while let Some(start) = text.find("<think>") {
        if let Some(end) = text.find("</think>") {
            let end_pos = end + "</think>".len();
            text = format!("{}{}", &text[..start], &text[end_pos..]);
        } else {
            // No closing tag, remove everything from <think> onwards
            text = text[..start].to_string();
            break;
        }
    }

    // Remove <unused94>thought reasoning blocks (MedGemma reasoning leak)
    // These can appear in two forms:
    // 1. <unused94>thought...reasoning...</unused94>actual content
    // 2. <unused94>thought...reasoning...<unused95>actual content (no closing tag)
    if let Some(start) = text.find("<unused94>") {
        info!("Found <unused94> at position {}", start);
        // Check for </unused94> closing tag
        if let Some(end) = text.find("</unused94>") {
            info!("Found </unused94> at position {}", end);
            let end_pos = end + "</unused94>".len();
            text = format!("{}{}", &text[..start], &text[end_pos..]);
        } else if let Some(content_start) = text.find("<unused95>") {
            info!("Found <unused95> at position {}, extracting content after it", content_start);
            text = text[content_start + "<unused95>".len()..].to_string();
            info!("After unused95 extraction: {} chars, starts with: {:?}", text.len(), &text[..text.len().min(100)]);
        } else {
            info!("No </unused94> or <unused95> found, looking for S: marker");
            // No clear end marker - try to find the SOAP note by looking for "S:" or "**S:**"
            let after_unused94 = &text[start + "<unused94>".len()..];
            if let Some(soap_start) = after_unused94.find("\nS:").or_else(|| after_unused94.find("\n**S:")) {
                info!("Found S: marker at position {} after <unused94>", soap_start);
                text = after_unused94[soap_start..].to_string();
            } else {
                // Last resort: keep text before <unused94>
                info!("No S: marker found, keeping text before <unused94> (will be empty if at start)");
                text = text[..start].to_string();
            }
        }
    } else {
        info!("No <unused94> found in response");
    }
    info!("After unused94/95 handling: {} chars", text.len());

    debug!("Before strip_transcript_echo: {} chars, starts with: {:?}", text.len(), &text[..text.len().min(100)]);

    // Strip transcript echoing - model sometimes echoes the transcript before the SOAP note
    // Look for the start of the actual SOAP note (S: or Subjective:)
    text = strip_transcript_echo(&text);
    debug!("After strip_transcript_echo: {} chars", text.len());

    // Remove markdown formatting while preserving line breaks
    text = remove_markdown_formatting(&text);
    debug!("After remove_markdown_formatting: {} chars", text.len());

    let result = text.trim().to_string();
    debug!("clean_llm_response output: {} chars", result.len());
    result
}

/// Strip transcript echoing from LLM response
/// The model sometimes echoes the transcript before outputting the SOAP note.
/// This function finds the start of the actual SOAP note and removes the preamble.
fn strip_transcript_echo(text: &str) -> String {
    // Look for SOAP note markers - the note should start with one of these
    let soap_markers = [
        "\nS:\n",           // Standard format with newlines
        "\nS:\r\n",         // Windows newlines
        "S:\n•",            // Starts with S: and bullet
        "\nS:\n•",          // Newline before S:
        "\nSubjective:\n",  // Full word format
        "\nSubjective\n",   // Without colon
        "**S:**",           // Bold markdown format
        "**Subjective:**",  // Bold full word
        "## S:",            // Header format
        "## Subjective:",   // Header full word
    ];

    // First check if the text starts cleanly with S:
    let trimmed = text.trim_start();
    if trimmed.starts_with("S:") || trimmed.starts_with("S\n") || trimmed.starts_with("**S:**") {
        debug!("strip_transcript_echo: text already starts with S:, returning as-is");
        return text.to_string();
    }

    // Look for SOAP markers in the text
    for marker in soap_markers {
        if let Some(idx) = text.find(marker) {
            // Found a SOAP marker - extract from just before it
            // We want to keep any leading whitespace/newline before S:
            let start = if marker.starts_with('\n') { idx + 1 } else { idx };
            let extracted = text[start..].trim_start();
            if !extracted.is_empty() {
                debug!("strip_transcript_echo: found marker {:?} at {}, extracted {} chars", marker, idx, extracted.len());
                return extracted.to_string();
            }
        }
    }

    // Alternative: look for a line that starts with "S:" (case sensitive)
    for (i, line) in text.lines().enumerate() {
        let trimmed_line = line.trim();
        if trimmed_line == "S:" || trimmed_line.starts_with("S:\n") || trimmed_line.starts_with("S:•")
            || (trimmed_line.len() > 2 && trimmed_line.starts_with("S:") && !trimmed_line.chars().nth(2).unwrap_or(' ').is_alphabetic())
        {
            // Found start of SOAP - join from this line onwards
            let remaining: Vec<&str> = text.lines().skip(i).collect();
            debug!("strip_transcript_echo: found S: on line {}, returning {} lines", i, remaining.len());
            return remaining.join("\n");
        }
    }

    // No clear SOAP marker found - return original text
    debug!("strip_transcript_echo: no SOAP marker found, returning original {} chars", text.len());
    text.to_string()
}

/// Remove markdown formatting (bold, italic, headers) but preserve structure
fn remove_markdown_formatting(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let mut result = line.to_string();

            // Remove markdown headers (# ## ### etc) but keep the text
            let trimmed = result.trim_start();
            if trimmed.starts_with('#') {
                let leading_ws = result.len() - trimmed.len();
                let content = trimmed.trim_start_matches('#').trim_start();
                result = format!("{}{}", " ".repeat(leading_ws), content);
            }

            // Remove ** (bold) - do this before single *
            result = result.replace("**", "");

            // Remove __ (alternate bold)
            result = result.replace("__", "");

            // Remove backticks (code formatting)
            result = result.replace('`', "");

            // Remove italic markers (* and _) but preserve list markers
            result = remove_italic_markers(&result);

            result
        })
        .collect();

    lines.join("\n")
}

/// Remove italic markers (* or _) that wrap text, preserving list markers
fn remove_italic_markers(line: &str) -> String {
    let trimmed = line.trim_start();

    // Check if line starts with a list marker (-, *, +, or number.)
    let is_list_item = trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
        || (trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
            && trimmed.contains(". "));

    if is_list_item {
        // For list items, only remove * or _ that appear after the list marker
        let leading_ws = line.len() - trimmed.len();
        let marker_end = trimmed.find(' ').unwrap_or(1) + 1;
        let (marker, rest) = trimmed.split_at(marker_end.min(trimmed.len()));
        let cleaned_rest = rest.replace('*', "").replace('_', "");
        format!("{}{}{}", " ".repeat(leading_ws), marker, cleaned_rest)
    } else {
        // Not a list item, remove all * and _ (italic markers)
        line.replace('*', "").replace('_', "")
    }
}

/// Extract JSON from LLM response (handles markdown code blocks and cleanup)
/// Fix unescaped newlines inside JSON strings
/// LLMs sometimes produce JSON with literal newlines in strings which is invalid
fn fix_json_newlines(json: &str) -> String {
    let mut result = String::with_capacity(json.len());
    let mut in_string = false;
    let mut escape_next = false;

    for ch in json.chars() {
        if escape_next {
            result.push(ch);
            escape_next = false;
            continue;
        }

        match ch {
            '\\' if in_string => {
                result.push(ch);
                escape_next = true;
            }
            '"' => {
                in_string = !in_string;
                result.push(ch);
            }
            '\n' if in_string => {
                // Escape the newline inside a string
                result.push_str("\\n");
            }
            '\r' if in_string => {
                // Skip carriage returns inside strings
            }
            _ => {
                result.push(ch);
            }
        }
    }

    result
}

/// Fix truncated JSON by adding missing closing brackets
/// LLMs sometimes get cut off before completing the JSON structure
fn fix_truncated_json(json: &str) -> String {
    // Count unmatched brackets
    let mut brace_count = 0;  // {}
    let mut bracket_count = 0; // []
    let mut in_string = false;
    let mut escape_next = false;

    for ch in json.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => brace_count += 1,
            '}' if !in_string => brace_count -= 1,
            '[' if !in_string => bracket_count += 1,
            ']' if !in_string => bracket_count -= 1,
            _ => {}
        }
    }

    // If balanced, return as-is
    if bracket_count == 0 && brace_count == 0 {
        return json.to_string();
    }

    // Build the missing closers in reverse order (] before })
    let mut closers = String::new();
    for _ in 0..bracket_count {
        closers.push(']');
    }
    for _ in 0..brace_count {
        closers.push('}');
    }

    // If JSON ends with }, we need to insert missing ] before it
    let trimmed = json.trim_end();
    if trimmed.ends_with('}') && bracket_count > 0 {
        // Find the last } and insert brackets before it
        if let Some(last_brace) = trimmed.rfind('}') {
            let mut result = trimmed[..last_brace].to_string();
            for _ in 0..bracket_count {
                result.push(']');
            }
            result.push('}');
            return result;
        }
    }

    // Otherwise just append
    format!("{}{}", json, closers)
}

fn extract_json_from_response(response: &str) -> String {
    let mut text = response.to_string();

    // Remove <unused94>...<unused95> reasoning blocks
    if let Some(unused95_pos) = text.find("<unused95>") {
        text = text[unused95_pos + "<unused95>".len()..].to_string();
    } else if let Some(unused94_pos) = text.find("<unused94>") {
        // If no <unused95>, look for JSON after reasoning
        if let Some(json_start) = text[unused94_pos..].find('{') {
            text = text[unused94_pos + json_start..].to_string();
        }
    }

    // Remove markdown code block markers
    text = text.replace("```json", "").replace("```", "");

    // Find the JSON object boundaries
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            let json_str = text[start..=end].to_string();
            // Fix unescaped newlines inside strings (common LLM error)
            let fixed_newlines = fix_json_newlines(&json_str);
            // Fix truncated JSON (missing closing brackets)
            return fix_truncated_json(&fixed_newlines);
        }
    }

    text.trim().to_string()
}

/// Parse JSON SOAP response and format as bullet-point text
fn parse_and_format_soap_json(response: &str) -> String {
    let json_str = extract_json_from_response(response);
    info!("Extracted JSON for parsing: {} chars", json_str.len());

    match serde_json::from_str::<SoapJsonResponse>(&json_str) {
        Ok(soap) => {
            info!("Successfully parsed SOAP JSON: S={}, O={}, A={}, P={}",
                  soap.subjective.len(), soap.objective.len(),
                  soap.assessment.len(), soap.plan.len());
            format_soap_as_text(&soap)
        }
        Err(e) => {
            warn!("Failed to parse SOAP JSON: {}. Raw: {:?}", e, &json_str[..json_str.len().min(200)]);
            // Try to extract SOAP from text format as fallback
            let cleaned = clean_llm_response(response);
            if let Some(soap) = try_parse_text_soap(&cleaned) {
                info!("Successfully parsed SOAP from text format");
                format_soap_as_text(&soap)
            } else {
                // Return cleaned text as last resort
                cleaned
            }
        }
    }
}

/// Try to parse SOAP note from text format (S:/O:/A:/P: sections)
fn try_parse_text_soap(text: &str) -> Option<SoapJsonResponse> {
    let mut subjective = Vec::new();
    let mut objective = Vec::new();
    let mut assessment = Vec::new();
    let mut plan = Vec::new();

    let mut current_section: Option<&str> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check for section headers
        let lower = trimmed.to_lowercase();
        if lower.starts_with("s:") || lower == "s" || lower.starts_with("subjective") {
            current_section = Some("S");
            // Check if there's content after the colon on same line
            if let Some(content) = trimmed.split_once(':').map(|(_, c)| c.trim()) {
                if !content.is_empty() {
                    subjective.push(content.to_string());
                }
            }
            continue;
        } else if lower.starts_with("o:") || lower == "o" || lower.starts_with("objective") {
            current_section = Some("O");
            if let Some(content) = trimmed.split_once(':').map(|(_, c)| c.trim()) {
                if !content.is_empty() {
                    objective.push(content.to_string());
                }
            }
            continue;
        } else if lower.starts_with("a:") || lower == "a" || lower.starts_with("assessment") {
            current_section = Some("A");
            if let Some(content) = trimmed.split_once(':').map(|(_, c)| c.trim()) {
                if !content.is_empty() {
                    assessment.push(content.to_string());
                }
            }
            continue;
        } else if lower.starts_with("p:") || lower == "p" || lower.starts_with("plan") {
            current_section = Some("P");
            if let Some(content) = trimmed.split_once(':').map(|(_, c)| c.trim()) {
                if !content.is_empty() {
                    plan.push(content.to_string());
                }
            }
            continue;
        }

        // Add content to current section
        if let Some(section) = current_section {
            // Strip bullet markers
            let content = trimmed
                .trim_start_matches('•')
                .trim_start_matches('-')
                .trim_start_matches('*')
                .trim();

            if !content.is_empty() && content.to_lowercase() != "not documented" {
                match section {
                    "S" => subjective.push(content.to_string()),
                    "O" => objective.push(content.to_string()),
                    "A" => assessment.push(content.to_string()),
                    "P" => plan.push(content.to_string()),
                    _ => {}
                }
            }
        }
    }

    // Only return if we found at least one section
    if subjective.is_empty() && objective.is_empty() && assessment.is_empty() && plan.is_empty() {
        None
    } else {
        Some(SoapJsonResponse {
            subjective,
            objective,
            assessment,
            plan,
        })
    }
}

/// Strip markdown formatting from a single item string
fn strip_markdown_from_item(item: &str) -> String {
    item.replace("**", "")
        .replace("__", "")
        .replace('`', "")
        .replace("###", "")
        .replace("##", "")
        .replace("# ", "")
}

/// Format parsed SOAP JSON as bullet-point text for EMR copy-paste
fn format_soap_as_text(soap: &SoapJsonResponse) -> String {
    let mut output = String::new();

    // Subjective
    output.push_str("S:\n");
    if soap.subjective.is_empty() {
        output.push_str("• Not documented\n");
    } else {
        for item in &soap.subjective {
            output.push_str(&format!("• {}\n", strip_markdown_from_item(item)));
        }
    }

    // Objective
    output.push_str("\nO:\n");
    if soap.objective.is_empty() {
        output.push_str("• Not documented\n");
    } else {
        for item in &soap.objective {
            output.push_str(&format!("• {}\n", strip_markdown_from_item(item)));
        }
    }

    // Assessment
    output.push_str("\nA:\n");
    if soap.assessment.is_empty() {
        output.push_str("• Not documented\n");
    } else {
        for item in &soap.assessment {
            output.push_str(&format!("• {}\n", strip_markdown_from_item(item)));
        }
    }

    // Plan
    output.push_str("\nP:\n");
    if soap.plan.is_empty() {
        output.push_str("• Not documented\n");
    } else {
        for item in &soap.plan {
            output.push_str(&format!("• {}\n", strip_markdown_from_item(item)));
        }
    }

    output.trim_end().to_string()
}

/// Build user content for SOAP generation
fn build_soap_user_content(
    transcript: &str,
    audio_events: Option<&[AudioEvent]>,
    session_notes: Option<&str>,
    speaker_context: Option<&SpeakerContext>,
) -> String {
    let mut content = String::new();

    // Add speaker context FIRST (before transcript)
    if let Some(ctx) = speaker_context {
        let speaker_section = ctx.format_for_prompt();
        if !speaker_section.is_empty() {
            content.push_str(&speaker_section);
            content.push_str("\n");
        }
    }

    // Add transcript
    content.push_str(&format!("TRANSCRIPT:\n{}", transcript));

    // Add audio events
    let audio_section = audio_events
        .filter(|e| !e.is_empty())
        .map(format_audio_events)
        .unwrap_or_default();

    if !audio_section.is_empty() {
        content.push_str("\n\n");
        content.push_str(&audio_section);
    }

    // Add session notes
    let notes_section = session_notes
        .filter(|n| !n.trim().is_empty())
        .map(|n| format!("CLINICIAN NOTES:\n{}", n.trim()))
        .unwrap_or_default();

    if !notes_section.is_empty() {
        content.push_str("\n\n");
        content.push_str(&notes_section);
    }

    content
}

/// Format audio events for inclusion in the prompt
fn format_audio_events(events: &[AudioEvent]) -> String {
    if events.is_empty() {
        return String::new();
    }

    let mut output = String::from("AUDIO EVENTS DETECTED:\n");
    for event in events {
        let total_seconds = event.timestamp_ms / 1000;
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        let conf_pct = 100.0 / (1.0 + (-event.confidence).exp());

        output.push_str(&format!(
            "- {} at {}:{:02} (confidence: {:.0}%)\n",
            event.label, minutes, seconds, conf_pct
        ));
    }
    output
}

/// Parse greeting detection response from the LLM
fn parse_greeting_response(response: &str, _sensitivity: f32) -> Result<GreetingResult, String> {
    let cleaned = response.trim();

    // Try to find JSON in the response
    let json_str = if let Some(start) = cleaned.find('{') {
        if let Some(end) = cleaned.rfind('}') {
            &cleaned[start..=end]
        } else {
            return Err("No valid JSON found in response".to_string());
        }
    } else {
        return Err("No JSON object found in response".to_string());
    };

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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prepare_transcript_valid() {
        let transcript = "Hello, this is a valid transcript with enough words and content.";
        let result = LLMClient::prepare_transcript(transcript);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), transcript.trim());
    }

    #[test]
    fn test_prepare_transcript_empty() {
        let result = LLMClient::prepare_transcript("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_prepare_transcript_too_short() {
        let result = LLMClient::prepare_transcript("Hi there");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too short"));
    }

    #[test]
    fn test_prepare_transcript_too_few_words() {
        // Must be at least 50 chars but fewer than 5 words to hit the word count check
        // Using long words to meet character minimum but not word minimum
        let result = LLMClient::prepare_transcript("Thiswordisquitelong anotherverylongword anotherlongonehere lastlongword");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too few words"));
    }

    #[test]
    fn test_truncate_transcript() {
        // Create a transcript with 15,000 words (above the 10,000 limit)
        let words: Vec<&str> = (0..15_000).map(|_| "word").collect();
        let long_transcript = words.join(" ");

        let result = LLMClient::prepare_transcript(&long_transcript);
        assert!(result.is_ok());

        let truncated = result.unwrap();
        // Should contain the omitted message
        assert!(truncated.contains("words omitted from middle"));

        // Check word count is approximately at limit
        let output_words: Vec<&str> = truncated.split_whitespace().collect();
        // Account for the "[... X words omitted ...]" message which adds ~6 words
        assert!(output_words.len() <= LLMClient::MAX_WORDS_FOR_LLM + 10);
    }

    #[test]
    fn test_truncate_preserves_structure() {
        // Create a compact transcript where we can identify start and end portions
        // Uses short identifiers to stay under 100KB limit
        let start_words: Vec<String> = (0..2000).map(|i| format!("s{}", i)).collect();
        let middle_words: Vec<String> = (0..12000).map(|i| format!("m{}", i)).collect();
        let end_words: Vec<String> = (0..2000).map(|i| format!("e{}", i)).collect();

        let full_transcript = format!(
            "{} {} {}",
            start_words.join(" "),
            middle_words.join(" "),
            end_words.join(" ")
        );

        let result = LLMClient::prepare_transcript(&full_transcript);
        assert!(result.is_ok());

        let truncated = result.unwrap();
        // Should have start words
        assert!(truncated.contains("s0"));
        // Should have end words
        assert!(truncated.contains("e1999"));
        // Should indicate truncation
        assert!(truncated.contains("words omitted"));
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
    fn test_llm_client_new() {
        let client = LLMClient::new("http://localhost:4000", "test-key", "ai-scribe", "fast-model").unwrap();
        assert_eq!(client.base_url, "http://localhost:4000");
        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.client_id, "ai-scribe");
    }

    #[test]
    fn test_llm_client_new_trailing_slash() {
        let client = LLMClient::new("http://localhost:4000/", "key", "client", "fast-model").unwrap();
        assert_eq!(client.base_url, "http://localhost:4000");
    }

    #[test]
    fn test_llm_client_new_invalid_url() {
        let result = LLMClient::new("not-a-valid-url", "key", "client", "fast-model");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid LLM router URL"));
    }

    #[test]
    fn test_llm_client_new_invalid_scheme() {
        let result = LLMClient::new("ftp://localhost:4000", "key", "client", "fast-model");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("http or https"));
    }

    #[test]
    fn test_llm_client_new_with_credentials() {
        let result = LLMClient::new("http://user:pass@localhost:4000", "key", "client", "fast-model");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must not contain credentials"));
    }

    #[test]
    fn test_soap_options_default() {
        let opts = SoapOptions::default();
        assert_eq!(opts.detail_level, 5);
        assert_eq!(opts.format, SoapFormat::ProblemBased);
        assert!(opts.custom_instructions.is_empty());
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
    fn test_llm_status_serialization() {
        let status = LLMStatus {
            connected: true,
            available_models: vec!["soap-model".to_string(), "fast-model".to_string()],
            error: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: LLMStatus = serde_json::from_str(&json).unwrap();

        assert!(parsed.connected);
        assert_eq!(parsed.available_models.len(), 2);
        assert!(parsed.error.is_none());
    }

    #[test]
    fn test_parse_greeting_response_valid() {
        let response = r#"{"is_greeting": true, "confidence": 0.85, "detected_phrase": "Hello"}"#;
        let result = parse_greeting_response(response, 0.7).unwrap();
        assert!(result.is_greeting);
        assert!((result.confidence - 0.85).abs() < 0.01);
        assert_eq!(result.detected_phrase, Some("Hello".to_string()));
    }

    #[test]
    fn test_parse_greeting_response_not_greeting() {
        let response = r#"{"is_greeting": false, "confidence": 0.2, "detected_phrase": null}"#;
        let result = parse_greeting_response(response, 0.7).unwrap();
        assert!(!result.is_greeting);
        assert!(result.detected_phrase.is_none());
    }

    #[test]
    fn test_audio_event_serialization() {
        let event = AudioEvent {
            timestamp_ms: 12345,
            duration_ms: 500,
            confidence: 2.5,
            label: "Cough".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AudioEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.timestamp_ms, 12345);
        assert_eq!(parsed.label, "Cough");
    }
}
