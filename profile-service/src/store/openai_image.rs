//! OpenAI image generation proxy (v0.10.54+).
//!
//! Server holds `OPENAI_API_KEY`; workstations POST `{prompt, quality}` and
//! receive base64 PNG bytes. Unlike the Medplum proxy, there's no token
//! cache — OpenAI uses the API key directly as the credential. When the env
//! var is unset, `generate()` returns `ServiceUnavailable` (HTTP 503) so
//! clients can fall back to a local key without treating it as a fault.

use crate::error::ApiError;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};

const OPENAI_IMAGES_ENDPOINT: &str = "https://api.openai.com/v1/images/generations";
const OPENAI_IMAGE_MODEL: &str = "gpt-image-2";

/// `OPENAI_API_KEY` lifted out of env at startup. `None` when unset — the
/// route then returns 503 so the workstation can fall back to its local key.
/// Secret is NEVER serialized or logged.
#[derive(Clone)]
pub struct OpenAIImageConfig {
    api_key: String,
}

impl OpenAIImageConfig {
    pub fn from_env() -> Option<Self> {
        Self::from_value(std::env::var("OPENAI_API_KEY").ok())
    }

    /// Pure parser — tests use this to avoid racing `std::env`.
    pub fn from_value(api_key: Option<String>) -> Option<Self> {
        let api_key = api_key?;
        if api_key.trim().is_empty() {
            return None;
        }
        Some(Self { api_key })
    }
}

/// Request shape the workstation POSTs. Mirrors what the Tauri command's
/// OpenAI branch passes to the local client.
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIImageRequest {
    pub prompt: String,
    /// "low" | "medium" | "high"
    pub quality: String,
    /// Optional — defaults to "1024x1024" to match Gemini visually.
    #[serde(default)]
    pub size: Option<String>,
}

/// Response shape. `image_base64` is the raw base64 PNG.
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIImageResponse {
    pub image_base64: String,
}

pub struct OpenAIImageProxy {
    config: Option<OpenAIImageConfig>,
    http: reqwest::Client,
}

impl OpenAIImageProxy {
    pub fn new(config: Option<OpenAIImageConfig>) -> Self {
        Self {
            config,
            // Match Tauri's Gemini timeout (45s default, clamped by the
            // workstation's DetectionThresholds snapshot). OpenAI high-tier
            // can take ~30s so we leave headroom.
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("reqwest client"),
        }
    }

    pub async fn generate(
        &self,
        req: &OpenAIImageRequest,
    ) -> Result<OpenAIImageResponse, ApiError> {
        let Some(cfg) = &self.config else {
            return Err(ApiError::ServiceUnavailable(
                "OpenAI image proxy not configured — set OPENAI_API_KEY".into(),
            ));
        };
        if req.prompt.trim().is_empty() {
            return Err(ApiError::BadRequest("prompt is empty".into()));
        }
        if !matches!(req.quality.as_str(), "low" | "medium" | "high") {
            return Err(ApiError::BadRequest(format!(
                "quality must be low|medium|high, got {:?}",
                req.quality
            )));
        }

        let size = req.size.clone().unwrap_or_else(|| "1024x1024".to_string());
        let body = serde_json::json!({
            "model": OPENAI_IMAGE_MODEL,
            "prompt": req.prompt,
            "quality": req.quality,
            "size": size,
            "n": 1,
        });

        let response = self
            .http
            .post(OPENAI_IMAGES_ENDPOINT)
            .bearer_auth(&cfg.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "openai image proxy: network error");
                ApiError::Internal(format!("OpenAI request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let truncated: String = body.chars().take(200).collect();
            return Err(ApiError::Internal(format!(
                "OpenAI returned {}: {}",
                status, truncated
            )));
        }

        let parsed: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ApiError::Internal(format!("OpenAI response parse: {}", e)))?;

        let image_base64 = parsed["data"][0]["b64_json"]
            .as_str()
            .ok_or_else(|| ApiError::Internal("OpenAI response missing b64_json".into()))?
            .to_string();

        info!(
            event = "openai_image_generated",
            quality = %req.quality,
            bytes_base64 = image_base64.len(),
            "generated image via OpenAI proxy"
        );

        Ok(OpenAIImageResponse { image_base64 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_rejects_missing_or_empty() {
        assert!(OpenAIImageConfig::from_value(None).is_none());
        assert!(OpenAIImageConfig::from_value(Some("".into())).is_none());
        assert!(OpenAIImageConfig::from_value(Some("   ".into())).is_none());
    }

    #[test]
    fn config_accepts_non_empty() {
        let cfg = OpenAIImageConfig::from_value(Some("sk-proj-abc".into())).expect("present");
        assert_eq!(cfg.api_key, "sk-proj-abc");
    }

    #[tokio::test]
    async fn generate_rejects_unconfigured_with_503() {
        let proxy = OpenAIImageProxy::new(None);
        let req = OpenAIImageRequest {
            prompt: "Test".into(),
            quality: "low".into(),
            size: None,
        };
        let r = proxy.generate(&req).await;
        assert!(r.is_err());
        match r.unwrap_err() {
            ApiError::ServiceUnavailable(m) => {
                assert!(m.contains("not configured"), "msg: {m}")
            }
            e => panic!("expected ServiceUnavailable, got: {e:?}"),
        }
    }

    #[tokio::test]
    async fn generate_rejects_empty_prompt() {
        let cfg = OpenAIImageConfig::from_value(Some("sk-test".into()));
        let proxy = OpenAIImageProxy::new(cfg);
        let req = OpenAIImageRequest {
            prompt: "   ".into(),
            quality: "low".into(),
            size: None,
        };
        let r = proxy.generate(&req).await;
        assert!(r.is_err());
        match r.unwrap_err() {
            ApiError::BadRequest(m) => assert!(m.contains("prompt is empty")),
            e => panic!("unexpected error: {e:?}"),
        }
    }

    #[tokio::test]
    async fn generate_rejects_invalid_quality() {
        let cfg = OpenAIImageConfig::from_value(Some("sk-test".into()));
        let proxy = OpenAIImageProxy::new(cfg);
        let req = OpenAIImageRequest {
            prompt: "Test".into(),
            quality: "ultra".into(),
            size: None,
        };
        let r = proxy.generate(&req).await;
        assert!(r.is_err());
        match r.unwrap_err() {
            ApiError::BadRequest(m) => assert!(m.contains("quality must be")),
            e => panic!("unexpected error: {e:?}"),
        }
    }

}
