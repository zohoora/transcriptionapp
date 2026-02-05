//! Local Archive Module
//!
//! Production-ready local storage for session data. Unlike debug_storage (dev-only),
//! this module provides persistent local storage for the calendar/history view.
//!
//! Storage location: ~/.transcriptionapp/archive/YYYY/MM/DD/session_id/
//!
//! Files stored per session:
//! - metadata.json - Session metadata (timestamps, duration, etc.)
//! - transcript.txt - Full transcript text
//! - soap_note.txt - Generated SOAP note (if available)
//! - audio.wav - Recorded audio (if available)

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Get the archive base directory
pub fn get_archive_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    Ok(home.join(".transcriptionapp").join("archive"))
}

/// Get the date-based directory for a session
fn get_date_dir(date: &DateTime<Utc>) -> Result<PathBuf, String> {
    let base = get_archive_dir()?;
    Ok(base
        .join(format!("{:04}", date.year()))
        .join(format!("{:02}", date.month()))
        .join(format!("{:02}", date.day())))
}

/// Get the session-specific archive directory
pub fn get_session_archive_dir(session_id: &str, date: &DateTime<Utc>) -> Result<PathBuf, String> {
    let date_dir = get_date_dir(date)?;
    Ok(date_dir.join(session_id))
}

/// Ensure the archive directory exists for a session
fn ensure_session_dir(session_id: &str, date: &DateTime<Utc>) -> Result<PathBuf, String> {
    let dir = get_session_archive_dir(session_id, date)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create archive directory: {}", e))?;
    Ok(dir)
}

/// Session metadata for local archive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveMetadata {
    pub session_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_ms: Option<u64>,
    pub segment_count: usize,
    pub word_count: usize,
    pub has_soap_note: bool,
    pub has_audio: bool,
    pub auto_ended: bool,
    pub auto_end_reason: Option<String>,
    /// SOAP detail level used when generating (1-10), None if no SOAP or pre-feature
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_detail_level: Option<u8>,
    /// SOAP format used when generating, None if no SOAP or pre-feature
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_format: Option<String>,
    /// Charting mode that created this session ("session" or "continuous")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charting_mode: Option<String>,
    /// Encounter number within a continuous mode day (e.g., 3 for "Encounter #3")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_number: Option<u32>,
}

impl ArchiveMetadata {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            started_at: Utc::now().to_rfc3339(),
            ended_at: None,
            duration_ms: None,
            segment_count: 0,
            word_count: 0,
            has_soap_note: false,
            has_audio: false,
            auto_ended: false,
            auto_end_reason: None,
            soap_detail_level: None,
            soap_format: None,
            charting_mode: None,
            encounter_number: None,
        }
    }
}

/// Summary of an archived session (for list views)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveSummary {
    pub session_id: String,
    pub date: String,
    pub duration_ms: Option<u64>,
    pub word_count: usize,
    pub has_soap_note: bool,
    pub has_audio: bool,
    pub auto_ended: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charting_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_number: Option<u32>,
}

/// Detailed archived session (for detail view)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveDetails {
    pub session_id: String,
    pub metadata: ArchiveMetadata,
    pub transcript: Option<String>,
    pub soap_note: Option<String>,
    pub audio_path: Option<String>,
}

