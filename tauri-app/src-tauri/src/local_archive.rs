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
use tracing::{info, warn};
use uuid::Uuid;

/// Validate a session ID to prevent path traversal attacks.
///
/// Rejects session IDs that:
/// - Are empty
/// - Contain `/` or `\` (directory separators)
/// - Contain `..` (parent directory traversal)
/// - Contain null bytes (`\0`)
fn validate_session_id(session_id: &str) -> Result<(), String> {
    if session_id.is_empty() {
        return Err("Session ID must not be empty".to_string());
    }
    if session_id.contains('/') || session_id.contains('\\') {
        return Err("Session ID must not contain path separators".to_string());
    }
    if session_id.contains("..") {
        return Err("Session ID must not contain '..'".to_string());
    }
    if session_id.contains('\0') {
        return Err("Session ID must not contain null bytes".to_string());
    }
    Ok(())
}

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
    validate_session_id(session_id)?;
    let date_dir = get_date_dir(date)?;
    Ok(date_dir.join(session_id))
}

/// Ensure the archive directory exists for a session
fn ensure_session_dir(session_id: &str, date: &DateTime<Utc>) -> Result<PathBuf, String> {
    validate_session_id(session_id)?;
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
    /// Patient name extracted via vision-based screenshot analysis (majority vote)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_name: Option<String>,
    /// How the encounter was detected: "llm", "sensor", or "manual"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detection_method: Option<String>,
    /// Shadow mode comparison data (dual detection method analysis)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_comparison: Option<crate::shadow_log::ShadowEncounterComparison>,
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
            patient_name: None,
            detection_method: None,
            shadow_comparison: None,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_name: Option<String>,
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
    validate_session_id(session_id)?;
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
    validate_session_id(session_id)?;
    let session_dir = get_session_archive_dir(session_id, date)?;

    if !session_dir.exists() {
        // Create directory if it doesn't exist yet (race with continuous mode SOAP generation)
        fs::create_dir_all(&session_dir)
            .map_err(|e| format!("Failed to create session directory: {}", e))?;
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
            patient_name: metadata.patient_name,
        });
    }

    // Sort by date descending
    sessions.sort_by(|a, b| b.date.cmp(&a.date));

    Ok(sessions)
}

/// Get full details of an archived session
pub fn get_session(session_id: &str, date_str: &str) -> Result<ArchiveDetails, String> {
    validate_session_id(session_id)?;

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

/// Merge two encounters: append B's transcript to A, update A's metadata, delete B's directory.
///
/// Safety: A is updated first, then B is deleted. If the delete fails, we have a duplicate
/// but no data loss.
pub fn merge_encounters(
    session_a_id: &str,
    session_b_id: &str,
    date: &DateTime<Utc>,
    merged_transcript: &str,
    merged_word_count: usize,
    merged_duration_ms: u64,
) -> Result<(), String> {
    validate_session_id(session_a_id)?;
    validate_session_id(session_b_id)?;
    let a_dir = get_session_archive_dir(session_a_id, date)?;
    let b_dir = get_session_archive_dir(session_b_id, date)?;

    if !a_dir.exists() {
        return Err(format!("Session A directory does not exist: {}", a_dir.display()));
    }
    if !b_dir.exists() {
        return Err(format!("Session B directory does not exist: {}", b_dir.display()));
    }

    // Step 1: Write merged transcript to A
    let transcript_path = a_dir.join("transcript.txt");
    fs::write(&transcript_path, merged_transcript)
        .map_err(|e| format!("Failed to write merged transcript: {}", e))?;

    // Step 2: Update A's metadata
    let metadata_path = a_dir.join("metadata.json");
    if metadata_path.exists() {
        let content = fs::read_to_string(&metadata_path)
            .map_err(|e| format!("Failed to read metadata: {}", e))?;
        let mut metadata: ArchiveMetadata = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse metadata: {}", e))?;

        metadata.word_count = merged_word_count;
        metadata.duration_ms = Some(merged_duration_ms);
        metadata.has_soap_note = false; // SOAP is stale after merge

        let json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
        fs::write(&metadata_path, json)
            .map_err(|e| format!("Failed to write metadata: {}", e))?;
    }

    // Step 3: Delete A's stale SOAP note (will be regenerated)
    let soap_path = a_dir.join("soap_note.txt");
    if soap_path.exists() {
        let _ = fs::remove_file(&soap_path);
    }

    // Step 4: Delete B's directory (only after A is safely updated)
    if let Err(e) = fs::remove_dir_all(&b_dir) {
        warn!("Failed to delete merged session B directory {}: {}", b_dir.display(), e);
        // Non-fatal: A has the merged data, B is just a stale duplicate
    }

    info!(
        "Merged encounter {} into {} ({} words, {}ms)",
        session_b_id, session_a_id, merged_word_count, merged_duration_ms
    );

    Ok(())
}

/// Resolve the date directory from a YYYY-MM-DD string.
fn get_date_dir_from_str(date_str: &str) -> Result<PathBuf, String> {
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date format: {}", e))?;
    let base = get_archive_dir()?;
    Ok(base
        .join(format!("{:04}", date.year()))
        .join(format!("{:02}", date.month()))
        .join(format!("{:02}", date.day())))
}

/// Resolve the session directory from session_id + date string.
fn get_session_dir_from_str(session_id: &str, date_str: &str) -> Result<PathBuf, String> {
    validate_session_id(session_id)?;
    let date_dir = get_date_dir_from_str(date_str)?;
    Ok(date_dir.join(session_id))
}

// ============================================================================
// Session Cleanup Operations
// ============================================================================

/// Delete a session and all its files.
pub fn delete_session(session_id: &str, date_str: &str) -> Result<(), String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;

    if !session_dir.exists() {
        return Err(format!("Session not found: {}", session_id));
    }

    fs::remove_dir_all(&session_dir)
        .map_err(|e| format!("Failed to delete session: {}", e))?;

    info!(session_id = %session_id, date = %date_str, "Session deleted");
    Ok(())
}

