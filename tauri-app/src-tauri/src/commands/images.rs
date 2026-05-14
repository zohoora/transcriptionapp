//! AI image generation command.
//!
//! Routes to Gemini (Flash or Pro) or OpenAI (gpt-image-2 at low/medium/high
//! quality) based on the resolved `image_model` key. The canonical allowlist
//! lives in `config::IMAGE_MODEL_ALLOWLIST` — `test_allowlist_and_router_stay_in_sync`
//! asserts drift-free.

use super::CommandError;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::commands::physicians::SharedServerConfig;
use crate::commands::SharedProfileClient;
use crate::config::Config;
use crate::gemini_client::{GeminiClient, GEMINI_FLASH_MODEL, GEMINI_PRO_MODEL};
use crate::openai_image_client::{OpenAIImageClient, OpenAIImageQuality};
use crate::profile_client::ProxyImageOutcome;

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
            // Proxy-first; fall back to local key only when the proxy signals
            // it has no key (HTTP 503 → `Unconfigured`). Real OpenAI errors
            // bubble up.
            let via_proxy = match profile_client.read().await.clone() {
                Some(pf) => Some(
                    pf.fetch_openai_image(
                        &prompt,
                        quality.as_api_str(),
                        timeout_secs.saturating_add(15),
                    )
                    .await
                    .map_err(|e| CommandError::Network(e.to_string()))?,
                ),
                None => None,
            };
            match via_proxy {
                Some(ProxyImageOutcome::Ok(bytes)) => bytes,
                Some(ProxyImageOutcome::Unconfigured) | None => {
                    warn!(
                        event = "openai_proxy_unconfigured_falling_back",
                        "Profile-service has no OPENAI_API_KEY; using local key"
                    );
                    let client = OpenAIImageClient::new(&config.openai_api_key, timeout_secs)
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

/// Write a base64-encoded PNG to a path supplied by the frontend's native
/// save dialog (`tauri-plugin-dialog`). The OS dialog gates path picking, so
/// no path validation here.
#[tauri::command]
pub async fn save_image_png(image_base64: String, dest_path: String) -> Result<(), CommandError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image_base64)
        .map_err(|e| CommandError::Validation(format!("invalid base64: {e}")))?;
    std::fs::write(&dest_path, &bytes)?;
    info!("Saved AI image: {} bytes → {}", bytes.len(), dest_path);
    Ok(())
}

/// Open the native macOS print dialog for a base64-encoded PNG.
///
/// Strategy: write the PNG to `std::env::temp_dir()` and invoke
/// `osascript` with `tell application "Preview" to print POSIX file ...`.
/// AppleScript's `print` verb defaults to **showing the print dialog**
/// (NSPrintOperation under the hood), so the user gets a real Cmd+P-style
/// experience rather than a silent spool to the default printer.
///
/// macOS cleans `/tmp` on reboot, so no manual cleanup is needed for the
/// temp PNG. We don't `Drop` it because Preview reads asynchronously after
/// the AppleScript call returns.
#[tauri::command]
pub async fn print_image_png(image_base64: String) -> Result<(), CommandError> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = image_base64;
        Err(CommandError::Config(
            "Printing is only supported on macOS".into(),
        ))
    }
    #[cfg(target_os = "macos")]
    {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&image_base64)
            .map_err(|e| CommandError::Validation(format!("invalid base64: {e}")))?;

        let temp_path = std::env::temp_dir().join(format!(
            "ami-print-{}.png",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::write(&temp_path, &bytes)?;

        // `activate` brings Preview to foreground so the print dialog appears
        // on top of the main app instead of behind it. The path string is
        // backslash/quote-escaped — we generate the temp path ourselves so it
        // can't currently contain those chars, but escape anyway to stay
        // robust if std::env::temp_dir() ever changes shape.
        let escaped_path = temp_path
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let script = format!(
            "tell application \"Preview\"\n    activate\n    print POSIX file \"{escaped_path}\"\nend tell"
        );

        let output = std::process::Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| CommandError::Io(format!("failed to spawn osascript: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CommandError::Io(format!(
                "osascript failed: {}",
                stderr.trim()
            )));
        }
        info!(
            "Print dialog opened for {} bytes at {}",
            bytes.len(),
            temp_path.display()
        );
        Ok(())
    }
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
