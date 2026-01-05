//! Remote Whisper server client for transcription
//!
//! This module provides integration with faster-whisper-server (Speaches) which offers
//! an OpenAI-compatible API for audio transcription. This allows running Whisper on a
//! remote server for devices with limited RAM/CPU.
//!
//! Server: https://github.com/speaches-ai/speaches
//! API: OpenAI-compatible /v1/audio/transcriptions endpoint

use serde::{Deserialize, Serialize};
use std::io::{Cursor, Write};
use std::time::Duration;
use tracing::{debug, error, info};

/// Default timeout for transcription requests (2 minutes for long audio)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Sample rate for Whisper (16kHz)
const WHISPER_SAMPLE_RATE: u32 = 16000;

/// Response from Whisper server transcription endpoint
#[derive(Debug, Clone, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

/// Status of the Whisper server connection
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

/// Remote Whisper server client
#[derive(Debug)]
pub struct WhisperServerClient {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl WhisperServerClient {
    /// Create a new Whisper server client with URL validation
    pub fn new(base_url: &str, model: &str) -> Result<Self, String> {
        let cleaned_url = base_url.trim_end_matches('/');

        // Validate URL format and scheme
        let parsed = reqwest::Url::parse(cleaned_url)
            .map_err(|e| format!("Invalid Whisper server URL '{}': {}", cleaned_url, e))?;

        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(format!(
                "Whisper server URL must use http or https scheme, got: {}",
                parsed.scheme()
            ));
        }

        // Reject URLs with credentials (security risk)
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("Whisper server URL must not contain credentials".to_string());
        }

        let client = reqwest::Client::builder()
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