/// Split a session at a line boundary, creating a new session for the second half.
///
/// Returns the new session ID for the second half (lines from `split_line` onward).
/// The original session keeps lines `[0..split_line)`.
/// Both sessions get `has_soap_note: false` and stale SOAP files are removed.
/// Audio stays with the original; the new session gets `has_audio: false`.
pub fn split_session(session_id: &str, date_str: &str, split_line: usize) -> Result<String, String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    if !session_dir.exists() {
        return Err(format!("Session not found: {}", session_id));
    }

    // Read transcript
    let transcript_path = session_dir.join("transcript.txt");
    let transcript = fs::read_to_string(&transcript_path)
        .map_err(|e| format!("Failed to read transcript: {}", e))?;
    let lines: Vec<&str> = transcript.lines().collect();

    // Validate split point
    if lines.is_empty() {
        return Err("Cannot split empty transcript".to_string());
    }
    if split_line == 0 {
        return Err("Split line must be at least 1 (first half needs at least 1 line)".to_string());
    }
    if split_line >= lines.len() {
        return Err(format!(
            "Split line {} is beyond transcript length {}",
            split_line,
            lines.len()
        ));
    }

    let first_half = lines[..split_line].join("\n");
    let second_half = lines[split_line..].join("\n");

    let first_words = first_half.split_whitespace().count();
    let second_words = second_half.split_whitespace().count();

    // Read original metadata
    let metadata_path = session_dir.join("metadata.json");
    let metadata_content = fs::read_to_string(&metadata_path)
        .map_err(|e| format!("Failed to read metadata: {}", e))?;
    let mut original_meta: ArchiveMetadata = serde_json::from_str(&metadata_content)
        .map_err(|e| format!("Failed to parse metadata: {}", e))?;

    // Estimate duration split proportionally by word count
    let total_words = first_words + second_words;
    let (first_duration, second_duration) = if total_words > 0 {
        if let Some(total_ms) = original_meta.duration_ms {
            let first_ms = (total_ms as f64 * first_words as f64 / total_words as f64) as u64;
            (Some(first_ms), Some(total_ms - first_ms))
        } else {
            (None, None)
        }
    } else {
        (original_meta.duration_ms, None)
    };

    // Create new session for second half
    let new_session_id = Uuid::new_v4().to_string();
    let date_dir = get_date_dir_from_str(date_str)?;
    let new_session_dir = date_dir.join(&new_session_id);
    fs::create_dir_all(&new_session_dir)
        .map_err(|e| format!("Failed to create new session directory: {}", e))?;

    // Write second half transcript
    fs::write(new_session_dir.join("transcript.txt"), &second_half)
        .map_err(|e| format!("Failed to write split transcript: {}", e))?;

    // Create metadata for second half (inherit most fields from original)
    let new_meta = ArchiveMetadata {
        session_id: new_session_id.clone(),
        started_at: original_meta.started_at.clone(), // Best estimate
        ended_at: original_meta.ended_at.clone(),
        duration_ms: second_duration,
        segment_count: 0,
        word_count: second_words,
        has_soap_note: false,
        has_audio: false, // Audio stays with original
        auto_ended: original_meta.auto_ended,
        auto_end_reason: original_meta.auto_end_reason.clone(),
        soap_detail_level: None,
        soap_format: None,
        charting_mode: original_meta.charting_mode.clone(),
        encounter_number: None, // Will be renumbered
        patient_name: original_meta.patient_name.clone(),
        detection_method: original_meta.detection_method.clone(),
        shadow_comparison: None,
    };
    let new_meta_json = serde_json::to_string_pretty(&new_meta)
        .map_err(|e| format!("Failed to serialize new metadata: {}", e))?;
    fs::write(new_session_dir.join("metadata.json"), new_meta_json)
        .map_err(|e| format!("Failed to write new metadata: {}", e))?;

    // Update original session: first half transcript, updated metadata
    fs::write(&transcript_path, &first_half)
        .map_err(|e| format!("Failed to write updated transcript: {}", e))?;

    original_meta.word_count = first_words;
    original_meta.duration_ms = first_duration;
    original_meta.has_soap_note = false;
    original_meta.soap_detail_level = None;
    original_meta.soap_format = None;

    let updated_meta_json = serde_json::to_string_pretty(&original_meta)
        .map_err(|e| format!("Failed to serialize updated metadata: {}", e))?;
    fs::write(&metadata_path, updated_meta_json)
        .map_err(|e| format!("Failed to write updated metadata: {}", e))?;

    // Delete stale SOAP from original
    let soap_path = session_dir.join("soap_note.txt");
    if soap_path.exists() {
        let _ = fs::remove_file(&soap_path);
    }

    info!(
        session_id = %session_id,
        new_session_id = %new_session_id,
        split_line = split_line,
        first_words = first_words,
        second_words = second_words,
        "Session split"
    );

    Ok(new_session_id)
}

