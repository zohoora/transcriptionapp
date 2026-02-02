//! MIIS (Medical Illustration Image Server) commands
//!
//! Proxies HTTP requests to the MIIS server to avoid CORS issues
//! when the frontend tries to fetch images.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Concept for MIIS image search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiisConcept {
    pub text: String,
    #[serde(default = "default_weight")]
    pub weight: f64,
}

fn default_weight() -> f64 {
    1.0
}

/// Sanitize concept text for FTS5 compatibility
/// Removes special characters that break FTS5 syntax: - * " ( ) : / '
fn sanitize_fts5_text(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' ' // Replace special chars with space (including apostrophes)
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ") // Normalize whitespace
}

/// MIIS suggestion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiisSuggestion {
    pub image_id: i64,
    pub score: f64,
    pub sha256: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub thumb_url: String,
    pub display_url: String,
}

/// Response from MIIS suggest endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestResponse {
    pub suggestions: Vec<MiisSuggestion>,
    pub suggestion_set_id: String,
}

/// Telemetry event for MIIS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    pub image_id: i64,
    /// Event type: impression, click, open_full, print, dismiss
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: String,
}

/// Response from MIIS usage endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageResponse {
    pub accepted: i32,
    pub failed: i32,
}

/// Fetch image suggestions from MIIS server
#[tauri::command]
pub async fn miis_suggest(
    server_url: String,
    session_id: String,
    concepts: Vec<MiisConcept>,
) -> Result<SuggestResponse, String> {
    if server_url.is_empty() {
        return Err("MIIS server URL not configured".to_string());
    }

    if concepts.is_empty() {
        return Ok(SuggestResponse {
            suggestions: Vec::new(),
            suggestion_set_id: String::new(),
        });
    }

    // Sanitize concept text to avoid FTS5 syntax errors
    let sanitized_concepts: Vec<MiisConcept> = concepts
        .into_iter()
        .map(|c| MiisConcept {
            text: sanitize_fts5_text(&c.text),
            weight: c.weight,
        })
        .filter(|c| !c.text.is_empty()) // Remove empty concepts after sanitization
        .collect();

    if sanitized_concepts.is_empty() {
        return Ok(SuggestResponse {
            suggestions: Vec::new(),
            suggestion_set_id: String::new(),
        });
    }

    let url = format!("{}/v5/ambient/suggest", server_url.trim_end_matches('/'));
    info!("MIIS suggest request to {} with {} concepts: {:?}", url, sanitized_concepts.len(), sanitized_concepts);

    #[derive(Serialize)]
    struct SuggestRequest {
        session_id: String,
        concepts: Vec<MiisConcept>,
        limit: i32,
    }

    let request_body = SuggestRequest {
        session_id,
        concepts: sanitized_concepts,
        limit: 6,
    };

    // Log the request body for debugging
    let body_json = serde_json::to_string(&request_body).unwrap_or_default();
    info!("MIIS request body: {}", body_json);

    let client = Client::new();
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("MIIS request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        warn!("MIIS suggest failed: {} - body: {}", status, body);
        return Err(format!("MIIS server error: {} - {}", status, body));
    }

    let result: SuggestResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse MIIS response: {}", e))?;

    info!(
        "MIIS returned {} suggestions (set_id: {})",
        result.suggestions.len(),
        result.suggestion_set_id
    );

    Ok(result)
}

/// Send usage telemetry to MIIS server
#[tauri::command]
pub async fn miis_send_usage(
    server_url: String,
    session_id: String,
    suggestion_set_id: Option<String>,
    events: Vec<UsageEvent>,
) -> Result<UsageResponse, String> {
    if server_url.is_empty() {
        return Err("MIIS server URL not configured".to_string());
    }

    if events.is_empty() {
        return Ok(UsageResponse {
            accepted: 0,
            failed: 0,
        });
    }

    let url = format!("{}/v5/usage", server_url.trim_end_matches('/'));
    info!("MIIS usage request to {} with {} events", url, events.len());

    #[derive(Serialize)]
    struct UsageRequest {
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        suggestion_set_id: Option<String>,
        events: Vec<UsageEvent>,
    }

    let request_body = UsageRequest {
        session_id,
        suggestion_set_id,
        events,
    };

    let client = Client::new();
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("MIIS usage request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        warn!("MIIS usage failed: {}", status);
        return Err(format!("MIIS server error: {}", status));
    }

    let result: UsageResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse MIIS usage response: {}", e))?;

    debug!("MIIS usage: accepted={}, failed={}", result.accepted, result.failed);

    Ok(result)
}
