//! Archive command handlers for local session history

use crate::local_archive::{
    self, ArchiveDetails, ArchiveSummary,
};
use std::path::PathBuf;
use tracing::info;

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
