//! LLM Router API client for SOAP note generation
//!
//! This module provides integration with an OpenAI-compatible LLM router for generating
//! structured SOAP (Subjective, Objective, Assessment, Plan) notes from clinical transcripts.

use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::encounter_detection::MultiPatientDetectionResult;

/// Truncate HTTP error bodies to prevent PHI leakage and log flooding.
/// Proxy error pages (e.g. nginx 502) can echo request bodies containing
/// patient transcripts; capping at `max_len` chars prevents that data from
/// entering logs or error strings.
fn truncate_error_body(body: &str, max_len: usize) -> &str {
    if body.len() <= max_len {
        body
    } else {
        let end = body.ceil_char_boundary(max_len);
        &body[..end]
    }
}

/// Full outcome of a multi-patient detection LLM call, including the raw
/// prompt/response for pipeline logging.
pub struct MultiPatientDetectionOutcome {
    /// `Some` if multiple patients detected with sufficient confidence.
    pub detection: Option<MultiPatientDetectionResult>,
    pub system_prompt: String,
    pub user_prompt: String,
    pub model: String,
    pub response_raw: Option<String>,
    pub latency_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

/// Default timeout for LLM API requests (5 minutes for long SOAP generation)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

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

/// A single part of a multimodal message content array
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    /// Text content
    #[serde(rename = "text")]
    Text { text: String },
    /// Image as a data URL or remote URL
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrlContent },
}

/// Image URL payload for multimodal messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlContent {
    /// data:image/jpeg;base64,... or https://...
    pub url: String,
}

/// Chat message content — either a plain string or a multimodal array.
/// Serializes as a JSON string or array depending on the variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatMessageContent {
    /// Plain text (backward-compatible with all existing call sites)
    Text(String),
    /// Multimodal content array (text + images)
    Multimodal(Vec<ContentPart>),
}

impl ChatMessageContent {
    /// Extract the text content. For Text variant returns the string directly.
    /// For Multimodal, concatenates all text parts.
    pub fn as_text(&self) -> &str {
        match self {
            ChatMessageContent::Text(s) => s,
            ChatMessageContent::Multimodal(_) => "",
        }
    }
}

/// OpenAI-compatible chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: ChatMessageContent,
}

/// OpenAI-compatible chat completion request
#[derive(Debug, Clone, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repetition_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    repetition_context_size: Option<u32>,
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

impl MultiPatientSoapResult {
    /// Format SOAP notes for archive storage.
    /// Single-patient: bare content. Multi-patient: `=== Patient Label ===` headers.
    pub fn format_for_archive(&self) -> String {
        if self.notes.len() > 1 {
            self.notes.iter()
                .map(|n| format!("=== {} ===\n{}", n.patient_label, n.content))
                .collect::<Vec<_>>()
                .join("\n\n---\n\n")
        } else {
            self.notes.iter()
                .map(|n| n.content.clone())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n")
        }
    }
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

/// Sentinel substring in the placeholder returned when all SOAP JSON parsers fail.
/// Used by the retry logic to detect malformed output.
const MALFORMED_SOAP_SENTINEL: &str = "SOAP generation produced malformed output";

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

impl SoapFormat {
    /// Convert a config string to SoapFormat (defaults to ProblemBased for unknown values)
    pub fn from_config_str(s: &str) -> Self {
        if s == "comprehensive" { SoapFormat::Comprehensive } else { SoapFormat::ProblemBased }
    }
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
    /// Global personal instructions from the physician (persisted in settings)
    #[serde(default)]
    pub custom_instructions: String,
    /// Per-session instructions from the physician (entered in ReviewMode, not persisted)
    #[serde(default)]
    pub session_custom_instructions: String,
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
            session_custom_instructions: String::new(),
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
    /// Count of currently in-flight `generate_timed` calls. Incremented on entry,
    /// decremented on return. Snapshotted into `CallMetrics.concurrent_at_start`
    /// so we can attribute tail latencies to app-side concurrency vs router-side
    /// processing. Added Apr 2026 after the Apr 16 Room 6 audit showed a
    /// p90=27s / p99=40s tail on encounter_detection with no way to distinguish
    /// those causes from the existing `latency_ms` alone.
    in_flight: AtomicUsize,
}

/// Timing + concurrency metrics for a single LLM call. Emitted alongside the
/// pipeline_log event so post-hoc analysis can answer:
///   • "how much of `wall_ms` was us vs the network?"  (scheduling_ms vs network_ms)
///   • "how many LLM calls were queued at the router when we started?"  (concurrent_at_start)
///   • "did we have to retry the call?"  (retry_count)
///
/// `scheduling_ms` captures the time between the caller invoking `generate_timed`
/// and the first HTTP byte going out — a combination of tokio wake latency,
/// prompt serialization, and any contention on our side. `network_ms` is the
/// *cumulative* HTTP + response-body time across retries, so on a successful
/// first-try call `wall_ms ≈ scheduling_ms + network_ms`.
#[derive(Debug, Clone, Copy, Default)]
pub struct CallMetrics {
    pub wall_ms: u64,
    pub scheduling_ms: u64,
    pub network_ms: u64,
    pub concurrent_at_start: usize,
    pub retry_count: u32,
}

impl CallMetrics {
    /// Merge the metrics into an existing pipeline_log context object. Callers
    /// pass the mutated value to `log_llm_call` / `log_soap` / etc. — no API
    /// surface changes required on the logger side. If `context` isn't an
    /// object (unexpected), the metrics are silently skipped.
    pub fn attach_to(&self, context: &mut serde_json::Value) {
        if let Some(obj) = context.as_object_mut() {
            obj.insert("scheduling_ms".into(), serde_json::json!(self.scheduling_ms));
            obj.insert("network_ms".into(), serde_json::json!(self.network_ms));
            obj.insert("concurrent_at_start".into(), serde_json::json!(self.concurrent_at_start));
            if self.retry_count > 0 {
                obj.insert("retry_count".into(), serde_json::json!(self.retry_count));
            }
        }
    }
}

// Re-export as OllamaClient for backward compatibility
pub type OllamaClient = LLMClient;

/// RAII guard that decrements the in-flight counter on drop, guaranteeing
/// correctness across every early-return path in `generate_timed` without
/// forcing each arm to remember to decrement.
struct InFlightGuard<'a> {
    counter: &'a AtomicUsize,
}