    /// Check connection status and list available models
    pub async fn check_status(&self) -> WhisperServerStatus {
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

    /// List available models from the Whisper server
    pub async fn list_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/v1/models", self.base_url);
        debug!("Listing Whisper server models from {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to Whisper server: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Whisper server returned error status: {}",
                response.status()
            ));
        }

        let models: ModelsResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Whisper server response: {}", e))?;

        let model_ids: Vec<String> = models.data.into_iter().map(|m| m.id).collect();
        info!("Found {} Whisper server models", model_ids.len());

        Ok(model_ids)
    }

    /// Transcribe audio samples using the remote Whisper server
    ///
    /// # Arguments
    /// * `audio` - Audio samples as f32, normalized to [-1.0, 1.0], 16kHz mono
    /// * `language` - Language code (e.g., "en", "auto")
    ///
    /// # Returns
    /// The transcribed text
    pub async fn transcribe(&self, audio: &[f32], language: &str) -> Result<String, String> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        // Encode audio as WAV
        let wav_bytes = encode_wav(audio, WHISPER_SAMPLE_RATE)?;

        let url = format!("{}/v1/audio/transcriptions", self.base_url);
        debug!(
            "Transcribing {} samples ({:.2}s) via {}",
            audio.len(),
            audio.len() as f32 / WHISPER_SAMPLE_RATE as f32,
            url
        );

        // Build multipart form
        let file_part = reqwest::multipart::Part::bytes(wav_bytes)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| format!("Failed to create file part: {}", e))?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", self.model.clone())
            .text("response_format", "json")
            // Anti-hallucination parameters
            .text("temperature", "0.0") // Deterministic output, reduces hallucination
            .text("no_speech_threshold", "0.8") // Higher threshold to filter silence (default 0.6)
            .text("condition_on_previous_text", "false"); // Prevents repetitive phrases

        // Add language if not auto-detect
        if language != "auto" && !language.is_empty() {
            form = form.text("language", language.to_string());
        }

        let response = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Failed to send transcription request: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Whisper server transcription failed: {} - {}", status, body);
            return Err(format!(
                "Whisper server returned error: {} - {}",
                status, body
            ));
        }

        let result: TranscriptionResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse transcription response: {}", e))?;

        debug!("Transcription result: {} chars", result.text.len());
        Ok(result.text)
    }

    /// Blocking version of transcribe for use in synchronous pipeline code
    ///
    /// This creates a tokio runtime to run the async transcribe method.
    /// Should be called from a non-async context (like the pipeline thread).
    pub fn transcribe_blocking(&self, audio: &[f32], language: &str) -> Result<String, String> {
        // Create a new runtime for blocking execution
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
fn encode_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, String> {
    let mut buffer = Cursor::new(Vec::new());

    // WAV header constants
    let num_channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * num_channels as u32 * bits_per_sample as u32 / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size = samples.len() as u32 * 2; // 2 bytes per i16 sample
    let file_size = 36 + data_size;

    // RIFF header
    buffer
        .write_all(b"RIFF")
        .map_err(|e| format!("WAV write error: {}", e))?;
    buffer
        .write_all(&file_size.to_le_bytes())
        .map_err(|e| format!("WAV write error: {}", e))?;
    buffer
        .write_all(b"WAVE")
        .map_err(|e| format!("WAV write error: {}", e))?;

    // fmt chunk
    buffer
        .write_all(b"fmt ")
        .map_err(|e| format!("WAV write error: {}", e))?;
    buffer
        .write_all(&16u32.to_le_bytes())
        .map_err(|e| format!("WAV write error: {}", e))?; // chunk size
    buffer
        .write_all(&1u16.to_le_bytes())
        .map_err(|e| format!("WAV write error: {}", e))?; // audio format (PCM)
    buffer
        .write_all(&num_channels.to_le_bytes())
        .map_err(|e| format!("WAV write error: {}", e))?;
    buffer
        .write_all(&sample_rate.to_le_bytes())
        .map_err(|e| format!("WAV write error: {}", e))?;
    buffer
        .write_all(&byte_rate.to_le_bytes())
        .map_err(|e| format!("WAV write error: {}", e))?;
    buffer
        .write_all(&block_align.to_le_bytes())
        .map_err(|e| format!("WAV write error: {}", e))?;
    buffer
        .write_all(&bits_per_sample.to_le_bytes())
        .map_err(|e| format!("WAV write error: {}", e))?;

    // data chunk
    buffer
        .write_all(b"data")
        .map_err(|e| format!("WAV write error: {}", e))?;
    buffer
        .write_all(&data_size.to_le_bytes())
        .map_err(|e| format!("WAV write error: {}", e))?;

    // Convert f32 samples to i16 and write
    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let i16_sample = (clamped * 32767.0) as i16;
        buffer
            .write_all(&i16_sample.to_le_bytes())
            .map_err(|e| format!("WAV write error: {}", e))?;
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

        // Test trailing slash removal
        let client2 =
            WhisperServerClient::new("http://localhost:8000/", "large-v3-turbo").unwrap();
        assert_eq!(client2.base_url, "http://localhost:8000");

        // Test https scheme
        let client3 =
            WhisperServerClient::new("https://whisper.example.com", "large-v3").unwrap();
        assert_eq!(client3.base_url, "https://whisper.example.com");
    }

    #[test]
    fn test_whisper_server_client_new_invalid_url() {
        // Test invalid URL format
        let result = WhisperServerClient::new("not-a-valid-url", "model");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid Whisper server URL"));

        // Test invalid scheme
        let result2 = WhisperServerClient::new("ftp://localhost:8000", "model");
        assert!(result2.is_err());
        assert!(result2.unwrap_err().contains("http or https"));

        // Test URL with credentials (security risk)
        let result3 = WhisperServerClient::new("http://user:pass@localhost:8000", "model");
        assert!(result3.is_err());
        assert!(result3.unwrap_err().contains("must not contain credentials"));

        // Test URL with username only
        let result4 = WhisperServerClient::new("http://admin@localhost:8000", "model");
        assert!(result4.is_err());
        assert!(result4.unwrap_err().contains("must not contain credentials"));
    }

    #[test]
    fn test_encode_wav_empty() {
        let samples: Vec<f32> = vec![];
        let result = encode_wav(&samples, 16000).unwrap();
        // 44 bytes header + 0 bytes data
        assert_eq!(result.len(), 44);
    }

    #[test]
    fn test_encode_wav_samples() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let result = encode_wav(&samples, 16000).unwrap();
        // 44 bytes header + 10 bytes data (5 samples * 2 bytes)
        assert_eq!(result.len(), 54);

        // Verify RIFF header
        assert_eq!(&result[0..4], b"RIFF");
        assert_eq!(&result[8..12], b"WAVE");
        assert_eq!(&result[12..16], b"fmt ");
        assert_eq!(&result[36..40], b"data");
    }

    #[test]
    fn test_encode_wav_sample_conversion() {
        // Test that samples are correctly converted to i16
        let samples = vec![0.0f32, 1.0, -1.0, 0.5, -0.5];
        let result = encode_wav(&samples, 16000).unwrap();

        // Check sample values (after 44-byte header)
        let i16_samples: Vec<i16> = result[44..]
            .chunks(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        assert_eq!(i16_samples[0], 0); // 0.0
        assert_eq!(i16_samples[1], 32767); // 1.0
        assert_eq!(i16_samples[2], -32767); // -1.0
        assert_eq!(i16_samples[3], 16383); // 0.5 (approximately)
        assert_eq!(i16_samples[4], -16383); // -0.5 (approximately)
    }

    #[test]
    fn test_encode_wav_clamping() {
        // Test that out-of-range samples are clamped
        let samples = vec![2.0f32, -2.0, 1.5, -1.5];
        let result = encode_wav(&samples, 16000).unwrap();

        let i16_samples: Vec<i16> = result[44..]
            .chunks(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        assert_eq!(i16_samples[0], 32767); // 2.0 clamped to 1.0
        assert_eq!(i16_samples[1], -32767); // -2.0 clamped to -1.0
        assert_eq!(i16_samples[2], 32767); // 1.5 clamped to 1.0
        assert_eq!(i16_samples[3], -32767); // -1.5 clamped to -1.0
    }

    #[test]
    fn test_encode_wav_sample_rate() {
        // Verify sample rate is written correctly
        let samples = vec![0.0f32];
        let result = encode_wav(&samples, 48000).unwrap();

        // Sample rate is at bytes 24-27
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
}
