//! Archive command handlers for local session history

use crate::config::Config;
use crate::local_archive::{
    self, ArchiveDetails, ArchiveSummary,
};
use crate::ollama::LLMClient;
use serde::Serialize;
use std::path::PathBuf;
use tracing::{info, warn};

/// Get all dates that have archived sessions
#[tauri::command]
pub fn get_local_session_dates() -> Result<Vec<String>, String> {
    info!("Getting local archive dates");
    local_archive::list_session_dates()
}

/// Get sessions for a specific date
#[tauri::command]
pub fn get_local_sessions_by_date(date: String) -> Result<Vec<ArchiveSummary>, String> {
    info!("Getting local sessions for date: {}", date);
    local_archive::list_sessions_by_date(&date)
}

/// Get full details of an archived session
#[tauri::command]
pub fn get_local_session_details(session_id: String, date: String) -> Result<ArchiveDetails, String> {
    info!("Getting local session details: {} on {}", session_id, date);
    local_archive::get_session(&session_id, &date)
}

/// Save SOAP note to an archived session
#[tauri::command]
pub fn save_local_soap_note(
    session_id: String,
    date: String,
    soap_content: String,
    detail_level: Option<u8>,
    format: Option<String>,
) -> Result<(), String> {
    info!(
        "Saving SOAP note to local archive: {} (detail: {:?}, format: {:?})",
        session_id, detail_level, format
    );

    // Parse date and convert to DateTime
    let naive_date = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date format: {}", e))?;
    let datetime = naive_date
        .and_hms_opt(12, 0, 0)
        .ok_or("Invalid time")?;
    let utc_datetime = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(datetime, chrono::Utc);

    local_archive::add_soap_note(
        &session_id,
        &utc_datetime,
        &soap_content,
        detail_level,
        format.as_deref(),
    )
}

// ============================================================================
// Session Cleanup Commands
// ============================================================================

/// Delete a session from the local archive
#[tauri::command]
pub fn delete_local_session(session_id: String, date: String) -> Result<(), String> {
    info!("Deleting local session: {} on {}", session_id, date);
    local_archive::delete_session(&session_id, &date)
}

/// Split a session at a line boundary, returning the new session ID
#[tauri::command]
pub fn split_local_session(
    session_id: String,
    date: String,
    split_line: usize,
) -> Result<String, String> {
    info!("Splitting local session: {} at line {}", session_id, split_line);
    local_archive::split_session(&session_id, &date, split_line)
}

/// Merge multiple sessions into one, returning the surviving session ID
#[tauri::command]
pub fn merge_local_sessions(
    session_ids: Vec<String>,
    date: String,
) -> Result<String, String> {
    info!("Merging {} local sessions on {}", session_ids.len(), date);
    local_archive::merge_sessions(&session_ids, &date)
}

/// Update the patient name for a session
#[tauri::command]
pub fn update_session_patient_name(
    session_id: String,
    date: String,
    patient_name: String,
) -> Result<(), String> {
    info!("Updating patient name for session: {}", session_id);
    local_archive::update_patient_name(&session_id, &date, &patient_name)
}

/// Renumber encounter numbers for continuous mode sessions on a date
#[tauri::command]
pub fn renumber_local_encounters(date: String) -> Result<(), String> {
    info!("Renumbering encounters for date: {}", date);
    local_archive::renumber_encounters(&date)
}

/// Get transcript lines for split UI
#[tauri::command]
pub fn get_session_transcript_lines(
    session_id: String,
    date: String,
) -> Result<Vec<String>, String> {
    info!("Getting transcript lines for session: {}", session_id);
    local_archive::get_transcript_lines(&session_id, &date)
}

/// Read a local audio file and return its bytes
/// Used by the history window's audio player for locally-archived sessions
#[tauri::command]
pub fn read_local_audio_file(path: String) -> Result<Vec<u8>, String> {
    let file_path = PathBuf::from(&path);
    if !file_path.exists() {
        return Err(format!("Audio file not found: {}", path));
    }

    // Validate the path is within the archive directory to prevent arbitrary file reads
    let archive_dir = local_archive::get_archive_dir()?;
    if !file_path.starts_with(&archive_dir) {
        return Err("Access denied: path is outside the archive directory".to_string());
    }

    info!("Reading local audio file: {}", path);
    std::fs::read(&file_path).map_err(|e| format!("Failed to read audio file: {}", e))
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
) -> Result<Vec<SuggestedSplit>, String> {
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
