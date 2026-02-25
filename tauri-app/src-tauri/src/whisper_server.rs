//! STT server client for transcription
//!
//! This module provides integration with the STT Router server, which routes audio
//! to different transcription backends via named aliases. Supports both batch (HTTP)
//! and streaming (WebSocket) transcription modes.
//!
//! Streaming mode (medical-streaming alias) uses Voxtral for real-time transcription
//! with partial chunks delivered as they're generated.
//!
//! Also retains the legacy OpenAI-compatible /v1/audio/transcriptions endpoint
//! for backward compatibility (used by listening mode for greeting detection).

use serde::{Deserialize, Serialize};
use std::io::{Cursor, Write};
use std::time::Duration;
use tracing::{debug, error, info, warn};
use tungstenite::protocol::Message as WsMessage;

/// Default timeout for transcription requests (2 minutes for long audio)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Default number of retry attempts for transient failures
const DEFAULT_MAX_RETRIES: u32 = 3;

/// Initial backoff delay for retries
const INITIAL_BACKOFF_MS: u64 = 500;

/// Maximum backoff delay
const MAX_BACKOFF_MS: u64 = 5000;

/// Sample rate for transcription (16kHz)
const WHISPER_SAMPLE_RATE: u32 = 16000;

/// Response from legacy transcription endpoint
#[derive(Debug, Clone, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

/// Response from batch alias endpoint
#[derive(Debug, Clone, Deserialize)]
struct BatchAliasResponse {
    text: String,
}

/// Health check response from STT server
#[derive(Debug, Clone, Deserialize)]
pub struct SttHealthResponse {
    pub status: String,
    pub model: Option<String>,
    pub router: Option<bool>,
}

/// Alias info from STT server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttAlias {
    pub alias: String,
    pub backend: String,
    pub mode: String,
    pub postprocess: String,
    pub has_prompt: bool,
}

/// Aliases list response from STT server
#[derive(Debug, Clone, Deserialize)]
struct AliasesResponse {
    aliases: Vec<SttAlias>,
}

/// Status of the STT server connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperServerStatus {
    pub connected: bool,
    pub available_models: Vec<String>,
    pub error: Option<String>,
}

/// Model info from Whisper server
#[derive(Debug, Clone, Deserialize)]
struct ModelInfo {
    id: String,
}

/// Response from models endpoint
#[derive(Debug, Clone, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

/// Message received from STT WebSocket streaming
#[derive(Debug, Clone, Deserialize)]
struct WsStreamMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    postprocessed: Option<bool>,
}

/// Remote STT server client
///
/// Supports both batch (HTTP) and streaming (WebSocket) transcription
/// via the STT Router's alias system.
#[derive(Debug)]
pub struct WhisperServerClient {
    client: reqwest::Client,
    base_url: String,
    model: String,
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

/// Convert an HTTP URL to a WebSocket URL
fn http_to_ws_url(http_url: &str) -> String {
    http_url
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1)
}

