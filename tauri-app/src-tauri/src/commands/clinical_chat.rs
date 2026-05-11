//! Clinical chat Tauri commands.
//!
//! Provides HTTP proxy for clinical assistant chat to work around browser CSP restrictions.
//! Full conversation logging to `~/.transcriptionapp/logs/chat_log.jsonl` for debugging
//! and quality control.

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;
use tracing::{debug, error, info, warn};

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

/// A single chat exchange logged to JSONL for debugging and quality control.
#[derive(Serialize)]
struct ChatLogEntry {
    ts: String,
    user_message: String,
    system_prompt: String,
    response: String,
    tools_used: Vec<String>,
    model: String,
    duration_ms: u64,
    error: Option<String>,
}

/// Append a chat log entry to `~/.transcriptionapp/logs/chat_log.jsonl`.
fn log_chat_exchange(entry: &ChatLogEntry) {
    let Ok(log_dir) = dirs::home_dir()
        .map(|h| h.join(".transcriptionapp").join("logs"))
        .ok_or(())
    else {
        return;
    };
    if std::fs::create_dir_all(&log_dir).is_err() {
        return;
    }
    let path = log_dir.join("chat_log.jsonl");
    let Ok(line) = serde_json::to_string(entry) else {
        return;
    };
    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut f) => {
            let _ = writeln!(f, "{}", line);
        }
        Err(e) => warn!("Failed to write chat log: {e}"),
    }
}

/// Build a single-line system context message from a med list. Empty input
/// returns None — caller should NOT insert a context message in that case.
fn build_medication_context_message(
    meds: &[crate::medication_extraction::MedEntry],
) -> Option<String> {
    if meds.is_empty() {
        return None;
    }
    // Cap at 32 meds to bound token budget; very long lists are nearly always OCR noise.
    let lines: Vec<String> = meds
        .iter()
        .take(32)
        .map(|m| {
            let mut line = format!("- {}", m.name);
            if let Some(dose) = &m.dose {
                line.push(' ');
                line.push_str(dose);
            }
            if let Some(freq) = &m.frequency {
                line.push(' ');
                line.push_str(freq);
            }
            line
        })
        .collect();
    Some(format!(
        "Current medications (extracted from chart screenshot, clinician-reviewed):\n{}",
        lines.join("\n")
    ))
}

/// Send a message to the clinical assistant LLM
#[tauri::command]
pub async fn clinical_chat_send(
    llm_router_url: String,
    llm_api_key: String,
    llm_client_id: String,
    mut messages: Vec<ChatMessage>,
    current_medications: Option<Vec<crate::medication_extraction::MedEntry>>,
) -> Result<ClinicalChatResponse, super::CommandError> {
    use super::CommandError;
    // Med-list context goes at index 1 so the persona system prompt at
    // index 0 still anchors the conversation.
    if let Some(meds) = current_medications.as_ref() {
        if let Some(ctx) = build_medication_context_message(meds) {
            let insert_at = if messages.first().map(|m| m.role.as_str()) == Some("system") {
                1
            } else {
                0
            };
            messages.insert(
                insert_at,
                ChatMessage {
                    role: "system".to_string(),
                    content: ctx,
                },
            );
        }
    }

    info!(
        "Clinical chat: sending {} messages to {} (meds_attached={})",
        messages.len(),
        llm_router_url,
        current_medications.as_ref().map(|m| m.len()).unwrap_or(0)
    );

    if llm_router_url.is_empty() {
        return Err(CommandError::Config(
            "LLM Router URL is not configured".into(),
        ));
    }

    let start = std::time::Instant::now();

    // Extract the user's latest question and system prompt for logging
    let user_message = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let system_prompt = messages
        .iter()
        .find(|m| m.role == "system")
        .map(|m| m.content.clone())
        .unwrap_or_default();

    // Build headers
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    if !llm_api_key.is_empty() {
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", llm_api_key))
                .map_err(|e| CommandError::Validation(format!("Invalid API key: {}", e)))?,
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
        .map_err(|e| CommandError::Network(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .post(&url)
        .headers(headers)
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            error!("Clinical chat request failed: {}", e);
            if e.is_timeout() {
                CommandError::Network("Request timed out".into())
            } else if e.is_connect() {
                CommandError::Network(format!(
                    "Failed to connect to LLM router at {}: {}",
                    llm_router_url, e
                ))
            } else {
                CommandError::Network(format!("Request failed: {}", e))
            }
        })?;

    let status = response.status();
    debug!("Clinical chat response status: {}", status);

    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        error!("Clinical chat API error: {} - {}", status, error_text);
        log_chat_exchange(&ChatLogEntry {
            ts: chrono::Utc::now().to_rfc3339(),
            user_message,
            system_prompt,
            response: String::new(),
            tools_used: vec![],
            model: "clinical-assistant".to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            error: Some(format!("{} - {}", status, error_text)),
        });
        return Err(CommandError::Network(format!(
            "API error: {} - {}",
            status, error_text
        )));
    }

    let chat_response: ChatCompletionResponse = response
        .json()
        .await
        .map_err(|e| CommandError::Network(format!("Failed to parse response: {}", e)))?;

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

    log_chat_exchange(&ChatLogEntry {
        ts: chrono::Utc::now().to_rfc3339(),
        user_message,
        system_prompt,
        response: content.clone(),
        tools_used: tools_used.clone(),
        model: "clinical-assistant".to_string(),
        duration_ms: start.elapsed().as_millis() as u64,
        error: None,
    });

    Ok(ClinicalChatResponse {
        content,
        tools_used,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::medication_extraction::MedEntry;

    #[test]
    fn empty_meds_returns_none() {
        assert!(build_medication_context_message(&[]).is_none());
    }

    #[test]
    fn formats_one_med_per_line_with_optional_fields() {
        let meds = vec![
            MedEntry {
                name: "metformin".into(),
                dose: Some("500 mg".into()),
                frequency: Some("BID".into()),
            },
            MedEntry {
                name: "aspirin".into(),
                dose: None,
                frequency: None,
            },
        ];
        let ctx = build_medication_context_message(&meds).expect("context message");
        assert!(ctx.starts_with("Current medications"));
        assert!(ctx.contains("- metformin 500 mg BID"));
        assert!(ctx.contains("- aspirin"));
        assert!(ctx.contains("clinician-reviewed"));
    }

    #[test]
    fn caps_at_32_medications() {
        let meds: Vec<MedEntry> = (0..50)
            .map(|i| MedEntry {
                name: format!("drug{}", i),
                dose: None,
                frequency: None,
            })
            .collect();
        let ctx = build_medication_context_message(&meds).expect("context message");
        // 32 lines + header — anything past should be dropped.
        assert!(ctx.contains("drug0"));
        assert!(ctx.contains("drug31"));
        assert!(!ctx.contains("drug32"));
    }
}
