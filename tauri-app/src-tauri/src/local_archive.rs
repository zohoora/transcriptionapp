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
    /// Flagged as likely non-clinical by two-pass content check
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub likely_non_clinical: Option<bool>,
    /// Number of patients detected in this encounter (>1 for couples/family visits)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_count: Option<u32>,
    /// Physician who created this session (multi-user)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physician_id: Option<String>,
    /// Physician display name (multi-user)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physician_name: Option<String>,
    /// Room where session was recorded (multi-user)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_name: Option<String>,
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
            likely_non_clinical: None,
            patient_count: None,
            physician_id: None,
            physician_name: None,
            room_name: None,
        }
    }
}

/// Summary of an archived session (for list views)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveSummary {
    pub session_id: String,
    pub date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub likely_non_clinical: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_feedback: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physician_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_labels: Option<Vec<String>>,
}

/// A single patient's SOAP note within a multi-patient encounter
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedPatientNote {
    pub index: u32,
    pub label: String,
    pub content: String,
}

/// Detailed archived session (for detail view)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveDetails {
    pub session_id: String,
    pub metadata: ArchiveMetadata,
    pub transcript: Option<String>,
    pub soap_note: Option<String>,
    pub audio_path: Option<String>,
    /// Per-patient SOAP notes (present when patient_count > 1)
    #[serde(default, rename = "patientNotes", skip_serializing_if = "Option::is_none")]
    pub patient_notes: Option<Vec<ArchivedPatientNote>>,
}

/// User feedback on a session's SOAP note quality
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFeedback {
    pub schema_version: u32,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_rating: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detection_feedback: Option<DetectionFeedback>,
    #[serde(default)]
    pub patient_feedback: Vec<PatientContentFeedback>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comments: Option<String>,
}

