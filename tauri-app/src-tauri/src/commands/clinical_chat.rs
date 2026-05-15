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

/// Hard cap on the free-form chart-context system message. Matches
/// `CLINICAL_CONTEXT_MAX_CHARS` in `medication_extraction.rs` (4000) so
/// vision-extracted content always fits, but the cap is enforced again
/// here in case the clinician hand-edits the textarea to something huge.
const CHART_CONTEXT_MAX_CHARS: usize = 4000;

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
///
/// PHI-aware: the persona `system_prompt` is logged verbatim (it's static,
/// no patient data), but the meds / patient / chart context messages are
/// recorded only via boolean flags + character counts so reviewers can
/// confirm context was attached without leaking content to disk.
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
    #[serde(default)]
    meds_attached: bool,
    #[serde(default)]
    meds_count: usize,
    #[serde(default)]
    patient_context_attached: bool,
    #[serde(default)]
    chart_context_attached: bool,
    #[serde(default)]
    chart_context_chars: usize,
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

/// Patient identity context passed from the sidebar's `useMedicationAssessment`
/// state. All three fields are optional so the frontend can send whatever it
/// has — empty struct (all None) is treated the same as no patient context
/// at all and produces no system message.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatientContext {
    #[serde(default)]
    pub name: Option<String>,
    pub dob: Option<String>,
    pub age: Option<i32>,
}

/// Build a single-line patient-identity system message. Returns None when
/// nothing identifying is present (all fields None/empty) — caller should
/// skip inserting a context message in that case.
fn build_patient_context_message(p: &PatientContext) -> Option<String> {
    let name = p.name.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let dob = p.dob.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let age = p.age;
    if name.is_none() && dob.is_none() && age.is_none() {
        return None;
    }
    let mut parts = Vec::with_capacity(2);
    if let Some(n) = name {
        parts.push(format!("Patient: {}", n));
    } else {
        parts.push("Patient context:".to_string());
    }
    let mut detail = Vec::with_capacity(2);
    if let Some(d) = dob {
        detail.push(format!("DOB {}", d));
    }
    if let Some(a) = age {
        detail.push(format!("age {}", a));
    }
    if !detail.is_empty() {
        parts.push(format!("({})", detail.join(", ")));
    }
    Some(parts.join(" "))
}