impl<'a> Drop for InFlightGuard<'a> {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

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
            in_flight: AtomicUsize::new(0),
        })
    }

    /// Parse SOAP JSON response, retrying the LLM call once if the result is the malformed placeholder.
    /// Transient truncation from the LLM usually succeeds on retry.
    async fn parse_soap_with_retry(
        &self,
        model: &str,
        system_prompt: &str,
        user_content: &str,
        initial_response: &str,
    ) -> String {
        let content = parse_and_format_soap_json(initial_response);
        if !content.contains(MALFORMED_SOAP_SENTINEL) {
            return content;
        }
        warn!("SOAP parse returned malformed placeholder, retrying LLM call once");
        match self.generate(model, system_prompt, user_content, tasks::SOAP_NOTE).await {
            Ok(retry_response) => {
                let retry_content = parse_and_format_soap_json(&retry_response);
                if retry_content.contains(MALFORMED_SOAP_SENTINEL) {
                    warn!("SOAP retry also produced malformed output, using placeholder");
                } else {
                    info!("SOAP retry succeeded ({} chars)", retry_content.len());
                }
                retry_content
            }
            Err(e) => {
                warn!("SOAP retry LLM call failed: {}", e);
                content
            }
        }
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
                content: ChatMessageContent::Text("Say OK".to_string()),
            }],
            stream: false,
            max_tokens: Some(10),
            temperature: None,
            repetition_penalty: None,
            repetition_context_size: None,
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
                    let truncated = truncate_error_body(&body, 200);
                    error!("Failed to pre-warm model {}: {} - {}", model, status, truncated);
                    Err(format!("Failed to pre-warm model: {} - {}", status, truncated))
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

    /// Generate text using the LLM with retry logic.
    /// Thin wrapper over [`generate_timed`] for callers that don't need metrics.
    pub async fn generate(
        &self,
        model: &str,
        system_prompt: &str,
        user_content: &str,
        task: &str,
    ) -> Result<String, String> {
        self.generate_timed(model, system_prompt, user_content, task).await.0
    }

    /// Same as [`generate`] but returns timing + concurrency metrics alongside
    /// the result. Use this for any call path that feeds `pipeline_log` so
    /// post-hoc analysis can tell "app-side scheduling latency" apart from
    /// "LLM-side processing latency".
    pub async fn generate_timed(
        &self,
        model: &str,
        system_prompt: &str,
        user_content: &str,
        task: &str,
    ) -> (Result<String, String>, CallMetrics) {
        let entry_ts = Instant::now();
        let concurrent_at_start = self.in_flight.fetch_add(1, Ordering::Relaxed);
        let _guard = InFlightGuard { counter: &self.in_flight };

        if model.trim().is_empty() {
            return (
                Err("Model name cannot be empty".to_string()),
                CallMetrics {
                    wall_ms: entry_ts.elapsed().as_millis() as u64,
                    scheduling_ms: entry_ts.elapsed().as_millis() as u64,
                    network_ms: 0,
                    concurrent_at_start,
                    retry_count: 0,
                },
            );
        }
        if user_content.trim().is_empty() {
            return (
                Err("User content cannot be empty".to_string()),
                CallMetrics {
                    wall_ms: entry_ts.elapsed().as_millis() as u64,
                    scheduling_ms: entry_ts.elapsed().as_millis() as u64,
                    network_ms: 0,
                    concurrent_at_start,
                    retry_count: 0,
                },
            );
        }

        let url = format!("{}/v1/chat/completions", self.base_url);
        debug!("Generating with LLM model {} at {}", model, url);

        let mut messages = Vec::new();
        if !system_prompt.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: ChatMessageContent::Text(system_prompt.to_string()),
            });
        }
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: ChatMessageContent::Text(user_content.to_string()),
        });

        // SOAP generation needs explicit max_tokens to avoid output truncation
        // on large transcripts (router/model defaults may be too low)
        let max_tokens = if task == tasks::SOAP_NOTE { Some(4096) } else { None };

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages,
            stream: false,
            max_tokens,
            temperature: None,
            repetition_penalty: None,
            repetition_context_size: None,
        };

        let mut last_error = String::new();
        let mut network_ms_total: u64 = 0;
        let mut retry_count: u32 = 0;

        // Scheduling latency is measured once: from `generate_timed` entry to
        // just before the FIRST send() call. Everything else on the wall-clock
        // path is network (HTTP + body parse) plus retry backoff.
        let mut scheduling_ms: Option<u64> = None;

        for attempt in 0..DEFAULT_MAX_RETRIES {
            if attempt > 0 {
                retry_count = attempt;
                let backoff = calculate_backoff(attempt - 1);
                warn!(
                    "LLM generate attempt {} failed, retrying in {:?}",
                    attempt, backoff
                );
                tokio::time::sleep(backoff).await;
            }

            let http_start = Instant::now();
            if scheduling_ms.is_none() {
                scheduling_ms = Some(entry_ts.elapsed().as_millis() as u64);
            }

            let result = self.client
                .post(&url)
                .headers(self.auth_headers(task))
                .json(&request)
                .send()
                .await;

            match result {
                Ok(response) => {
                    if response.status().is_success() {
                        let body_result = response.json::<ChatCompletionResponse>().await;
                        network_ms_total = network_ms_total.saturating_add(http_start.elapsed().as_millis() as u64);
                        match body_result {
                            Ok(chat_response) => {
                                let res = if let Some(choice) = chat_response.choices.first() {
                                    Ok(choice.message.content.as_text().to_string())
                                } else {
                                    Err("No response choices returned".to_string())
                                };
                                return (
                                    res,
                                    CallMetrics {
                                        wall_ms: entry_ts.elapsed().as_millis() as u64,
                                        scheduling_ms: scheduling_ms.unwrap_or(0),
                                        network_ms: network_ms_total,
                                        concurrent_at_start,
                                        retry_count,
                                    },
                                );
                            }
                            Err(e) => {
                                last_error = format!("Failed to parse LLM response: {}", e);
                                break;
                            }
                        }
                    } else if is_retryable_status(response.status()) {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        network_ms_total = network_ms_total.saturating_add(http_start.elapsed().as_millis() as u64);
                        last_error = format!("LLM router returned error: {} - {}", status, truncate_error_body(&body, 200));
                        continue;
                    } else {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        network_ms_total = network_ms_total.saturating_add(http_start.elapsed().as_millis() as u64);
                        let truncated = truncate_error_body(&body, 200);
                        error!("LLM generate failed: {} - {}", status, truncated);
                        return (
                            Err(format!("LLM router returned error: {} - {}", status, truncated)),
                            CallMetrics {
                                wall_ms: entry_ts.elapsed().as_millis() as u64,
                                scheduling_ms: scheduling_ms.unwrap_or(0),
                                network_ms: network_ms_total,
                                concurrent_at_start,
                                retry_count,
                            },
                        );
                    }
                }
                Err(e) => {
                    network_ms_total = network_ms_total.saturating_add(http_start.elapsed().as_millis() as u64);
                    if is_retryable_error(&e) {
                        last_error = format!("Failed to connect to LLM router: {}", e);
                        continue;
                    } else {
                        return (
                            Err(format!("Failed to connect to LLM router: {}", e)),
                            CallMetrics {
                                wall_ms: entry_ts.elapsed().as_millis() as u64,
                                scheduling_ms: scheduling_ms.unwrap_or(0),
                                network_ms: network_ms_total,
                                concurrent_at_start,
                                retry_count,
                            },
                        );
                    }
                }
            }
        }

        error!(
            "LLM generate failed after {} attempts: {}",
            DEFAULT_MAX_RETRIES, last_error
        );
        (
            Err(last_error),
            CallMetrics {
                wall_ms: entry_ts.elapsed().as_millis() as u64,
                scheduling_ms: scheduling_ms.unwrap_or(entry_ts.elapsed().as_millis() as u64),
                network_ms: network_ms_total,
                concurrent_at_start,
                retry_count,
            },
        )
    }

    /// Maximum transcript size (500KB) to prevent memory issues
    /// Allows sessions up to ~5 hours before hitting this limit
    const MAX_TRANSCRIPT_SIZE: usize = 500_000;

    /// Minimum transcript length (50 chars) to ensure meaningful SOAP generation
    const MIN_TRANSCRIPT_LENGTH: usize = 50;

    /// Minimum word count for meaningful SOAP generation
    const MIN_WORD_COUNT: usize = 5;

    /// Maximum words to send to LLM (Mistral Small 3 8B has 128K token context)
    /// ~30,000 words ≈ 39,000 tokens, leaving room for system prompt and response
    const MAX_WORDS_FOR_LLM: usize = 30_000;

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

        let system_prompt = build_simple_soap_prompt(&opts, None);
        let session_notes = if opts.session_notes.trim().is_empty() { None } else { Some(opts.session_notes.as_str()) };
        let user_content = build_soap_user_content(&prepared_transcript, audio_events, session_notes, speaker_context);

        let response = self.generate(model, &system_prompt, &user_content, tasks::SOAP_NOTE).await?;
        let content = self.parse_soap_with_retry(model, &system_prompt, &user_content, &response).await;

        info!("Successfully generated SOAP note ({} chars)", content.len());
        Ok(SoapNote {
            content,
            generated_at: Utc::now().to_rfc3339(),
            model_used: model.to_string(),
        })
    }

    /// Generate a SOAP note scoped to a single patient within a multi-patient transcript.
    /// Uses `build_single_patient_soap_prompt` which appends a patient-scoping constraint
    /// to the base SOAP system prompt.
    ///
    /// # Arguments
    /// * `model` - LLM model name to use
    /// * `transcript` - Full clinical transcript (may contain multiple patients)
    /// * `patient_label` - The patient label to scope the note to (e.g. "Patient A")
    /// * `audio_events` - Optional detected audio events
    /// * `options` - SOAP generation options
    /// * `speaker_context` - Optional speaker identification context
    pub async fn generate_single_patient_soap_note(
        &self,
        model: &str,
        transcript: &str,
        patient_label: &str,
        audio_events: Option<&[AudioEvent]>,
        options: Option<&SoapOptions>,
        speaker_context: Option<&SpeakerContext>,
    ) -> Result<SoapNote, String> {
        let prepared_transcript = Self::prepare_transcript(transcript)?;
        let opts = options.cloned().unwrap_or_default();
        info!(
            "Generating single-patient SOAP note for \"{}\" with model {} ({} chars, {} words)",
            patient_label,
            model,
            prepared_transcript.len(),
            prepared_transcript.split_whitespace().count(),
        );

        let system_prompt = build_single_patient_soap_prompt(&opts, patient_label, None);
        let session_notes = if opts.session_notes.trim().is_empty() { None } else { Some(opts.session_notes.as_str()) };
        let user_content = build_soap_user_content(&prepared_transcript, audio_events, session_notes, speaker_context);

        let response = self.generate(model, &system_prompt, &user_content, tasks::SOAP_NOTE).await?;
        let content = self.parse_soap_with_retry(model, &system_prompt, &user_content, &response).await;

        info!("Successfully generated single-patient SOAP note for \"{}\" ({} chars)", patient_label, content.len());
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
        multi_patient_detection: Option<&MultiPatientDetectionResult>,
    ) -> Result<MultiPatientSoapResult, String> {
        let prepared_transcript = Self::prepare_transcript(transcript)?;
        let opts = options.cloned().unwrap_or_default();

        // Per-patient path: callers pass Some() only after confidence gating
        if let Some(detection) = multi_patient_detection {
            let word_count = prepared_transcript.split_whitespace().count();
            info!(
                "Generating {} per-patient SOAP notes with model {} ({} chars, {} words)",
                detection.patient_count, model, prepared_transcript.len(), word_count,
            );
            return self.generate_per_patient_soap(model, &prepared_transcript, &opts, detection).await;
        }

        // Single-patient path (existing behavior)
        info!(
            "Generating multi-patient SOAP note with model {} for transcript of {} chars ({} words), {} speakers",
            model,
            prepared_transcript.len(),
            prepared_transcript.split_whitespace().count(),
            speaker_context.map(|c| c.speakers.len()).unwrap_or(0)
        );

        let system_prompt = build_simple_soap_prompt(&opts, None);
        let session_notes = if opts.session_notes.trim().is_empty() { None } else { Some(opts.session_notes.as_str()) };
        let user_content = build_soap_user_content(&prepared_transcript, audio_events, session_notes, speaker_context);

        let response = self.generate(model, &system_prompt, &user_content, tasks::SOAP_NOTE).await?;
        let content = self.parse_soap_with_retry(model, &system_prompt, &user_content, &response).await;

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

    /// Generate N concurrent per-patient SOAP notes from the full transcript.
    async fn generate_per_patient_soap(
        &self,
        model: &str,
        transcript: &str,
        options: &SoapOptions,
        detection: &MultiPatientDetectionResult,
    ) -> Result<MultiPatientSoapResult, String> {
        let system_prompt = build_per_patient_soap_prompt(options, None);
        let all_patients_desc: String = detection.patients.iter()
            .map(|p| format!("{}: {}", p.label, p.summary))
            .collect::<Vec<_>>()
            .join("; ");

        // Build futures for concurrent generation
        let futures: Vec<_> = detection.patients.iter().map(|patient| {
            let user_content = build_per_patient_user_content(
                transcript, &patient.label, &patient.summary, &all_patients_desc,
            );
            let sys = system_prompt.clone();
            let mdl = model.to_string();
            async move {
                let response = self.generate(&mdl, &sys, &user_content, tasks::SOAP_NOTE).await?;
                let content = self.parse_soap_with_retry(&mdl, &sys, &user_content, &response).await;
                Ok::<PatientSoapNote, String>(PatientSoapNote {
                    patient_label: patient.label.clone(),
                    speaker_id: "All".to_string(),
                    content,
                })
            }
        }).collect();

        let results = futures_util::future::join_all(futures).await;

        let mut notes = Vec::new();
        let mut errors = Vec::new();
        for result in results {
            match result {
                Ok(note) => {
                    info!("Per-patient SOAP generated for '{}' ({} chars)", note.patient_label, note.content.len());
                    notes.push(note);
                }
                Err(e) => {
                    warn!("Per-patient SOAP generation failed: {}", e);
                    errors.push(e);
                }
            }
        }

        if notes.is_empty() {
            return Err(format!("All per-patient SOAP generations failed: {}", errors.join("; ")));
        }

        info!("Successfully generated {}/{} per-patient SOAP notes", notes.len(), detection.patient_count);
        Ok(MultiPatientSoapResult {
            notes,
            physician_speaker: None,
            generated_at: Utc::now().to_rfc3339(),
            model_used: model.to_string(),
        })
    }

    /// Run multi-patient detection on a transcript.
    /// Returns full outcome (detection result + LLM call details for logging).
    /// `outcome.detection` is `Some` only if multiple patients detected with sufficient confidence.
    pub async fn run_multi_patient_detection(
        &self,
        fast_model: &str,
        transcript: &str,
    ) -> MultiPatientDetectionOutcome {
        use crate::encounter_detection::{
            MULTI_PATIENT_DETECT_PROMPT, MULTI_PATIENT_DETECT_MIN_CONFIDENCE,
            parse_multi_patient_detection,
        };

        let mp_user = format!("Transcript (segments numbered with speaker labels):\n{}", transcript);
        let start = std::time::Instant::now();
        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            self.generate(fast_model, MULTI_PATIENT_DETECT_PROMPT, &mp_user, "multi_patient_detect"),
        ).await;
        let latency_ms = start.elapsed().as_millis() as u64;

        let (detection, response_raw, success, error) = match result {
            Ok(Ok(resp)) => {
                match parse_multi_patient_detection(&resp) {
                    Ok(det) => {
                        info!(
                            "Multi-patient detection: count={}, conf={:?}, reasoning={:?}",
                            det.patient_count, det.confidence, det.reasoning
                        );
                        let accepted = det.patient_count > 1
                            && det.confidence.unwrap_or(0.0) >= MULTI_PATIENT_DETECT_MIN_CONFIDENCE
                            && det.patients.len() > 1;
                        (if accepted { Some(det) } else { None }, Some(resp), true, None)
                    }
                    Err(e) => {
                        warn!("Failed to parse multi-patient detection: {}", e);
                        (None, Some(resp), false, Some(format!("Parse error: {}", e)))
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("Multi-patient detection LLM error: {}", e);
                (None, None, false, Some(e.to_string()))
            }
            Err(_) => {
                warn!("Multi-patient detection timed out");
                (None, None, false, Some("Timeout after 30s".to_string()))
            }
        };

        MultiPatientDetectionOutcome {
            detection,
            system_prompt: MULTI_PATIENT_DETECT_PROMPT.to_string(),
            user_prompt: mp_user,
            model: fast_model.to_string(),
            response_raw,
            latency_ms,
            success,
            error,
        }
    }

    /// Timeout for greeting detection
    const GREETING_TIMEOUT: Duration = Duration::from_secs(45);

    /// Check if a transcript contains a greeting that should start a session
    pub async fn check_greeting(
        &self,
        transcript: &str,
        sensitivity: f32,
        templates: Option<&crate::server_config::PromptTemplates>,
    ) -> Result<GreetingResult, String> {
        let trimmed = transcript.trim();

        if trimmed.is_empty() || trimmed.len() < 3 {
            return Ok(GreetingResult {
                is_greeting: false,
                confidence: 0.0,
                detected_phrase: None,
            });
        }

        let system_prompt = templates
            .and_then(|t| (!t.greeting_detection.is_empty()).then(|| {
                // Server template may contain {sensitivity} placeholder
                t.greeting_detection.replace("{sensitivity}", &format!("{:.2}", sensitivity))
            }))
            .unwrap_or_else(|| format!(
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
        ));

        let url = format!("{}/v1/chat/completions", self.base_url);
        info!("Checking greeting with LLM at {} (timeout={}s)", url, Self::GREETING_TIMEOUT.as_secs());

        let request = ChatCompletionRequest {
            model: self.fast_model.clone(), // Use configured fast model for greeting detection
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: ChatMessageContent::Text(system_prompt),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: ChatMessageContent::Text(trimmed.to_string()),
                },
            ],
            stream: false,
            max_tokens: Some(100),
            temperature: None,
            repetition_penalty: None,
            repetition_context_size: None,
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
            return Err(format!("LLM router returned error: {} - {}", status, truncate_error_body(&body, 200)));
        }

        let chat_response: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse LLM response: {}", e))?;

        let content = chat_response
            .choices
            .first()
            .map(|c| c.message.content.as_text())
            .unwrap_or("");

        info!("LLM greeting check completed, parsing response");
        debug!("Greeting check raw response: {}", &content.chars().take(200).collect::<String>());

        parse_greeting_response(content, sensitivity)
    }

    /// Generate text using the LLM with a multimodal user message (text + images).
    /// The system prompt is sent as a plain text message, but the user message
    /// uses a content array with ContentPart items.
    pub async fn generate_vision(
        &self,
        model: &str,
        system_prompt: &str,
        user_content: Vec<ContentPart>,
        task: &str,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
        repetition_penalty: Option<f32>,
        repetition_context_size: Option<u32>,
    ) -> Result<String, String> {
        if model.trim().is_empty() {
            return Err("Model name cannot be empty".to_string());
        }

        let url = format!("{}/v1/chat/completions", self.base_url);
        info!("Generating vision response with model {} at {}", model, url);

        let mut messages = Vec::new();

        if !system_prompt.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: ChatMessageContent::Text(system_prompt.to_string()),
            });
        }

        messages.push(ChatMessage {
            role: "user".to_string(),
            content: ChatMessageContent::Multimodal(user_content),
        });

        let request = ChatCompletionRequest {
            model: model.to_string(),
            messages,
            stream: false,
            max_tokens,
            temperature,
            repetition_penalty,
            repetition_context_size,
        };

        let mut last_error = String::new();

        for attempt in 0..DEFAULT_MAX_RETRIES {
            if attempt > 0 {
                let backoff = calculate_backoff(attempt - 1);
                warn!(
                    "Vision generate attempt {} failed, retrying in {:?}",
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
                                    return Ok(choice.message.content.as_text().to_string());
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
                        last_error = format!("LLM router returned error: {} - {}", status, truncate_error_body(&body, 200));
                        continue;
                    } else {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        let truncated = truncate_error_body(&body, 200);
                        error!("Vision generate failed: {} - {}", status, truncated);
                        return Err(format!("LLM router returned error: {} - {}", status, truncated));
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
            "Vision generate failed after {} attempts: {}",
            DEFAULT_MAX_RETRIES, last_error
        );
        Err(last_error)
    }

    /// Generate a SOAP note from a clinical transcript + screenshot composite.
    /// Uses the vision-model alias and sends the stitched image as a data URL.
    ///
    /// # Arguments
    /// * `model` - Vision-capable model name (e.g., "vision-model")
    /// * `transcript` - Clinical transcript text
    /// * `image_base64` - Base64-encoded JPEG of stitched thumbnails
    /// * `audio_events` - Optional audio events
    /// * `options` - Optional SOAP generation options
    pub async fn generate_vision_soap_note(
        &self,
        model: &str,
        transcript: &str,
        image_base64: &str,
        audio_events: Option<&[AudioEvent]>,
        options: Option<&SoapOptions>,
    ) -> Result<SoapNote, String> {
        let prepared_transcript = Self::prepare_transcript(transcript)?;
        let opts = options.cloned().unwrap_or_default();

        info!(
            "Generating vision SOAP note with model {} for transcript of {} chars, image {} bytes",
            model,
            prepared_transcript.len(),
            image_base64.len(),
        );

        let system_prompt = build_vision_soap_prompt(&opts, None);

        // Build user content: text BEFORE image (slightly faster per integration guide)
        let session_notes = if opts.session_notes.trim().is_empty() { None } else { Some(opts.session_notes.as_str()) };
        let text_content = build_soap_user_content(&prepared_transcript, audio_events, session_notes, None);

        let user_parts = vec![
            ContentPart::Text { text: text_content },
            ContentPart::ImageUrl {
                image_url: ImageUrlContent {
                    url: format!("data:image/jpeg;base64,{}", image_base64),
                },
            },
        ];

        let response = self.generate_vision(
            model,
            &system_prompt,
            user_parts,
            tasks::SOAP_NOTE,
            Some(0.3),           // temperature
            Some(2000),          // max_tokens
            Some(1.1),           // repetition_penalty - prevents repetitive output
            Some(50),            // repetition_context_size - window for penalty
        ).await?;

        // Parse JSON response and format as bullet-point text
        let content = parse_and_format_soap_json(&response);

        info!("Successfully generated vision SOAP note ({} chars)", content.len());
        Ok(SoapNote {
            content,
            generated_at: Utc::now().to_rfc3339(),
            model_used: model.to_string(),
        })
    }
}