impl WhisperServerClient {
    /// Create a new STT server client with URL validation
    pub fn new(base_url: &str, model: &str) -> Result<Self, String> {
        let cleaned_url = base_url.trim_end_matches('/');

        let parsed = reqwest::Url::parse(cleaned_url)
            .map_err(|e| format!("Invalid STT server URL '{}': {}", cleaned_url, e))?;

        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(format!(
                "STT server URL must use http or https scheme, got: {}",
                parsed.scheme()
            ));
        }

        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("STT server URL must not contain credentials".to_string());
        }

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        info!(
            "WhisperServerClient created for {} with model {}",
            cleaned_url, model
        );

        Ok(Self {
            client,
            base_url: cleaned_url.to_string(),
            model: model.to_string(),
        })
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    // ─── Health & Discovery ───────────────────────────────────────────

    /// Check STT server health via /health endpoint
    pub async fn check_health(&self) -> Result<SttHealthResponse, String> {
        let url = format!("{}/health", self.base_url);
        debug!("Checking STT server health at {}", url);

        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Failed to connect to STT server: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "STT server health check failed: {}",
                response.status()
            ));
        }

        response
            .json::<SttHealthResponse>()
            .await
            .map_err(|e| format!("Failed to parse health response: {}", e))
    }

    /// List available aliases from the STT server
    pub async fn list_aliases(&self) -> Result<Vec<SttAlias>, String> {
        let url = format!("{}/v1/aliases", self.base_url);
        debug!("Listing STT aliases from {}", url);

        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Failed to list aliases: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to list aliases: {}",
                response.status()
            ));
        }

        let parsed: AliasesResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse aliases response: {}", e))?;

        info!("Found {} STT aliases", parsed.aliases.len());
        Ok(parsed.aliases)
    }

    /// Check connection status and list available models (legacy compat)
    pub async fn check_status(&self) -> WhisperServerStatus {
        // Try the new /health endpoint first
        match self.check_health().await {
            Ok(health) => {
                let router_active = health.router.unwrap_or(false);
                let model_name = health.model.unwrap_or_default();
                WhisperServerStatus {
                    connected: true,
                    available_models: if model_name.is_empty() {
                        vec![]
                    } else {
                        vec![model_name]
                    },
                    error: if !router_active {
                        Some("STT server running but router not configured".to_string())
                    } else {
                        None
                    },
                }
            }
            Err(_) => {
                // Fall back to legacy /v1/models endpoint
                match self.list_models().await {
                    Ok(models) => WhisperServerStatus {
                        connected: true,
                        available_models: models,
                        error: None,
                    },
                    Err(e) => WhisperServerStatus {
                        connected: false,
                        available_models: vec![],
                        error: Some(e),
                    },
                }
            }
        }
    }

    /// List available models from the Whisper server (legacy endpoint)
    pub async fn list_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/v1/models", self.base_url);
        debug!("Listing Whisper server models from {}", url);

        let mut last_error = String::new();

        for attempt in 0..DEFAULT_MAX_RETRIES {
            if attempt > 0 {
                let backoff = calculate_backoff(attempt - 1);
                warn!(
                    "Whisper server list_models attempt {} failed, retrying in {:?}",
                    attempt, backoff
                );
                tokio::time::sleep(backoff).await;
            }

            match self.client.get(&url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<ModelsResponse>().await {
                            Ok(models) => {
                                let model_ids: Vec<String> =
                                    models.data.into_iter().map(|m| m.id).collect();
                                info!("Found {} Whisper server models", model_ids.len());
                                return Ok(model_ids);
                            }
                            Err(e) => {
                                last_error = format!("Failed to parse Whisper server response: {}", e);
                                break;
                            }
                        }
                    } else if is_retryable_status(response.status()) {
                        last_error = format!(
                            "Whisper server returned error status: {}",
                            response.status()
                        );
                        continue;
                    } else {
                        return Err(format!(
                            "Whisper server returned error status: {}",
                            response.status()
                        ));
                    }
                }
                Err(e) => {
                    if is_retryable_error(&e) {
                        last_error = format!("Failed to connect to Whisper server: {}", e);
                        continue;
                    } else {
                        return Err(format!("Failed to connect to Whisper server: {}", e));
                    }
                }
            }
        }

        error!(
            "Whisper server list_models failed after {} attempts: {}",
            DEFAULT_MAX_RETRIES, last_error
        );
        Err(last_error)
    }

    // ─── Streaming Transcription (WebSocket) ──────────────────────────

    /// Transcribe audio via WebSocket streaming with chunk callback.
    ///
    /// Opens a WebSocket connection, sends the audio as WAV, and receives
    /// partial transcript chunks followed by a final transcript. Each chunk
    /// is passed to `on_chunk` for real-time display.
    ///
    /// This is a blocking call suitable for the pipeline thread.
    ///
    /// # Arguments
    /// * `audio` - Audio samples as f32, normalized to [-1.0, 1.0], 16kHz mono
    /// * `alias` - STT alias (e.g., "medical-streaming")
    /// * `postprocess` - Whether to enable medical term post-processing
    /// * `on_chunk` - Callback invoked with each partial transcript chunk
    pub fn transcribe_streaming_blocking(
        &self,
        audio: &[f32],
        alias: &str,
        postprocess: bool,
        mut on_chunk: impl FnMut(&str),
    ) -> Result<String, String> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let wav_bytes = encode_wav(audio, WHISPER_SAMPLE_RATE)?;
        let ws_url = format!("{}/v1/audio/stream", http_to_ws_url(&self.base_url));

        let audio_duration_s = audio.len() as f32 / WHISPER_SAMPLE_RATE as f32;
        info!(
            "Streaming transcription: {:.1}s audio via {} (alias={})",
            audio_duration_s, ws_url, alias
        );

        let mut last_error = String::new();

        for attempt in 0..DEFAULT_MAX_RETRIES {
            if attempt > 0 {
                let backoff = calculate_backoff(attempt - 1);
                warn!(
                    "Streaming transcribe attempt {} failed, retrying in {:?}",
                    attempt, backoff
                );
                std::thread::sleep(backoff);
            }

            match self.try_streaming_transcription(&ws_url, &wav_bytes, alias, postprocess, &mut on_chunk) {
                Ok(text) => return Ok(text),
                Err(e) => {
                    // Check if error is retryable (connection failures)
                    if e.contains("connect") || e.contains("Connection") || e.contains("timed out") {
                        last_error = e;
                        continue;
                    } else {
                        // Non-retryable error (server sent error message, parse failure)
                        return Err(e);
                    }
                }
            }
        }

        error!(
            "Streaming transcribe failed after {} attempts: {}",
            DEFAULT_MAX_RETRIES, last_error
        );
        Err(last_error)
    }

    /// Single attempt at streaming transcription via WebSocket
    fn try_streaming_transcription(
        &self,
        ws_url: &str,
        wav_bytes: &[u8],
        alias: &str,
        postprocess: bool,
        on_chunk: &mut impl FnMut(&str),
    ) -> Result<String, String> {
        // Connect to WebSocket
        let (mut ws, _response) = tungstenite::connect(ws_url)
            .map_err(|e| format!("Failed to connect to STT WebSocket: {}", e))?;

        debug!("WebSocket connected to {}", ws_url);

        // Step 1: Send configuration
        let config = serde_json::json!({
            "alias": alias,
            "postprocess": postprocess,
        });
        ws.send(WsMessage::Text(config.to_string()))
            .map_err(|e| format!("Failed to send STT config: {}", e))?;

        debug!("Sent streaming config: alias={}, postprocess={}", alias, postprocess);

        // Step 2: Send audio as binary
        ws.send(WsMessage::Binary(wav_bytes.to_vec()))
            .map_err(|e| format!("Failed to send audio data: {}", e))?;

        debug!("Sent {} bytes of audio data", wav_bytes.len());

        // Step 3: Receive transcript chunks and final result
        let mut accumulated_chunks = String::new();

        loop {
            let msg = ws.read()
                .map_err(|e| format!("WebSocket read error: {}", e))?;

            match msg {
                WsMessage::Text(text) => {
                    let parsed: WsStreamMessage = serde_json::from_str(&text)
                        .map_err(|e| format!("Failed to parse STT message: {} (raw: {})", e, text))?;

                    match parsed.msg_type.as_str() {
                        "transcript_chunk" => {
                            if let Some(ref chunk_text) = parsed.text {
                                debug!("Streaming chunk: {} chars", chunk_text.len());
                                accumulated_chunks.push_str(chunk_text);
                                on_chunk(chunk_text);
                            }
                        }
                        "transcript_final" => {
                            let final_text = parsed.text.unwrap_or_default();
                            let was_postprocessed = parsed.postprocessed.unwrap_or(false);
                            info!(
                                "Streaming complete: {} chars (postprocessed={})",
                                final_text.len(),
                                was_postprocessed
                            );
                            // Close the WebSocket gracefully
                            let _ = ws.close(None);
                            return Ok(final_text);
                        }
                        "error" => {
                            let detail = parsed.detail.unwrap_or_else(|| "Unknown STT error".to_string());
                            error!("STT streaming error: {}", detail);
                            let _ = ws.close(None);
                            return Err(format!("STT error: {}", detail));
                        }
                        other => {
                            debug!("Unknown STT message type: {}", other);
                        }
                    }
                }
                WsMessage::Close(_) => {
                    debug!("WebSocket closed by server");
                    // If we got chunks but no final, return accumulated chunks
                    if !accumulated_chunks.is_empty() {
                        warn!("WebSocket closed without final message, using accumulated chunks");
                        return Ok(accumulated_chunks);
                    }
                    return Err("WebSocket closed without transcript".to_string());
                }
                WsMessage::Ping(data) => {
                    let _ = ws.send(WsMessage::Pong(data));
                }
                _ => {} // Ignore binary, pong, frame messages
            }
        }
    }

    // ─── Batch Transcription (HTTP alias endpoint) ────────────────────

    /// Transcribe audio via batch HTTP endpoint using an alias.
    ///
    /// Uses the /v1/audio/transcribe/{alias} endpoint for batch transcription.
    /// Suitable for non-realtime use cases (e.g., greeting detection in listening mode).
    pub async fn transcribe_batch(
        &self,
        audio: &[f32],
        alias: &str,
        postprocess: bool,
        language: &str,
    ) -> Result<String, String> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let wav_bytes = encode_wav(audio, WHISPER_SAMPLE_RATE)?;
        let url = format!("{}/v1/audio/transcribe/{}", self.base_url, alias);

        debug!(
            "Batch transcribing {} samples ({:.2}s) via {} (alias={})",
            audio.len(),
            audio.len() as f32 / WHISPER_SAMPLE_RATE as f32,
            url,
            alias,
        );

        let mut last_error = String::new();

        for attempt in 0..DEFAULT_MAX_RETRIES {
            if attempt > 0 {
                let backoff = calculate_backoff(attempt - 1);
                warn!(
                    "Batch transcribe attempt {} failed, retrying in {:?}",
                    attempt, backoff
                );
                tokio::time::sleep(backoff).await;
            }

            let file_part = reqwest::multipart::Part::bytes(wav_bytes.clone())
                .file_name("audio.wav")
                .mime_str("audio/wav")
                .map_err(|e| format!("Failed to create file part: {}", e))?;

            let mut form = reqwest::multipart::Form::new()
                .part("file", file_part)
                .text("postprocess", postprocess.to_string())
                .text("response_format", "json");

            if language != "auto" && !language.is_empty() {
                form = form.text("language", language.to_string());
            }

            match self.client.post(&url).multipart(form).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<BatchAliasResponse>().await {
                            Ok(result) => {
                                debug!("Batch transcription result: {} chars", result.text.len());
                                return Ok(result.text);
                            }
                            Err(e) => {
                                last_error = format!("Failed to parse batch response: {}", e);
                                break;
                            }
                        }
                    } else if is_retryable_status(response.status()) {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        last_error = format!("STT server error: {} - {}", status, body);
                        continue;
                    } else {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        return Err(format!("STT server error: {} - {}", status, body));
                    }
                }
                Err(e) => {
                    if is_retryable_error(&e) {
                        last_error = format!("Failed to send batch request: {}", e);
                        continue;
                    } else {
                        return Err(format!("Failed to send batch request: {}", e));
                    }
                }
            }
        }

        error!(
            "Batch transcribe failed after {} attempts: {}",
            DEFAULT_MAX_RETRIES, last_error
        );
        Err(last_error)
    }

    /// Blocking version of transcribe_batch for synchronous contexts
    pub fn transcribe_batch_blocking(
        &self,
        audio: &[f32],
        alias: &str,
        postprocess: bool,
        language: &str,
    ) -> Result<String, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;

        rt.block_on(self.transcribe_batch(audio, alias, postprocess, language))
    }

    // ─── Legacy Transcription (OpenAI-compatible) ─────────────────────

    /// Transcribe using the legacy /v1/audio/transcriptions endpoint.
    ///
    /// This endpoint works without alias routing and is retained for
    /// backward compatibility (listening mode greeting detection).
    pub async fn transcribe(&self, audio: &[f32], language: &str) -> Result<String, String> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let wav_bytes = encode_wav(audio, WHISPER_SAMPLE_RATE)?;

        let url = format!("{}/v1/audio/transcriptions", self.base_url);
        debug!(
            "Transcribing {} samples ({:.2}s) via {}",
            audio.len(),
            audio.len() as f32 / WHISPER_SAMPLE_RATE as f32,
            url
        );

        let mut last_error = String::new();

        for attempt in 0..DEFAULT_MAX_RETRIES {
            if attempt > 0 {
                let backoff = calculate_backoff(attempt - 1);
                warn!(
                    "Whisper server transcribe attempt {} failed, retrying in {:?}",
                    attempt, backoff
                );
                tokio::time::sleep(backoff).await;
            }

            let file_part = reqwest::multipart::Part::bytes(wav_bytes.clone())
                .file_name("audio.wav")
                .mime_str("audio/wav")
                .map_err(|e| format!("Failed to create file part: {}", e))?;

            let mut form = reqwest::multipart::Form::new()
                .part("file", file_part)
                .text("model", self.model.clone())
                .text("response_format", "json")
                .text("temperature", "0.0")
                .text("no_speech_threshold", "0.8")
                .text("condition_on_previous_text", "false");

            if language != "auto" && !language.is_empty() {
                form = form.text("language", language.to_string());
            }

            match self.client.post(&url).multipart(form).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<TranscriptionResponse>().await {
                            Ok(result) => {
                                debug!("Transcription result: {} chars", result.text.len());
                                return Ok(result.text);
                            }
                            Err(e) => {
                                last_error = format!("Failed to parse transcription response: {}", e);
                                break;
                            }
                        }
                    } else if is_retryable_status(response.status()) {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        last_error = format!(
                            "Whisper server returned error: {} - {}",
                            status, body
                        );
                        continue;
                    } else {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        error!("Whisper server transcription failed: {} - {}", status, body);
                        return Err(format!(
                            "Whisper server returned error: {} - {}",
                            status, body
                        ));
                    }
                }
                Err(e) => {
                    if is_retryable_error(&e) {
                        last_error = format!("Failed to send transcription request: {}", e);
                        continue;
                    } else {
                        return Err(format!("Failed to send transcription request: {}", e));
                    }
                }
            }
        }

        error!(
            "Whisper server transcribe failed after {} attempts: {}",
            DEFAULT_MAX_RETRIES, last_error
        );
        Err(last_error)
    }

    /// Blocking version of legacy transcribe
    pub fn transcribe_blocking(&self, audio: &[f32], language: &str) -> Result<String, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;

        rt.block_on(self.transcribe(audio, language))
    }
}