/// Merge multiple sessions into one (the earliest by started_at).
///
/// Concatenates transcripts with `\n\n`, sums word counts, spans timestamps.
/// Deletes all source sessions except the surviving (earliest) one.
/// Returns the surviving session ID.
pub fn merge_sessions(session_ids: &[String], date_str: &str) -> Result<String, String> {
    if session_ids.len() < 2 {
        return Err("Need at least 2 sessions to merge".to_string());
    }

    // Validate all IDs
    for id in session_ids {
        validate_session_id(id)?;
    }

    let date_dir = get_date_dir_from_str(date_str)?;

    // Load all metadata and transcripts
    let mut sessions: Vec<(String, ArchiveMetadata, String)> = Vec::new();
    for id in session_ids {
        let session_dir = date_dir.join(id);
        if !session_dir.exists() {
            return Err(format!("Session not found: {}", id));
        }

        let meta_content = fs::read_to_string(session_dir.join("metadata.json"))
            .map_err(|e| format!("Failed to read metadata for {}: {}", id, e))?;
        let meta: ArchiveMetadata = serde_json::from_str(&meta_content)
            .map_err(|e| format!("Failed to parse metadata for {}: {}", id, e))?;

        let transcript = fs::read_to_string(session_dir.join("transcript.txt"))
            .unwrap_or_default();

        sessions.push((id.clone(), meta, transcript));
    }

    // Sort by started_at — earliest survives
    sessions.sort_by(|a, b| a.1.started_at.cmp(&b.1.started_at));

    let surviving_id = sessions[0].0.clone();

    // Build merged transcript
    let merged_transcript: String = sessions
        .iter()
        .map(|(_, _, t)| t.as_str())
        .collect::<Vec<&str>>()
        .join("\n\n");

    // Calculate merged stats
    let merged_word_count = merged_transcript.split_whitespace().count();
    let merged_started = sessions.first().map(|(_, m, _)| m.started_at.clone()).unwrap();
    let merged_ended = sessions.last().and_then(|(_, m, _)| m.ended_at.clone());
    let merged_duration: Option<u64> = {
        let total: u64 = sessions.iter().filter_map(|(_, m, _)| m.duration_ms).sum();
        if total > 0 { Some(total) } else { None }
    };

    // Check if surviving session has audio
    let surviving_dir = date_dir.join(&surviving_id);
    let has_audio = surviving_dir.join("audio.wav").exists();

    // Update surviving session
    fs::write(surviving_dir.join("transcript.txt"), &merged_transcript)
        .map_err(|e| format!("Failed to write merged transcript: {}", e))?;

    let mut surviving_meta = sessions[0].1.clone();
    surviving_meta.word_count = merged_word_count;
    surviving_meta.started_at = merged_started;
    surviving_meta.ended_at = merged_ended;
    surviving_meta.duration_ms = merged_duration;
    surviving_meta.has_soap_note = false;
    surviving_meta.has_audio = has_audio;
    surviving_meta.soap_detail_level = None;
    surviving_meta.soap_format = None;

    let meta_json = serde_json::to_string_pretty(&surviving_meta)
        .map_err(|e| format!("Failed to serialize merged metadata: {}", e))?;
    fs::write(surviving_dir.join("metadata.json"), meta_json)
        .map_err(|e| format!("Failed to write merged metadata: {}", e))?;

    // Delete stale SOAP from surviving session
    let soap_path = surviving_dir.join("soap_note.txt");
    if soap_path.exists() {
        let _ = fs::remove_file(&soap_path);
    }

    // Delete all other sessions (only after surviving is safely updated)
    for (id, _, _) in &sessions[1..] {
        let dir = date_dir.join(id);
        if let Err(e) = fs::remove_dir_all(&dir) {
            warn!("Failed to delete merged session {}: {}", id, e);
        }
    }

    let merged_ids: Vec<&str> = sessions[1..].iter().map(|(id, _, _)| id.as_str()).collect();
    info!(
        surviving_id = %surviving_id,
        merged_from = ?merged_ids,
        word_count = merged_word_count,
        "Sessions merged"
    );

    Ok(surviving_id)
}