/// Build system prompt for vision SOAP note generation
/// Uses the "Verified Steps" prompt strategy (P11) which achieved perfect scores in experiments
/// When `templates` is provided and the relevant field is non-empty, it overrides the hardcoded default.
fn build_vision_soap_prompt(
    _options: &SoapOptions,
    templates: Option<&crate::server_config::PromptTemplates>,
) -> String {
    templates
        .and_then(|t| (!t.soap_vision_template.is_empty()).then(|| t.soap_vision_template.clone()))
        .unwrap_or_else(|| {
    // Note: detail_level and custom_instructions are intentionally ignored for vision path
    // The step-by-step prompt with explicit verification prevents irrelevant EHR data extraction
    r#"Medical scribe AI. Follow these steps EXACTLY:

STEP 1: Read ONLY the transcript. Identify:
- Patient name (if mentioned in greeting)
- Chief complaint and symptoms discussed
- Treatment plan mentioned (medications, tests, follow-up)

STEP 2: Look at the EHR header ONLY. Find:
- Full patient name (to complete partial name from transcript)

STEP 3: Look at the medications section ONLY. Find:
- Specific medication names referenced in the transcript (e.g., "weekly medication" -> actual drug name)

STEP 4: VERIFY you are NOT including:
- Past medical history from EHR (unless discussed in transcript)
- Previous visits from EHR
- Unrelated conditions, allergies, or medications from EHR
- Family history from EHR

Output a concise JSON SOAP note:
{"subjective":[...],"objective":[...],"assessment":[...],"plan":[...]}

Rules:
- Only include information from the transcript PLUS specific lookups from Steps 2-3
- Do NOT include EHR data that wasn't referenced in the conversation
- Stop immediately after the closing brace
- Do not repeat information"#.to_string()
    })
}