/// Archive a completed session
pub fn save_session(
    session_id: &str,
    transcript: &str,
    duration_ms: u64,
    audio_path: Option<&PathBuf>,
    auto_ended: bool,
    auto_end_reason: Option<&str>,
) -> Result<PathBuf, String> {
    let now = Utc::now();
    let session_dir = ensure_session_dir(session_id, &now)?;

    // Create metadata
    let word_count = transcript.split_whitespace().count();
    let mut metadata = ArchiveMetadata::new(session_id);
    metadata.ended_at = Some(now.to_rfc3339());
    metadata.duration_ms = Some(duration_ms);
    metadata.word_count = word_count;
    metadata.auto_ended = auto_ended;
    metadata.auto_end_reason = auto_end_reason.map(|s| s.to_string());

    // Save transcript
    let transcript_path = session_dir.join("transcript.txt");
    let mut file = File::create(&transcript_path)
        .map_err(|e| format!("Failed to create transcript file: {}", e))?;
    file.write_all(transcript.as_bytes())
        .map_err(|e| format!("Failed to write transcript: {}", e))?;

    info!(
        session_id = %session_id,
        words = word_count,
        "Transcript saved to archive"
    );

    // Copy audio file if provided
    if let Some(src_audio) = audio_path {
        if src_audio.exists() {
            let dest_audio = session_dir.join("audio.wav");
            if let Err(e) = fs::copy(src_audio, &dest_audio) {
                warn!("Failed to copy audio to archive: {}", e);
            } else {
                metadata.has_audio = true;
                info!(
                    session_id = %session_id,
                    "Audio copied to archive"
                );
            }
        }
    }

    // Save metadata
    let metadata_path = session_dir.join("metadata.json");
    let metadata_json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
    let mut file = File::create(&metadata_path)
        .map_err(|e| format!("Failed to create metadata file: {}", e))?;
    file.write_all(metadata_json.as_bytes())
        .map_err(|e| format!("Failed to write metadata: {}", e))?;

    info!(
        session_id = %session_id,
        path = %session_dir.display(),
        "Session archived successfully"
    );

    Ok(session_dir)
}

/// Add or update SOAP note for an archived session
pub fn add_soap_note(
    session_id: &str,
    date: &DateTime<Utc>,
    soap_content: &str,
    detail_level: Option<u8>,
    format: Option<&str>,
) -> Result<(), String> {
    let session_dir = get_session_archive_dir(session_id, date)?;

    if !session_dir.exists() {
        return Err(format!("Session archive not found: {}", session_id));
    }

    // Save SOAP note
    let soap_path = session_dir.join("soap_note.txt");
    let mut file = File::create(&soap_path)
        .map_err(|e| format!("Failed to create SOAP note file: {}", e))?;
    file.write_all(soap_content.as_bytes())
        .map_err(|e| format!("Failed to write SOAP note: {}", e))?;

    // Update metadata
    let metadata_path = session_dir.join("metadata.json");
    if metadata_path.exists() {
        let content = fs::read_to_string(&metadata_path)
            .map_err(|e| format!("Failed to read metadata: {}", e))?;
        let mut metadata: ArchiveMetadata = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse metadata: {}", e))?;

        metadata.has_soap_note = true;
        metadata.soap_detail_level = detail_level;
        metadata.soap_format = format.map(|s| s.to_string());

        let metadata_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
        fs::write(&metadata_path, metadata_json)
            .map_err(|e| format!("Failed to write metadata: {}", e))?;
    }

    info!(
        session_id = %session_id,
        detail_level = ?detail_level,
        format = ?format,
        "SOAP note added to archive"
    );

    Ok(())
}

/// List all dates that have archived sessions
pub fn list_session_dates() -> Result<Vec<String>, String> {
    let base = get_archive_dir()?;
    if !base.exists() {
        return Ok(Vec::new());
    }

    let mut dates = Vec::new();

    // Walk year directories
    for year_entry in fs::read_dir(&base).map_err(|e| format!("Failed to read archive: {}", e))? {
        let year_entry = year_entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let year_path = year_entry.path();
        if !year_path.is_dir() {
            continue;
        }

        // Walk month directories
        for month_entry in fs::read_dir(&year_path).map_err(|e| format!("Failed to read year dir: {}", e))? {
            let month_entry = month_entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let month_path = month_entry.path();
            if !month_path.is_dir() {
                continue;
            }

            // Walk day directories
            for day_entry in fs::read_dir(&month_path).map_err(|e| format!("Failed to read month dir: {}", e))? {
                let day_entry = day_entry.map_err(|e| format!("Failed to read entry: {}", e))?;
                let day_path = day_entry.path();
                if !day_path.is_dir() {
                    continue;
                }

                // Check if this day has any session directories
                let has_sessions = fs::read_dir(&day_path)
                    .map(|entries| entries.count() > 0)
                    .unwrap_or(false);

                if has_sessions {
                    if let (Some(year), Some(month), Some(day)) = (
                        year_path.file_name().and_then(|n| n.to_str()),
                        month_path.file_name().and_then(|n| n.to_str()),
                        day_path.file_name().and_then(|n| n.to_str()),
                    ) {
                        dates.push(format!("{}-{}-{}", year, month, day));
                    }
                }
            }
        }
    }

    // Sort descending (most recent first)
    dates.sort();
    dates.reverse();

    Ok(dates)
}