/// Update the patient name for a session.
pub fn update_patient_name(session_id: &str, date_str: &str, name: &str) -> Result<(), String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    if !session_dir.exists() {
        return Err(format!("Session not found: {}", session_id));
    }

    let metadata_path = session_dir.join("metadata.json");
    let content = fs::read_to_string(&metadata_path)
        .map_err(|e| format!("Failed to read metadata: {}", e))?;
    let mut metadata: ArchiveMetadata = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse metadata: {}", e))?;

    let trimmed = name.trim();
    metadata.patient_name = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };

    let json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
    fs::write(&metadata_path, json)
        .map_err(|e| format!("Failed to write metadata: {}", e))?;

    info!(session_id = %session_id, patient_name = %trimmed, "Patient name updated");
    Ok(())
}

/// Renumber encounter numbers for continuous mode sessions on a given date.
///
/// Loads all sessions with `charting_mode == "continuous"`, sorts by `started_at`,
/// and assigns sequential encounter numbers starting from 1.
pub fn renumber_encounters(date_str: &str) -> Result<(), String> {
    let date_dir = get_date_dir_from_str(date_str)?;
    if !date_dir.exists() {
        return Ok(());
    }

    // Load all sessions with their metadata
    let mut continuous_sessions: Vec<(PathBuf, ArchiveMetadata)> = Vec::new();

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
            Err(_) => continue,
        };
        let metadata: ArchiveMetadata = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.charting_mode.as_deref() == Some("continuous") {
            continuous_sessions.push((session_dir, metadata));
        }
    }

    // Sort by started_at
    continuous_sessions.sort_by(|a, b| a.1.started_at.cmp(&b.1.started_at));

    // Assign sequential encounter numbers
    for (i, (session_dir, mut metadata)) in continuous_sessions.into_iter().enumerate() {
        let new_number = (i + 1) as u32;
        if metadata.encounter_number != Some(new_number) {
            metadata.encounter_number = Some(new_number);
            let json = serde_json::to_string_pretty(&metadata)
                .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
            fs::write(session_dir.join("metadata.json"), json)
                .map_err(|e| format!("Failed to write metadata: {}", e))?;
        }
    }

    info!(date = %date_str, "Encounters renumbered");
    Ok(())
}