/// Build system prompt for SOAP note generation (JSON output required).
/// Used by both single-patient and multi-patient SOAP generation.
/// When `templates` is provided and the relevant fields are non-empty, they override the hardcoded defaults.
/// The server provides TEXT FRAGMENTS; the client assembles them using the same dynamic logic
/// (detail_level matching, format matching, custom_instructions branching).
pub fn build_simple_soap_prompt(
    options: &SoapOptions,
    templates: Option<&crate::server_config::PromptTemplates>,
) -> String {
    // If a full base template is provided, use it directly (with detail/format/custom appended)
    if let Some(base) = templates.and_then(|t| (!t.soap_base_template.is_empty()).then(|| t.soap_base_template.clone())) {
        let detail_instruction = build_soap_detail_instruction(options, templates);
        let format_instruction = build_soap_format_instruction(options, templates);
        let custom_section = build_soap_custom_section(options, templates);
        return format!("{base}{custom_section}\n\n{format_instruction}\n\n- {detail_instruction}");
    }

    let detail_instruction = build_soap_detail_instruction(options, templates);
    let format_instruction = build_soap_format_instruction(options, templates);
    let custom_section = build_soap_custom_section(options, templates);

    format!(
        r#"You are a medical scribe that outputs ONLY valid JSON. Extract clinical information from transcripts into SOAP notes.{custom_section}

The transcript is from speech-to-text and may contain errors. Interpret medical terms correctly:
- "human blade 1c" or "h b a 1 c" → HbA1c (hemoglobin A1c)
- "ekg" or "e k g" → EKG/ECG
- Homophones and phonetic errors are common - use clinical context

RESPOND WITH ONLY THIS JSON STRUCTURE - NO OTHER TEXT:
{{"subjective":["item"],"objective":["item"],"assessment":["item"],"plan":["item"]}}

{format_instruction}

SECTION DEFINITIONS:
- SUBJECTIVE: What the patient reports — symptoms, complaints, history of present illness, past medical/surgical history, medication history, social history, family history, review of systems, and any information prefaced by "patient reports/states/denies/describes". Also include historical test results the patient or physician recounts from previous visits (e.g. "previous EKG showed...", "labs from September...").
- OBJECTIVE: ONLY findings from TODAY'S encounter — vital signs measured today, physical examination findings observed by the clinician, point-of-care test results obtained today, and imaging/lab results reviewed for the first time today. If the physician did not perform an exam or obtain new results, use an empty array []. Do NOT put patient-reported information here. Do NOT put historical results, prior imaging, or previous lab values here — those go in Subjective.
- ASSESSMENT: Clinical impressions, diagnoses, differential diagnoses, and the clinician's interpretation of the findings.
- PLAN: Treatments ordered, prescriptions, referrals, follow-up instructions, procedures performed, patient education provided, and next steps — ONLY what the doctor actually stated. Do NOT add recommendations not mentioned in the transcript.

Rules:
- Your entire response must be valid JSON - nothing else
- Use simple string arrays, no nested objects
- Do NOT use newlines inside JSON strings - keep each array item as a single line
- Use empty arrays [] for sections with no information
- Use correct medical terminology
- Do NOT use any markdown formatting (no **, no __, no #, no backticks) - output plain text only
- Do NOT include specific patient names or healthcare provider names - use "patient" or "the physician/provider" instead
- Do NOT hallucinate or embellish - only include what was explicitly stated
- CLINICIAN NOTES: If provided, incorporate clinician observations into the appropriate SOAP sections (usually Objective for physical observations, Subjective for reported symptoms).
- {detail_instruction}"#
    )
}

/// Build the detail-level instruction fragment for SOAP prompts.
fn build_soap_detail_instruction(
    options: &SoapOptions,
    templates: Option<&crate::server_config::PromptTemplates>,
) -> String {
    // Check server-provided detail instructions by level key
    let level_key = match options.detail_level {
        1..=3 => "low",
        4..=6 => "medium",
        7..=10 => "high",
        _ => "default",
    };
    if let Some(text) = templates
        .and_then(|t| t.soap_detail_instructions.get(level_key))
        .filter(|s| !s.is_empty())
    {
        return text.replace("{level}", &options.detail_level.to_string());
    }

    match options.detail_level {
        1..=3 => format!(
            "DETAIL LEVEL: {}/10 - Be BRIEF. Use short phrases, 1-2 items per section. Omit minor details.",
            options.detail_level
        ),
        4..=6 => format!(
            "DETAIL LEVEL: {}/10 - Use STANDARD clinical detail. Include key findings and relevant history.",
            options.detail_level
        ),
        7..=10 => format!(
            "DETAIL LEVEL: {}/10 - Be THOROUGH. Include all findings, measurements, pertinent negatives, and clinical reasoning.",
            options.detail_level
        ),
        _ => format!(
            "DETAIL LEVEL: {}/10 - Use standard clinical detail.",
            options.detail_level
        ),
    }
}

/// Build the format instruction fragment for SOAP prompts.
fn build_soap_format_instruction(
    options: &SoapOptions,
    templates: Option<&crate::server_config::PromptTemplates>,
) -> String {
    let format_key = match options.format {
        SoapFormat::ProblemBased => "problem_based",
        SoapFormat::Comprehensive => "comprehensive",
    };
    if let Some(text) = templates
        .and_then(|t| t.soap_format_instructions.get(format_key))
        .filter(|s| !s.is_empty())
    {
        return text.clone();
    }

    match options.format {
        SoapFormat::ProblemBased => "ORGANIZATION: If multiple medical problems are discussed, group items by problem. Prefix each bullet with the problem name in square brackets, e.g., '[Hypertension] Elevated BP reported at home' in Subjective, '[Hypertension] BP 150/90' in Objective, '[Hypertension] Uncontrolled, consider med adjustment' in Assessment, '[Hypertension] Increase amlodipine to 10mg' in Plan. Every item MUST have a [Problem] prefix. Problems with only one mention still get a prefix.".to_string(),
        SoapFormat::Comprehensive => "ORGANIZATION: Create a single unified SOAP note covering all problems together in each section. Do NOT prefix items with problem labels.".to_string(),
    }
}

/// Build the custom instructions section for SOAP prompts.
fn build_soap_custom_section(
    options: &SoapOptions,
    templates: Option<&crate::server_config::PromptTemplates>,
) -> String {
    let global_instr = options.custom_instructions.trim();
    let session_instr = options.session_custom_instructions.trim();

    // Check for server-provided custom section templates
    let global_tmpl = templates
        .and_then(|t| t.soap_custom_section_templates.get("global"))
        .filter(|s| !s.is_empty());
    let session_tmpl = templates
        .and_then(|t| t.soap_custom_section_templates.get("session"))
        .filter(|s| !s.is_empty());
    let combined_tmpl = templates
        .and_then(|t| t.soap_custom_section_templates.get("combined"))
        .filter(|s| !s.is_empty());

    match (global_instr.is_empty(), session_instr.is_empty()) {
        (true, true) => String::new(),
        (false, true) => {
            if let Some(tmpl) = global_tmpl {
                format!("\n\n{}", tmpl.replace("{global}", global_instr))
            } else {
                format!(
                    "\n\nCRITICAL - PHYSICIAN'S REQUIRED STYLE (you MUST follow these instructions exactly): {global_instr}"
                )
            }
        }
        (true, false) => {
            if let Some(tmpl) = session_tmpl {
                format!("\n\n{}", tmpl.replace("{session}", session_instr))
            } else {
                format!(
                    "\n\nCRITICAL - SESSION-SPECIFIC INSTRUCTIONS (you MUST follow these exactly): {session_instr}"
                )
            }
        }
        (false, false) => {
            if let Some(tmpl) = combined_tmpl {
                format!("\n\n{}", tmpl.replace("{global}", global_instr).replace("{session}", session_instr))
            } else {
                format!(
                    "\n\nCRITICAL - PHYSICIAN'S REQUIRED STYLE (you MUST follow these instructions exactly): {global_instr}\n\nSESSION-SPECIFIC INSTRUCTIONS (these take precedence where they conflict): {session_instr}"
                )
            }
        }
    }
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

// ── JSON repair pipeline ─────────────────────────────────────────────────
// Functions: fix_json_newlines → remove_leading_commas → remove_trailing_commas
//            → fix_truncated_json
// Applied in sequence by extract_json_from_response() and the aggressive
// repair fallback in parse_and_format_soap_json().
// ─────────────────────────────────────────────────────────────────────────

/// Fix unescaped newlines inside JSON strings.
/// LLMs sometimes produce JSON with literal newlines in strings which is invalid.
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

/// Remove leading commas inside arrays/objects (e.g. `[,"item"]` → `["item"]`)
/// LLMs sometimes produce empty leading elements before real content.
fn remove_leading_commas(json: &str) -> String {
    let mut result = String::with_capacity(json.len());
    let mut in_string = false;
    let mut escape_next = false;
    let mut prev_bracket = false; // true if last non-whitespace was [ or {

    for ch in json.chars() {
        if escape_next {
            result.push(ch);
            escape_next = false;
            prev_bracket = false;
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
                prev_bracket = false;
            }
            '[' | '{' if !in_string => {
                result.push(ch);
                prev_bracket = true;
            }
            ',' if !in_string && prev_bracket => {
                // Skip comma immediately after opening bracket (leading comma)
                // prev_bracket stays true to handle `[,,"item"]`
            }
            _ if !in_string && ch.is_whitespace() => {
                result.push(ch);
                // Don't reset prev_bracket on whitespace: `[ , "item"]`
            }
            _ => {
                result.push(ch);
                prev_bracket = false;
            }
        }
    }

    result
}

/// Remove trailing commas before closing brackets (e.g. `["item",]` → `["item"]`)
/// Also handles `,"item",}` patterns.
fn remove_trailing_commas(json: &str) -> String {
    let mut result = String::with_capacity(json.len());
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = json.chars().collect();

    for i in 0..chars.len() {
        let ch = chars[i];
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
            ',' if !in_string => {
                // Look ahead: skip this comma if the next non-whitespace is ] or }
                let next_significant = chars[i + 1..].iter().find(|c| !c.is_whitespace());
                if matches!(next_significant, Some(']') | Some('}')) {
                    // Skip trailing comma
                } else {
                    result.push(ch);
                }
            }
            _ => {
                result.push(ch);
            }
        }
    }

    result
}

