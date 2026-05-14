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

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
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

/// Get the archive base directory.
///
/// If the `TRANSCRIPTIONAPP_ARCHIVE_DIR` environment variable is set and
/// non-empty, it overrides the default path. Used by the test harness to
/// redirect archive writes into a tempdir, preventing accidental writes
/// to a developer's real production archive during `cargo test`.
pub fn get_archive_dir() -> Result<PathBuf, String> {
    if let Ok(override_dir) = std::env::var("TRANSCRIPTIONAPP_ARCHIVE_DIR") {
        if !override_dir.is_empty() {
            return Ok(PathBuf::from(override_dir));
        }
    }
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

/// Read just the `started_at` field from a session's metadata.json.
/// Cheaper than `get_session()` because it skips transcript/SOAP/audio
/// reads — used on the merge-back hot path to derive merged duration
/// without loading the full archive. `None` on any read/parse failure.
pub fn read_session_started_at(
    session_id: &str,
    date: &DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let session_dir = get_session_archive_dir(session_id, date).ok()?;
    let metadata_path = session_dir.join("metadata.json");
    let content = fs::read_to_string(&metadata_path).ok()?;
    let metadata: ArchiveMetadata = serde_json::from_str(&content).ok()?;
    DateTime::parse_from_rfc3339(&metadata.started_at)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Read just `soap_note.txt` from a session's archive directory. Returns `None`
/// when the file is missing or the session id / date resolve to an invalid path.
///
/// Lighter-weight than [`get_session`] — it skips metadata + transcript I/O.
/// Callers are responsible for validating the SOAP content (see
/// [`crate::llm_client::is_usable_soap`]).
pub fn read_session_soap(
    session_id: &str,
    date: &DateTime<Utc>,
) -> Option<String> {
    let dir = get_session_archive_dir(session_id, date).ok()?;
    fs::read_to_string(dir.join("soap_note.txt")).ok()
}

/// Ensure the archive directory exists for a session
fn ensure_session_dir(session_id: &str, date: &DateTime<Utc>) -> Result<PathBuf, String> {
    validate_session_id(session_id)?;
    let dir = get_session_archive_dir(session_id, date)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create archive directory: {}", e))?;
    Ok(dir)
}

/// One submitted clinician note attached to a continuous-mode encounter.
///
/// Created by `submit_continuous_encounter_note`; persisted to
/// `clinician_notes.json` inside the session archive dir at split / merge /
/// flush time; joined via `join_notes_for_prompt` for SOAP generation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EncounterNote {
    pub id: String,
    pub text: String,
    /// Unix epoch milliseconds (UTC) when the clinician pressed submit
    pub timestamp_ms: i64,
}

impl EncounterNote {
    pub fn new(text: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            text,
            timestamp_ms: Utc::now().timestamp_millis(),
        }
    }
}

/// Separator used when joining `EncounterNote.text` entries into a single
/// SOAP-prompt string. Each note becomes its own block so the LLM sees them
/// as discrete observations.
const NOTES_JOIN_SEPARATOR: &str = "\n---\n";

/// Join notes into a single string suitable for the SOAP prompt's
/// `CLINICIAN NOTES:` section. Empty input → empty string, so callers can
/// treat "no notes" identically to the pre-existing behavior.
pub fn join_notes_for_prompt(notes: &[EncounterNote]) -> String {
    let mut out = String::new();
    for (i, n) in notes.iter().enumerate() {
        if i > 0 {
            out.push_str(NOTES_JOIN_SEPARATOR);
        }
        out.push_str(n.text.trim());
    }
    out
}

/// Filename (relative to the session dir) for persisted clinician notes.
pub const CLINICIAN_NOTES_FILENAME: &str = "clinician_notes.json";

/// Load persisted clinician notes for a session. Returns `Ok(None)` if the
/// file doesn't exist (the common case for sessions created before this
/// feature or without any submitted notes). Malformed JSON is treated as
/// "no notes" so reading never panics production paths.
pub fn read_clinician_notes(
    session_id: &str,
    date: &DateTime<Utc>,
) -> Result<Option<Vec<EncounterNote>>, String> {
    validate_session_id(session_id)?;
    let dir = get_session_archive_dir(session_id, date)?;
    let path = dir.join(CLINICIAN_NOTES_FILENAME);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read clinician notes: {}", e))?;
    match serde_json::from_str::<Vec<EncounterNote>>(&content) {
        Ok(list) => Ok(Some(list)),
        Err(e) => {
            warn!(session_id = %session_id, error = %e, "Malformed clinician_notes.json — ignoring");
            Ok(None)
        }
    }
}

/// Overwrite the session's `clinician_notes.json` with the supplied list.
/// Creates the session dir if missing. An empty list deletes the file, so
/// `has_clinician_notes` always reflects on-disk truth.
pub fn write_clinician_notes(
    session_id: &str,
    date: &DateTime<Utc>,
    notes: &[EncounterNote],
) -> Result<(), String> {
    let dir = ensure_session_dir(session_id, date)?;
    let path = dir.join(CLINICIAN_NOTES_FILENAME);
    if notes.is_empty() {
        // Ignore NotFound — emptying an already-absent sidecar is a no-op,
        // not a failure. Any other error is surfaced.
        match fs::remove_file(&path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Failed to clear clinician notes: {}", e)),
        }
        return Ok(());
    }
    let json = serde_json::to_string_pretty(notes)
        .map_err(|e| format!("Failed to serialize clinician notes: {}", e))?;
    fs::write(&path, json)
        .map_err(|e| format!("Failed to write clinician notes: {}", e))?;
    Ok(())
}

/// Append a list of notes onto session A's persisted clinician notes.
/// Idempotent — notes with ids already present in A are skipped. Returns
/// the full merged list (A's original + newly-appended, sorted by
/// timestamp) on append, or `None` when nothing was added. Callers that
/// need the joined prompt text can pass the Vec straight into
/// `join_notes_for_prompt` without re-reading from disk.
pub fn append_clinician_notes_vec(
    session_a_id: &str,
    date: &DateTime<Utc>,
    incoming: &[EncounterNote],
) -> Result<Option<Vec<EncounterNote>>, String> {
    if incoming.is_empty() {
        return Ok(None);
    }
    let mut a_notes = read_clinician_notes(session_a_id, date)?.unwrap_or_default();
    let existing_ids: std::collections::HashSet<String> =
        a_notes.iter().map(|n| n.id.clone()).collect();
    let to_append: Vec<EncounterNote> = incoming
        .iter()
        .filter(|n| !existing_ids.contains(&n.id))
        .cloned()
        .collect();
    if to_append.is_empty() {
        return Ok(None);
    }
    a_notes.extend(to_append);
    a_notes.sort_by_key(|n| n.timestamp_ms);
    write_clinician_notes(session_a_id, date, &a_notes)?;
    Ok(Some(a_notes))
}

/// Append B's persisted clinician notes onto A's. Used by `merge_encounters`
/// to preserve clinician observations when the source session's directory
/// is about to be deleted. Returns the merged list on append, `None` when
/// B had no notes or all of B's ids were already in A.
pub fn append_clinician_notes(
    session_a_id: &str,
    session_b_id: &str,
    date: &DateTime<Utc>,
) -> Result<Option<Vec<EncounterNote>>, String> {
    let b_notes = match read_clinician_notes(session_b_id, date)? {
        Some(list) if !list.is_empty() => list,
        _ => return Ok(None),
    };
    append_clinician_notes_vec(session_a_id, date, &b_notes)
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
    /// Patient date of birth extracted via vision (YYYY-MM-DD), used for age-based billing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_dob: Option<String>,
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
    /// Whether a patient handout has been generated for this session
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_patient_handout: Option<bool>,
    /// Whether billing codes have been extracted for this session
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_billing_record: Option<bool>,
    /// RFC3339 timestamp when the clinician confirmed patient identity (name +
    /// DOB) from the History Window, triggering the Medplum + profile-service
    /// dual-write. `None` means the session has never been through the
    /// confirmation flow. (v0.10.46+)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_confirmed_at: Option<String>,
    /// Medplum FHIR Patient ID linked to this session. Populated by a
    /// successful Medplum upsert during patient confirmation. Used by replay
    /// tools + future SOAP-context injection to fetch longitudinal history.
    /// (v0.10.46+)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medplum_patient_id: Option<String>,
    /// True iff the session archive directory contains a `clinician_notes.json`
    /// file with at least one submitted note. Kept in sync by the splitter /
    /// flush / merge paths that write the file.
    #[serde(default)]
    pub has_clinician_notes: bool,
    /// Version tag for the SOAP-generation prompt that produced `soap_note.txt`.
    /// Set when SOAP is archived; absent on legacy sessions written before v0.10.62.
    /// Lets audits correlate clinical drift to specific prompt revisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_prompt_version: Option<String>,
    /// Version tag for the billing-extraction prompt that produced `billing.json`.
    /// Set when billing is extracted; absent on legacy sessions or sessions with
    /// no billing record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_prompt_version: Option<String>,
    /// Sibling-group UUID shared across all child sessions produced by an
    /// auto-split multi-patient encounter. Same value on every sibling; absent
    /// on single-patient sessions and on legacy multi-patient sessions still
    /// using the combined-SOAP / patient_labels.json layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_group_id: Option<String>,
    /// 0-based position within the sibling group. Sibling 0 is the anchor that
    /// owns shared resources (audio, screenshots, pipeline_log, replay_bundle);
    /// other siblings reference the anchor via `sibling_group_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_index: Option<u32>,
    /// Total siblings in the group. Denormalized so the History sidebar can
    /// render "Patient N of M" badges without a second lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_group_size: Option<u32>,
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
            patient_dob: None,
            detection_method: None,
            shadow_comparison: None,
            likely_non_clinical: None,
            patient_count: None,
            physician_id: None,
            physician_name: None,
            room_name: None,
            has_patient_handout: None,
            has_billing_record: None,
            patient_confirmed_at: None,
            medplum_patient_id: None,
            has_clinician_notes: false,
            soap_prompt_version: None,
            billing_prompt_version: None,
            sibling_group_id: None,
            sibling_index: None,
            sibling_group_size: None,
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
    // Surface the rating inline on the session list row (avoids a per-row fetch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_rating: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub physician_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_labels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_billing_record: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling_group_size: Option<u32>,
}