/// Encode f32 audio samples as WAV bytes
///
/// # Arguments
/// * `samples` - Audio samples normalized to [-1.0, 1.0]
/// * `sample_rate` - Sample rate in Hz
///
/// # Returns
/// WAV file bytes
pub fn encode_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, String> {
    let mut buffer = Cursor::new(Vec::new());

    let num_channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * num_channels as u32 * bits_per_sample as u32 / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size: u32 = (samples.len() * 2)
        .try_into()
        .map_err(|_| "Audio too large for WAV format (exceeds u32 data size limit)".to_string())?;
    let file_size = 36 + data_size;

    // RIFF header
    buffer.write_all(b"RIFF").map_err(|e| format!("WAV write error: {}", e))?;
    buffer.write_all(&file_size.to_le_bytes()).map_err(|e| format!("WAV write error: {}", e))?;
    buffer.write_all(b"WAVE").map_err(|e| format!("WAV write error: {}", e))?;

    // fmt chunk
    buffer.write_all(b"fmt ").map_err(|e| format!("WAV write error: {}", e))?;
    buffer.write_all(&16u32.to_le_bytes()).map_err(|e| format!("WAV write error: {}", e))?;
    buffer.write_all(&1u16.to_le_bytes()).map_err(|e| format!("WAV write error: {}", e))?;
    buffer.write_all(&num_channels.to_le_bytes()).map_err(|e| format!("WAV write error: {}", e))?;
    buffer.write_all(&sample_rate.to_le_bytes()).map_err(|e| format!("WAV write error: {}", e))?;
    buffer.write_all(&byte_rate.to_le_bytes()).map_err(|e| format!("WAV write error: {}", e))?;
    buffer.write_all(&block_align.to_le_bytes()).map_err(|e| format!("WAV write error: {}", e))?;
    buffer.write_all(&bits_per_sample.to_le_bytes()).map_err(|e| format!("WAV write error: {}", e))?;

    // data chunk
    buffer.write_all(b"data").map_err(|e| format!("WAV write error: {}", e))?;
    buffer.write_all(&data_size.to_le_bytes()).map_err(|e| format!("WAV write error: {}", e))?;

    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let i16_sample = (clamped * 32767.0) as i16;
        buffer.write_all(&i16_sample.to_le_bytes()).map_err(|e| format!("WAV write error: {}", e))?;
    }

    Ok(buffer.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whisper_server_client_new() {
        let client = WhisperServerClient::new("http://localhost:8000", "large-v3-turbo").unwrap();
        assert_eq!(client.base_url, "http://localhost:8000");
        assert_eq!(client.model, "large-v3-turbo");

        let client2 = WhisperServerClient::new("http://localhost:8000/", "large-v3-turbo").unwrap();
        assert_eq!(client2.base_url, "http://localhost:8000");

        let client3 = WhisperServerClient::new("https://whisper.example.com", "large-v3").unwrap();
        assert_eq!(client3.base_url, "https://whisper.example.com");
    }

    #[test]
    fn test_whisper_server_client_new_invalid_url() {
        let result = WhisperServerClient::new("not-a-valid-url", "model");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid STT server URL"));

        let result2 = WhisperServerClient::new("ftp://localhost:8000", "model");
        assert!(result2.is_err());
        assert!(result2.unwrap_err().contains("http or https"));

        let result3 = WhisperServerClient::new("http://user:pass@localhost:8000", "model");
        assert!(result3.is_err());
        assert!(result3.unwrap_err().contains("must not contain credentials"));

        let result4 = WhisperServerClient::new("http://admin@localhost:8000", "model");
        assert!(result4.is_err());
        assert!(result4.unwrap_err().contains("must not contain credentials"));
    }

    #[test]
    fn test_http_to_ws_url() {
        assert_eq!(http_to_ws_url("http://localhost:8001"), "ws://localhost:8001");
        assert_eq!(http_to_ws_url("https://example.com"), "wss://example.com");
        assert_eq!(
            http_to_ws_url("http://10.241.15.154:8001"),
            "ws://10.241.15.154:8001"
        );
    }

    #[test]
    fn test_encode_wav_empty() {
        let samples: Vec<f32> = vec![];
        let result = encode_wav(&samples, 16000).unwrap();
        assert_eq!(result.len(), 44);
    }

    #[test]
    fn test_encode_wav_samples() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let result = encode_wav(&samples, 16000).unwrap();
        assert_eq!(result.len(), 54);

        assert_eq!(&result[0..4], b"RIFF");
        assert_eq!(&result[8..12], b"WAVE");
        assert_eq!(&result[12..16], b"fmt ");
        assert_eq!(&result[36..40], b"data");
    }

    #[test]
    fn test_encode_wav_sample_conversion() {
        let samples = vec![0.0f32, 1.0, -1.0, 0.5, -0.5];
        let result = encode_wav(&samples, 16000).unwrap();

        let i16_samples: Vec<i16> = result[44..]
            .chunks(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        assert_eq!(i16_samples[0], 0);
        assert_eq!(i16_samples[1], 32767);
        assert_eq!(i16_samples[2], -32767);
        assert_eq!(i16_samples[3], 16383);
        assert_eq!(i16_samples[4], -16383);
    }

    #[test]
    fn test_encode_wav_clamping() {
        let samples = vec![2.0f32, -2.0, 1.5, -1.5];
        let result = encode_wav(&samples, 16000).unwrap();

        let i16_samples: Vec<i16> = result[44..]
            .chunks(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        assert_eq!(i16_samples[0], 32767);
        assert_eq!(i16_samples[1], -32767);
        assert_eq!(i16_samples[2], 32767);
        assert_eq!(i16_samples[3], -32767);
    }

    #[test]
    fn test_encode_wav_sample_rate() {
        let samples = vec![0.0f32];
        let result = encode_wav(&samples, 48000).unwrap();
        let sample_rate = u32::from_le_bytes([result[24], result[25], result[26], result[27]]);
        assert_eq!(sample_rate, 48000);
    }

    #[test]
    fn test_whisper_server_status_serialization() {
        let status = WhisperServerStatus {
            connected: true,
            available_models: vec!["large-v3-turbo".to_string(), "small".to_string()],
            error: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: WhisperServerStatus = serde_json::from_str(&json).unwrap();
        assert!(parsed.connected);
        assert_eq!(parsed.available_models.len(), 2);
        assert!(parsed.error.is_none());
    }

    #[test]
    fn test_whisper_server_status_with_error() {
        let status = WhisperServerStatus {
            connected: false,
            available_models: vec![],
            error: Some("Connection refused".to_string()),
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: WhisperServerStatus = serde_json::from_str(&json).unwrap();
        assert!(!parsed.connected);
        assert!(parsed.available_models.is_empty());
        assert_eq!(parsed.error.as_deref(), Some("Connection refused"));
    }

    #[test]
    fn test_ws_stream_message_parse_chunk() {
        let json = r#"{"type": "transcript_chunk", "text": "The patient"}"#;
        let msg: WsStreamMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type, "transcript_chunk");
        assert_eq!(msg.text.as_deref(), Some("The patient"));
    }

    #[test]
    fn test_ws_stream_message_parse_final() {
        let json = r#"{"type": "transcript_final", "text": "The patient presents with dyspnea.", "postprocessed": true}"#;
        let msg: WsStreamMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type, "transcript_final");
        assert_eq!(msg.text.as_deref(), Some("The patient presents with dyspnea."));
        assert_eq!(msg.postprocessed, Some(true));
    }

    #[test]
    fn test_ws_stream_message_parse_error() {
        let json = r#"{"type": "error", "detail": "Unknown alias"}"#;
        let msg: WsStreamMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type, "error");
        assert_eq!(msg.detail.as_deref(), Some("Unknown alias"));
    }

    #[test]
    fn test_stt_alias_serialization() {
        let alias = SttAlias {
            alias: "medical-streaming".to_string(),
            backend: "voxtral".to_string(),
            mode: "streaming".to_string(),
            postprocess: "caller_choice".to_string(),
            has_prompt: false,
        };
        let json = serde_json::to_string(&alias).unwrap();
        let parsed: SttAlias = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.alias, "medical-streaming");
        assert_eq!(parsed.backend, "voxtral");
        assert_eq!(parsed.mode, "streaming");
    }

    /// Integration test: sends real audio to the STT Router via WebSocket streaming.
    /// Run with: cargo test test_streaming_transcription_integration --ignored
    #[test]
    #[ignore = "Requires live STT Router at 10.241.15.154:8001"]
    fn test_streaming_transcription_integration() {
        // Generate 2 seconds of silence (16kHz mono) — enough to trigger a response
        let sample_rate = 16000;
        let duration_secs = 2;
        let num_samples = sample_rate * duration_secs;
        // Use low-level noise rather than pure silence to avoid "no speech" edge cases
        let audio: Vec<f32> = (0..num_samples)
            .map(|i| {
                // Small sine wave at 440Hz to give the server something to process
                let t = i as f32 / sample_rate as f32;
                (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.1
            })
            .collect();

        let client = WhisperServerClient::new(
            "http://10.241.15.154:8001",
            "large-v3-turbo",
        ).expect("Failed to create client");

        let mut chunks_received = Vec::new();
        let result = client.transcribe_streaming_blocking(
            &audio,
            "medical-streaming",
            true,
            |chunk| {
                println!("  chunk: {:?}", chunk);
                chunks_received.push(chunk.to_string());
            },
        );

        match result {
            Ok(text) => {
                println!("Final transcript: {:?}", text);
                println!("Chunks received: {}", chunks_received.len());
                // Server should return *something* — even empty string for non-speech
                // The key test is that the WebSocket protocol completed without error
            }
            Err(e) => {
                panic!("Streaming transcription failed: {}", e);
            }
        }
    }

    /// Integration test: sends audio via batch HTTP endpoint.
    /// Run with: cargo test test_batch_transcription_integration --ignored
    #[test]
    #[ignore = "Requires live STT Router at 10.241.15.154:8001"]
    fn test_batch_transcription_integration() {
        let sample_rate = 16000;
        let audio: Vec<f32> = (0..sample_rate * 2)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.1
            })
            .collect();

        let client = WhisperServerClient::new(
            "http://10.241.15.154:8001",
            "large-v3-turbo",
        ).expect("Failed to create client");

        let result = client.transcribe_batch_blocking(&audio, "medical-batch", true, "en");

        match result {
            Ok(text) => {
                println!("Batch transcript: {:?}", text);
            }
            Err(e) => {
                panic!("Batch transcription failed: {}", e);
            }
        }
    }

    #[test]
    fn test_stt_health_response_parse() {
        let json = r#"{"status": "healthy", "model": "mlx-community/whisper-large-v3-turbo", "router": true}"#;
        let health: SttHealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(health.status, "healthy");
        assert_eq!(health.model.as_deref(), Some("mlx-community/whisper-large-v3-turbo"));
        assert_eq!(health.router, Some(true));
    }
}
