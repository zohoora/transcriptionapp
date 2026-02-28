//! Google Gemini API client for AI image generation
//!
//! Thin wrapper around the Gemini generateContent endpoint for
//! generating medical illustrations via Nano Banana 2.

use reqwest::header::{HeaderValue, CONTENT_TYPE};
use serde::Deserialize;
use std::time::Duration;
use tracing::info;

const GEMINI_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const DEFAULT_MODEL: &str = "gemini-3.1-flash-image-preview";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

pub struct GeminiClient {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

// -- Response types --

#[derive(Debug, Deserialize)]
pub struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiResponseContent,
}

#[derive(Debug, Deserialize)]
struct GeminiResponseContent {
    parts: Vec<GeminiResponsePart>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponsePart {
    inline_data: Option<GeminiInlineData>,
    #[allow(dead_code)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiInlineData {
    #[allow(dead_code)]
    mime_type: String,
    data: String,
}

impl GeminiClient {
    pub fn new(api_key: &str) -> Result<Self, String> {
        if api_key.trim().is_empty() {
            return Err("Gemini API key is required".to_string());
        }

        let client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            client,
            api_key: api_key.to_string(),
            model: DEFAULT_MODEL.to_string(),
        })
    }

    pub fn build_request_body(prompt: &str, aspect_ratio: &str) -> serde_json::Value {
        serde_json::json!({
            "contents": [{
                "parts": [{"text": prompt}]
            }],
            "generationConfig": {
                "responseModalities": ["IMAGE"],
                "imageConfig": {
                    "aspectRatio": aspect_ratio
                }
            }
        })
    }

    pub fn extract_image_base64(response: &GeminiResponse) -> Option<String> {
        response
            .candidates
            .first()
            .and_then(|c| c.content.parts.iter().find_map(|p| p.inline_data.as_ref()))
            .map(|d| d.data.clone())
    }

    pub async fn generate_image(&self, prompt: &str) -> Result<String, String> {
        let url = format!("{}/{}:generateContent", GEMINI_ENDPOINT, self.model);
        let body = Self::build_request_body(prompt, "4:3");

        info!("Gemini image generation: prompt={} chars", prompt.len());

        let response = self
            .client
            .post(&url)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .header(
                "x-goog-api-key",
                HeaderValue::from_str(&self.api_key)
                    .map_err(|e| format!("Invalid API key header: {}", e))?,
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Gemini API request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            // Truncate error body to avoid leaking sensitive data
            let truncated = if error_body.len() > 200 {
                &error_body[..200]
            } else {
                &error_body
            };
            return Err(format!("Gemini API error {}: {}", status, truncated));
        }

        let gemini_response: GeminiResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

        Self::extract_image_base64(&gemini_response)
            .ok_or_else(|| "Gemini response contained no image data".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_request_body() {
        let body = GeminiClient::build_request_body("Draw a knee", "4:3");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "Draw a knee");
        assert_eq!(body["generationConfig"]["responseModalities"][0], "IMAGE");
        assert_eq!(
            body["generationConfig"]["imageConfig"]["aspectRatio"],
            "4:3"
        );
    }

    #[test]
    fn test_parse_response_valid() {
        let response_json = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "inlineData": {
                            "mimeType": "image/png",
                            "data": "iVBORw0KGgo="
                        }
                    }]
                }
            }]
        });
        let response: GeminiResponse = serde_json::from_value(response_json).unwrap();
        let base64 = GeminiClient::extract_image_base64(&response);
        assert_eq!(base64, Some("iVBORw0KGgo=".to_string()));
    }

    #[test]
    fn test_parse_response_no_image() {
        let response_json = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "text": "I cannot generate that image"
                    }]
                }
            }]
        });
        let response: GeminiResponse = serde_json::from_value(response_json).unwrap();
        let base64 = GeminiClient::extract_image_base64(&response);
        assert!(base64.is_none());
    }

    #[test]
    fn test_parse_response_empty_candidates() {
        let response_json = serde_json::json!({
            "candidates": []
        });
        let response: GeminiResponse = serde_json::from_value(response_json).unwrap();
        let base64 = GeminiClient::extract_image_base64(&response);
        assert!(base64.is_none());
    }

    #[test]
    fn test_new_empty_api_key() {
        let result = GeminiClient::new("");
        assert!(result.is_err());
    }

    #[test]
    fn test_new_valid_api_key() {
        let result = GeminiClient::new("test-key-123");
        assert!(result.is_ok());
    }
}
