//! OpenAI image generation proxy (v0.10.54+).
//!
//! Profile-service holds the `OPENAI_API_KEY` in a launchd env var and makes
//! the image call on behalf of rooms. The workstation sends `{prompt, quality}`
//! and gets back base64 PNG bytes; the secret never leaves this machine.
//!
//! Follows the same shape as `medplum_auth.rs` but without a token cache —
//! OpenAI's API key IS the credential (no token-exchange grant), so every
//! call goes straight to `/v1/images/generations`.
//!
//! If `OPENAI_API_KEY` is unset at startup, the `POST /openai/image` route
//! returns `503` and clients are expected to fall back to a local key (or
//! surface the "server has no key" error to the clinician).

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

/// Response shape. `image_base64` is the raw base64 PNG, same as what the
/// Tauri command's `AiImageResponse` carries back to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIImageResponse {
    pub image_base64: String,
    pub model: String,
    pub quality: String,
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

    pub fn is_configured(&self) -> bool {
        self.config.is_some()
    }

    pub async fn generate(
        &self,
        req: &OpenAIImageRequest,
    ) -> Result<OpenAIImageResponse, ApiError> {
        let Some(cfg) = &self.config else {
            return Err(ApiError::Internal(
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

        Ok(OpenAIImageResponse {
            image_base64,
            model: OPENAI_IMAGE_MODEL.to_string(),
            quality: req.quality.clone(),
        })
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
    async fn generate_rejects_unconfigured() {
        let proxy = OpenAIImageProxy::new(None);
        let req = OpenAIImageRequest {
            prompt: "Test".into(),
            quality: "low".into(),
            size: None,
        };
        let r = proxy.generate(&req).await;
        assert!(r.is_err());
        match r.unwrap_err() {
            ApiError::Internal(m) => assert!(m.contains("not configured"), "msg: {m}"),
            e => panic!("unexpected error: {e:?}"),
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

    #[test]
    fn is_configured_reports_state() {
        assert!(!OpenAIImageProxy::new(None).is_configured());
        let cfg = OpenAIImageConfig::from_value(Some("sk-test".into()));
        assert!(OpenAIImageProxy::new(cfg).is_configured());
    }
}