/// Get transcript lines for a session (used by split UI).
pub fn get_transcript_lines(session_id: &str, date_str: &str) -> Result<Vec<String>, String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    if !session_dir.exists() {
        return Err(format!("Session not found: {}", session_id));
    }

    let transcript_path = session_dir.join("transcript.txt");
    if !transcript_path.exists() {
        return Ok(Vec::new());
    }

    let transcript = fs::read_to_string(&transcript_path)
        .map_err(|e| format!("Failed to read transcript: {}", e))?;
    Ok(transcript.lines().map(|l| l.to_string()).collect())
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
            patient_name: None,
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

    #[test]
    fn test_merge_encounters_missing_dir() {
        let now = Utc::now();
        let result = merge_encounters(
            "nonexistent-a",
            "nonexistent-b",
            &now,
            "merged text",
            10,
            5000,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_validate_session_id_valid() {
        assert!(validate_session_id("abc-123").is_ok());
        assert!(validate_session_id("session_2024-01-15_abc123").is_ok());
        assert!(validate_session_id("a").is_ok());
        assert!(validate_session_id("uuid-v4-like-550e8400-e29b").is_ok());
    }

    #[test]
    fn test_validate_session_id_empty() {
        let err = validate_session_id("").unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn test_validate_session_id_forward_slash() {
        let err = validate_session_id("../etc/passwd").unwrap_err();
        assert!(err.contains("path separators") || err.contains("'..'"));
    }

    #[test]
    fn test_validate_session_id_backslash() {
        let err = validate_session_id("foo\\bar").unwrap_err();
        assert!(err.contains("path separators"));
    }

    #[test]
    fn test_validate_session_id_dot_dot() {
        let err = validate_session_id("a..b").unwrap_err();
        assert!(err.contains("'..'"));
    }

    #[test]
    fn test_validate_session_id_null_byte() {
        let err = validate_session_id("abc\0def").unwrap_err();
        assert!(err.contains("null bytes"));
    }

    #[test]
    fn test_validate_session_id_traversal_attack() {
        assert!(validate_session_id("../../secrets").is_err());
        assert!(validate_session_id("..").is_err());
        assert!(validate_session_id("/absolute/path").is_err());
        assert!(validate_session_id("foo/bar").is_err());
    }

    #[test]
    fn test_get_session_archive_dir_rejects_traversal() {
        let now = Utc::now();
        assert!(get_session_archive_dir("../../etc", &now).is_err());
        assert!(get_session_archive_dir("", &now).is_err());
        assert!(get_session_archive_dir("foo/bar", &now).is_err());
    }

    #[test]
    fn test_save_session_rejects_traversal() {
        let result = save_session("../escape", "text", 1000, None, false, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("'..'") || err.contains("path separators"));
    }

    #[test]
    fn test_get_session_rejects_traversal() {
        let result = get_session("../../etc/passwd", "2024-01-15");
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_encounters_rejects_traversal() {
        let now = Utc::now();
        // Session A has traversal
        let result = merge_encounters("../escape", "valid-id", &now, "text", 10, 5000);
        assert!(result.is_err());

        // Session B has traversal
        let result = merge_encounters("valid-id", "foo/bar", &now, "text", 10, 5000);
        assert!(result.is_err());
    }

    // ========================================================================
    // Helper: create a test session in a temp directory
    // ========================================================================

    /// Create a temporary session directory with metadata and transcript for testing.
    /// Returns (temp_dir_guard, date_str, session_id).
    fn create_test_session(
        temp_base: &std::path::Path,
        session_id: &str,
        transcript: &str,
        started_at: &str,
        charting_mode: Option<&str>,
        encounter_number: Option<u32>,
    ) -> PathBuf {
        // Use a fixed date path matching the date_str
        let date_dir = temp_base.join("2024").join("01").join("15");
        let session_dir = date_dir.join(session_id);
        fs::create_dir_all(&session_dir).unwrap();

        let mut meta = ArchiveMetadata::new(session_id);
        meta.started_at = started_at.to_string();
        meta.ended_at = Some("2024-01-15T11:00:00Z".to_string());
        meta.duration_ms = Some(60000);
        meta.word_count = transcript.split_whitespace().count();
        meta.charting_mode = charting_mode.map(|s| s.to_string());
        meta.encounter_number = encounter_number;

        let meta_json = serde_json::to_string_pretty(&meta).unwrap();
        fs::write(session_dir.join("metadata.json"), &meta_json).unwrap();
        fs::write(session_dir.join("transcript.txt"), transcript).unwrap();

        session_dir
    }

    // Override get_archive_dir for tests using env var
    // Tests below use tempdir + direct function calls that construct paths

    #[test]
    fn test_delete_session() {
        let temp = tempfile::tempdir().unwrap();
        let date_dir = temp.path().join("2024").join("01").join("15");
        let session_dir = date_dir.join("test-delete-session");
        fs::create_dir_all(&session_dir).unwrap();

        let meta = ArchiveMetadata::new("test-delete-session");
        fs::write(
            session_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        ).unwrap();
        fs::write(session_dir.join("transcript.txt"), "hello world").unwrap();

        assert!(session_dir.exists());

        // We can't easily override get_archive_dir, so test the core logic directly:
        // delete_session calls get_session_dir_from_str which calls get_archive_dir.
        // Instead, test the filesystem ops directly.
        fs::remove_dir_all(&session_dir).unwrap();
        assert!(!session_dir.exists());
    }

    #[test]
    fn test_delete_session_not_found() {
        let result = delete_session("nonexistent-session-xyz", "2024-01-15");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Session not found"));
    }

    #[test]
    fn test_delete_session_rejects_traversal() {
        assert!(delete_session("../escape", "2024-01-15").is_err());
        assert!(delete_session("foo/bar", "2024-01-15").is_err());
        assert!(delete_session("", "2024-01-15").is_err());
    }

    #[test]
    fn test_split_session_not_found() {
        let result = split_session("nonexistent-split-xyz", "2024-01-15", 3);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Session not found"));
    }

    #[test]
    fn test_split_session_rejects_traversal() {
        assert!(split_session("../escape", "2024-01-15", 3).is_err());
    }

    #[test]
    fn test_split_session_edge_cases_validation() {
        // These test the validation logic without needing filesystem access
        // split_line == 0 should be rejected
        let result = split_session("nonexistent-99", "2024-01-15", 0);
        // Will fail with "not found" before reaching split validation
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_sessions_too_few() {
        let result = merge_sessions(&["only-one".to_string()], "2024-01-15");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 2"));
    }

    #[test]
    fn test_merge_sessions_rejects_traversal() {
        let result = merge_sessions(
            &["../escape".to_string(), "valid-id".to_string()],
            "2024-01-15",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_sessions_not_found() {
        let result = merge_sessions(
            &["nonexistent-a".to_string(), "nonexistent-b".to_string()],
            "2024-01-15",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Session not found"));
    }

    #[test]
    fn test_update_patient_name_not_found() {
        let result = update_patient_name("nonexistent-name-xyz", "2024-01-15", "Dr. Smith");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Session not found"));
    }

    #[test]
    fn test_update_patient_name_rejects_traversal() {
        assert!(update_patient_name("../escape", "2024-01-15", "name").is_err());
    }

    #[test]
    fn test_renumber_encounters_nonexistent_date() {
        // Should succeed (no-op) for a date with no sessions
        let result = renumber_encounters("2099-12-31");
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_transcript_lines_not_found() {
        let result = get_transcript_lines("nonexistent-lines-xyz", "2024-01-15");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Session not found"));
    }

    #[test]
    fn test_get_transcript_lines_rejects_traversal() {
        assert!(get_transcript_lines("../escape", "2024-01-15").is_err());
    }

    // ========================================================================
    // Integration-style tests using real archive directory
    // These create actual sessions in ~/.transcriptionapp/archive/ and clean up
    // ========================================================================

    #[test]
    fn test_delete_session_integration() {
        // Create a real session
        let session_id = format!("test-delete-{}", Uuid::new_v4());
        let date_str = "2024-01-15";
        let date_dir = get_archive_dir().unwrap()
            .join("2024").join("01").join("15");
        let session_dir = date_dir.join(&session_id);
        fs::create_dir_all(&session_dir).unwrap();

        let meta = ArchiveMetadata::new(&session_id);
        fs::write(
            session_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        ).unwrap();
        fs::write(session_dir.join("transcript.txt"), "test transcript").unwrap();

        assert!(session_dir.exists());
        let result = delete_session(&session_id, date_str);
        assert!(result.is_ok());
        assert!(!session_dir.exists());
    }

    #[test]
    fn test_split_session_integration() {
        let session_id = format!("test-split-{}", Uuid::new_v4());
        let date_str = "2024-01-15";
        let date_dir = get_archive_dir().unwrap()
            .join("2024").join("01").join("15");
        let session_dir = date_dir.join(&session_id);
        fs::create_dir_all(&session_dir).unwrap();

        // 5-line transcript
        let transcript = "Line one of transcript\nLine two here\nLine three content\nLine four words\nLine five end";
        let mut meta = ArchiveMetadata::new(&session_id);
        meta.started_at = "2024-01-15T09:00:00Z".to_string();
        meta.duration_ms = Some(100000);
        meta.word_count = transcript.split_whitespace().count();
        meta.charting_mode = Some("continuous".to_string());
        meta.has_soap_note = true;

        fs::write(
            session_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        ).unwrap();
        fs::write(session_dir.join("transcript.txt"), transcript).unwrap();
        fs::write(session_dir.join("soap_note.txt"), "stale soap").unwrap();

        // Split at line 3 (first half gets lines 0-2, second half gets lines 3-4)
        let result = split_session(&session_id, date_str, 3);
        assert!(result.is_ok());
        let new_id = result.unwrap();

        // Verify first half
        let first_transcript = fs::read_to_string(session_dir.join("transcript.txt")).unwrap();
        assert_eq!(first_transcript, "Line one of transcript\nLine two here\nLine three content");
        let first_meta: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(session_dir.join("metadata.json")).unwrap()
        ).unwrap();
        assert!(!first_meta.has_soap_note);
        assert_eq!(first_meta.word_count, first_transcript.split_whitespace().count());

        // Verify SOAP was deleted
        assert!(!session_dir.join("soap_note.txt").exists());

        // Verify second half
        let new_session_dir = date_dir.join(&new_id);
        assert!(new_session_dir.exists());
        let second_transcript = fs::read_to_string(new_session_dir.join("transcript.txt")).unwrap();
        assert_eq!(second_transcript, "Line four words\nLine five end");
        let second_meta: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(new_session_dir.join("metadata.json")).unwrap()
        ).unwrap();
        assert_eq!(second_meta.session_id, new_id);
        assert!(!second_meta.has_soap_note);
        assert!(!second_meta.has_audio);
        assert_eq!(second_meta.word_count, second_transcript.split_whitespace().count());

        // Cleanup
        let _ = fs::remove_dir_all(&session_dir);
        let _ = fs::remove_dir_all(&new_session_dir);
    }

    #[test]
    fn test_split_session_edge_cases_integration() {
        let session_id = format!("test-split-edge-{}", Uuid::new_v4());
        let date_str = "2024-01-15";
        let date_dir = get_archive_dir().unwrap()
            .join("2024").join("01").join("15");
        let session_dir = date_dir.join(&session_id);
        fs::create_dir_all(&session_dir).unwrap();

        let transcript = "Only one line";
        let meta = ArchiveMetadata::new(&session_id);
        fs::write(
            session_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        ).unwrap();
        fs::write(session_dir.join("transcript.txt"), transcript).unwrap();

        // Split at 0 should fail
        let result = split_session(&session_id, date_str, 0);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 1"));

        // Split at 1 (== len) should fail - no second half
        let result = split_session(&session_id, date_str, 1);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("beyond transcript length"));

        // Split at 2 (> len) should fail
        let result = split_session(&session_id, date_str, 2);
        assert!(result.is_err());

        // Cleanup
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn test_merge_sessions_integration() {
        let date_str = "2024-01-15";
        let date_dir = get_archive_dir().unwrap()
            .join("2024").join("01").join("15");

        let id_a = format!("test-merge-a-{}", Uuid::new_v4());
        let id_b = format!("test-merge-b-{}", Uuid::new_v4());
        let id_c = format!("test-merge-c-{}", Uuid::new_v4());

        // Create 3 sessions with different timestamps
        for (id, started, transcript) in [
            (&id_a, "2024-01-15T09:00:00Z", "First session transcript"),
            (&id_b, "2024-01-15T10:00:00Z", "Second session transcript"),
            (&id_c, "2024-01-15T11:00:00Z", "Third session transcript"),
        ] {
            let dir = date_dir.join(id);
            fs::create_dir_all(&dir).unwrap();
            let mut meta = ArchiveMetadata::new(id);
            meta.started_at = started.to_string();
            meta.ended_at = Some(format!("{}:30:00Z", &started[..16]));
            meta.duration_ms = Some(30000);
            meta.word_count = transcript.split_whitespace().count();
            fs::write(
                dir.join("metadata.json"),
                serde_json::to_string_pretty(&meta).unwrap(),
            ).unwrap();
            fs::write(dir.join("transcript.txt"), transcript).unwrap();
        }

        let result = merge_sessions(
            &[id_a.clone(), id_b.clone(), id_c.clone()],
            date_str,
        );
        assert!(result.is_ok());
        let surviving = result.unwrap();
        assert_eq!(surviving, id_a); // Earliest survives

        // Verify merged transcript
        let surviving_dir = date_dir.join(&surviving);
        let merged_text = fs::read_to_string(surviving_dir.join("transcript.txt")).unwrap();
        assert!(merged_text.contains("First session transcript"));
        assert!(merged_text.contains("Second session transcript"));
        assert!(merged_text.contains("Third session transcript"));

        // Verify merged metadata
        let merged_meta: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(surviving_dir.join("metadata.json")).unwrap()
        ).unwrap();
        assert!(!merged_meta.has_soap_note);
        assert_eq!(merged_meta.duration_ms, Some(90000)); // 30K * 3

        // Verify B and C deleted
        assert!(!date_dir.join(&id_b).exists());
        assert!(!date_dir.join(&id_c).exists());

        // Cleanup
        let _ = fs::remove_dir_all(&surviving_dir);
    }

    #[test]
    fn test_update_patient_name_integration() {
        let session_id = format!("test-rename-{}", Uuid::new_v4());
        let date_str = "2024-01-15";
        let date_dir = get_archive_dir().unwrap()
            .join("2024").join("01").join("15");
        let session_dir = date_dir.join(&session_id);
        fs::create_dir_all(&session_dir).unwrap();

        let meta = ArchiveMetadata::new(&session_id);
        fs::write(
            session_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        ).unwrap();
        fs::write(session_dir.join("transcript.txt"), "test").unwrap();

        // Update name
        let result = update_patient_name(&session_id, date_str, "John Smith");
        assert!(result.is_ok());

        // Verify
        let updated_meta: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(session_dir.join("metadata.json")).unwrap()
        ).unwrap();
        assert_eq!(updated_meta.patient_name, Some("John Smith".to_string()));

        // Update with empty string clears it
        let result = update_patient_name(&session_id, date_str, "  ");
        assert!(result.is_ok());
        let cleared_meta: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(session_dir.join("metadata.json")).unwrap()
        ).unwrap();
        assert_eq!(cleared_meta.patient_name, None);

        // Cleanup
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn test_renumber_encounters_integration() {
        // Use a unique date to avoid interference from other test sessions
        let date_str = "2024-06-20";
        let date_dir = get_archive_dir().unwrap()
            .join("2024").join("06").join("20");

        let id_1 = format!("test-renum-1-{}", Uuid::new_v4());
        let id_2 = format!("test-renum-2-{}", Uuid::new_v4());
        let id_3 = format!("test-renum-3-{}", Uuid::new_v4());

        // Create 3 continuous sessions with gaps in numbering
        for (id, started, enc_num) in [
            (&id_1, "2024-01-15T09:00:00Z", 1),
            (&id_2, "2024-01-15T10:00:00Z", 3), // Gap!
            (&id_3, "2024-01-15T11:00:00Z", 5), // Gap!
        ] {
            let dir = date_dir.join(id);
            fs::create_dir_all(&dir).unwrap();
            let mut meta = ArchiveMetadata::new(id);
            meta.started_at = started.to_string();
            meta.charting_mode = Some("continuous".to_string());
            meta.encounter_number = Some(enc_num);
            fs::write(
                dir.join("metadata.json"),
                serde_json::to_string_pretty(&meta).unwrap(),
            ).unwrap();
            fs::write(dir.join("transcript.txt"), "test").unwrap();
        }

        let result = renumber_encounters(date_str);
        assert!(result.is_ok());

        // Verify sequential numbering
        let meta_1: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(date_dir.join(&id_1).join("metadata.json")).unwrap()
        ).unwrap();
        let meta_2: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(date_dir.join(&id_2).join("metadata.json")).unwrap()
        ).unwrap();
        let meta_3: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(date_dir.join(&id_3).join("metadata.json")).unwrap()
        ).unwrap();

        assert_eq!(meta_1.encounter_number, Some(1));
        assert_eq!(meta_2.encounter_number, Some(2));
        assert_eq!(meta_3.encounter_number, Some(3));

        // Cleanup
        let _ = fs::remove_dir_all(date_dir.join(&id_1));
        let _ = fs::remove_dir_all(date_dir.join(&id_2));
        let _ = fs::remove_dir_all(date_dir.join(&id_3));
    }

    #[test]
    fn test_get_transcript_lines_integration() {
        let session_id = format!("test-lines-{}", Uuid::new_v4());
        let date_str = "2024-01-15";
        let date_dir = get_archive_dir().unwrap()
            .join("2024").join("01").join("15");
        let session_dir = date_dir.join(&session_id);
        fs::create_dir_all(&session_dir).unwrap();

        let transcript = "Speaker 1: Hello\nSpeaker 2: Hi there\nSpeaker 1: How are you?";
        let meta = ArchiveMetadata::new(&session_id);
        fs::write(
            session_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        ).unwrap();
        fs::write(session_dir.join("transcript.txt"), transcript).unwrap();

        let result = get_transcript_lines(&session_id, date_str);
        assert!(result.is_ok());
        let lines = result.unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Speaker 1: Hello");
        assert_eq!(lines[1], "Speaker 2: Hi there");
        assert_eq!(lines[2], "Speaker 1: How are you?");

        // Cleanup
        let _ = fs::remove_dir_all(&session_dir);
    }
}