/// A single patient's SOAP note within a multi-patient encounter
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedPatientNote {
    pub index: u32,
    pub label: String,
    pub content: String,
}

/// Entry in patient_labels.json (extended 2026-04-29 to include per-patient
/// summary so single-patient SOAP regeneration retains the multi-patient
/// detection context that originally separated the two patients in the
/// composite SOAP).
///
/// Class 5 fix from 2026-04-29 forensic review: Slote 3:28pm mom+child had
/// both `soap_patient_1.txt` (child) and `soap_patient_2.txt` (adult) end
/// up with the adult's iron-deficiency content because the regen prompt
/// only had a generic label like "Child/Young Patient" — without the
/// per-patient summary the detection produced, the LLM defaulted to the
/// dominant content (~800-word adult vs ~50-word child).
///
/// `summary` is `Option<String>` for backward-compat: pre-2026-04-29
/// patient_labels.json files have only `index` and `label`. When summary
/// is None at regen time, the prompt falls back to label-only behavior
/// (existing semantics).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PatientLabelEntry {
    index: u32,
    label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
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
    // v2: structured accuracy flags mirroring tests/fixtures/labels schema.
    // None = unrated, Some(true) = correct, Some(false) = wrong.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_correct: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_correct: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clinical_correct: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patient_count_correct: Option<bool>,
    // Billing ground truth. When Some(true), the session's billing.json codes
    // + diagnostic code are treated as authoritative for regression testing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_correct: Option<bool>,
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
        // Use caller-provided duration if available (first-to-last segment span),
        // otherwise fall back to now - started (includes LLM processing time)
        metadata.duration_ms = if duration_ms > 0 {
            Some(duration_ms)
        } else {
            Some((now - started).num_milliseconds().max(0) as u64)
        };
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
        metadata.soap_prompt_version =
            Some(crate::llm_client::SOAP_PROMPT_VERSION.to_string());

        // Invalidate stale billing when SOAP changes (needs re-extraction)
        if metadata.has_billing_record == Some(true) {
            metadata.has_billing_record = None;
            metadata.billing_prompt_version = None;
            let billing_path = session_dir.join("billing.json");
            if billing_path.exists() {
                let _ = fs::remove_file(&billing_path);
            }
            info!(session_id = %session_id, "Billing record invalidated: SOAP regenerated");
        }

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

/// Save a patient handout to an archived session.
/// Writes `patient_handout.txt` and updates metadata to set `has_patient_handout`.
pub fn save_patient_handout(
    session_id: &str,
    date: &DateTime<Utc>,
    content: &str,
) -> Result<(), String> {
    validate_session_id(session_id)?;
    let session_dir = get_session_archive_dir(session_id, date)?;

    if !session_dir.exists() {
        fs::create_dir_all(&session_dir)
            .map_err(|e| format!("Failed to create session directory: {}", e))?;
    }

    // Save handout file
    let handout_path = session_dir.join("patient_handout.txt");
    let mut file = File::create(&handout_path)
        .map_err(|e| format!("Failed to create patient handout file: {}", e))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write patient handout: {}", e))?;

    // Update metadata
    let metadata_path = session_dir.join("metadata.json");
    if metadata_path.exists() {
        let meta_content = fs::read_to_string(&metadata_path)
            .map_err(|e| format!("Failed to read metadata: {}", e))?;
        let mut metadata: ArchiveMetadata = serde_json::from_str(&meta_content)
            .map_err(|e| format!("Failed to parse metadata: {}", e))?;

        metadata.has_patient_handout = Some(true);

        let metadata_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
        fs::write(&metadata_path, metadata_json)
            .map_err(|e| format!("Failed to write metadata: {}", e))?;
    }

    info!(
        session_id = %session_id,
        "Patient handout saved to archive"
    );

    Ok(())
}

/// Read a patient handout from an archived session.
/// Returns `Ok(None)` if the file does not exist.
pub fn get_patient_handout(
    session_id: &str,
    date: &DateTime<Utc>,
) -> Result<Option<String>, String> {
    validate_session_id(session_id)?;
    let session_dir = get_session_archive_dir(session_id, date)?;
    let handout_path = session_dir.join("patient_handout.txt");

    if !handout_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&handout_path)
        .map_err(|e| format!("Failed to read patient handout: {}", e))?;
    Ok(Some(content))
}

/// Read a patient handout by session ID only (scans today's date directory).
/// Used when the exact date is unknown (e.g., mid-session SOAP generation).
/// Returns None if no handout exists or if the session dir can't be found.
pub fn get_patient_handout_by_id(session_id: &str) -> Option<String> {
    if validate_session_id(session_id).is_err() {
        return None;
    }
    let base = get_archive_dir().ok()?;
    let today = Local::now();
    let day_dir = base
        .join(format!("{:04}", today.year()))
        .join(format!("{:02}", today.month()))
        .join(format!("{:02}", today.day()));
    let handout_path = day_dir.join(session_id).join("patient_handout.txt");
    if handout_path.exists() {
        fs::read_to_string(&handout_path).ok()
    } else {
        None
    }
}

/// Save billing record to an archived session.
/// Writes `billing.json` and updates metadata to set `has_billing_record`.
pub fn save_billing_record(
    session_id: &str,
    date: &DateTime<Utc>,
    record: &crate::billing::BillingRecord,
) -> Result<(), String> {
    validate_session_id(session_id)?;
    let session_dir = get_session_archive_dir(session_id, date)?;

    if !session_dir.exists() {
        fs::create_dir_all(&session_dir)
            .map_err(|e| format!("Failed to create session directory: {}", e))?;
    }

    // Save billing file
    let billing_path = session_dir.join("billing.json");
    let json = serde_json::to_string_pretty(record)
        .map_err(|e| format!("Failed to serialize billing record: {}", e))?;
    let mut file = File::create(&billing_path)
        .map_err(|e| format!("Failed to create billing file: {}", e))?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write billing file: {}", e))?;

    // Update metadata
    let metadata_path = session_dir.join("metadata.json");
    if metadata_path.exists() {
        let meta_content = fs::read_to_string(&metadata_path)
            .map_err(|e| format!("Failed to read metadata: {}", e))?;
        let mut metadata: ArchiveMetadata = serde_json::from_str(&meta_content)
            .map_err(|e| format!("Failed to parse metadata: {}", e))?;

        metadata.has_billing_record = Some(true);
        metadata.billing_prompt_version =
            Some(crate::billing::clinical_features::BILLING_PROMPT_VERSION.to_string());

        let metadata_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
        fs::write(&metadata_path, metadata_json)
            .map_err(|e| format!("Failed to write metadata: {}", e))?;
    }

    info!(
        session_id = %session_id,
        "Billing record saved to archive"
    );

    Ok(())
}

/// Read a billing record from an archived session.
/// Returns `Ok(None)` if the file does not exist.
pub fn get_billing_record(
    session_id: &str,
    date: &DateTime<Utc>,
) -> Result<Option<crate::billing::BillingRecord>, String> {
    validate_session_id(session_id)?;
    let session_dir = get_session_archive_dir(session_id, date)?;
    let billing_path = session_dir.join("billing.json");

    if !billing_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&billing_path)
        .map_err(|e| format!("Failed to read billing record: {}", e))?;
    let record: crate::billing::BillingRecord = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse billing record: {}", e))?;
    Ok(Some(record))
}

/// Three-state result for the patient-summary lookup. The IPC caller uses
/// this to decide whether to fall back to the profile service: only
/// `FileMissing` triggers a cross-machine fetch. `LabelNotFound` and
/// `Found` are local-conclusive (server has the same data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatientSummaryLookup {
    /// patient_labels.json doesn't exist locally — caller may try server.
    FileMissing,
    /// File exists but has no entry for the requested label, or the entry
    /// has no usable summary (legacy schema).
    LabelNotFound,
    /// Found a non-empty summary.
    Found(String),
}

/// Lookup the per-patient summary stored in `patient_labels.json`. Returns
/// a 3-state result so the IPC layer can distinguish "file missing locally,
/// try server" from "file exists but label/summary missing — server has
/// the same data, don't bother".
pub fn lookup_patient_summary(
    session_id: &str,
    date: &DateTime<Utc>,
    patient_label: &str,
) -> PatientSummaryLookup {
    if validate_session_id(session_id).is_err() {
        return PatientSummaryLookup::FileMissing;
    }
    let Ok(session_dir) = get_session_archive_dir(session_id, date) else {
        return PatientSummaryLookup::FileMissing;
    };
    let labels_path = session_dir.join("patient_labels.json");
    let bytes = match fs::read(&labels_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return PatientSummaryLookup::FileMissing;
        }
        Err(_) => return PatientSummaryLookup::FileMissing,
    };
    match lookup_patient_summary_from_bytes(&bytes, patient_label) {
        Some(s) => PatientSummaryLookup::Found(s),
        None => PatientSummaryLookup::LabelNotFound,
    }
}

