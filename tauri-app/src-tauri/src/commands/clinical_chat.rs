//! Clinical chat Tauri commands.
//!
//! Provides HTTP proxy for clinical assistant chat to work around browser CSP restrictions.

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info};

/// Timeout for clinical chat requests (60 seconds - allows for tool use)
const CHAT_TIMEOUT: Duration = Duration::from_secs(60);

/// Chat message from frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Request body for chat completions
#[derive(Debug, Clone, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
}

/// Tool usage info from response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
    pub success: bool,
}

/// Tool usage summary from response
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolUsage {
    pub rounds: u32,
    pub tools_called: Vec<ToolCall>,
}

/// Chat completion response
#[derive(Debug, Clone, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    tool_usage: Option<ToolUsage>,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

/// Response returned to frontend
#[derive(Debug, Clone, Serialize)]
pub struct ClinicalChatResponse {
    pub content: String,
    pub tools_used: Vec<String>,
}

/// Send a message to the clinical assistant LLM
#[tauri::command]
pub async fn clinical_chat_send(
    llm_router_url: String,
    llm_api_key: String,
    llm_client_id: String,
    messages: Vec<ChatMessage>,
) -> Result<ClinicalChatResponse, String> {
    info!(
        "Clinical chat: sending {} messages to {}",
        messages.len(),
        llm_router_url
    );

    if llm_router_url.is_empty() {
        return Err("LLM Router URL is not configured".to_string());
    }

    // Build headers
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    if !llm_api_key.is_empty() {
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", llm_api_key))
                .map_err(|e| format!("Invalid API key: {}", e))?,
        );
    }

    headers.insert(
        "X-Client-Id",
        HeaderValue::from_str(if llm_client_id.is_empty() {
            "ai-scribe"
        } else {
            &llm_client_id
        })
        .unwrap_or_else(|_| HeaderValue::from_static("ai-scribe")),
    );

    headers.insert(
        "X-Clinic-Task",
        HeaderValue::from_static("clinical_assistant"),
    );

    // Build request
    let request = ChatCompletionRequest {
        model: "clinical-assistant".to_string(),
        messages,
        max_tokens: Some(500),
        temperature: Some(0.3),
    };

    let url = format!("{}/v1/chat/completions", llm_router_url.trim_end_matches('/'));
    debug!("Clinical chat URL: {}", url);

    // Create client and send request
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(CHAT_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .post(&url)
        .headers(headers)
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            error!("Clinical chat request failed: {}", e);
            if e.is_timeout() {
                "Request timed out".to_string()
            } else if e.is_connect() {
                format!("Failed to connect to LLM router at {}: {}", llm_router_url, e)
            } else {
                format!("Request failed: {}", e)
            }
        })?;

    let status = response.status();
    debug!("Clinical chat response status: {}", status);

    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        error!("Clinical chat API error: {} - {}", status, error_text);
        return Err(format!("API error: {} - {}", status, error_text));
    }

    let chat_response: ChatCompletionResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let content = chat_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_else(|| "No response received".to_string());

    let tools_used: Vec<String> = chat_response
        .tool_usage
        .map(|tu| tu.tools_called.into_iter().map(|t| t.name).collect())
        .unwrap_or_default();

    info!(
        "Clinical chat: received {} char response, {} tools used",
        content.len(),
        tools_used.len()
    );

    Ok(ClinicalChatResponse {
        content,
        tools_used,
    })
}