/// Fix truncated JSON by closing unclosed strings and adding missing closing brackets
/// LLMs sometimes get cut off before completing the JSON structure
fn fix_truncated_json(json: &str) -> String {
    // Phase 1: Close unclosed strings
    // Count unescaped quotes to detect if we're mid-string
    let mut quote_count = 0u64;
    let mut escape_next = false;
    for ch in json.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' => escape_next = true,
            '"' => quote_count += 1,
            _ => {}
        }
    }

    let json = if quote_count % 2 != 0 {
        // Odd quote count means unclosed string — close it
        let mut fixed = json.to_string();
        // Strip trailing backslash if present (incomplete escape sequence)
        if fixed.ends_with('\\') {
            fixed.pop();
        }
        fixed.push('"');
        fixed
    } else {
        json.to_string()
    };

    // Phase 2: Count unmatched brackets
    let mut brace_count = 0i32;  // {}
    let mut bracket_count = 0i32; // []
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
            // Fix leading/trailing commas (e.g. [,"item"] or ["item",])
            let fixed_commas = remove_trailing_commas(&remove_leading_commas(&fixed_newlines));
            // Fix truncated JSON (missing closing brackets)
            return fix_truncated_json(&fixed_commas);
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
            // Filter out empty string elements (LLM artifact: ["", "real item"])
            let soap = SoapJsonResponse {
                subjective: soap.subjective.into_iter().filter(|s| !s.trim().is_empty()).collect(),
                objective: soap.objective.into_iter().filter(|s| !s.trim().is_empty()).collect(),
                assessment: soap.assessment.into_iter().filter(|s| !s.trim().is_empty()).collect(),
                plan: soap.plan.into_iter().filter(|s| !s.trim().is_empty()).collect(),
            };
            info!("Successfully parsed SOAP JSON: S={}, O={}, A={}, P={}",
                  soap.subjective.len(), soap.objective.len(),
                  soap.assessment.len(), soap.plan.len());
            format_soap_as_text(&soap)
        }
        Err(e) => {
            warn!("Failed to parse SOAP JSON: {}. Raw: {:?}", e, &json_str[..json_str.len().min(200)]);

            // Try to parse nested JSON structure (e.g., {"subjective": [{"Problem 1": [...]}]})
            if let Some(soap) = try_parse_nested_json_soap(&json_str) {
                info!("Successfully parsed SOAP from nested JSON structure");
                return format_soap_as_text(&soap);
            }

            // Try to extract SOAP from text format as fallback
            let cleaned = clean_llm_response(response);
            if let Some(soap) = try_parse_text_soap(&cleaned) {
                info!("Successfully parsed SOAP from text format");
                format_soap_as_text(&soap)
            } else if cleaned.trim_start().starts_with('{') || cleaned.contains("\"subjective\"") {
                // Last resort: result looks like raw/broken JSON — try aggressive repair
                warn!("Fallback result appears to be raw JSON, attempting aggressive repair");
                let repaired = fix_truncated_json(&remove_trailing_commas(&remove_leading_commas(&fix_json_newlines(&cleaned))));
                match serde_json::from_str::<SoapJsonResponse>(&repaired) {
                    Ok(soap) => {
                        let soap = SoapJsonResponse {
                            subjective: soap.subjective.into_iter().filter(|s| !s.trim().is_empty()).collect(),
                            objective: soap.objective.into_iter().filter(|s| !s.trim().is_empty()).collect(),
                            assessment: soap.assessment.into_iter().filter(|s| !s.trim().is_empty()).collect(),
                            plan: soap.plan.into_iter().filter(|s| !s.trim().is_empty()).collect(),
                        };
                        info!("Aggressive JSON repair succeeded");
                        format_soap_as_text(&soap)
                    }
                    Err(e2) => {
                        warn!("Aggressive JSON repair also failed: {}. Returning placeholder.", e2);
                        format!("S:\n- [{} — review transcript directly]\n\nO:\n- [See transcript]\n\nA:\n- [See transcript]\n\nP:\n- [See transcript]", MALFORMED_SOAP_SENTINEL)
                    }
                }
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
            // Strip bullet markers (including double dashes that LLM may use for sub-bullets)
            let content = trimmed
                .trim_start_matches("-- ")
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

/// Try to parse SOAP from nested JSON structure (where sections contain objects instead of strings)
/// E.g.: {"subjective": [{"Problem 1: Cancer": ["item1", "item2"]}]}
fn try_parse_nested_json_soap(json_str: &str) -> Option<SoapJsonResponse> {
    use serde_json::Value;

    let value: Value = serde_json::from_str(json_str).ok()?;
    let obj = value.as_object()?;

    /// Recursively extract all string values from a JSON value
    fn extract_strings(value: &Value) -> Vec<String> {
        match value {
            Value::String(s) => vec![s.clone()],
            Value::Array(arr) => arr.iter().flat_map(extract_strings).collect(),
            Value::Object(map) => {
                // For objects, include the key as a header if it looks like a problem/category
                // and extract values from the nested structure
                let mut result = Vec::new();
                for (key, val) in map {
                    // Add key as a contextual prefix if it looks like a problem header
                    if key.to_lowercase().contains("problem")
                        || key.contains(':')
                        || key.len() > 20
                    {
                        // This is likely a category header like "Problem 1: Chest Pain"
                        // Extract the items without the problem header (LLM is grouping by problem)
                        result.extend(extract_strings(val));
                    } else {
                        // Regular key, just extract the values
                        result.extend(extract_strings(val));
                    }
                }
                result
            }
            _ => vec![],
        }
    }

    let subjective = obj
        .get("subjective")
        .map(extract_strings)
        .unwrap_or_default();
    let objective = obj
        .get("objective")
        .map(extract_strings)
        .unwrap_or_default();
    let assessment = obj
        .get("assessment")
        .map(extract_strings)
        .unwrap_or_default();
    let plan = obj.get("plan").map(extract_strings).unwrap_or_default();

    // Only return if we found at least one section
    if subjective.is_empty() && objective.is_empty() && assessment.is_empty() && plan.is_empty() {
        None
    } else {
        info!(
            "Parsed nested JSON SOAP: S={}, O={}, A={}, P={}",
            subjective.len(),
            objective.len(),
            assessment.len(),
            plan.len()
        );
        Some(SoapJsonResponse {
            subjective,
            objective,
            assessment,
            plan,
        })
    }
}

/// Strip markdown formatting and sub-bullet markers from a single item string
fn strip_markdown_from_item(item: &str) -> String {
    let cleaned = item
        .replace("**", "")
        .replace("__", "")
        .replace('`', "")
        .replace("###", "")
        .replace("##", "")
        .replace("# ", "");

    // Remove leading bullet/dash markers that LLM may add (e.g., "-- ", "- ", "• ")
    let trimmed = cleaned.trim();
    let without_bullets = trimmed
        .strip_prefix("-- ")
        .or_else(|| trimmed.strip_prefix("- "))
        .or_else(|| trimmed.strip_prefix("• "))
        .or_else(|| trimmed.strip_prefix("* "))
        .unwrap_or(trimmed);

    without_bullets.to_string()
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
pub fn build_soap_user_content(
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

/// Build a SOAP system prompt specialized for per-patient extraction from a multi-patient transcript.
/// Extends `build_simple_soap_prompt` with multi-patient context.
/// When `templates` is provided and `soap_per_patient_extension` is non-empty, it overrides the hardcoded extension.
pub(crate) fn build_per_patient_soap_prompt(
    options: &SoapOptions,
    templates: Option<&crate::server_config::PromptTemplates>,
) -> String {
    let base = build_simple_soap_prompt(options, templates);
    let extension = templates
        .and_then(|t| (!t.soap_per_patient_extension.is_empty()).then(|| t.soap_per_patient_extension.clone()))
        .unwrap_or_else(|| "\
        IMPORTANT CONTEXT: This transcript was recorded during a visit where MULTIPLE PATIENTS were seen together \
        (e.g., a couple, family members). The conversation is interwoven — the doctor goes back and forth between patients \
        throughout the visit. Clinical discussions for different patients may be interleaved.\n\n\
        You are generating a SOAP note for ONE SPECIFIC PATIENT only. Focus exclusively on clinical content relevant to that patient:\n\
        - Their symptoms, history, and concerns\n\
        - The doctor's examination findings and clinical reasoning for that patient\n\
        - Treatment plans, prescriptions, and follow-ups for that patient\n\
        - Ignore clinical content that belongs to the other patient(s), even if it appears nearby".to_string());
    format!("{}\n\n{}", base, extension)
}

/// Build a SOAP system prompt scoped to a single patient within a multi-patient transcript.
/// Used when the physician regenerates SOAP for one specific patient (flattened sidebar entry).
/// Appends a single-patient constraint to the base SOAP prompt.
/// When `templates` is provided and `soap_single_patient_scope_template` is non-empty, it overrides the hardcoded constraint.
pub fn build_single_patient_soap_prompt(
    options: &SoapOptions,
    patient_label: &str,
    templates: Option<&crate::server_config::PromptTemplates>,
) -> String {
    let base = build_simple_soap_prompt(options, templates);
    let scope = templates
        .and_then(|t| (!t.soap_single_patient_scope_template.is_empty()).then(|| {
            t.soap_single_patient_scope_template.replace("{patient_label}", patient_label)
        }))
        .unwrap_or_else(|| format!(
            "IMPORTANT: This transcript contains multiple patients. \
             Generate a SOAP note ONLY for the patient identified as \"{}\". \
             Ignore clinical content belonging to other patients in the transcript.",
            patient_label
        ));
    format!("{}\n\n{}", base, scope)
}

/// Build user content for a per-patient SOAP note from a multi-patient transcript.
pub(crate) fn build_per_patient_user_content(
    transcript: &str,
    patient_label: &str,
    patient_summary: &str,
    all_patients_desc: &str,
) -> String {
    format!(
        "Generate a SOAP note for THIS PATIENT ONLY:\n\
        Patient: {}\n\
        Reason for visit: {}\n\n\
        Other patients in this transcript (IGNORE their clinical content):\n\
        {}\n\n\
        The conversation below is interwoven — the doctor goes back and forth between patients. \
        Extract ONLY the clinical content relevant to the patient identified above.\n\n\
        TRANSCRIPT:\n{}",
        patient_label, patient_summary, all_patients_desc, transcript
    )
}

/// Build a system prompt for generating a plain-language patient handout
/// from a clinical transcript. The handout should be easy for patients to
/// understand (5th-8th grade reading level) and use warm, reassuring language.
/// When `templates` is provided and the relevant field is non-empty, it overrides the hardcoded default.
pub fn build_patient_handout_prompt(
    templates: Option<&crate::server_config::PromptTemplates>,
) -> String {
    templates
        .and_then(|t| (!t.patient_handout.is_empty()).then(|| t.patient_handout.clone()))
        .unwrap_or_else(|| r#"You are a caring medical assistant who writes visit summaries for patients.
Write a clear, easy-to-understand summary of this medical visit for the patient to take home.

RULES:
- Write at a 5th to 8th grade reading level
- Avoid medical jargon; if you must use a medical term, explain it in parentheses
- Address the patient directly using "you" and "your"
- Be warm, reassuring, and professional
- Target 200-500 words
- Do NOT wrap the output in JSON or any other format — output plain text only
- Do NOT include the patient's name or the physician's name
- Only include information that was actually discussed in the visit

USE THESE SECTIONS (with the exact headings below):

What We Discussed Today
- Summarize the main reasons for the visit and topics covered

What We Found
- Summarize any exam findings, test results, or observations shared during the visit

Your Plan
- List next steps: medications, lifestyle changes, procedures, or referrals
- Explain each item simply so the patient understands what to do and why

When to Come Back
- State when the patient should return or follow up

Warning Signs — Call Us If...
- List specific symptoms or situations that should prompt the patient to call the office or seek urgent care

If a section has no relevant information from the visit, skip that section entirely."#.to_string())
}

/// Build a prompt for merging incorrectly split patients within one encounter.
/// Used when the physician determines that the LLM's multi-patient detection was wrong
/// and two or more detected "patients" are actually the same person.
/// When `templates` is provided and the relevant field is non-empty, it overrides the hardcoded system prompt.
pub fn build_patient_merge_correction_prompt(
    transcript: &str,
    all_patient_labels: &[(u32, String, String)], // (index, label, soap_content)
    merged_indices: &[u32],
    templates: Option<&crate::server_config::PromptTemplates>,
) -> (String, String) {
    let mut context = String::new();
    context.push_str("The following patients were detected in this encounter:\n\n");

    for (idx, label, soap) in all_patient_labels {
        let status = if merged_indices.contains(idx) {
            "TO BE MERGED"
        } else {
            "correct, keep separate"
        };
        context.push_str(&format!(
            "--- Patient {} ({}) [{}] ---\n{}\n\n",
            idx, label, status, soap
        ));
    }

    let merged_names: Vec<&str> = all_patient_labels
        .iter()
        .filter(|(idx, _, _)| merged_indices.contains(idx))
        .map(|(_, label, _)| label.as_str())
        .collect();

    let system = templates
        .and_then(|t| (!t.patient_merge_correction.is_empty()).then(|| {
            t.patient_merge_correction.replace("{merged_names}", &merged_names.join(", "))
        }))
        .unwrap_or_else(|| format!(
        "You are a medical scribe assistant. The physician has reviewed automatically detected \
         patient notes from a multi-patient encounter and determined that the following patients \
         are actually the SAME person and should be merged: {}.\n\n\
         Generate a single unified SOAP note for this patient, incorporating clinical details \
         from all the notes marked TO BE MERGED. Do not include content from patients marked \
         as 'correct, keep separate'.",
        merged_names.join(", ")
    ));

    let user = format!(
        "TRANSCRIPT:\n{}\n\nDETECTED PATIENTS AND THEIR CURRENT SOAP NOTES:\n{}\n\n\
         Generate a single merged SOAP note for the patients marked TO BE MERGED.",
        transcript, context
    );

    (system, user)
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
        // Create a transcript with 35,000 words (above the 30,000 limit)
        let words: Vec<&str> = (0..35_000).map(|_| "word").collect();
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
        let start_words: Vec<String> = (0..6000).map(|i| format!("s{}", i)).collect();
        let middle_words: Vec<String> = (0..26000).map(|i| format!("m{}", i)).collect();
        let end_words: Vec<String> = (0..6000).map(|i| format!("e{}", i)).collect();

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
        assert!(truncated.contains("e5999"));
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
        assert!(opts.session_custom_instructions.is_empty());
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

    // ── JSON repair tests ──────────────────────────────────────────

    #[test]
    fn test_remove_leading_commas_array() {
        assert_eq!(remove_leading_commas(r#"[,"item"]"#), r#"["item"]"#);
    }

    #[test]
    fn test_remove_leading_commas_double() {
        assert_eq!(remove_leading_commas(r#"[,,"item"]"#), r#"["item"]"#);
    }

    #[test]
    fn test_remove_leading_commas_with_spaces() {
        // Comma is removed, whitespace around it preserved
        assert_eq!(remove_leading_commas(r#"[ , "item"]"#), r#"[  "item"]"#);
    }

    #[test]
    fn test_remove_leading_commas_no_change() {
        let valid = r#"["a","b"]"#;
        assert_eq!(remove_leading_commas(valid), valid);
    }

    #[test]
    fn test_remove_leading_commas_preserves_strings() {
        // Comma inside a string after [ should NOT be touched
        let input = r#"["[,test"]"#;
        assert_eq!(remove_leading_commas(input), input);
    }

    #[test]
    fn test_remove_trailing_commas_array() {
        assert_eq!(remove_trailing_commas(r#"["item",]"#), r#"["item"]"#);
    }

    #[test]
    fn test_remove_trailing_commas_object() {
        assert_eq!(remove_trailing_commas(r#"{"key":"val",}"#), r#"{"key":"val"}"#);
    }

    #[test]
    fn test_remove_trailing_commas_with_whitespace() {
        assert_eq!(remove_trailing_commas(r#"["item" , ]"#), r#"["item"  ]"#);
    }

    #[test]
    fn test_remove_trailing_commas_no_change() {
        let valid = r#"["a","b"]"#;
        assert_eq!(remove_trailing_commas(valid), valid);
    }

    #[test]
    fn test_remove_trailing_commas_preserves_strings() {
        let input = r#"["item,]inside"]"#;
        assert_eq!(remove_trailing_commas(input), input);
    }

    #[test]
    fn test_fix_truncated_json_missing_bracket() {
        let input = r#"{"subjective":["item1","item2"}"#;
        let fixed = fix_truncated_json(input);
        assert!(serde_json::from_str::<serde_json::Value>(&fixed).is_ok(),
            "Should produce valid JSON, got: {}", fixed);
    }

    #[test]
    fn test_fix_truncated_json_balanced() {
        let input = r#"{"key":["a","b"]}"#;
        assert_eq!(fix_truncated_json(input), input);
    }

    #[test]
    fn test_extract_json_from_response_leading_comma() {
        let response = r#"{"subjective":[,"Patient reports pain"],"objective":["BP 120/80"],"assessment":["Headache"],"plan":["Tylenol"]}"#;
        let json = extract_json_from_response(response);
        let parsed: Result<SoapJsonResponse, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok(), "Should parse after leading comma removal, got: {}", json);
        assert_eq!(parsed.unwrap().subjective, vec!["Patient reports pain"]);
    }

    #[test]
    fn test_extract_json_from_response_trailing_comma() {
        let response = r#"{"subjective":["Patient reports pain",],"objective":["BP 120/80"],"assessment":["Headache"],"plan":["Tylenol"]}"#;
        let json = extract_json_from_response(response);
        let parsed: Result<SoapJsonResponse, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok(), "Should parse after trailing comma removal, got: {}", json);
    }

    #[test]
    fn test_parse_and_format_soap_json_empty_strings() {
        // LLM produces empty elements mixed with real content
        let response = r#"{"subjective":["","Patient reports headache",""],"objective":["BP normal"],"assessment":["Tension headache"],"plan":["Follow up"]}"#;
        let result = parse_and_format_soap_json(response);
        assert!(result.contains("Patient reports headache"));
        // Empty strings should be filtered out — only real items produce bullets
        assert_eq!(result.matches("•").count(), 4); // one per non-empty item across all sections
    }

    #[test]
    fn test_fix_truncated_json_unclosed_string() {
        // LLM truncated mid-string-value
        let input = r#"{"subjective":["Patient reports hea"#;
        let fixed = fix_truncated_json(input);
        // Should close the string, then close the array and object
        assert!(fixed.ends_with("]}"), "Should end with ]}} got: {}", fixed);
        assert!(serde_json::from_str::<serde_json::Value>(&fixed).is_ok(),
            "Should produce valid JSON, got: {}", fixed);
    }

    #[test]
    fn test_fix_truncated_json_unclosed_string_trailing_backslash() {
        // LLM truncated mid-escape-sequence
        let input = r#"{"subjective":["Patient said \"hel\"#;
        let fixed = fix_truncated_json(input);
        assert!(serde_json::from_str::<serde_json::Value>(&fixed).is_ok(),
            "Should produce valid JSON, got: {}", fixed);
    }

    #[test]
    fn test_fix_truncated_json_closed_string_still_works() {
        // Already-valid string but missing brackets
        let input = r#"{"subjective":["item1","item2"}"#;
        let fixed = fix_truncated_json(input);
        assert!(serde_json::from_str::<serde_json::Value>(&fixed).is_ok(),
            "Should produce valid JSON, got: {}", fixed);
    }

    #[test]
    fn test_parse_and_format_soap_json_raw_json_fallback() {
        // Broken JSON that looks like SOAP structure — should get placeholder, not raw JSON
        let broken = r#"{"subjective":["Patient reports headache","#;
        let result = parse_and_format_soap_json(broken);
        // Should not contain raw JSON braces
        assert!(!result.starts_with('{'), "Should not return raw JSON, got: {}", result);
        // Should either repair successfully or return placeholder
        assert!(
            result.contains("Patient reports headache") || result.contains("malformed output"),
            "Should either repair or return placeholder, got: {}", result
        );
    }

    // ========================================================================
    // format_for_archive tests
    // ========================================================================

    fn make_soap_result(notes: Vec<(&str, &str)>) -> MultiPatientSoapResult {
        MultiPatientSoapResult {
            notes: notes.into_iter().map(|(label, content)| PatientSoapNote {
                patient_label: label.to_string(),
                content: content.to_string(),
                speaker_id: String::new(),
            }).collect(),
            physician_speaker: None,
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            model_used: "test-model".to_string(),
        }
    }

    #[test]
    fn test_format_for_archive_single_patient() {
        let result = make_soap_result(vec![("Patient 1", "S: Headache\nO: Normal\nA: Migraine\nP: Rest")]);
        let formatted = result.format_for_archive();
        // Single patient: bare content, no header
        assert_eq!(formatted, "S: Headache\nO: Normal\nA: Migraine\nP: Rest");
        assert!(!formatted.contains("==="));
    }

    #[test]
    fn test_format_for_archive_two_patients() {
        let result = make_soap_result(vec![
            ("Patient 1", "S: Headache"),
            ("Patient 2", "S: Back pain"),
        ]);
        let formatted = result.format_for_archive();
        assert!(formatted.contains("=== Patient 1 ===\nS: Headache"));
        assert!(formatted.contains("=== Patient 2 ===\nS: Back pain"));
        assert!(formatted.contains("\n\n---\n\n"));
    }

    #[test]
    fn test_format_for_archive_three_patients() {
        let result = make_soap_result(vec![
            ("Patient 1", "content1"),
            ("Patient 2", "content2"),
            ("Patient 3", "content3"),
        ]);
        let formatted = result.format_for_archive();
        // Should have 2 separators for 3 patients
        assert_eq!(formatted.matches("\n\n---\n\n").count(), 2);
    }

    #[test]
    fn test_format_for_archive_empty_notes() {
        let result = make_soap_result(vec![]);
        assert_eq!(result.format_for_archive(), "");
    }
}