/// Build the free-form chart-context system message. Returns None when the
/// trimmed text is empty. Otherwise wraps the text in framing that makes
/// clear to the LLM (a) this was captured from the chart screen at chat-
/// open time and may be stale, and (b) it might not be relevant to the
/// clinician's question. Caps the body at [`CHART_CONTEXT_MAX_CHARS`] using
/// `ceil_char_boundary` (project convention).
fn build_chart_context_message(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let body = if trimmed.len() > CHART_CONTEXT_MAX_CHARS {
        &trimmed[..trimmed.ceil_char_boundary(CHART_CONTEXT_MAX_CHARS)]
    } else {
        trimmed
    };
    Some(format!(
        "The following clinical context was visible on the patient's chart screen \
         when this chat began. It may or may not be relevant to the conversation, \
         and the clinician may have updated it since the initial capture:\n\n{}",
        body
    ))
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

/// Insert a system-role message at `at` when `content` is non-empty. Returns
/// whether anything was inserted, so the caller can record an "attached"
/// flag in the chat log without re-checking the option.
fn insert_system_at(
    messages: &mut Vec<ChatMessage>,
    at: usize,
    content: Option<String>,
) -> bool {
    let Some(c) = content else { return false };
    messages.insert(
        at,
        ChatMessage {
            role: "system".to_string(),
            content: c,
        },
    );
    true
}

/// Send a message to the clinical assistant LLM.
///
/// Context-message ordering in the outgoing request, when present, is:
///   [0] persona (the frontend's static SYSTEM_PROMPT)
///   [1] medications (from `current_medications`)
///   [2] patient identity (from `current_patient`)
///   [3] chart context (from `chart_context`)
///   [...] conversation history
///   [N] latest user message
///
/// Each context slot is conditional on its param being non-empty. The three
/// inserts use the same `insert_at` index — each shifts the prior inserts
/// down, so we insert in REVERSE on-wire order (chart → patient → meds).
#[tauri::command]
pub async fn clinical_chat_send(
    llm_router_url: String,
    llm_api_key: String,
    llm_client_id: String,
    mut messages: Vec<ChatMessage>,
    current_medications: Option<Vec<crate::medication_extraction::MedEntry>>,
    current_patient: Option<PatientContext>,
    chart_context: Option<String>,
) -> Result<ClinicalChatResponse, super::CommandError> {
    use super::CommandError;
    let insert_at = if messages.first().map(|m| m.role.as_str()) == Some("system") {
        1
    } else {
        0
    };

    let chart_attached = insert_system_at(
        &mut messages,
        insert_at,
        chart_context
            .as_deref()
            .and_then(build_chart_context_message),
    );
    let patient_attached = insert_system_at(
        &mut messages,
        insert_at,
        current_patient.as_ref().and_then(build_patient_context_message),
    );
    insert_system_at(
        &mut messages,
        insert_at,
        current_medications
            .as_ref()
            .and_then(|m| build_medication_context_message(m)),
    );

    info!(
        "Clinical chat: sending {} messages to {} (meds_attached={} patient_attached={} chart_context_attached={})",
        messages.len(),
        llm_router_url,
        current_medications.as_ref().map(|m| m.len()).unwrap_or(0),
        patient_attached,
        chart_attached
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

    // Computed once and reused by both the error-path and success-path
    // log_chat_exchange calls below.
    let meds_count = current_medications
        .as_ref()
        .map(|v| v.len())
        .unwrap_or(0);
    let meds_attached_flag = meds_count > 0;
    let chart_context_chars = chart_context
        .as_deref()
        .map(|s| s.trim().chars().count())
        .unwrap_or(0);

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
            meds_attached: meds_attached_flag,
            meds_count,
            patient_context_attached: patient_attached,
            chart_context_attached: chart_attached,
            chart_context_chars,
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
        meds_attached: meds_attached_flag,
        meds_count,
        patient_context_attached: patient_attached,
        chart_context_attached: chart_attached,
        chart_context_chars,
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

    #[test]
    fn patient_context_all_none_returns_none() {
        let p = PatientContext::default();
        assert!(build_patient_context_message(&p).is_none());
    }

    #[test]
    fn patient_context_with_just_name() {
        let p = PatientContext {
            name: Some("Jane Doe".into()),
            dob: None,
            age: None,
        };
        let msg = build_patient_context_message(&p).expect("msg");
        assert!(msg.contains("Jane Doe"));
        assert!(!msg.contains("DOB"));
        assert!(!msg.contains("age"));
    }

    #[test]
    fn patient_context_with_name_dob_age() {
        let p = PatientContext {
            name: Some("Jane Doe".into()),
            dob: Some("1965-04-12".into()),
            age: Some(60),
        };
        let msg = build_patient_context_message(&p).expect("msg");
        assert!(msg.contains("Jane Doe"));
        assert!(msg.contains("DOB 1965-04-12"));
        assert!(msg.contains("age 60"));
    }

    #[test]
    fn patient_context_age_only_no_name_still_produces_message() {
        let p = PatientContext {
            name: None,
            dob: None,
            age: Some(78),
        };
        let msg = build_patient_context_message(&p).expect("msg");
        // No leaked name; just demographic detail.
        assert!(msg.contains("age 78"));
    }

    #[test]
    fn patient_context_whitespace_only_name_treated_as_empty() {
        let p = PatientContext {
            name: Some("   ".into()),
            dob: Some("1990-01-01".into()),
            age: None,
        };
        let msg = build_patient_context_message(&p).expect("msg");
        assert!(!msg.contains("   "));
        assert!(msg.contains("DOB 1990-01-01"));
    }

    #[test]
    fn chart_context_empty_returns_none() {
        assert!(build_chart_context_message("").is_none());
        assert!(build_chart_context_message("   \n  ").is_none());
    }

    #[test]
    fn chart_context_includes_framing_about_chat_start() {
        let msg = build_chart_context_message("CBC: WBC 12.4 (elevated)").expect("msg");
        assert!(msg.contains("chart screen when this chat began"));
        assert!(msg.contains("may or may not be relevant"));
        assert!(msg.contains("may have updated it since"));
        assert!(msg.contains("CBC: WBC 12.4 (elevated)"));
    }

    #[test]
    fn chart_context_truncates_at_cap_utf8_safe() {
        // ASCII overshoot — straight character count.
        let big = "a".repeat(CHART_CONTEXT_MAX_CHARS + 500);
        let msg = build_chart_context_message(&big).expect("msg");
        // Body length is bounded; framing adds a fixed prefix.
        // Just assert we didn't blow past the cap by an unreasonable amount.
        assert!(
            msg.len() < CHART_CONTEXT_MAX_CHARS + 300,
            "framing should not bloat past cap+prefix, got {}",
            msg.len()
        );

        // UTF-8: a multi-byte char straddling the boundary must not panic.
        let mut s = String::new();
        // ~3 bytes per char (é). Build something well past the cap.
        for _ in 0..(CHART_CONTEXT_MAX_CHARS) {
            s.push('é');
        }
        let _ = build_chart_context_message(&s).expect("must not panic on UTF-8 boundary");
    }
}
