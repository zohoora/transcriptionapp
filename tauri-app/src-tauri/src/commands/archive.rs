//! Archive command handlers for local session history

use crate::local_archive::{
    self, ArchiveDetails, ArchiveSummary,
};
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
