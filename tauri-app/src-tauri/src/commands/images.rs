//! AI image generation command

use super::CommandError;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::config::Config;
use crate::gemini_client::GeminiClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiImageResponse {
    pub image_base64: String,
    pub prompt: String,
}

#[tauri::command]
pub async fn generate_ai_image(prompt: String) -> Result<AiImageResponse, CommandError> {
    if prompt.trim().is_empty() {
        return Err(CommandError::Validation("Image prompt is empty".into()));
    }

    let config = Config::load_or_default();

    if config.image_source != "ai" {
        return Err(CommandError::Config(
            "AI image generation is not enabled".into(),
        ));
    }

    let client = GeminiClient::new(&config.gemini_api_key)
        .map_err(|e| CommandError::Config(e))?;

    info!("Generating AI image: prompt={} chars", prompt.len());

    let image_base64 = client
        .generate_image(&prompt)
        .await
        .map_err(|e| CommandError::Network(e))?;

    info!("AI image generated: {} bytes base64", image_base64.len());

    Ok(AiImageResponse {
        image_base64,
        prompt,
    })
}
