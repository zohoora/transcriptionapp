//! Archive command handlers for local session history

use super::CommandError;
use crate::commands::{SharedActivePhysician, SharedProfileClient};
use crate::config::Config;
use crate::local_archive::{
    self, ArchiveDetails, ArchiveSummary, SessionFeedback,
};
use crate::ollama::LLMClient;
use crate::profile_client::ProfileClient;
use serde::Serialize;
use std::future::Future;
use std::path::PathBuf;
use tauri::State;
use tracing::{info, warn};

/// Clone the active physician ID and ProfileClient out of their locks,
/// then run the provided async closure if both are available.
/// The locks are released before any HTTP calls happen.
fn spawn_sync<F, Fut>(
    physician_arc: SharedActivePhysician,
    client_arc: SharedProfileClient,
    op_name: &'static str,
    f: F,
) where
    F: FnOnce(String, ProfileClient) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    tauri::async_runtime::spawn(async move {
        let physician_id = physician_arc.read().await.as_ref().map(|p| p.id.clone());
        let client = client_arc.read().await.clone();
        if let (Some(phys_id), Some(client)) = (physician_id, client) {
            f(phys_id, client).await;
        } else {
            warn!("Skipping server sync ({op_name}): no active physician or client");
        }
    });
}

/// Get all dates that have archived sessions.
/// Tries local first, then merges with server dates.
#[tauri::command]
pub async fn get_local_session_dates(
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<Vec<String>, CommandError> {
    info!("Getting local archive dates");

    // Start with local dates (fast path)
    let local_dates = local_archive::list_session_dates().unwrap_or_default();

    // Try to enrich with server dates (may include sessions from other machines)
    let physician_id = active_physician.read().await.as_ref().map(|p| p.id.clone());
    let client = profile_client.read().await.clone();
    if let (Some(phys_id), Some(client)) = (physician_id, client) {
        match client.get_session_dates(&phys_id, None, None).await {
            Ok(mut server_dates) => {
                // Merge: add any local dates not on server
                let server_set: std::collections::HashSet<String> =
                    server_dates.iter().cloned().collect();
                for d in &local_dates {
                    if !server_set.contains(d) {
                        server_dates.push(d.clone());
                    }
                }
                server_dates.sort();
                server_dates.dedup();
                return Ok(server_dates);
            }
            Err(e) => warn!("Server fetch failed, using local only: {e}"),
        }
    }

    Ok(local_dates)
}

/// Get sessions for a specific date.
/// Tries local first, then merges with server sessions.
#[tauri::command]
pub async fn get_local_sessions_by_date(
    date: String,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<Vec<ArchiveSummary>, CommandError> {
    info!("Getting local sessions for date: {}", date);

    // Start with local sessions (fast path)
    let local_sessions = local_archive::list_sessions_by_date(&date).unwrap_or_default();

    // Try to enrich with server sessions (may include sessions from other machines)
    let physician_id = active_physician.read().await.as_ref().map(|p| p.id.clone());
    let client = profile_client.read().await.clone();
    if let (Some(phys_id), Some(client)) = (physician_id, client) {
        match client.get_sessions_by_date(&phys_id, &date).await {
            Ok(mut server_sessions) => {
                // Merge: add any local sessions not on server
                let server_ids: std::collections::HashSet<String> =
                    server_sessions.iter().map(|s| s.session_id.clone()).collect();
                for local in &local_sessions {
                    if !server_ids.contains(&local.session_id) {
                        server_sessions.push(local.clone());
                    }
                }
                if !server_sessions.is_empty() {
                    // Re-sort merged list by started_at (server sessions may be unordered)
                    server_sessions.sort_by(|a, b| a.started_at.cmp(&b.started_at));
                    return Ok(server_sessions);
                }
            }
            Err(e) => warn!("Server fetch failed, using local only: {e}"),
        }
    }

    Ok(local_sessions)
}

/// Get full details of an archived session.
/// Tries local first (fast path), then server if not found locally.
#[tauri::command]
pub async fn get_local_session_details(
    session_id: String,
    date: String,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<ArchiveDetails, CommandError> {
    info!("Getting local session details: {} on {}", session_id, date);

    // Try local first (fast path)
    if let Ok(details) = local_archive::get_session(&session_id, &date) {
        return Ok(details);
    }

    // Try server (session may exist on a different machine)
    let physician_id = active_physician.read().await.as_ref().map(|p| p.id.clone());
    let client = profile_client.read().await.clone();
    if let (Some(phys_id), Some(client)) = (physician_id, client) {
        match client.get_session(&phys_id, &session_id).await {
            Ok(details) => return Ok(details),
            Err(e) => warn!("Server fetch also failed: {e}"),
        }
    }

    Err(CommandError::NotFound(format!(
        "Session {} not found locally or on server",
        session_id
    )))
}

/// Save SOAP note to an archived session
#[tauri::command]
pub fn save_local_soap_note(
    session_id: String,
    date: String,
    soap_content: String,
    detail_level: Option<u8>,
    format: Option<String>,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<(), CommandError> {
    info!(
        "Saving SOAP note to local archive: {} (detail: {:?}, format: {:?})",
        session_id, detail_level, format
    );

    // Parse date and convert to DateTime
    let naive_date = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|e| CommandError::Validation(format!("Invalid date format: {}", e)))?;
    let datetime = naive_date
        .and_hms_opt(12, 0, 0)
        .ok_or_else(|| CommandError::Validation("Invalid time".into()))?;
    let utc_datetime = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(datetime, chrono::Utc);

    local_archive::add_soap_note(
        &session_id,
        &utc_datetime,
        &soap_content,
        detail_level,
        format.as_deref(),
    )?;

    // Best-effort server sync
    let sid = session_id.clone();
    let soap = soap_content.clone();
    spawn_sync(
        active_physician.inner().clone(),
        profile_client.inner().clone(),
        "update_soap",
        move |phys_id, client| async move {
            let body = serde_json::json!({
                "content": soap,
                "detail_level": detail_level,
                "format": format,
            });
            if let Err(e) = client.update_soap(&phys_id, &sid, &body).await {
                warn!("Server sync failed (update_soap): {e}");
            }
        },
    );

    Ok(())
}

// ============================================================================
// Session Cleanup Commands
// ============================================================================

/// Delete a session from the local archive
#[tauri::command]
pub fn delete_local_session(
    session_id: String,
    date: String,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<(), CommandError> {
    info!("Deleting local session: {} on {}", session_id, date);
    local_archive::delete_session(&session_id, &date)?;

    let sid = session_id.clone();
    spawn_sync(
        active_physician.inner().clone(),
        profile_client.inner().clone(),
        "delete_session",
        move |phys_id, client| async move {
            if let Err(e) = client.delete_session(&phys_id, &sid).await {
                warn!("Server sync failed (delete_session): {e}");
            }
        },
    );

    Ok(())
}

/// Split a session at a line boundary, returning the new session ID
#[tauri::command]
pub fn split_local_session(
    session_id: String,
    date: String,
    split_line: usize,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<String, CommandError> {
    info!("Splitting local session: {} at line {}", session_id, split_line);
    let new_session_id = local_archive::split_session(&session_id, &date, split_line)?;

    // Best-effort server sync: upload both halves
    let original_sid = session_id.clone();
    let new_sid = new_session_id.clone();
    let date_clone = date.clone();
    spawn_sync(
        active_physician.inner().clone(),
        profile_client.inner().clone(),
        "split_session",
        move |phys_id, client| async move {
            // Upload updated original (first half)
            if let Ok(details) = local_archive::get_session(&original_sid, &date_clone) {
                if let Ok(body) = serde_json::to_value(&details) {
                    if let Err(e) = client.upload_session(&phys_id, &original_sid, &body).await {
                        warn!("Server sync failed (upload original half): {e}");
                    }
                }
            }
            // Upload new session (second half)
            if let Ok(details) = local_archive::get_session(&new_sid, &date_clone) {
                if let Ok(body) = serde_json::to_value(&details) {
                    if let Err(e) = client.upload_session(&phys_id, &new_sid, &body).await {
                        warn!("Server sync failed (upload new half): {e}");
                    }
                }
            }
        },
    );

    Ok(new_session_id)
}

/// Merge multiple sessions into one, returning the surviving session ID
#[tauri::command]
pub fn merge_local_sessions(
    session_ids: Vec<String>,
    date: String,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<String, CommandError> {
    info!("Merging {} local sessions on {}", session_ids.len(), date);
    let surviving_id = local_archive::merge_sessions(&session_ids, &date)?;

    let surv_id = surviving_id.clone();
    let consumed_ids: Vec<String> = session_ids
        .iter()
        .filter(|id| **id != surviving_id)
        .cloned()
        .collect();
    let date_clone = date.clone();
    spawn_sync(
        active_physician.inner().clone(),
        profile_client.inner().clone(),
        "merge_sessions",
        move |phys_id, client| async move {
            // Upload merged (surviving) session
            if let Ok(details) = local_archive::get_session(&surv_id, &date_clone) {
                if let Ok(body) = serde_json::to_value(&details) {
                    if let Err(e) = client.upload_session(&phys_id, &surv_id, &body).await {
                        warn!("Server sync failed (upload merged session): {e}");
                    }
                }
            }
            // Delete consumed sessions from server
            for consumed_id in &consumed_ids {
                if let Err(e) = client.delete_session(&phys_id, consumed_id).await {
                    warn!("Server sync failed (delete consumed session {consumed_id}): {e}");
                }
            }
        },
    );

    Ok(surviving_id)
}

/// Update the patient name for a session
#[tauri::command]
pub fn update_session_patient_name(
    session_id: String,
    date: String,
    patient_name: String,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<(), CommandError> {
    info!("Updating patient name for session: {}", session_id);
    local_archive::update_patient_name(&session_id, &date, &patient_name)?;

    let sid = session_id.clone();
    let name = patient_name.clone();
    spawn_sync(
        active_physician.inner().clone(),
        profile_client.inner().clone(),
        "update_patient_name",
        move |phys_id, client| async move {
            let body = serde_json::json!({ "patient_name": name });
            if let Err(e) = client.update_metadata(&phys_id, &sid, &body).await {
                warn!("Server sync failed (update_metadata patient_name): {e}");
            }
        },
    );

    Ok(())
}

/// Renumber encounter numbers for continuous mode sessions on a date
#[tauri::command]
pub fn renumber_local_encounters(
    date: String,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<(), CommandError> {
    info!("Renumbering encounters for date: {}", date);
    local_archive::renumber_encounters(&date)?;

    let date_clone = date.clone();
    spawn_sync(
        active_physician.inner().clone(),
        profile_client.inner().clone(),
        "renumber_encounters",
        move |phys_id, client| async move {
            // Re-read the sessions to get updated encounter numbers
            if let Ok(sessions) = local_archive::list_sessions_by_date(&date_clone) {
                for session in &sessions {
                    if let Some(enc_num) = session.encounter_number {
                        let body = serde_json::json!({ "encounter_number": enc_num });
                        if let Err(e) = client
                            .update_metadata(&phys_id, &session.session_id, &body)
                            .await
                        {
                            warn!(
                                "Server sync failed (renumber session {}): {e}",
                                session.session_id
                            );
                        }
                    }
                }
            }
        },
    );

    Ok(())
}

/// Get transcript lines for split UI
#[tauri::command]
pub fn get_session_transcript_lines(
    session_id: String,
    date: String,
) -> Result<Vec<String>, CommandError> {
    info!("Getting transcript lines for session: {}", session_id);
    Ok(local_archive::get_transcript_lines(&session_id, &date)?)
}

/// Read a local audio file and return its bytes
/// Used by the history window's audio player for locally-archived sessions
#[tauri::command]
pub fn read_local_audio_file(path: String) -> Result<Vec<u8>, CommandError> {
    let file_path = PathBuf::from(&path);
    if !file_path.exists() {
        return Err(CommandError::NotFound(format!("Audio file: {}", path)));
    }

    // Validate the path is within the archive directory to prevent arbitrary file reads
    let archive_dir = local_archive::get_archive_dir()
        .map_err(|e| CommandError::Io(e))?;
    if !file_path.starts_with(&archive_dir) {
        return Err(CommandError::Validation(
            "Access denied: path is outside the archive directory".into(),
        ));
    }

    info!("Reading local audio file: {}", path);
    std::fs::read(&file_path)
        .map_err(|e| CommandError::Io(format!("Failed to read audio file: {}", e)))
}

// ============================================================================
// Session Feedback Commands
// ============================================================================

/// Get feedback for a session
#[tauri::command]
pub fn get_session_feedback(
    session_id: String,
    date: String,
) -> Result<Option<SessionFeedback>, CommandError> {
    info!("Getting feedback for session: {} on {}", session_id, date);
    Ok(local_archive::read_feedback(&session_id, &date)?)
}

/// Save feedback for a session
#[tauri::command]
pub fn save_session_feedback(
    session_id: String,
    date: String,
    feedback: SessionFeedback,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<(), CommandError> {
    info!("Saving feedback for session: {} on {}", session_id, date);
    local_archive::write_feedback(&session_id, &date, &feedback)?;

    let sid = session_id.clone();
    let fb = feedback.clone();
    spawn_sync(
        active_physician.inner().clone(),
        profile_client.inner().clone(),
        "save_feedback",
        move |phys_id, client| async move {
            if let Ok(body) = serde_json::to_value(&fb) {
                if let Err(e) = client.update_metadata(&phys_id, &sid, &body).await {
                    warn!("Server sync failed (save_session_feedback): {e}");
                }
            }
        },
    );

    Ok(())
}

// ============================================================================
// LLM-Suggested Split Points
// ============================================================================

/// An LLM-suggested split point in a transcript
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedSplit {
    pub line_index: usize,
    pub confidence: f64,
    pub reason: String,
}

/// System prompt for split-point detection
const SPLIT_DETECTION_PROMPT: &str = r#"You MUST respond in English with ONLY a JSON object. No other text.

You are analyzing a clinical transcript that was recorded continuously in a medical office.
The transcript contains multiple patient encounters concatenated together.

Your task: identify where the FIRST complete patient encounter ends. Find the single best split point — the line where the first encounter wraps up and a new one is about to begin.

Signs the first encounter is ending:
- Farewell/wrap-up ("take care", "we'll see you in X weeks", "have a good day")
- New patient greeting/introduction after prior clinical discussion
- Clear topic shift from one patient's conditions to another's

Return the LINE NUMBER of the LAST line of the first encounter.

Return a JSON object (or empty object {} if no transition found):
{"line_index": <line number>, "confidence": <0.0-1.0>, "reason": "<brief explanation>"}

Respond with ONLY the JSON."#;

/// Parse the LLM response into suggested split points
pub fn parse_split_suggestions(response: &str, max_line: usize) -> Vec<SuggestedSplit> {
    // Strip markdown fences if present
    let trimmed = response.trim();
    let json_str = if trimmed.starts_with("```") {
        let inner = trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        inner
    } else {
        trimmed
    };

    #[derive(serde::Deserialize)]
    struct RawSuggestion {
        line_index: Option<usize>,
        confidence: Option<f64>,
        reason: Option<String>,
    }

    // Try parsing as a single object first, then fall back to array
    let raw_list: Vec<RawSuggestion> = if json_str.starts_with('{') {
        match serde_json::from_str::<RawSuggestion>(json_str) {
            Ok(s) => vec![s],
            Err(e) => {
                warn!("Failed to parse split suggestion JSON: {} — response: {}", e, &json_str[..json_str.len().min(200)]);
                return Vec::new();
            }
        }
    } else {
        match serde_json::from_str::<Vec<RawSuggestion>>(json_str) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse split suggestions JSON: {} — response: {}", e, &json_str[..json_str.len().min(200)]);
                return Vec::new();
            }
        }
    };

    let mut suggestions: Vec<SuggestedSplit> = raw_list
        .into_iter()
        .filter_map(|s| {
            let idx = s.line_index?;
            if idx > 0 && idx <= max_line {
                Some(SuggestedSplit {
                    line_index: idx,
                    confidence: s.confidence.unwrap_or(0.5).clamp(0.0, 1.0),
                    reason: s.reason.unwrap_or_default(),
                })
            } else {
                None
            }
        })
        .collect();

    suggestions.sort_by_key(|s| s.line_index);
    suggestions.dedup_by_key(|s| s.line_index);
    suggestions
}

/// Build the user prompt with numbered transcript lines
pub fn build_split_user_prompt(lines: &[String]) -> String {
    let mut prompt = String::with_capacity(lines.len() * 80);
    for (i, line) in lines.iter().enumerate() {
        prompt.push_str(&format!("{}: {}\n", i + 1, line));
    }
    prompt
}

/// Ask the LLM to suggest split points in a transcript
#[tauri::command]
pub async fn suggest_split_points(
    session_id: String,
    date: String,
) -> Result<Vec<SuggestedSplit>, CommandError> {
    info!("Suggesting split points for session: {} on {}", session_id, date);

    let lines = local_archive::get_transcript_lines(&session_id, &date)?;
    if lines.len() < 2 {
        return Ok(Vec::new());
    }

    let config = Config::load_or_default();
    let client = LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &config.fast_model,
    )?;

    let user_prompt = build_split_user_prompt(&lines);

    let response = client
        .generate(
            &config.fast_model,
            SPLIT_DETECTION_PROMPT,
            &user_prompt,
            "split_detection",
        )
        .await?;

    let suggestions = parse_split_suggestions(&response, lines.len());
    info!(
        "LLM suggested {} split points for session {}",
        suggestions.len(),
        session_id
    );

    Ok(suggestions)
}

/// Delete a single patient's SOAP from a multi-patient session
#[tauri::command]
pub async fn delete_patient_from_session(
    session_id: String,
    date: String,
    patient_index: u32,
) -> Result<(), CommandError> {
    local_archive::delete_patient_from_session(&session_id, &date, patient_index)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_split_user_prompt() {
        let lines = vec![
            "Doctor: Hello, how are you?".to_string(),
            "Patient: I'm doing well.".to_string(),
            "Doctor: Great, take care.".to_string(),
        ];
        let prompt = build_split_user_prompt(&lines);
        assert!(prompt.contains("1: Doctor: Hello, how are you?"));
        assert!(prompt.contains("2: Patient: I'm doing well."));
        assert!(prompt.contains("3: Doctor: Great, take care."));
    }

    #[test]
    fn test_parse_single_object() {
        let response = r#"{"line_index": 5, "confidence": 0.9, "reason": "Farewell detected"}"#;
        let suggestions = parse_split_suggestions(response, 10);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].line_index, 5);
        assert!((suggestions[0].confidence - 0.9).abs() < f64::EPSILON);
        assert_eq!(suggestions[0].reason, "Farewell detected");
    }

    #[test]
    fn test_parse_empty_object() {
        let response = "{}";
        let suggestions = parse_split_suggestions(response, 10);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_parse_array_fallback() {
        let response = r#"[{"line_index": 3, "confidence": 0.8, "reason": "Wrap-up"}]"#;
        let suggestions = parse_split_suggestions(response, 10);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].line_index, 3);
    }

    #[test]
    fn test_parse_empty_array() {
        let response = "[]";
        let suggestions = parse_split_suggestions(response, 10);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_parse_invalid_json() {
        let response = "not json at all";
        let suggestions = parse_split_suggestions(response, 10);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_parse_markdown_fences() {
        let response = "```json\n{\"line_index\": 4, \"confidence\": 0.7, \"reason\": \"Topic shift\"}\n```";
        let suggestions = parse_split_suggestions(response, 10);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].line_index, 4);
    }

    #[test]
    fn test_parse_filters_out_of_range() {
        let response = r#"{"line_index": 15, "confidence": 0.9, "reason": "Past end"}"#;
        let suggestions = parse_split_suggestions(response, 10);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_parse_clamps_confidence() {
        let response = r#"{"line_index": 5, "confidence": 1.5, "reason": "Over max"}"#;
        let suggestions = parse_split_suggestions(response, 10);
        assert_eq!(suggestions.len(), 1);
        assert!((suggestions[0].confidence - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_missing_optional_fields() {
        let response = r#"{"line_index": 5}"#;
        let suggestions = parse_split_suggestions(response, 10);
        assert_eq!(suggestions.len(), 1);
        assert!((suggestions[0].confidence - 0.5).abs() < f64::EPSILON);
        assert_eq!(suggestions[0].reason, "");
    }

    #[test]
    fn test_parse_zero_line_index_filtered() {
        let response = r#"{"line_index": 0, "confidence": 0.8, "reason": "Bad"}"#;
        let suggestions = parse_split_suggestions(response, 10);
        assert!(suggestions.is_empty());
    }
}
