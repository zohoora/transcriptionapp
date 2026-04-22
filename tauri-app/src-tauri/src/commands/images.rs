//! AI image generation command.
//!
//! Routes to Gemini (Flash or Pro) or OpenAI (gpt-image-2 at low/medium/high
//! quality) based on the resolved `image_model` key. The canonical allowlist
//! lives in `config::IMAGE_MODEL_ALLOWLIST` — `test_allowlist_and_router_stay_in_sync`
//! asserts drift-free.

use super::CommandError;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::commands::physicians::SharedServerConfig;
use crate::commands::SharedProfileClient;
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
    profile_client: tauri::State<'_, SharedProfileClient>,
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

    let effective_key = image_model.unwrap_or_else(|| config.image_model.clone());
    let backend = resolve_backend(&effective_key)?;

    // OpenAI's `high` tier has a comparable 95th-percentile wall time to
    // Gemini's, so one knob suffices for both providers.
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
            // Proxy-first: the profile-service holds OPENAI_API_KEY in a
            // launchd env var (v0.10.54+) so new workstations don't need
            // per-machine key plumbing. Fall back to the local key only if
            // the proxy is unreachable/unconfigured AND a local key exists
            // (dev/offline path). Proxy errors (OpenAI 4xx/5xx, auth, quota)
            // bubble up without falling back — those aren't "proxy missing".
            let proxy_client = profile_client.read().await.clone();
            let proxy_result = if let Some(pf) = proxy_client {
                Some(pf.fetch_openai_image(&prompt, quality.as_api_str()).await)
            } else {
                None
            };
            match proxy_result {
                Some(Ok(bytes)) => bytes,
                Some(Err(e)) => {
                    let msg = e.to_string();
                    let proxy_missing = msg.contains("not configured");
                    if proxy_missing && !config.openai_api_key.trim().is_empty() {
                        warn!(
                            event = "openai_proxy_unconfigured_falling_back",
                            "Profile-service has no OPENAI_API_KEY; using local key"
                        );
                        let client =
                            OpenAIImageClient::new(&config.openai_api_key, timeout_secs)
                                .map_err(CommandError::Config)?;
                        client
                            .generate_image(&prompt, quality)
                            .await
                            .map_err(CommandError::Network)?
                    } else {
                        return Err(CommandError::Network(msg));
                    }
                }
                None => {
                    // No profile client registered (unusual in prod). Local-key
                    // path as last resort.
                    let client =
                        OpenAIImageClient::new(&config.openai_api_key, timeout_secs)
                            .map_err(CommandError::Config)?;
                    client
                        .generate_image(&prompt, quality)
                        .await
                        .map_err(CommandError::Network)?
                }
            }
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
        for bad in ["", "gpt-5-mega", "gemini-flash ", "openai-"] {
            let err = resolve_backend(bad);
            assert!(err.is_err(), "expected err for {bad:?}");
            if let Err(CommandError::Validation(msg)) = err {
                assert!(msg.contains("unknown image_model"), "msg={msg}");
            } else {
                panic!("expected Validation error for {bad:?}");
            }
        }
    }

    /// Drift guard: every entry in `IMAGE_MODEL_ALLOWLIST` must route.
    /// If you add a key to the allowlist, you must also add a router arm —
    /// this test will fail loudly instead of silently rejecting at runtime.
    #[test]
    fn test_allowlist_and_router_stay_in_sync() {
        for key in crate::config::IMAGE_MODEL_ALLOWLIST {
            assert!(
                resolve_backend(key).is_ok(),
                "allowlist entry {key:?} has no router arm"
            );
        }
    }
}
