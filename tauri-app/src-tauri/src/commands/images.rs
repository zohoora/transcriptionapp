//! AI image generation command

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
pub async fn generate_ai_image(prompt: String) -> Result<AiImageResponse, String> {
    if prompt.trim().is_empty() {
        return Err("Image prompt is empty".to_string());
    }

    let config = Config::load_or_default();

    if config.image_source != "ai" {
        return Err("AI image generation is not enabled".to_string());
    }

    let client = GeminiClient::new(&config.gemini_api_key)?;

    info!("Generating AI image: prompt={} chars", prompt.len());

    let image_base64 = client.generate_image(&prompt).await?;

    info!("AI image generated: {} bytes base64", image_base64.len());

    Ok(AiImageResponse {
        image_base64,
        prompt,
    })
}