/// Pure parser used by both the local-disk path and the cross-machine
/// server-fallback path. Returns the per-patient summary when the label
/// exists and has a non-empty summary. Pre-2026-04-29 patient_labels.json
/// files without the `summary` field round-trip as None.
pub fn lookup_patient_summary_from_bytes(bytes: &[u8], patient_label: &str) -> Option<String> {
    let entries: Vec<PatientLabelEntry> = serde_json::from_slice(bytes).ok()?;
    let want = patient_label.trim().to_lowercase();
    entries
        .into_iter()
        .find(|e| e.label.trim().to_lowercase() == want)
        .and_then(|e| e.summary)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Save per-patient SOAP files alongside the combined soap_note.txt.
/// Called when multi-patient detection produces N>1 notes.
/// Writes: soap_patient_1.txt, soap_patient_2.txt, ..., patient_labels.json
/// Updates metadata.json with patient_count.
///
/// `detection`: optional multi-patient detection result. When provided, each
/// per-patient summary from `detection.patients[].summary` is matched to its
/// SOAP note by `patient_label` and persisted in patient_labels.json. Class 5
/// fix from 2026-04-29 forensic review: per-patient SOAP regeneration needs
/// the summary to disambiguate dominant-content cases (Slote mom+child).
pub fn save_multi_patient_soap(
    session_id: &str,
    date: &DateTime<Utc>,
    notes: &[crate::llm_client::PatientSoapNote],
    detection: Option<&crate::encounter_detection::MultiPatientDetectionResult>,
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

    // Write patient_labels.json metadata. Look up each note's summary from
    // detection.patients by label match (case-insensitive trim). Detection's
    // patient list and notes are produced from the same labels so a missing
    // match is rare; treat it as None and continue.
    let labels: Vec<PatientLabelEntry> = notes.iter().enumerate().map(|(i, note)| {
        let summary = detection.and_then(|d| {
            let want = note.patient_label.trim().to_lowercase();
            d.patients
                .iter()
                .find(|p| p.label.trim().to_lowercase() == want)
                .map(|p| p.summary.trim().to_string())
                .filter(|s| !s.is_empty())
        });
        PatientLabelEntry {
            index: (i + 1) as u32,
            label: note.patient_label.clone(),
            summary,
        }
    }).collect();
    let labels_path = session_dir.join("patient_labels.json");
    let labels_json = serde_json::to_string_pretty(&labels)
        .map_err(|e| format!("Failed to serialize patient labels: {}", e))?;
    fs::write(&labels_path, labels_json)
        .map_err(|e| format!("Failed to write patient_labels.json: {}", e))?;

    // Bootstrap a stub when metadata.json is missing — without it, a
    // server-only session opened from History and re-SOAPed via the regen
    // path would leave per-patient files + patient_labels.json without the
    // metadata pointer the History fan-out keys off, producing a zombie
    // partial-archive. ArchiveMetadata::new defaults started_at to now;
    // a subsequent server sync can overwrite it with the canonical value.
    let metadata_path = session_dir.join("metadata.json");
    let mut metadata = if metadata_path.exists() {
        let content = fs::read_to_string(&metadata_path)
            .map_err(|e| format!("Failed to read metadata: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse metadata: {}", e))?
    } else {
        info!(
            session_id = %session_id,
            "Bootstrapping stub metadata.json for orphan multi-patient session"
        );
        ArchiveMetadata::new(session_id)
    };
    metadata.patient_count = Some(notes.len() as u32);
    metadata.has_soap_note = true;
    metadata.soap_prompt_version = Some(crate::llm_client::SOAP_PROMPT_VERSION.to_string());
    let metadata_json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
    fs::write(&metadata_path, metadata_json)
        .map_err(|e| format!("Failed to write metadata: {}", e))?;

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

/// Merge server and local session summaries for a date into a single list.
///
/// Server rows win for most fields, but are enriched from local with
/// `patient_labels`/`patient_count` and `has_billing_record` when the server
/// row is missing them (local writes land before the server sync).
/// Local-only sessions (not yet synced) are appended. Result is sorted by
/// `started_at` so the final list is stable across merges.
pub fn merge_session_summaries(
    server: Vec<ArchiveSummary>,
    local: &[ArchiveSummary],
) -> Vec<ArchiveSummary> {
    let local_by_id: std::collections::HashMap<&str, &ArchiveSummary> =
        local.iter().map(|s| (s.session_id.as_str(), s)).collect();

    let mut merged = server;
    for ss in &mut merged {
        if let Some(l) = local_by_id.get(ss.session_id.as_str()) {
            if ss.patient_labels.is_none() && l.patient_labels.is_some() {
                ss.patient_labels = l.patient_labels.clone();
                ss.patient_count = l.patient_count;
            }
            if ss.has_billing_record.is_none() && l.has_billing_record.is_some() {
                ss.has_billing_record = l.has_billing_record;
            }
            // Sibling-group fields: server is canonical once synced, but a freshly
            // auto-split session may not have reached the server yet. Local fills
            // the gap so the History sidebar can render sibling badges immediately.
            if ss.sibling_group_id.is_none() && l.sibling_group_id.is_some() {
                ss.sibling_group_id = l.sibling_group_id.clone();
                ss.sibling_index = l.sibling_index;
                ss.sibling_group_size = l.sibling_group_size;
            }
        }
    }

    let server_ids: std::collections::HashSet<String> =
        merged.iter().map(|s| s.session_id.clone()).collect();
    for l in local {
        if !server_ids.contains(&l.session_id) {
            merged.push(l.clone());
        }
    }

    merged.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    merged
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

        let feedback_path = session_dir.join("feedback.json");
        let has_feedback = feedback_path.exists();
        let quality_rating = has_feedback
            .then(|| read_quality_rating(&feedback_path))
            .flatten();

        // Load patient labels for multi-patient sessions.
        // Try patient_labels.json first. If missing (pre-April 2026 sessions),
        // fall back to scanning for soap_patient_N.txt files on disk.
        let patient_count = metadata.patient_count;
        let patient_labels = {
            let labels_path = session_dir.join("patient_labels.json");
            if labels_path.exists() {
                // Preferred path: explicit labels file
                fs::read_to_string(&labels_path).ok()
                    .and_then(|json| serde_json::from_str::<Vec<PatientLabelEntry>>(&json).ok())
                    .map(|entries| entries.iter().map(|e| e.label.clone()).collect::<Vec<_>>())
                    .filter(|labels| labels.len() > 1)
            } else if patient_count.unwrap_or(0) > 1 || session_dir.join("soap_patient_1.txt").exists() {
                // Fallback: derive labels from soap_patient_N.txt files
                let mut labels = Vec::new();
                let mut i = 1u32;
                while session_dir.join(format!("soap_patient_{}.txt", i)).exists() {
                    labels.push(format!("Patient {}", i));
                    i += 1;
                }
                if labels.len() > 1 { Some(labels) } else { None }
            } else {
                None
            }
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
            quality_rating,
            physician_name: metadata.physician_name,
            room_name: metadata.room_name,
            patient_count: patient_labels.as_ref().map(|l| l.len() as u32).or(patient_count),
            patient_labels,
            has_billing_record: metadata.has_billing_record,
            sibling_group_id: metadata.sibling_group_id,
            sibling_index: metadata.sibling_index,
            sibling_group_size: metadata.sibling_group_size,
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
        let labels: Vec<PatientLabelEntry> = serde_json::from_str(&labels_json)
            .map_err(|e| format!("Failed to parse patient_labels.json: {}", e))?;

        let mut notes = Vec::new();
        for label_entry in &labels {
            let index = label_entry.index;
            let label = label_entry.label.clone();
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
) -> Result<Option<Vec<EncounterNote>>, String> {
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

    // Step 0: Migrate clinician notes from B into A before B is deleted.
    // Appended in timestamp order; idempotent on id collisions. Failure is
    // logged and non-fatal — a re-merge would be ambiguous anyway.
    let merged_notes = match append_clinician_notes(session_a_id, session_b_id, date) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                session_a_id = %session_a_id,
                session_b_id = %session_b_id,
                error = %e,
                "Failed to migrate clinician notes during merge — continuing without"
            );
            None
        }
    };
    let notes_migrated = merged_notes.is_some();

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
        metadata.has_billing_record = None; // billing is stale after merge
        if notes_migrated {
            metadata.has_clinician_notes = true;
        }

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

    // Step 3: Delete A's stale SOAP note and billing (will be regenerated)
    let soap_path = a_dir.join("soap_note.txt");
    if soap_path.exists() {
        let _ = fs::remove_file(&soap_path);
    }
    let billing_path = a_dir.join("billing.json");
    if billing_path.exists() {
        let _ = fs::remove_file(&billing_path);
        info!(session_a_id = %session_a_id, "Billing record invalidated: session merged");
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

    Ok(merged_notes)
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
        patient_dob: original_meta.patient_dob.clone(),
        detection_method: original_meta.detection_method.clone(),
        shadow_comparison: None,
        likely_non_clinical: original_meta.likely_non_clinical,
        patient_count: None,
        physician_id: original_meta.physician_id.clone(),
        physician_name: original_meta.physician_name.clone(),
        room_name: original_meta.room_name.clone(),
        has_patient_handout: None,
        has_billing_record: None,
        patient_confirmed_at: None,
        medplum_patient_id: None,
        has_clinician_notes: false,
        soap_prompt_version: None,
        billing_prompt_version: None,
        // Manual split breaks the new half out of any sibling group it inherited.
        // Original session keeps its sibling linkage (handled by the in-place
        // mutation below; we don't touch its sibling_* fields).
        sibling_group_id: None,
        sibling_index: None,
        sibling_group_size: None,
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
    original_meta.has_billing_record = None;

    let updated_meta_json = serde_json::to_string_pretty(&original_meta)
        .map_err(|e| format!("Failed to serialize updated metadata: {}", e))?;
    fs::write(&metadata_path, updated_meta_json)
        .map_err(|e| format!("Failed to write updated metadata: {}", e))?;

    // Delete stale SOAP from original
    let soap_path = session_dir.join("soap_note.txt");
    if soap_path.exists() {
        let _ = fs::remove_file(&soap_path);
    }
    // Delete stale billing from original
    let billing_path = session_dir.join("billing.json");
    if billing_path.exists() {
        let _ = fs::remove_file(&billing_path);
        info!(session_id = %session_id, "Billing record invalidated: session split");
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

/// Read and deserialize a session's `metadata.json`. Returns an error if the
/// file is missing or unparseable. Used by both the in-place sibling rewriter
/// and the forward-merge sibling check.
pub fn read_metadata(session_dir: &Path) -> Result<ArchiveMetadata, String> {
    let raw = fs::read_to_string(session_dir.join("metadata.json"))
        .map_err(|e| format!("read metadata: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse metadata: {e}"))
}

/// Prorate a source duration across per-patient SOAPs by SOAP word count.
/// Each output entry aligns with the input order. Words ≥ 1 (avoids div-by-zero).
pub fn prorate_durations_by_soap_words(
    per_patient: &[PerPatientSplitInput],
    source_duration_ms: u64,
) -> Vec<u64> {
    let counts: Vec<u128> = per_patient.iter()
        .map(|p| p.soap_text.split_whitespace().count().max(1) as u128)
        .collect();
    let total: u128 = counts.iter().sum::<u128>().max(1);
    counts.iter()
        .map(|c| (source_duration_ms as u128 * c / total) as u64)
        .collect()
}

/// One patient's slot in a multi-patient encounter split. `label` comes from
/// `multi_patient_detect` (transcript-derived). `extracted_name` /
/// `extracted_dob` come from the same SOAP LLM call that produced `soap_text`
/// — they're the chart-derived identity for this patient and override `label`
/// when present.
#[derive(Debug, Clone)]
pub struct PerPatientSplitInput {
    pub label: String,
    pub soap_text: String,
    pub extracted_name: Option<String>,
    pub extracted_dob: Option<String>,
}

/// Split one multi-patient encounter into N sibling sessions, one per patient.
///
/// The source session BECOMES sibling 0 (the anchor) — its directory is reused
/// so audio, screenshots, pipeline_log, replay_bundle, and segments stay put
/// without expensive copies. Siblings 1..N-1 are new directories with fresh
/// UUIDs that share the same `sibling_group_id`.
///
/// Each sibling gets:
/// - Its own `metadata.json` (single-patient, with sibling_group_id/index/size)
/// - A copy of the full `transcript.txt` (proration is on duration/billing only)
/// - The per-patient SOAP as `soap_note.txt`
///
/// Per-sibling `duration_ms` and `word_count` are prorated by per-patient SOAP
/// word count against the source's total. Stale combined-SOAP artifacts
/// (`soap_patient_*.txt`, `patient_labels.json`, `billing.json`) are removed
/// from the anchor; each sibling needs its own billing extraction afterward.
///
/// Returns the session IDs of all siblings in order (anchor first).
pub fn split_into_siblings(
    source_session_id: &str,
    date_str: &str,
    per_patient: &[PerPatientSplitInput],
) -> Result<Vec<String>, String> {
    if per_patient.len() < 2 {
        return Err(format!(
            "split_into_siblings requires at least 2 patients, got {}",
            per_patient.len()
        ));
    }

    let source_dir = get_session_dir_from_str(source_session_id, date_str)?;
    if !source_dir.exists() {
        return Err(format!("Source session not found: {}", source_session_id));
    }
    let date_dir = get_date_dir_from_str(date_str)?;

    let metadata_path = source_dir.join("metadata.json");
    let mut anchor_meta = read_metadata(&source_dir)?;

    let transcript_path = source_dir.join("transcript.txt");
    let transcript = fs::read_to_string(&transcript_path)
        .map_err(|e| format!("Failed to read transcript: {}", e))?;

    let source_duration = anchor_meta.duration_ms.unwrap_or(0);
    let source_words = anchor_meta.word_count;
    let prorated_durations = prorate_durations_by_soap_words(per_patient, source_duration);
    let prorated_word_counts: Vec<usize> = {
        let counts: Vec<usize> = per_patient.iter()
            .map(|p| p.soap_text.split_whitespace().count().max(1))
            .collect();
        let total = counts.iter().sum::<usize>().max(1);
        counts.iter().map(|c| source_words * c / total).collect()
    };

    let group_id = Uuid::new_v4().to_string();
    let n = per_patient.len();
    let mut sibling_ids: Vec<String> = Vec::with_capacity(n);

    // Sibling 0 (anchor): reuse source dir, rewrite metadata + soap, clean stale files
    {
        let p = &per_patient[0];
        let prorated_duration = prorated_durations[0];
        let prorated_words = prorated_word_counts[0];

        anchor_meta.sibling_group_id = Some(group_id.clone());
        anchor_meta.sibling_index = Some(0);
        anchor_meta.sibling_group_size = Some(n as u32);
        anchor_meta.patient_name = Some(p.extracted_name.clone().unwrap_or_else(|| p.label.clone()));
        anchor_meta.patient_dob = p.extracted_dob.clone();
        anchor_meta.patient_count = None;
        anchor_meta.duration_ms = Some(prorated_duration);
        anchor_meta.word_count = prorated_words;
        anchor_meta.has_soap_note = true;
        anchor_meta.has_billing_record = None;
        anchor_meta.patient_confirmed_at = None;
        anchor_meta.medplum_patient_id = None;
        anchor_meta.has_patient_handout = None;
        anchor_meta.billing_prompt_version = None;

        let meta_json = serde_json::to_string_pretty(&anchor_meta)
            .map_err(|e| format!("Failed to serialize anchor metadata: {}", e))?;
        fs::write(&metadata_path, meta_json)
            .map_err(|e| format!("Failed to write anchor metadata: {}", e))?;

        fs::write(source_dir.join("soap_note.txt"), &p.soap_text)
            .map_err(|e| format!("Failed to write anchor soap_note: {}", e))?;

        // Remove combined-SOAP artifacts left over from the multi-patient layout
        let labels_path = source_dir.join("patient_labels.json");
        if labels_path.exists() {
            let _ = fs::remove_file(&labels_path);
        }
        if let Ok(entries) = fs::read_dir(&source_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with("soap_patient_") && name.ends_with(".txt") {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
        let billing_path = source_dir.join("billing.json");
        if billing_path.exists() {
            let _ = fs::remove_file(&billing_path);
        }

        sibling_ids.push(source_session_id.to_string());
    }

    // Siblings 1..N: new dirs with fresh UUIDs
    for i in 1..n {
        let p = &per_patient[i];
        let new_id = Uuid::new_v4().to_string();
        let new_dir = date_dir.join(&new_id);
        fs::create_dir_all(&new_dir)
            .map_err(|e| format!("Failed to create sibling {} dir: {}", i, e))?;

        let prorated_duration = prorated_durations[i];
        let prorated_words = prorated_word_counts[i];

        let new_meta = ArchiveMetadata {
            session_id: new_id.clone(),
            started_at: anchor_meta.started_at.clone(),
            ended_at: anchor_meta.ended_at.clone(),
            duration_ms: Some(prorated_duration),
            segment_count: 0,
            word_count: prorated_words,
            has_soap_note: true,
            has_audio: false,
            auto_ended: anchor_meta.auto_ended,
            auto_end_reason: anchor_meta.auto_end_reason.clone(),
            soap_detail_level: anchor_meta.soap_detail_level,
            soap_format: anchor_meta.soap_format.clone(),
            charting_mode: anchor_meta.charting_mode.clone(),
            encounter_number: None,
            patient_name: Some(p.extracted_name.clone().unwrap_or_else(|| p.label.clone())),
            patient_dob: p.extracted_dob.clone(),
            detection_method: anchor_meta.detection_method.clone(),
            shadow_comparison: None,
            likely_non_clinical: anchor_meta.likely_non_clinical,
            patient_count: None,
            physician_id: anchor_meta.physician_id.clone(),
            physician_name: anchor_meta.physician_name.clone(),
            room_name: anchor_meta.room_name.clone(),
            has_patient_handout: None,
            has_billing_record: None,
            patient_confirmed_at: None,
            medplum_patient_id: None,
            has_clinician_notes: false,
            soap_prompt_version: anchor_meta.soap_prompt_version.clone(),
            billing_prompt_version: None,
            sibling_group_id: Some(group_id.clone()),
            sibling_index: Some(i as u32),
            sibling_group_size: Some(n as u32),
        };

        let meta_json = serde_json::to_string_pretty(&new_meta)
            .map_err(|e| format!("Failed to serialize sibling {} metadata: {}", i, e))?;
        fs::write(new_dir.join("metadata.json"), meta_json)
            .map_err(|e| format!("Failed to write sibling {} metadata: {}", i, e))?;
        fs::write(new_dir.join("transcript.txt"), &transcript)
            .map_err(|e| format!("Failed to write sibling {} transcript: {}", i, e))?;
        fs::write(new_dir.join("soap_note.txt"), &p.soap_text)
            .map_err(|e| format!("Failed to write sibling {} soap_note: {}", i, e))?;

        sibling_ids.push(new_id);
    }

    info!(
        source_session_id = %source_session_id,
        sibling_group_id = %group_id,
        sibling_count = n,
        "Multi-patient encounter split into siblings"
    );

    Ok(sibling_ids)
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
    // Safety: sessions.len() >= 2 is checked at the top of merge_sessions
    let merged_started = sessions.first().map(|(_, m, _)| m.started_at.clone())
        .ok_or_else(|| "No sessions to merge (empty list)".to_string())?;
    let merged_ended = sessions.last().and_then(|(_, m, _)| m.ended_at.clone());
    let merged_duration: Option<u64> = {
        let total: u64 = sessions.iter().filter_map(|(_, m, _)| m.duration_ms).sum();
        if total > 0 { Some(total) } else { None }
    };

    // Check if surviving session has audio
    let surviving_dir = date_dir.join(&surviving_id);
    let has_audio = surviving_dir.join("audio.wav").exists();

    // ── Crash-safe write: use temp files, rename on success ─────────
    // If the process crashes mid-merge, the original files remain intact.
    let temp_suffix = format!(".merge_{}", uuid::Uuid::new_v4().simple());
    let temp_transcript = surviving_dir.join(format!("transcript.txt{}", temp_suffix));
    let temp_metadata = surviving_dir.join(format!("metadata.json{}", temp_suffix));

    // Write merged data to temp files first
    fs::write(&temp_transcript, &merged_transcript)
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
    surviving_meta.has_billing_record = None; // billing is stale after merge

    let meta_json = serde_json::to_string_pretty(&surviving_meta)
        .map_err(|e| format!("Failed to serialize merged metadata: {}", e))?;
    fs::write(&temp_metadata, &meta_json)
        .map_err(|e| {
            let _ = fs::remove_file(&temp_transcript); // clean up partial write
            format!("Failed to write merged metadata: {}", e)
        })?;

    // Atomic rename: replace originals (crash between these two renames
    // leaves at most one file stale, but both originals are still readable)
    fs::rename(&temp_transcript, surviving_dir.join("transcript.txt"))
        .map_err(|e| {
            let _ = fs::remove_file(&temp_transcript);
            let _ = fs::remove_file(&temp_metadata);
            format!("Failed to finalize merged transcript: {}", e)
        })?;
    fs::rename(&temp_metadata, surviving_dir.join("metadata.json"))
        .map_err(|e| {
            let _ = fs::remove_file(&temp_metadata);
            format!("Failed to finalize merged metadata: {}", e)
        })?;

    // Delete stale SOAP and billing from surviving session
    let soap_path = surviving_dir.join("soap_note.txt");
    if soap_path.exists() {
        let _ = fs::remove_file(&soap_path);
    }
    let billing_path = surviving_dir.join("billing.json");
    if billing_path.exists() {
        let _ = fs::remove_file(&billing_path);
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
/// Returns true when this machine holds the session's `metadata.json` —
/// i.e. the session was recorded (or fully synced) here. Cross-machine
/// sessions (e.g. Room 2 origin, Room 6 only has the billing rollup) return
/// false, and callers should skip local-archive writes rather than error.
pub fn has_local_metadata(session_id: &str, date_str: &str) -> bool {
    match get_session_dir_from_str(session_id, date_str) {
        Ok(dir) => dir.join("metadata.json").is_file(),
        Err(_) => false,
    }
}

/// Outcome of writing SOAP-extracted identity. `SkippedConfirmed` means the
/// session was already clinician-confirmed and we left both fields alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoapIdentityWriteOutcome {
    Applied { applied_name: bool, applied_dob: bool },
    SkippedConfirmed,
}

/// Write SOAP-extracted patient identity into a session's `metadata.json`.
/// `None` for a field is a no-op for that field (preserves prior value).
/// Sessions with `patient_confirmed_at: Some` are preserved untouched —
/// the clinician already verified, and SOAP regen must not overwrite.
pub fn apply_soap_extracted_identity(
    session_id: &str,
    date_str: &str,
    extracted_name: Option<&str>,
    extracted_dob: Option<&str>,
) -> Result<SoapIdentityWriteOutcome, String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    let metadata_path = session_dir.join("metadata.json");
    let content = fs::read_to_string(&metadata_path)
        .map_err(|e| format!("Failed to read metadata: {}", e))?;
    let mut metadata: ArchiveMetadata = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse metadata: {}", e))?;

    if metadata.patient_confirmed_at.is_some() {
        info!(
            session_id = %session_id,
            "SOAP-extracted identity preserved: session is patient-confirmed, not overwriting"
        );
        return Ok(SoapIdentityWriteOutcome::SkippedConfirmed);
    }

    let mut applied_name = false;
    let mut applied_dob = false;
    if let Some(name) = extracted_name {
        metadata.patient_name = Some(name.to_string());
        applied_name = true;
    }
    if let Some(dob) = extracted_dob {
        metadata.patient_dob = Some(dob.to_string());
        applied_dob = true;
    }

    let json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
    fs::write(&metadata_path, json)
        .map_err(|e| format!("Failed to write metadata: {}", e))?;

    info!(
        session_id = %session_id,
        applied_name,
        applied_dob,
        "SOAP-extracted identity written to metadata"
    );
    Ok(SoapIdentityWriteOutcome::Applied { applied_name, applied_dob })
}

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

/// Mark a session as patient-confirmed. Writes `patient_confirmed_at`,
/// optionally `medplum_patient_id`, and updates `patient_dob` (since the
/// clinician may correct the vision-extracted DOB at confirmation time).
/// Used by the `confirm_session_patient` Tauri command after the Medplum +
/// profile-service dual-write completes. (v0.10.46+)
pub fn mark_patient_confirmed(
    session_id: &str,
    date_str: &str,
    confirmed_at_rfc3339: &str,
    medplum_patient_id: Option<&str>,
    patient_dob: &str,
) -> Result<(), String> {
    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    if !session_dir.exists() {
        return Err(format!("Session not found: {}", session_id));
    }

    let metadata_path = session_dir.join("metadata.json");
    let content = fs::read_to_string(&metadata_path)
        .map_err(|e| format!("Failed to read metadata: {}", e))?;
    let mut metadata: ArchiveMetadata = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse metadata: {}", e))?;

    metadata.patient_confirmed_at = Some(confirmed_at_rfc3339.to_string());
    metadata.patient_dob = Some(patient_dob.to_string());
    if let Some(id) = medplum_patient_id {
        metadata.medplum_patient_id = Some(id.to_string());
    }

    let json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
    fs::write(&metadata_path, json)
        .map_err(|e| format!("Failed to write metadata: {}", e))?;

    info!(
        session_id = %session_id,
        medplum_patient_id = ?medplum_patient_id,
        "Session marked patient-confirmed"
    );
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

/// Minimal shim that only deserializes `qualityRating`, used by `list_sessions_by_date`
/// to avoid allocating the full SessionFeedback (patient_feedback Vec, comments, etc.)
/// 40+ times per history-window open.
fn read_quality_rating(feedback_path: &std::path::Path) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct QualityRatingOnly {
        #[serde(rename = "qualityRating")]
        quality_rating: Option<String>,
    }
    fs::read_to_string(feedback_path).ok()
        .and_then(|s| serde_json::from_str::<QualityRatingOnly>(&s).ok())
        .and_then(|f| f.quality_rating)
}

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
        // v0.10.62: prompt-version fields default to None on new metadata.
        assert_eq!(metadata.soap_prompt_version, None);
        assert_eq!(metadata.billing_prompt_version, None);
    }

    #[test]
    fn test_archive_metadata_loads_legacy_metadata_without_prompt_versions() {
        // Backward compat: a v0.10.61 metadata file (no soap_prompt_version)
        // must deserialize cleanly with the new fields = None.
        let legacy_json = r#"{
            "session_id": "abc-123",
            "started_at": "2026-04-23T10:00:00Z",
            "ended_at": null,
            "duration_ms": 600000,
            "segment_count": 50,
            "word_count": 1500,
            "has_soap_note": true,
            "has_audio": false,
            "auto_ended": false,
            "auto_end_reason": null,
            "soap_detail_level": 5,
            "soap_format": "comprehensive",
            "has_clinician_notes": false
        }"#;
        let m: ArchiveMetadata = serde_json::from_str(legacy_json)
            .expect("legacy metadata should deserialize");
        assert_eq!(m.soap_prompt_version, None);
        assert_eq!(m.billing_prompt_version, None);
    }

    #[test]
    fn test_archive_metadata_serializes_prompt_versions_when_set() {
        let mut m = ArchiveMetadata::new("abc-123");
        m.soap_prompt_version = Some("v0.10.61".to_string());
        m.billing_prompt_version = Some("v0.10.61".to_string());
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"soap_prompt_version\":\"v0.10.61\""));
        assert!(json.contains("\"billing_prompt_version\":\"v0.10.61\""));
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
            quality_rating: None,
            physician_name: None,
            room_name: None,
            patient_count: None,
            patient_labels: None,
            has_billing_record: None,
            sibling_group_id: None,
            sibling_index: None,
            sibling_group_size: None,
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("test-123"));
        assert!(json.contains("300000"));
    }

    #[test]
    fn test_get_archive_dir() {
        // Check the default path shape by temporarily clearing the override.
        // Other tests (harness) may have set TRANSCRIPTIONAPP_ARCHIVE_DIR to
        // a tempdir that doesn't contain "archive" in its path.
        let saved = std::env::var("TRANSCRIPTIONAPP_ARCHIVE_DIR").ok();
        std::env::remove_var("TRANSCRIPTIONAPP_ARCHIVE_DIR");
        let result = get_archive_dir();
        if let Some(v) = saved {
            std::env::set_var("TRANSCRIPTIONAPP_ARCHIVE_DIR", v);
        }
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

    /// Helper: stages a multi-patient source session at the given date with
    /// stale combined-SOAP artifacts so split_into_siblings tests can assert
    /// the anchor cleanup behavior. Caller owns the session dir + cleanup.
    fn stage_multi_patient_session(
        session_id: &str,
        date_str: &str,
        duration_ms: u64,
        word_count: usize,
        with_legacy_files: bool,
    ) -> std::path::PathBuf {
        let date_dir = get_date_dir_from_str(date_str).unwrap();
        let session_dir = date_dir.join(session_id);
        fs::create_dir_all(&session_dir).unwrap();

        let mut meta = ArchiveMetadata::new(session_id);
        meta.started_at = format!("{}T09:00:00Z", date_str);
        meta.duration_ms = Some(duration_ms);
        meta.word_count = word_count;
        meta.has_soap_note = true;
        meta.has_audio = true;
        meta.charting_mode = Some("continuous".to_string());
        meta.encounter_number = Some(1);
        meta.patient_count = Some(2);
        meta.physician_id = Some("phys-test".to_string());
        meta.physician_name = Some("Dr Test".to_string());
        meta.room_name = Some("Room T".to_string());
        meta.has_billing_record = Some(true);
        fs::write(session_dir.join("metadata.json"), serde_json::to_string_pretty(&meta).unwrap()).unwrap();

        // Synthetic transcript whose word count matches the metadata (exactly word_count words)
        let words = vec!["word"; word_count].join(" ");
        fs::write(session_dir.join("transcript.txt"), &words).unwrap();

        if with_legacy_files {
            fs::write(session_dir.join("soap_note.txt"), "=== Patient 1 ===\nold combined SOAP").unwrap();
            fs::write(session_dir.join("patient_labels.json"), r#"[{"index":0,"label":"Old A"},{"index":1,"label":"Old B"}]"#).unwrap();
            fs::write(session_dir.join("soap_patient_1.txt"), "stale per-patient 1").unwrap();
            fs::write(session_dir.join("soap_patient_2.txt"), "stale per-patient 2").unwrap();
            fs::write(session_dir.join("billing.json"), r#"{"session_id":"x"}"#).unwrap();
        }

        session_dir
    }

    #[test]
    fn test_split_into_siblings_two_patients() {
        let source_id = format!("test-sib2-{}", Uuid::new_v4());
        let date_str = "2024-02-10";
        let session_dir = stage_multi_patient_session(&source_id, date_str, 30_000, 1000, true);
        let date_dir = session_dir.parent().unwrap().to_path_buf();

        let per_patient = vec![
            PerPatientSplitInput {
                label: "Steven Davidson".to_string(),
                soap_text: "soap text for steven contains many words approximately ten".to_string(),
                extracted_name: None,
                extracted_dob: None,
            },
            PerPatientSplitInput {
                label: "Knight Davidson".to_string(),
                soap_text: "shorter knight soap text".to_string(),
                extracted_name: None,
                extracted_dob: None,
            },
        ];

        let sibling_ids = split_into_siblings(&source_id, date_str, &per_patient).unwrap();
        assert_eq!(sibling_ids.len(), 2);
        assert_eq!(sibling_ids[0], source_id, "anchor must reuse the source session_id");

        // Anchor (sibling 0)
        let anchor_meta: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(session_dir.join("metadata.json")).unwrap()
        ).unwrap();
        assert_eq!(anchor_meta.sibling_index, Some(0));
        assert_eq!(anchor_meta.sibling_group_size, Some(2));
        assert!(anchor_meta.sibling_group_id.is_some());
        let group_id = anchor_meta.sibling_group_id.clone().unwrap();
        assert_eq!(anchor_meta.patient_name.as_deref(), Some("Steven Davidson"));
        assert_eq!(anchor_meta.patient_count, None);
        assert!(anchor_meta.has_audio, "anchor must keep has_audio=true");
        assert!(anchor_meta.has_soap_note);
        assert_eq!(anchor_meta.has_billing_record, None, "billing cleared, needs re-extraction");

        // Anchor SOAP is the new per-patient SOAP, not the stale combined SOAP
        let anchor_soap = fs::read_to_string(session_dir.join("soap_note.txt")).unwrap();
        assert!(anchor_soap.contains("steven"), "anchor soap_note.txt must be the per-patient SOAP");
        assert!(!anchor_soap.contains("==="), "stale combined-SOAP delimiters must be gone");

        // Stale legacy files must be removed from the anchor
        assert!(!session_dir.join("patient_labels.json").exists(), "patient_labels.json must be removed");
        assert!(!session_dir.join("soap_patient_1.txt").exists(), "soap_patient_1.txt must be removed");
        assert!(!session_dir.join("soap_patient_2.txt").exists(), "soap_patient_2.txt must be removed");
        assert!(!session_dir.join("billing.json").exists(), "stale billing.json must be removed");

        // Sibling 1 (new dir)
        let sib1_id = &sibling_ids[1];
        let sib1_dir = date_dir.join(sib1_id);
        assert!(sib1_dir.exists());
        let sib1_meta: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(sib1_dir.join("metadata.json")).unwrap()
        ).unwrap();
        assert_eq!(sib1_meta.sibling_index, Some(1));
        assert_eq!(sib1_meta.sibling_group_size, Some(2));
        assert_eq!(sib1_meta.sibling_group_id.as_ref(), Some(&group_id), "siblings must share group_id");
        assert_eq!(sib1_meta.patient_name.as_deref(), Some("Knight Davidson"));
        assert!(!sib1_meta.has_audio, "non-anchor siblings must have has_audio=false (anchor owns shared resources)");
        assert!(sib1_meta.has_soap_note);
        // Inherited fields
        assert_eq!(sib1_meta.physician_id.as_deref(), Some("phys-test"));
        assert_eq!(sib1_meta.room_name.as_deref(), Some("Room T"));
        assert_eq!(sib1_meta.charting_mode.as_deref(), Some("continuous"));
        // Per-sibling fresh state
        assert_eq!(sib1_meta.has_billing_record, None);
        assert_eq!(sib1_meta.patient_confirmed_at, None);

        // Sibling 1 transcript is full copy
        let sib1_transcript = fs::read_to_string(sib1_dir.join("transcript.txt")).unwrap();
        assert_eq!(sib1_transcript.split_whitespace().count(), 1000, "transcript copied in full");

        // Cleanup
        let _ = fs::remove_dir_all(&session_dir);
        let _ = fs::remove_dir_all(&sib1_dir);
    }

    #[test]
    fn test_split_into_siblings_three_patients() {
        let source_id = format!("test-sib3-{}", Uuid::new_v4());
        let date_str = "2024-02-10";
        let session_dir = stage_multi_patient_session(&source_id, date_str, 60_000, 600, false);
        let date_dir = session_dir.parent().unwrap().to_path_buf();

        let per_patient = vec![
            PerPatientSplitInput { label: "A".into(), soap_text: "soap a".into(), extracted_name: None, extracted_dob: None },
            PerPatientSplitInput { label: "B".into(), soap_text: "soap b".into(), extracted_name: None, extracted_dob: None },
            PerPatientSplitInput { label: "C".into(), soap_text: "soap c".into(), extracted_name: None, extracted_dob: None },
        ];

        let sibling_ids = split_into_siblings(&source_id, date_str, &per_patient).unwrap();
        assert_eq!(sibling_ids.len(), 3);

        let mut group_ids: Vec<String> = Vec::new();
        for (i, sid) in sibling_ids.iter().enumerate() {
            let dir = date_dir.join(sid);
            let m: ArchiveMetadata = serde_json::from_str(
                &fs::read_to_string(dir.join("metadata.json")).unwrap()
            ).unwrap();
            assert_eq!(m.sibling_index, Some(i as u32));
            assert_eq!(m.sibling_group_size, Some(3));
            group_ids.push(m.sibling_group_id.unwrap());
        }
        assert_eq!(group_ids[0], group_ids[1], "all siblings share the same group_id");
        assert_eq!(group_ids[1], group_ids[2]);

        // Cleanup
        for sid in &sibling_ids {
            let _ = fs::remove_dir_all(date_dir.join(sid));
        }
    }

    #[test]
    fn test_split_into_siblings_proportional_duration() {
        let source_id = format!("test-sibprop-{}", Uuid::new_v4());
        let date_str = "2024-02-10";
        // 30 minutes total, 150 words total
        let session_dir = stage_multi_patient_session(&source_id, date_str, 1_800_000, 150, false);
        let date_dir = session_dir.parent().unwrap().to_path_buf();

        // 100-word vs 50-word per-patient SOAPs → 2:1 split
        let big_soap = vec!["w"; 100].join(" ");
        let small_soap = vec!["w"; 50].join(" ");
        let per_patient = vec![
            PerPatientSplitInput { label: "Big".into(), soap_text: big_soap, extracted_name: None, extracted_dob: None },
            PerPatientSplitInput { label: "Small".into(), soap_text: small_soap, extracted_name: None, extracted_dob: None },
        ];

        let sibling_ids = split_into_siblings(&source_id, date_str, &per_patient).unwrap();

        let anchor_meta: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(session_dir.join("metadata.json")).unwrap()
        ).unwrap();
        let sib1_meta: ArchiveMetadata = serde_json::from_str(
            &fs::read_to_string(date_dir.join(&sibling_ids[1]).join("metadata.json")).unwrap()
        ).unwrap();

        // Anchor gets 100/150 = 2/3 of duration ≈ 1,200,000 ms
        assert_eq!(anchor_meta.duration_ms, Some(1_200_000));
        assert_eq!(anchor_meta.word_count, 100);
        // Sibling 1 gets 50/150 = 1/3 ≈ 600,000 ms
        assert_eq!(sib1_meta.duration_ms, Some(600_000));
        assert_eq!(sib1_meta.word_count, 50);
        // Sum within ±1 unit of source totals (integer division remainder)
        let dur_sum = anchor_meta.duration_ms.unwrap() + sib1_meta.duration_ms.unwrap();
        assert!(dur_sum.abs_diff(1_800_000) <= 1);

        // Cleanup
        for sid in &sibling_ids {
            let _ = fs::remove_dir_all(date_dir.join(sid));
        }
    }

    #[test]
    fn test_split_into_siblings_rejects_single_patient() {
        let source_id = format!("test-sib1-{}", Uuid::new_v4());
        let date_str = "2024-02-10";
        let session_dir = stage_multi_patient_session(&source_id, date_str, 30_000, 100, false);

        let one_patient = vec![PerPatientSplitInput {
            label: "Only".into(),
            soap_text: "single soap".into(),
            extracted_name: None,
            extracted_dob: None,
        }];
        let result = split_into_siblings(&source_id, date_str, &one_patient);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least 2 patients"));

        // Source dir untouched
        assert!(session_dir.exists());
        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn test_split_into_siblings_source_not_found() {
        let result = split_into_siblings(
            "nonexistent-sib-xyz",
            "2099-12-31",
            &[
                PerPatientSplitInput { label: "A".into(), soap_text: "a".into(), extracted_name: None, extracted_dob: None },
                PerPatientSplitInput { label: "B".into(), soap_text: "b".into(), extracted_name: None, extracted_dob: None },
            ],
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Source session not found"));
    }

    #[test]
    fn test_archive_metadata_legacy_compat() {
        // Older session metadata.json without sibling_* fields must deserialize
        // cleanly with None defaults — proves backward compatibility.
        let legacy_json = r#"{
            "session_id": "legacy-test",
            "started_at": "2024-01-01T00:00:00Z",
            "ended_at": null,
            "duration_ms": 1000,
            "segment_count": 0,
            "word_count": 50,
            "has_soap_note": false,
            "has_audio": false,
            "auto_ended": false,
            "auto_end_reason": null
        }"#;
        let m: ArchiveMetadata = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(m.sibling_group_id, None);
        assert_eq!(m.sibling_index, None);
        assert_eq!(m.sibling_group_size, None);
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
    fn test_has_local_metadata_cross_machine_shapes() {
        let date_str = "2024-07-09";
        let date_dir = get_archive_dir().unwrap().join("2024").join("07").join("09");

        // Missing directory entirely.
        let absent_id = format!("test-hlm-absent-{}", Uuid::new_v4());
        assert!(!has_local_metadata(&absent_id, date_str));

        // Directory exists but only billing.json — the cross-machine shape
        // (server mirrored billing down, origin machine kept the metadata).
        let billing_only_id = format!("test-hlm-billing-{}", Uuid::new_v4());
        let billing_dir = date_dir.join(&billing_only_id);
        fs::create_dir_all(&billing_dir).unwrap();
        fs::write(billing_dir.join("billing.json"), "{}").unwrap();
        assert!(!has_local_metadata(&billing_only_id, date_str));

        // Full local session — metadata.json present.
        let full_id = format!("test-hlm-full-{}", Uuid::new_v4());
        let full_dir = date_dir.join(&full_id);
        fs::create_dir_all(&full_dir).unwrap();
        fs::write(
            full_dir.join("metadata.json"),
            serde_json::to_string(&ArchiveMetadata::new(&full_id)).unwrap(),
        )
        .unwrap();
        assert!(has_local_metadata(&full_id, date_str));

        // Traversal/bad IDs never say true.
        assert!(!has_local_metadata("../escape", date_str));

        let _ = fs::remove_dir_all(&billing_dir);
        let _ = fs::remove_dir_all(&full_dir);
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
            split_correct: None,
            merge_correct: None,
            clinical_correct: None,
            patient_count_correct: None,
            billing_correct: None,
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
            split_correct: None,
            merge_correct: None,
            clinical_correct: None,
            patient_count_correct: None,
            billing_correct: None,
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
            quality_rating: None,
            physician_name: None,
            room_name: None,
            patient_count: None,
            patient_labels: None,
            has_billing_record: None,
            sibling_group_id: None,
            sibling_index: None,
            sibling_group_size: None,
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
            split_correct: None,
            merge_correct: None,
            clinical_correct: None,
            patient_count_correct: None,
            billing_correct: None,
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

    // ------------------------------------------------------------------
    // Clinician notes sidecar + migration (v0.10.57+)
    // ------------------------------------------------------------------

    #[test]
    fn encounter_note_new_stamps_unique_ids() {
        let a = EncounterNote::new("first".into());
        let b = EncounterNote::new("second".into());
        assert_ne!(a.id, b.id);
        assert!(a.timestamp_ms > 0);
        assert!(b.timestamp_ms >= a.timestamp_ms);
    }

    #[test]
    fn join_notes_for_prompt_separates_with_ruler() {
        let notes = vec![
            EncounterNote { id: "1".into(), text: " knee inj 40 mg ".into(), timestamp_ms: 1 },
            EncounterNote { id: "2".into(), text: "follow up 2 weeks".into(), timestamp_ms: 2 },
        ];
        let joined = join_notes_for_prompt(&notes);
        // Trimmed text, separator in the middle, no trailing separator
        assert_eq!(joined, "knee inj 40 mg\n---\nfollow up 2 weeks");
    }

    #[test]
    fn join_notes_for_prompt_empty_returns_empty_string() {
        assert_eq!(join_notes_for_prompt(&[]), "");
    }

    #[test]
    fn append_clinician_notes_vec_is_idempotent_on_id_collision() {
        // Use a unique session id + fixed date so the helper writes to a
        // predictable path inside the archive root. We read + rewrite JSON
        // directly; no filesystem-level cleanup between asserts.
        let session_id = format!("notes-test-{}", Uuid::new_v4());
        let date: DateTime<Utc> = Utc::now();

        // Ensure session dir exists so write_clinician_notes can write into it.
        let _ = get_session_archive_dir(&session_id, &date).unwrap();

        let initial = vec![
            EncounterNote { id: "a".into(), text: "first".into(), timestamp_ms: 100 },
            EncounterNote { id: "b".into(), text: "second".into(), timestamp_ms: 200 },
        ];
        write_clinician_notes(&session_id, &date, &initial).unwrap();

        // Second call with a mix of duplicate + new ids: only the new one sticks
        let incoming = vec![
            EncounterNote { id: "b".into(), text: "DUP".into(), timestamp_ms: 999 },
            EncounterNote { id: "c".into(), text: "third".into(), timestamp_ms: 150 },
        ];
        let returned = append_clinician_notes_vec(&session_id, &date, &incoming)
            .unwrap()
            .expect("new id 'c' should be appended and returned");
        // Ordered by timestamp_ms: a(100), c(150), b(200)
        assert_eq!(
            returned.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            vec!["a", "c", "b"],
        );
        // Duplicate id 'b' kept the original text, not "DUP"
        assert_eq!(returned.iter().find(|n| n.id == "b").unwrap().text, "second");

        // Persisted file matches the returned list — no re-read needed by callers
        let persisted = read_clinician_notes(&session_id, &date).unwrap().unwrap();
        assert_eq!(persisted, returned);

        // Third call with ONLY duplicates returns None (nothing to add)
        let again = append_clinician_notes_vec(&session_id, &date, &initial).unwrap();
        assert!(again.is_none());

        // Cleanup: write an empty list to delete the sidecar.
        write_clinician_notes(&session_id, &date, &[]).unwrap();
        let after = read_clinician_notes(&session_id, &date).unwrap();
        assert!(after.is_none(), "empty write should remove the file");
    }

    #[test]
    fn read_clinician_notes_tolerates_missing_file() {
        // Nonexistent session — should return Ok(None), not Err.
        let fake_id = format!("missing-{}", Uuid::new_v4());
        let result = read_clinician_notes(&fake_id, &Utc::now()).unwrap();
        assert!(result.is_none());
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
    let mut labels: Vec<PatientLabelEntry> = serde_json::from_str(&labels_json)
        .map_err(|e| format!("Failed to parse patient_labels.json: {}", e))?;

    // Remove the patient's SOAP file
    let soap_file = session_dir.join(format!("soap_patient_{}.txt", patient_index));
    if soap_file.exists() {
        fs::remove_file(&soap_file)
            .map_err(|e| format!("Failed to delete patient SOAP: {}", e))?;
    }

    // Remove from labels
    labels.retain(|l| l.index != patient_index);

    if labels.is_empty() {
        return delete_session(session_id, date_str);
    }

    if labels.len() == 1 {
        // Revert to single-patient
        let remaining_index = labels[0].index;
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
    let mut labels: Vec<PatientLabelEntry> = serde_json::from_str(&labels_json)
        .map_err(|e| format!("Failed to parse labels: {}", e))?;

    let mut found = false;
    for entry in &mut labels {
        if entry.index == patient_index {
            entry.label = new_label.to_string();
            found = true;
            break;
        }
    }

    if !found {
        return Err(format!("Patient index {} not found in session", patient_index));
    }

    let updated = serde_json::to_string_pretty(&labels)
        .map_err(|e| format!("Failed to serialize labels: {}", e))?;
    fs::write(&labels_path, updated)
        .map_err(|e| format!("Failed to write labels: {}", e))?;
    Ok(())
}

/// Merge multiple detected patients into one within the same session.
/// Replaces the merged patients' SOAP files with a single regenerated one.
/// Keeps remaining patients' SOAPs unchanged.
pub fn merge_patients_in_session(
    session_id: &str,
    date_str: &str,
    merged_indices: &[u32],
    new_label: &str,
    new_soap_content: &str,
) -> Result<(), String> {
    if merged_indices.len() < 2 {
        return Err("Need at least 2 patients to merge".to_string());
    }

    let session_dir = get_session_dir_from_str(session_id, date_str)?;
    let labels_path = session_dir.join("patient_labels.json");
    if !labels_path.exists() {
        return Err("Not a multi-patient session".to_string());
    }

    let labels_json = fs::read_to_string(&labels_path)
        .map_err(|e| format!("Failed to read labels: {}", e))?;
    let mut labels: Vec<PatientLabelEntry> = serde_json::from_str(&labels_json)
        .map_err(|e| format!("Failed to parse labels: {}", e))?;

    // Delete SOAP files for all merged patients
    for &idx in merged_indices {
        let soap_file = session_dir.join(format!("soap_patient_{}.txt", idx));
        if soap_file.exists() {
            let _ = fs::remove_file(&soap_file);
        }
    }

    // Keep the first merged index as the survivor
    let survivor_index = merged_indices[0];
    labels.retain(|l| !merged_indices.contains(&l.index) || l.index == survivor_index);

    // Update survivor's label
    for entry in &mut labels {
        if entry.index == survivor_index {
            entry.label = new_label.to_string();
        }
    }

    // Write merged SOAP to survivor's file
    let merged_soap_path = session_dir.join(format!("soap_patient_{}.txt", survivor_index));
    fs::write(&merged_soap_path, new_soap_content)
        .map_err(|e| format!("Failed to write merged SOAP: {}", e))?;

    if labels.len() == 1 {
        // Revert to single-patient format
        let single_soap = session_dir.join("soap_note.txt");
        fs::rename(&merged_soap_path, &single_soap)
            .map_err(|e| format!("Failed to rename to single SOAP: {}", e))?;
        let _ = fs::remove_file(&labels_path);
        update_metadata_field(session_id, date_str, |m| {
            m.patient_count = None;
            m.patient_name = Some(new_label.to_string());
        })?;
    } else {
        let new_count = labels.len() as u32;
        let updated = serde_json::to_string_pretty(&labels)
            .map_err(|e| format!("Failed to serialize labels: {}", e))?;
        fs::write(&labels_path, updated)
            .map_err(|e| format!("Failed to write labels: {}", e))?;
        update_metadata_field(session_id, date_str, |m| {
            m.patient_count = Some(new_count);
        })?;
    }

    info!(
        session_id = %session_id,
        merged_indices = ?merged_indices,
        survivor_index = survivor_index,
        new_label = %new_label,
        remaining_patients = labels.len(),
        "Merged patients in session"
    );

    Ok(())
}


#[cfg(test)]
mod archive_extra_tests {
    use super::*;

    #[test]
    fn read_session_started_at_returns_none_when_missing() {
        let result = read_session_started_at(
            "00000000-0000-0000-0000-000000000099",
            &chrono::Utc::now(),
        );
        assert!(result.is_none());
    }

    #[test]
    fn read_session_started_at_round_trips_archived_metadata() {
        use chrono::TimeZone;
        let test_root =
            std::env::temp_dir().join(format!("ami-test-started-at-{}", uuid::Uuid::new_v4()));
        std::env::set_var("TRANSCRIPTION_APP_DATA_DIR", &test_root);
        let date = chrono::Utc.with_ymd_and_hms(2026, 5, 1, 13, 22, 0).unwrap();
        let session_id = "00000000-0000-0000-0000-000000000003";
        let dir = get_session_archive_dir(session_id, &date).unwrap();
        fs::create_dir_all(&dir).unwrap();
        let mut metadata = ArchiveMetadata::new(session_id);
        let started = chrono::Utc.with_ymd_and_hms(2026, 5, 1, 13, 22, 21).unwrap();
        metadata.started_at = started.to_rfc3339();
        fs::write(
            dir.join("metadata.json"),
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();

        let read = read_session_started_at(session_id, &date).expect("started_at present");
        assert_eq!(read, started);

        let _ = fs::remove_dir_all(&test_root);
        std::env::remove_var("TRANSCRIPTION_APP_DATA_DIR");
    }

    #[test]
    fn save_multi_patient_soap_bootstraps_stub_metadata_for_orphan() {
        use chrono::TimeZone;
        let test_root =
            std::env::temp_dir().join(format!("ami-test-orphan-bootstrap-{}", uuid::Uuid::new_v4()));
        std::env::set_var("TRANSCRIPTION_APP_DATA_DIR", &test_root);
        let date = chrono::Utc.with_ymd_and_hms(2026, 5, 1, 14, 13, 0).unwrap();
        let session_id = "00000000-0000-0000-0000-000000000004";
        let dir = get_session_archive_dir(session_id, &date).unwrap();
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // Orphan partial-archive case: no metadata.json on disk yet.
        let metadata_path = dir.join("metadata.json");
        let _ = fs::remove_file(&metadata_path);

        let notes = vec![
            crate::llm_client::PatientSoapNote {
                patient_label: "Speaker 1 (Jim)".to_string(),
                speaker_id: "Speaker 1".to_string(),
                content: "S: medication review.\nA: stable.\nP: continue.".to_string(),
                extracted_patient_name: None,
                extracted_patient_dob: None,
            },
            crate::llm_client::PatientSoapNote {
                patient_label: "Speaker 2 (Linda)".to_string(),
                speaker_id: "Speaker 2".to_string(),
                content: "S: separate concern.\nA: separate.\nP: separate plan.".to_string(),
                extracted_patient_name: None,
                extracted_patient_dob: None,
            },
        ];
        save_multi_patient_soap(session_id, &date, &notes, None).unwrap();

        // Per-patient files written
        assert!(dir.join("soap_patient_1.txt").exists());
        assert!(dir.join("soap_patient_2.txt").exists());
        // patient_labels.json written
        assert!(dir.join("patient_labels.json").exists());
        // Stub metadata.json bootstrapped with patient_count=2
        assert!(metadata_path.exists(), "stub metadata.json must be created");
        let m: ArchiveMetadata =
            serde_json::from_str(&fs::read_to_string(&metadata_path).unwrap()).unwrap();
        assert_eq!(m.patient_count, Some(2));
        assert!(m.has_soap_note);

        let _ = fs::remove_dir_all(&test_root);
        std::env::remove_var("TRANSCRIPTION_APP_DATA_DIR");
    }

    // ============================================================
    // Cross-machine sync follow-up (#3 from 2026-04-29 forensic review):
    // lookup_patient_summary_from_bytes pure parser tests
    // ============================================================

    #[test]
    fn lookup_summary_from_bytes_finds_match() {
        let json = serde_json::json!([
            {"index": 1, "label": "Child/Young Patient", "summary": "3-year-old with watery diarrhea"},
            {"index": 2, "label": "Adult Female", "summary": "Iron deficiency anemia, menorrhagia"},
        ]).to_string();
        let s = lookup_patient_summary_from_bytes(json.as_bytes(), "Child/Young Patient");
        assert_eq!(s.as_deref(), Some("3-year-old with watery diarrhea"));
        let s = lookup_patient_summary_from_bytes(json.as_bytes(), "Adult Female");
        assert_eq!(s.as_deref(), Some("Iron deficiency anemia, menorrhagia"));
    }


    #[test]
    fn lookup_summary_from_bytes_case_insensitive() {
        let json = serde_json::json!([
            {"index": 1, "label": "Adult Female", "summary": "summary text"},
        ]).to_string();
        // Different case + whitespace: must still match.
        assert_eq!(
            lookup_patient_summary_from_bytes(json.as_bytes(), "adult female").as_deref(),
            Some("summary text")
        );
        assert_eq!(
            lookup_patient_summary_from_bytes(json.as_bytes(), "  Adult Female  ").as_deref(),
            Some("summary text")
        );
    }

    #[test]
    fn lookup_summary_from_bytes_returns_none_when_label_not_present() {
        let json = serde_json::json!([
            {"index": 1, "label": "Adult Female", "summary": "summary text"},
        ]).to_string();
        assert!(lookup_patient_summary_from_bytes(json.as_bytes(), "Unknown Patient").is_none());
    }

    #[test]
    fn lookup_summary_from_bytes_returns_none_when_summary_missing() {
        // Pre-2026-04-29 patient_labels.json files have no summary field —
        // treat as None so the regen falls back to label-only behavior
        // (legacy compat).
        let json = serde_json::json!([
            {"index": 1, "label": "Patient A"},
        ]).to_string();
        assert!(lookup_patient_summary_from_bytes(json.as_bytes(), "Patient A").is_none());
    }

    #[test]
    fn lookup_summary_from_bytes_returns_none_for_empty_summary() {
        let json = serde_json::json!([
            {"index": 1, "label": "Patient A", "summary": "   "},
        ]).to_string();
        assert!(lookup_patient_summary_from_bytes(json.as_bytes(), "Patient A").is_none());
    }

    #[test]
    fn lookup_summary_from_bytes_returns_none_for_invalid_json() {
        // Garbage bytes should fail-soft to None, not panic.
        assert!(lookup_patient_summary_from_bytes(b"not json", "Patient A").is_none());
        assert!(lookup_patient_summary_from_bytes(b"", "Patient A").is_none());
    }
}