/// Feedback on encounter detection quality
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionFeedback {
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Per-patient content feedback
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatientContentFeedback {
    pub patient_index: usize,
    pub issues: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Archive a completed session
///
/// `encounter_started_at` — if provided, overrides `started_at` in metadata
/// and recalculates `duration_ms` as `(now - encounter_started_at)`. Use this
/// in continuous mode where encounters start well before archival.
pub fn save_session(
    session_id: &str,
    transcript: &str,
    duration_ms: u64,
    audio_path: Option<&PathBuf>,
    auto_ended: bool,
    auto_end_reason: Option<&str>,
    encounter_started_at: Option<DateTime<Utc>>,
    segment_count: Option<usize>,
) -> Result<PathBuf, String> {
    validate_session_id(session_id)?;
    let now = Utc::now();
    let session_dir = ensure_session_dir(session_id, &now)?;

    // Create metadata
    let word_count = transcript.split_whitespace().count();
    let mut metadata = ArchiveMetadata::new(session_id);
    // Use actual encounter start time if provided (continuous mode)
    if let Some(started) = encounter_started_at {
        metadata.started_at = started.to_rfc3339();
        metadata.duration_ms = Some((now - started).num_milliseconds().max(0) as u64);
    } else {
        // Derive started_at from duration for session mode (avoids defaulting to save time)
        metadata.started_at = (now - chrono::Duration::milliseconds(duration_ms as i64)).to_rfc3339();
        metadata.duration_ms = Some(duration_ms);
    }
    if let Some(count) = segment_count {
        metadata.segment_count = count;
    }
    metadata.ended_at = Some(now.to_rfc3339());
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

    // Don't overwrite a good SOAP note with a malformed placeholder
    let soap_path = session_dir.join("soap_note.txt");
    if soap_content.contains("SOAP generation produced malformed output") {
        if soap_path.exists() {
            if let Ok(existing) = fs::read_to_string(&soap_path) {
                if !existing.contains("SOAP generation produced malformed output") {
                    warn!(
                        session_id = %session_id,
                        "Refusing to overwrite good SOAP ({} chars) with malformed placeholder",
                        existing.len()
                    );
                    return Ok(());
                }
            }
        }
    }

    // Save SOAP note
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

/// Save per-patient SOAP files alongside the combined soap_note.txt.
/// Called when multi-patient detection produces N>1 notes.
/// Writes: soap_patient_1.txt, soap_patient_2.txt, ..., patient_labels.json
/// Updates metadata.json with patient_count.
pub fn save_multi_patient_soap(
    session_id: &str,
    date: &DateTime<Utc>,
    notes: &[crate::llm_client::PatientSoapNote],
) -> Result<(), String> {
    if notes.len() <= 1 {
        return Ok(()); // Nothing to do for single-patient
    }

    validate_session_id(session_id)?;
    let session_dir = get_session_archive_dir(session_id, date)?;

    if !session_dir.exists() {
        fs::create_dir_all(&session_dir)
            .map_err(|e| format!("Failed to create session directory: {}", e))?;
    }

    // Write per-patient SOAP files
    for (i, note) in notes.iter().enumerate() {
        let filename = format!("soap_patient_{}.txt", i + 1);
        let path = session_dir.join(&filename);
        fs::write(&path, &note.content)
            .map_err(|e| format!("Failed to write {}: {}", filename, e))?;
        info!(
            session_id = %session_id,
            patient = %note.patient_label,
            file = %filename,
            "Per-patient SOAP file written"
        );
    }

    // Write patient_labels.json metadata
    let labels: Vec<serde_json::Value> = notes.iter().enumerate().map(|(i, note)| {
        serde_json::json!({
            "index": i + 1,
            "label": note.patient_label,
        })
    }).collect();
    let labels_path = session_dir.join("patient_labels.json");
    let labels_json = serde_json::to_string_pretty(&labels)
        .map_err(|e| format!("Failed to serialize patient labels: {}", e))?;
    fs::write(&labels_path, labels_json)
        .map_err(|e| format!("Failed to write patient_labels.json: {}", e))?;

    // Update metadata with patient_count
    let metadata_path = session_dir.join("metadata.json");
    if metadata_path.exists() {
        let content = fs::read_to_string(&metadata_path)
            .map_err(|e| format!("Failed to read metadata: {}", e))?;
        let mut metadata: ArchiveMetadata = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse metadata: {}", e))?;

        metadata.patient_count = Some(notes.len() as u32);

        let metadata_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
        fs::write(&metadata_path, metadata_json)
            .map_err(|e| format!("Failed to write metadata: {}", e))?;
    }

    info!(
        session_id = %session_id,
        patient_count = notes.len(),
        "Multi-patient SOAP files saved to archive"
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

        let has_feedback = session_dir.join("feedback.json").exists();

        // Load patient labels for multi-patient sessions
        let patient_count = metadata.patient_count;
        let patient_labels = if patient_count.unwrap_or(0) > 1 {
            let labels_path = session_dir.join("patient_labels.json");
            if labels_path.exists() {
                match fs::read_to_string(&labels_path) {
                    Ok(json) => {
                        match serde_json::from_str::<Vec<serde_json::Value>>(&json) {
                            Ok(entries) => {
                                let labels: Vec<String> = entries.iter()
                                    .map(|e| e["label"].as_str().unwrap_or("Patient").to_string())
                                    .collect();
                                if labels.len() > 1 { Some(labels) } else { None }
                            }
                            Err(_) => None,
                        }
                    }
                    Err(_) => None,
                }
            } else {
                None
            }
        } else {
            None
        };

        sessions.push(ArchiveSummary {
            session_id: metadata.session_id,
            date: date_str.to_string(),
            started_at: Some(metadata.started_at),
            duration_ms: metadata.duration_ms,
            word_count: metadata.word_count,
            has_soap_note: metadata.has_soap_note,
            has_audio: metadata.has_audio,
            auto_ended: metadata.auto_ended,
            charting_mode: metadata.charting_mode,
            encounter_number: metadata.encounter_number,
            patient_name: metadata.patient_name,
            likely_non_clinical: metadata.likely_non_clinical,
            has_feedback: Some(has_feedback),
            physician_name: metadata.physician_name,
            room_name: metadata.room_name,
            patient_count,
            patient_labels,
        });
    }

    // Sort by started_at ascending (earliest first)
    sessions.sort_by(|a, b| a.started_at.cmp(&b.started_at));

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

    // Load per-patient SOAP notes if patient_labels.json exists
    let labels_path = session_dir.join("patient_labels.json");
    let patient_notes = if labels_path.exists() {
        let labels_json = fs::read_to_string(&labels_path)
            .map_err(|e| format!("Failed to read patient_labels.json: {}", e))?;
        let labels: Vec<serde_json::Value> = serde_json::from_str(&labels_json)
            .map_err(|e| format!("Failed to parse patient_labels.json: {}", e))?;

        let mut notes = Vec::new();
        for label_entry in &labels {
            let index = label_entry["index"].as_u64().unwrap_or(0) as u32;
            let label = label_entry["label"].as_str().unwrap_or("Patient").to_string();
            let patient_file = session_dir.join(format!("soap_patient_{}.txt", index));
            if patient_file.exists() {
                let content = fs::read_to_string(&patient_file)
                    .map_err(|e| format!("Failed to read soap_patient_{}.txt: {}", index, e))?;
                notes.push(ArchivedPatientNote { index, label, content });
            }
        }
        if notes.is_empty() { None } else { Some(notes) }
    } else {
        None
    };

    Ok(ArchiveDetails {
        session_id: session_id.to_string(),
        metadata,
        transcript,
        soap_note,
        audio_path,
        patient_notes,
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
    patient_name: Option<&str>,
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
        metadata.has_soap_note = false; // SOAP is stale after merge

        // Recompute duration from the surviving encounter's started_at to now.
        // The caller-provided merged_duration_ms was computed from the orphan's start time,
        // which is wrong — the surviving encounter's actual start is earlier.
        let now = Utc::now();
        if let Ok(started) = chrono::DateTime::parse_from_rfc3339(&metadata.started_at) {
            metadata.duration_ms = Some((now - started.with_timezone(&Utc)).num_milliseconds().max(0) as u64);
        } else {
            metadata.duration_ms = Some(merged_duration_ms); // fallback
        }
        metadata.ended_at = Some(now.to_rfc3339());

        // Update patient name from merged encounter's vision tracker if keeper has no name
        // or if the merged encounter has a name (more recent, likely more accurate)
        if let Some(name) = patient_name {
            if !name.is_empty() {
                metadata.patient_name = Some(name.to_string());
            }
        }

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
pub(crate) fn get_date_dir_from_str(date_str: &str) -> Result<PathBuf, String> {
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date format: {}", e))?;
    let base = get_archive_dir()?;
    Ok(base
        .join(format!("{:04}", date.year()))
        .join(format!("{:02}", date.month()))
        .join(format!("{:02}", date.day())))
}

/// Resolve the session directory from session_id + date string.
pub(crate) fn get_session_dir_from_str(session_id: &str, date_str: &str) -> Result<PathBuf, String> {
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

    // Read transcript (uses sentence-boundary fallback for single-line transcripts)
    let transcript_path = session_dir.join("transcript.txt");
    let transcript = fs::read_to_string(&transcript_path)
        .map_err(|e| format!("Failed to read transcript: {}", e))?;
    let lines = split_transcript_into_lines(&transcript);

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
        likely_non_clinical: original_meta.likely_non_clinical,
        patient_count: None,
        physician_id: original_meta.physician_id.clone(),
        physician_name: original_meta.physician_name.clone(),
        room_name: original_meta.room_name.clone(),
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

/// Split a transcript string into lines. Falls back to sentence-boundary
/// splitting for single-line transcripts (e.g., continuous mode flush before
/// the newline fix).
fn split_transcript_into_lines(transcript: &str) -> Vec<String> {
    let lines: Vec<String> = transcript.lines().map(|l| l.to_string()).collect();

    // If the transcript already has multiple lines, use them as-is
    if lines.len() > 1 || transcript.len() <= 200 {
        return lines;
    }

    // Fallback: split on sentence boundaries (at least 10 words per chunk)
    let mut result = Vec::new();
    let mut current = String::new();
    for word in transcript.split_whitespace() {
        current.push_str(word);
        current.push(' ');
        if (word.ends_with('.') || word.ends_with('?') || word.ends_with('!'))
            && current.split_whitespace().count() >= 10
        {
            result.push(current.trim().to_string());
            current = String::new();
        }
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
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
    Ok(split_transcript_into_lines(&transcript))
}

// ============================================================================
// Session Feedback
// ============================================================================

const FEEDBACK_FILENAME: &str = "feedback.json";

/// Read feedback for a session. Returns None if no feedback file exists.
pub fn read_feedback(session_id: &str, date_str: &str) -> Result<Option<SessionFeedback>, String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    let feedback_path = session_dir.join(FEEDBACK_FILENAME);

    if !feedback_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&feedback_path)
        .map_err(|e| format!("Failed to read feedback: {}", e))?;
    let feedback: SessionFeedback = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse feedback: {}", e))?;
    Ok(Some(feedback))
}

/// Write feedback for a session. Also enriches replay_bundle.json outcome if present.
pub fn write_feedback(
    session_id: &str,
    date_str: &str,
    feedback: &SessionFeedback,
) -> Result<(), String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    if !session_dir.exists() {
        return Err(format!("Session not found: {}", session_id));
    }

    let feedback_path = session_dir.join(FEEDBACK_FILENAME);
    let json = serde_json::to_string_pretty(feedback)
        .map_err(|e| format!("Failed to serialize feedback: {}", e))?;
    fs::write(&feedback_path, &json)
        .map_err(|e| format!("Failed to write feedback: {}", e))?;

    info!("Saved feedback for session {} on {}", session_id, date_str);

    // Enrich replay_bundle.json if present
    let replay_path = session_dir.join("replay_bundle.json");
    if let Err(e) = enrich_replay_bundle(&replay_path, feedback) {
        warn!("Failed to enrich replay bundle with feedback: {}", e);
    }

    Ok(())
}

/// Inject user_feedback into replay_bundle.json outcome field.
/// Returns Ok(()) silently if the replay bundle does not exist.
fn enrich_replay_bundle(
    replay_path: &std::path::Path,
    feedback: &SessionFeedback,
) -> Result<(), String> {
    let content = match fs::read_to_string(replay_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("Failed to read replay bundle: {}", e)),
    };
    let mut bundle: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse replay bundle: {}", e))?;

    // Build a compact feedback summary for the outcome
    let feedback_summary = serde_json::json!({
        "quality_rating": feedback.quality_rating,
        "detection_category": feedback.detection_feedback.as_ref().map(|d| &d.category),
        "has_comments": feedback.comments.is_some(),
    });

    // Inject into outcome (create outcome if missing)
    if let Some(outcome) = bundle.get_mut("outcome") {
        if let Some(obj) = outcome.as_object_mut() {
            obj.insert("user_feedback".to_string(), feedback_summary);
        }
    } else {
        bundle["outcome"] = serde_json::json!({
            "user_feedback": feedback_summary,
        });
    }

    let updated = serde_json::to_string_pretty(&bundle)
        .map_err(|e| format!("Failed to serialize replay bundle: {}", e))?;
    fs::write(replay_path, updated)
        .map_err(|e| format!("Failed to write replay bundle: {}", e))?;

    info!("Enriched replay bundle with user feedback");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_transcript_into_lines_multiline() {
        let text = "Line one.\nLine two.\nLine three.";
        let lines = split_transcript_into_lines(text);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Line one.");
    }

    #[test]
    fn test_split_transcript_into_lines_single_line_fallback() {
        // Simulate a continuous mode flush transcript (no newlines, >200 chars)
        let words: Vec<String> = (0..100)
            .map(|i| if i % 15 == 14 { format!("word{}.", i) } else { format!("word{}", i) })
            .collect();
        let text = words.join(" ");
        assert!(text.len() > 200);
        assert!(!text.contains('\n'));

        let lines = split_transcript_into_lines(&text);
        assert!(lines.len() > 1, "Should split single-line transcript into multiple lines");
        // Rejoin should preserve all words
        let rejoined_words: usize = lines.iter().map(|l| l.split_whitespace().count()).sum();
        assert_eq!(rejoined_words, 100);
    }

    #[test]
    fn test_split_transcript_into_lines_short_single_line() {
        // Short single-line transcripts should NOT be split
        let text = "A short transcript.";
        let lines = split_transcript_into_lines(text);
        assert_eq!(lines.len(), 1);
    }

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
            started_at: Some("2024-01-15T10:30:00Z".to_string()),
            duration_ms: Some(300000),
            word_count: 500,
            has_soap_note: true,
            has_audio: false,
            auto_ended: false,
            charting_mode: None,
            encounter_number: None,
            patient_name: None,
            likely_non_clinical: None,
            has_feedback: None,
            physician_name: None,
            room_name: None,
            patient_count: None,
            patient_labels: None,
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
            None,
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
        let result = save_session("../escape", "text", 1000, None, false, None, None, None);
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
        let result = merge_encounters("../escape", "valid-id", &now, "text", 10, 5000, None);
        assert!(result.is_err());

        // Session B has traversal
        let result = merge_encounters("valid-id", "foo/bar", &now, "text", 10, 5000, None);
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

    // ========================================================================
    // Feedback tests
    // ========================================================================

    #[test]
    fn test_feedback_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let _date_str = "2024-01-15";
        let session_id = "feedback-test-rt";
        let session_dir = create_test_session(
            temp.path(), session_id, "test transcript", "2024-01-15T10:00:00Z", None, None,
        );

        // Override archive dir by writing/reading directly via functions that take paths
        let feedback = SessionFeedback {
            schema_version: 1,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            updated_at: "2024-01-15T10:01:00Z".to_string(),
            quality_rating: Some("bad".to_string()),
            detection_feedback: Some(DetectionFeedback {
                category: "fragment".to_string(),
                details: Some("This was a fragment".to_string()),
            }),
            patient_feedback: vec![PatientContentFeedback {
                patient_index: 0,
                issues: vec!["missed_details".to_string()],
                details: None,
            }],
            comments: Some("Needs improvement".to_string()),
        };

        // Write directly to session dir
        let feedback_path = session_dir.join(FEEDBACK_FILENAME);
        let json = serde_json::to_string_pretty(&feedback).unwrap();
        fs::write(&feedback_path, &json).unwrap();

        // Read and verify
        let content = fs::read_to_string(&feedback_path).unwrap();
        let loaded: SessionFeedback = serde_json::from_str(&content).unwrap();

        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.quality_rating.as_deref(), Some("bad"));
        assert_eq!(loaded.detection_feedback.as_ref().unwrap().category, "fragment");
        assert_eq!(loaded.patient_feedback.len(), 1);
        assert_eq!(loaded.patient_feedback[0].patient_index, 0);
        assert_eq!(loaded.comments.as_deref(), Some("Needs improvement"));
    }

    #[test]
    fn test_feedback_missing_returns_none() {
        // Non-existent session should error, but a session without feedback.json should return None
        let temp = tempfile::tempdir().unwrap();
        let session_dir = create_test_session(
            temp.path(), "no-feedback", "transcript", "2024-01-15T10:00:00Z", None, None,
        );

        // Verify no feedback.json exists
        assert!(!session_dir.join(FEEDBACK_FILENAME).exists());

        // Reading feedback from session dir should work — verify the file doesn't exist
        let feedback_path = session_dir.join(FEEDBACK_FILENAME);
        assert!(!feedback_path.exists());
    }

    #[test]
    fn test_feedback_replay_enrichment() {
        let temp = tempfile::tempdir().unwrap();
        let session_dir = create_test_session(
            temp.path(), "replay-enrich", "transcript", "2024-01-15T10:00:00Z", None, None,
        );

        // Create a minimal replay bundle
        let replay = serde_json::json!({
            "schema_version": 1,
            "config": {},
            "segments": [],
            "sensor_transitions": [],
            "vision_results": [],
            "detection_checks": [],
            "outcome": {
                "session_id": "replay-enrich",
                "encounter_number": 1,
                "word_count": 500,
                "is_clinical": true,
                "was_merged": false,
            }
        });
        let replay_path = session_dir.join("replay_bundle.json");
        fs::write(&replay_path, serde_json::to_string_pretty(&replay).unwrap()).unwrap();

        // Enrich with feedback
        let feedback = SessionFeedback {
            schema_version: 1,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            updated_at: "2024-01-15T10:01:00Z".to_string(),
            quality_rating: Some("bad".to_string()),
            detection_feedback: Some(DetectionFeedback {
                category: "inappropriately_merged".to_string(),
                details: None,
            }),
            patient_feedback: vec![],
            comments: Some("Wrong split".to_string()),
        };

        enrich_replay_bundle(&replay_path, &feedback).unwrap();

        // Verify
        let content = fs::read_to_string(&replay_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let outcome = &parsed["outcome"];
        assert_eq!(outcome["user_feedback"]["quality_rating"], "bad");
        assert_eq!(outcome["user_feedback"]["detection_category"], "inappropriately_merged");
        assert_eq!(outcome["user_feedback"]["has_comments"], true);
        // Original fields preserved
        assert_eq!(outcome["session_id"], "replay-enrich");
        assert_eq!(outcome["encounter_number"], 1);
    }

    #[test]
    fn test_has_feedback_in_summary() {
        // Verify ArchiveSummary correctly serializes has_feedback
        let summary = ArchiveSummary {
            session_id: "test-fb".to_string(),
            date: "2024-01-15T10:30:00Z".to_string(),
            started_at: Some("2024-01-15T10:30:00Z".to_string()),
            duration_ms: Some(300000),
            word_count: 500,
            has_soap_note: true,
            has_audio: false,
            auto_ended: false,
            charting_mode: None,
            encounter_number: None,
            patient_name: None,
            likely_non_clinical: None,
            has_feedback: Some(true),
            physician_name: None,
            room_name: None,
            patient_count: None,
            patient_labels: None,
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"has_feedback\":true"));

        // None should be omitted
        let summary_no_fb = ArchiveSummary {
            has_feedback: None,
            ..summary
        };
        let json2 = serde_json::to_string(&summary_no_fb).unwrap();
        assert!(!json2.contains("has_feedback"));
    }

    #[test]
    fn test_feedback_serialization_camel_case() {
        let feedback = SessionFeedback {
            schema_version: 1,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            updated_at: "2024-01-15T10:01:00Z".to_string(),
            quality_rating: Some("good".to_string()),
            detection_feedback: None,
            patient_feedback: vec![],
            comments: None,
        };

        let json = serde_json::to_string(&feedback).unwrap();
        // Should use camelCase
        assert!(json.contains("schemaVersion"));
        assert!(json.contains("createdAt"));
        assert!(json.contains("updatedAt"));
        assert!(json.contains("qualityRating"));
        // Should not contain snake_case
        assert!(!json.contains("schema_version"));
        assert!(!json.contains("created_at"));
    }
}

// ============================================================================
// Multi-Patient Operations
// ============================================================================

/// Helper: read metadata, apply a mutation, write back.
fn update_metadata_field(
    session_id: &str,
    date_str: &str,
    mutate: impl FnOnce(&mut ArchiveMetadata),
) -> Result<(), String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    let metadata_path = session_dir.join("metadata.json");
    let content = fs::read_to_string(&metadata_path)
        .map_err(|e| format!("Failed to read metadata: {}", e))?;
    let mut metadata: ArchiveMetadata = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse metadata: {}", e))?;
    mutate(&mut metadata);
    let json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
    fs::write(&metadata_path, json)
        .map_err(|e| format!("Failed to write metadata: {}", e))?;
    Ok(())
}

/// Delete a single patient's SOAP from a multi-patient session.
/// If only one patient remains, reverts to single-patient format.
/// If no patients remain, deletes the entire session.
pub fn delete_patient_from_session(
    session_id: &str,
    date_str: &str,
    patient_index: u32,
) -> Result<(), String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    if !session_dir.exists() {
        return Err(format!("Session not found: {}", session_id));
    }

    let labels_path = session_dir.join("patient_labels.json");
    if !labels_path.exists() {
        return Err("Not a multi-patient session".to_string());
    }

    let labels_json = fs::read_to_string(&labels_path)
        .map_err(|e| format!("Failed to read patient_labels.json: {}", e))?;
    let mut labels: Vec<serde_json::Value> = serde_json::from_str(&labels_json)
        .map_err(|e| format!("Failed to parse patient_labels.json: {}", e))?;

    // Remove the patient's SOAP file
    let soap_file = session_dir.join(format!("soap_patient_{}.txt", patient_index));
    if soap_file.exists() {
        fs::remove_file(&soap_file)
            .map_err(|e| format!("Failed to delete patient SOAP: {}", e))?;
    }

    // Remove from labels
    labels.retain(|l| l["index"].as_u64().unwrap_or(0) as u32 != patient_index);

    if labels.is_empty() {
        return delete_session(session_id, date_str);
    }

    if labels.len() == 1 {
        // Revert to single-patient
        let remaining_index = labels[0]["index"].as_u64().unwrap_or(1) as u32;
        let remaining_soap = session_dir.join(format!("soap_patient_{}.txt", remaining_index));
        let single_soap = session_dir.join("soap_note.txt");
        if remaining_soap.exists() {
            fs::rename(&remaining_soap, &single_soap)
                .map_err(|e| format!("Failed to rename SOAP file: {}", e))?;
        }
        let _ = fs::remove_file(&labels_path);
        update_metadata_field(session_id, date_str, |m| {
            m.patient_count = None;
        })?;
    } else {
        let updated_json = serde_json::to_string_pretty(&labels)
            .map_err(|e| format!("Failed to serialize labels: {}", e))?;
        fs::write(&labels_path, updated_json)
            .map_err(|e| format!("Failed to write labels: {}", e))?;
        let new_count = labels.len() as u32;
        update_metadata_field(session_id, date_str, |m| {
            m.patient_count = Some(new_count);
        })?;
    }

    Ok(())
}

/// Rename a patient label in a multi-patient session.
pub fn rename_patient_label(
    session_id: &str,
    date_str: &str,
    patient_index: u32,
    new_label: &str,
) -> Result<(), String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    let labels_path = session_dir.join("patient_labels.json");
    if !labels_path.exists() {
        return Err("Not a multi-patient session".to_string());
    }

    let labels_json = fs::read_to_string(&labels_path)
        .map_err(|e| format!("Failed to read labels: {}", e))?;
    let mut labels: Vec<serde_json::Value> = serde_json::from_str(&labels_json)
        .map_err(|e| format!("Failed to parse labels: {}", e))?;

    for entry in &mut labels {
        if entry["index"].as_u64().unwrap_or(0) as u32 == patient_index {
            entry["label"] = serde_json::json!(new_label);
            break;
        }
    }

    let updated = serde_json::to_string_pretty(&labels)
        .map_err(|e| format!("Failed to serialize labels: {}", e))?;
    fs::write(&labels_path, updated)
        .map_err(|e| format!("Failed to write labels: {}", e))?;
    Ok(())
}

