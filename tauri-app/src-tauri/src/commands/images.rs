//! AI image generation command.
//!
//! Routes to Gemini (Flash or Pro) or OpenAI (gpt-image-2 at low/medium/high
//! quality) based on the resolved `image_model` key. See ADR-driven rollout
//! (v0.10.53) for the provider/quality matrix.

use super::CommandError;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::commands::physicians::SharedServerConfig;
use crate::config::Config;
use crate::gemini_client::{GeminiClient, GEMINI_FLASH_MODEL, GEMINI_PRO_MODEL};
use crate::openai_image_client::{OpenAIImageClient, OpenAIImageQuality};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiImageResponse {
    pub image_base64: String,
    pub prompt: String,
    /// Echoes back the effective model key ("gemini-flash" etc.) so the UI
    /// can display "generated with …" without re-reading settings.
    pub image_model: String,
}

/// Route a resolved image-model key to the right provider + args.
/// Returns `Err` for any value not in the allowlist.
enum ImageBackend<'a> {
    Gemini { model: &'a str },
    OpenAI { quality: OpenAIImageQuality },
}

fn resolve_backend(key: &str) -> Result<ImageBackend<'static>, CommandError> {
    match key {
        "gemini-flash" => Ok(ImageBackend::Gemini {
            model: GEMINI_FLASH_MODEL,
        }),
        "gemini-pro" => Ok(ImageBackend::Gemini {
            model: GEMINI_PRO_MODEL,
        }),
        "openai-low" => Ok(ImageBackend::OpenAI {
            quality: OpenAIImageQuality::Low,
        }),
        "openai-medium" => Ok(ImageBackend::OpenAI {
            quality: OpenAIImageQuality::Medium,
        }),
        "openai-high" => Ok(ImageBackend::OpenAI {
            quality: OpenAIImageQuality::High,
        }),
        other => Err(CommandError::Validation(format!(
            "unknown image_model '{other}' — expected one of gemini-flash, gemini-pro, openai-low, openai-medium, openai-high"
        ))),
    }
}

#[tauri::command]
pub async fn generate_ai_image(
    prompt: String,
    image_model: Option<String>,
    server_config: tauri::State<'_, SharedServerConfig>,
) -> Result<AiImageResponse, CommandError> {
    if prompt.trim().is_empty() {
        return Err(CommandError::Validation("Image prompt is empty".into()));
    }

    let config = Config::load_or_default();

    if config.image_source != "ai" {
        return Err(CommandError::Config(
            "AI image generation is not enabled".into(),
        ));
    }

    // Override (per-call dropdown) beats the settings-persisted default.
    let effective_key = image_model.unwrap_or_else(|| config.image_model.clone());
    let backend = resolve_backend(&effective_key)?;

    // Reuse the server-configurable Gemini timeout for both providers —
    // the OpenAI `high` tier has a comparable 95th-percentile wall time and
    // adding a separate knob for observability-only gains isn't worth it.
    let timeout_secs = {
        let sc = server_config.read().await;
        sc.thresholds.gemini_generation_timeout_secs
    };

    info!(
        "Generating AI image: model={} prompt={} chars",
        effective_key,
        prompt.len()
    );

    let image_base64 = match backend {
        ImageBackend::Gemini { model } => {
            let client = GeminiClient::new(&config.gemini_api_key, model, timeout_secs)
                .map_err(CommandError::Config)?;
            client
                .generate_image(&prompt)
                .await
                .map_err(CommandError::Network)?
        }
        ImageBackend::OpenAI { quality } => {
            let client = OpenAIImageClient::new(&config.openai_api_key, timeout_secs)
                .map_err(CommandError::Config)?;
            client
                .generate_image(&prompt, quality)
                .await
                .map_err(CommandError::Network)?
        }
    };

    info!(
        "AI image generated: model={} {} bytes base64",
        effective_key,
        image_base64.len()
    );

    Ok(AiImageResponse {
        image_base64,
        prompt,
        image_model: effective_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_backend_gemini_flash() {
        let b = resolve_backend("gemini-flash").unwrap();
        match b {
            ImageBackend::Gemini { model } => assert_eq!(model, GEMINI_FLASH_MODEL),
            _ => panic!("expected Gemini Flash"),
        }
    }

    #[test]
    fn test_resolve_backend_gemini_pro() {
        let b = resolve_backend("gemini-pro").unwrap();
        match b {
            ImageBackend::Gemini { model } => assert_eq!(model, GEMINI_PRO_MODEL),
            _ => panic!("expected Gemini Pro"),
        }
    }

    #[test]
    fn test_resolve_backend_openai_each_tier() {
        for (key, expected) in [
            ("openai-low", OpenAIImageQuality::Low),
            ("openai-medium", OpenAIImageQuality::Medium),
            ("openai-high", OpenAIImageQuality::High),
        ] {
            let b = resolve_backend(key).unwrap();
            match b {
                ImageBackend::OpenAI { quality } => assert_eq!(quality, expected),
                _ => panic!("expected OpenAI for {key}"),
            }
        }
    }

    #[test]
    fn test_resolve_backend_unknown_rejected() {
        let err = resolve_backend("gpt-5-mega");
        assert!(err.is_err());
        if let Err(CommandError::Validation(msg)) = err {
            assert!(msg.contains("unknown image_model"), "msg={msg}");
        } else {
            panic!("expected Validation error");
        }
    }
}