/// List sessions for a specific date
pub fn list_sessions_by_date(date_str: &str) -> Result<Vec<ArchiveSummary>, String> {
    // Parse date string (YYYY-MM-DD)
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date format: {}", e))?;

    let base = get_archive_dir()?;
    let date_dir = base
        .join(format!("{:04}", date.year()))
        .join(format!("{:02}", date.month()))
        .join(format!("{:02}", date.day()));

    if !date_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();

    for entry in fs::read_dir(&date_dir).map_err(|e| format!("Failed to read date dir: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let session_dir = entry.path();
        if !session_dir.is_dir() {
            continue;
        }

        let metadata_path = session_dir.join("metadata.json");
        if !metadata_path.exists() {
            continue;
        }

        let content = match fs::read_to_string(&metadata_path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read metadata: {}", e);
                continue;
            }
        };

        let metadata: ArchiveMetadata = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                warn!("Failed to parse metadata: {}", e);
                continue;
            }
        };

        sessions.push(ArchiveSummary {
            session_id: metadata.session_id,
            date: metadata.started_at,
            duration_ms: metadata.duration_ms,
            word_count: metadata.word_count,
            has_soap_note: metadata.has_soap_note,
            has_audio: metadata.has_audio,
            auto_ended: metadata.auto_ended,
            charting_mode: metadata.charting_mode,
            encounter_number: metadata.encounter_number,
        });
    }

    // Sort by date descending
    sessions.sort_by(|a, b| b.date.cmp(&a.date));

    Ok(sessions)
}

/// Get full details of an archived session
pub fn get_session(session_id: &str, date_str: &str) -> Result<ArchiveDetails, String> {
    // Parse date string (YYYY-MM-DD)
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date format: {}", e))?;

    let base = get_archive_dir()?;
    let session_dir = base
        .join(format!("{:04}", date.year()))
        .join(format!("{:02}", date.month()))
        .join(format!("{:02}", date.day()))
        .join(session_id);

    if !session_dir.exists() {
        return Err(format!("Session not found: {}", session_id));
    }

    // Load metadata
    let metadata_path = session_dir.join("metadata.json");
    let metadata_content = fs::read_to_string(&metadata_path)
        .map_err(|e| format!("Failed to read metadata: {}", e))?;
    let metadata: ArchiveMetadata = serde_json::from_str(&metadata_content)
        .map_err(|e| format!("Failed to parse metadata: {}", e))?;

    // Load transcript if exists
    let transcript_path = session_dir.join("transcript.txt");
    let transcript = if transcript_path.exists() {
        Some(fs::read_to_string(&transcript_path)
            .map_err(|e| format!("Failed to read transcript: {}", e))?)
    } else {
        None
    };

    // Load SOAP note if exists
    let soap_path = session_dir.join("soap_note.txt");
    let soap_note = if soap_path.exists() {
        Some(fs::read_to_string(&soap_path)
            .map_err(|e| format!("Failed to read SOAP note: {}", e))?)
    } else {
        None
    };

    // Get audio path if exists
    let audio_file = session_dir.join("audio.wav");
    let audio_path = if audio_file.exists() {
        Some(audio_file.to_string_lossy().to_string())
    } else {
        None
    };

    Ok(ArchiveDetails {
        session_id: session_id.to_string(),
        metadata,
        transcript,
        soap_note,
        audio_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archive_metadata_new() {
        let metadata = ArchiveMetadata::new("test-123");
        assert_eq!(metadata.session_id, "test-123");
        assert!(!metadata.has_soap_note);
        assert!(!metadata.has_audio);
        assert!(!metadata.auto_ended);
    }

    #[test]
    fn test_archive_summary_serialization() {
        let summary = ArchiveSummary {
            session_id: "test-123".to_string(),
            date: "2024-01-15T10:30:00Z".to_string(),
            duration_ms: Some(300000),
            word_count: 500,
            has_soap_note: true,
            has_audio: false,
            auto_ended: false,
            charting_mode: None,
            encounter_number: None,
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("test-123"));
        assert!(json.contains("300000"));
    }

    #[test]
    fn test_get_archive_dir() {
        let result = get_archive_dir();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.to_string_lossy().contains("archive"));
    }
}
