//! OpenAI image generation API client.
//!
//! Thin wrapper around the `/v1/images/generations` endpoint for
//! `gpt-image-2`. Mirrors `gemini_client.rs`'s retry + error-body-truncation
//! shape so callers see a uniform surface across providers.

use reqwest::header::{HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use std::time::Duration;
use tracing::info;

const OPENAI_IMAGES_ENDPOINT: &str = "https://api.openai.com/v1/images/generations";
pub const OPENAI_IMAGE_MODEL: &str = "gpt-image-2";

/// Supported quality tiers on gpt-image-2. Drives compute budget (the
/// self-review loop), not resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAIImageQuality {
    Low,
    Medium,
    High,
}

impl OpenAIImageQuality {
    pub fn as_api_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

pub struct OpenAIImageClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenAIImageResponse {
    data: Vec<OpenAIImageData>,
}

#[derive(Debug, Deserialize)]
struct OpenAIImageData {
    b64_json: Option<String>,
}

impl OpenAIImageClient {
    /// Construct an OpenAI image API client. `timeout_secs` is shared with
    /// the Gemini path (see `commands/images.rs`).
    pub fn new(api_key: &str, timeout_secs: u64) -> Result<Self, String> {
        if api_key.trim().is_empty() {
            return Err("OpenAI API key is required".to_string());
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            client,
            api_key: api_key.to_string(),
            model: OPENAI_IMAGE_MODEL.to_string(),
        })
    }

    pub fn build_request_body(prompt: &str, quality: OpenAIImageQuality) -> serde_json::Value {
        // 1024×1024 matches Gemini so the two providers stay visually comparable.
        serde_json::json!({
            "model": OPENAI_IMAGE_MODEL,
            "prompt": prompt,
            "quality": quality.as_api_str(),
            "size": "1024x1024",
            "n": 1,
        })
    }

    pub fn extract_image_base64(response: &OpenAIImageResponse) -> Option<String> {
        response
            .data
            .first()
            .and_then(|d| d.b64_json.clone())
    }

    pub async fn generate_image(
        &self,
        prompt: &str,
        quality: OpenAIImageQuality,
    ) -> Result<String, String> {
        let body = Self::build_request_body(prompt, quality);

        info!(
            "OpenAI image generation: quality={} prompt={} chars",
            quality.as_api_str(),
            prompt.len()
        );

        let auth_header = HeaderValue::from_str(&format!("Bearer {}", self.api_key))
            .map_err(|e| format!("Invalid API key header: {}", e))?;

        // Retry once on network errors (transient DNS/connection failures).
        let response = match self.send_request(&body, &auth_header).await {
            Ok(resp) => resp,
            Err(first_err) => {
                tracing::warn!("OpenAI request failed, retrying once: {}", first_err);
                tokio::time::sleep(Duration::from_secs(2)).await;
                self.send_request(&body, &auth_header)
                    .await
                    .map_err(|e| format!("OpenAI API request failed after retry: {}", e))?
            }
        };

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            // UTF-8-safe truncation via shared helper — OpenAI 4xx bodies can
            // echo the clinician's prompt text (PHI-adjacent).
            let truncated = crate::llm_client::truncate_error_body(&error_body, 200);
            return Err(format!("OpenAI API error {}: {}", status, truncated));
        }

        let parsed: OpenAIImageResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

        Self::extract_image_base64(&parsed)
            .ok_or_else(|| "OpenAI response contained no image data".to_string())
    }

    async fn send_request(
        &self,
        body: &serde_json::Value,
        auth_header: &HeaderValue,
    ) -> Result<reqwest::Response, String> {
        self.client
            .post(OPENAI_IMAGES_ENDPOINT)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .header(AUTHORIZATION, auth_header.clone())
            .json(body)
            .send()
            .await
            .map_err(|e| format!("OpenAI API request failed: {}", e))
    }

    pub fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_parse_round_trip() {
        for q in [
            OpenAIImageQuality::Low,
            OpenAIImageQuality::Medium,
            OpenAIImageQuality::High,
        ] {
            assert_eq!(OpenAIImageQuality::parse(q.as_api_str()), Some(q));
        }
        assert_eq!(OpenAIImageQuality::parse("huge"), None);
        assert_eq!(OpenAIImageQuality::parse(""), None);
    }

    #[test]
    fn test_build_request_body_shape() {
        let body =
            OpenAIImageClient::build_request_body("Label the L4-L5 disc", OpenAIImageQuality::Medium);
        assert_eq!(body["model"], OPENAI_IMAGE_MODEL);
        assert_eq!(body["prompt"], "Label the L4-L5 disc");
        assert_eq!(body["quality"], "medium");
        assert_eq!(body["size"], "1024x1024");
        assert_eq!(body["n"], 1);
    }

    #[test]
    fn test_parse_response_valid() {
        let response_json = serde_json::json!({
            "data": [{"b64_json": "iVBORw0KGgo="}],
            "usage": {"total_tokens": 1}
        });
        let parsed: OpenAIImageResponse = serde_json::from_value(response_json).unwrap();
        assert_eq!(
            OpenAIImageClient::extract_image_base64(&parsed),
            Some("iVBORw0KGgo=".to_string())
        );
    }

    #[test]
    fn test_parse_response_empty_data() {
        let response_json = serde_json::json!({"data": []});
        let parsed: OpenAIImageResponse = serde_json::from_value(response_json).unwrap();
        assert!(OpenAIImageClient::extract_image_base64(&parsed).is_none());
    }

    #[test]
    fn test_parse_response_missing_b64() {
        let response_json = serde_json::json!({"data": [{"url": "https://example.com/x.png"}]});
        let parsed: OpenAIImageResponse = serde_json::from_value(response_json).unwrap();
        assert!(OpenAIImageClient::extract_image_base64(&parsed).is_none());
    }

    #[test]
    fn test_new_empty_api_key() {
        assert!(OpenAIImageClient::new("", 45).is_err());
    }

    #[test]
    fn test_new_valid_api_key() {
        let result = OpenAIImageClient::new("sk-test-123", 45);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().model(), OPENAI_IMAGE_MODEL);
    }
}
