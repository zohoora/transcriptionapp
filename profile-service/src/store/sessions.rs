use crate::error::ApiError;
use crate::types::{
    ArchiveDetails, ArchiveMetadata, ArchiveSummary, ArchivedPatientNote, SessionFeedback,
};
use chrono::{Datelike, NaiveDate};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::info;
use uuid::Uuid;

/// Validate an ID to prevent path traversal.
fn validate_id(id: &str, label: &str) -> Result<(), ApiError> {
    if id.is_empty() {
        return Err(ApiError::BadRequest(format!("{label} must not be empty")));
    }
    if id.contains('/') || id.contains('\\') {
        return Err(ApiError::BadRequest(format!(
            "{label} must not contain path separators"
        )));
    }
    if id.contains("..") {
        return Err(ApiError::BadRequest(format!(
            "{label} must not contain '..'"
        )));
    }
    if id.contains('\0') {
        return Err(ApiError::BadRequest(format!(
            "{label} must not contain null bytes"
        )));
    }
    Ok(())
}

/// Whitelist of allowed auxiliary file names for session-level uploads via the
/// generic `PUT /sessions/:id/files/:filename` route. Each entry also has a
/// dedicated typed route where relevant (e.g. `PUT /sessions/:id/patient-handout`
/// for `patient_handout.txt`). The generic route is primarily used by the tauri
/// `server_sync::upload_aux_files()` helper which copies replay + billing
/// artifacts to the server; the typed routes are used by interactive UI actions.
const ALLOWED_SESSION_FILES: &[&str] = &[
    "pipeline_log.jsonl",
    "replay_bundle.json",
    "segments.jsonl",
    "billing.json",
    "patient_handout.txt",
    "clinician_notes.json",
];

/// Check if a filename is allowed for session file upload.
/// Supports exact matches from ALLOWED_SESSION_FILES and screenshots/*.jpg pattern.
fn is_allowed_session_file(filename: &str) -> bool {
    if ALLOWED_SESSION_FILES.contains(&filename) {
        return true;
    }
    // Allow screenshot files: screenshots/{name}.jpg with safe filename chars only
    if let Some(rest) = filename.strip_prefix("screenshots/") {
        return rest.ends_with(".jpg")
            && !rest.contains("..")
            && !rest.contains('/')
            && rest.len() < 100;
    }
    false
}

pub struct SessionStore {
    base_dir: PathBuf,
    session_cache: tokio::sync::RwLock<HashMap<(String, String), PathBuf>>,
}

impl SessionStore {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            session_cache: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    /// sessions/{physician_id}/{YYYY}/{MM}/{DD}/{session_id}/
    fn session_dir(
        &self,
        physician_id: &str,
        date: &NaiveDate,
        session_id: &str,
    ) -> Result<PathBuf, ApiError> {
        validate_id(physician_id, "Physician ID")?;
        validate_id(session_id, "Session ID")?;
        Ok(self
            .base_dir
            .join(physician_id)
            .join(format!("{:04}", date.year()))
            .join(format!("{:02}", date.month()))
            .join(format!("{:02}", date.day()))
            .join(session_id))
    }

    fn date_dir(&self, physician_id: &str, date: &NaiveDate) -> Result<PathBuf, ApiError> {
        validate_id(physician_id, "Physician ID")?;
        Ok(self
            .base_dir
            .join(physician_id)
            .join(format!("{:04}", date.year()))
            .join(format!("{:02}", date.month()))
            .join(format!("{:02}", date.day())))
    }

    fn physician_dir(&self, physician_id: &str) -> Result<PathBuf, ApiError> {
        validate_id(physician_id, "Physician ID")?;
        Ok(self.base_dir.join(physician_id))
    }

    /// Upload/create a session (idempotent by session_id)
    pub async fn upload_session(
        &self,
        physician_id: &str,
        session_id: &str,
        metadata: &ArchiveMetadata,
        transcript: &str,
        soap_note: Option<&str>,
    ) -> Result<(), ApiError> {
        let date = parse_date_from_started_at(&metadata.started_at)?;
        let dir = self.session_dir(physician_id, &date, session_id)?;
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to create session dir: {e}")))?;

        // Write metadata atomically
        let meta_json = serde_json::to_string_pretty(metadata)
            .map_err(|e| ApiError::Internal(format!("Failed to serialize metadata: {e}")))?;
        atomic_write(&dir.join("metadata.json"), &meta_json).await?;

        // Write transcript atomically
        atomic_write(&dir.join("transcript.txt"), transcript.as_bytes()).await?;

        // Write SOAP if provided (atomically)
        if let Some(soap) = soap_note {
            atomic_write(&dir.join("soap_note.txt"), soap.as_bytes()).await?;
        }

        // Cache this session path
        {
            let mut cache = self.session_cache.write().await;
            cache.insert(
                (physician_id.to_string(), session_id.to_string()),
                dir.clone(),
            );
        }

        info!(physician_id, session_id, "Session uploaded");
        Ok(())
    }

    /// Update SOAP note
    pub async fn update_soap(
        &self,
        physician_id: &str,
        session_id: &str,
        content: &str,
        detail_level: Option<u8>,
        format: Option<&str>,
    ) -> Result<(), ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;

        atomic_write(&dir.join("soap_note.txt"), content.as_bytes()).await?;

        // Update metadata
        if let Ok(mut meta) = self.read_metadata(&dir).await {
            meta.has_soap_note = true;
            meta.soap_detail_level = detail_level;
            meta.soap_format = format.map(|s| s.to_string());
            self.write_metadata(&dir, &meta).await?;
        }

        info!(physician_id, session_id, "SOAP updated");
        Ok(())
    }

    /// Update metadata (full replacement)
    pub async fn update_metadata(
        &self,
        physician_id: &str,
        session_id: &str,
        metadata: &ArchiveMetadata,
    ) -> Result<(), ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;
        self.write_metadata(&dir, metadata).await?;
        info!(physician_id, session_id, "Metadata updated");
        Ok(())
    }

    /// Patch metadata by merging provided fields into the existing metadata.
    /// Accepts partial JSON — only non-null fields are overwritten.
    pub async fn patch_metadata(
        &self,
        physician_id: &str,
        session_id: &str,
        patch: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;
        let meta_path = dir.join("metadata.json");
        let existing_json = tokio::fs::read_to_string(&meta_path)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to read metadata: {e}")))?;
        let mut existing: serde_json::Value = serde_json::from_str(&existing_json)
            .map_err(|e| ApiError::Internal(format!("Failed to parse metadata: {e}")))?;

        // Merge patch fields into existing metadata
        if let (Some(base), Some(patch_obj)) = (existing.as_object_mut(), patch.as_object()) {
            for (key, value) in patch_obj {
                if !value.is_null() {
                    base.insert(key.clone(), value.clone());
                }
            }
        }

        let merged_json = serde_json::to_string_pretty(&existing)
            .map_err(|e| ApiError::Internal(format!("Failed to serialize metadata: {e}")))?;
        atomic_write(&meta_path, &merged_json).await?;
        info!(physician_id, session_id, "Metadata patched");
        Ok(())
    }

    /// Update feedback
    pub async fn update_feedback(
        &self,
        physician_id: &str,
        session_id: &str,
        feedback: &SessionFeedback,
    ) -> Result<(), ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;
        let json = serde_json::to_string_pretty(feedback)
            .map_err(|e| ApiError::Internal(format!("Failed to serialize feedback: {e}")))?;
        atomic_write(&dir.join("feedback.json"), &json).await?;
        info!(physician_id, session_id, "Feedback updated");
        Ok(())
    }

    /// Update patient name
    pub async fn update_patient_name(
        &self,
        physician_id: &str,
        session_id: &str,
        patient_name: &str,
    ) -> Result<(), ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;
        let mut meta = self.read_metadata(&dir).await?;
        meta.patient_name = Some(patient_name.to_string());
        self.write_metadata(&dir, &meta).await?;
        info!(physician_id, session_id, patient_name, "Patient name updated");
        Ok(())
    }

    /// Get full session details
    pub async fn get_session(
        &self,
        physician_id: &str,
        session_id: &str,
    ) -> Result<ArchiveDetails, ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;
        let metadata = self.read_metadata(&dir).await?;

        let transcript = read_optional_file(&dir.join("transcript.txt")).await;
        let soap_note = read_optional_file(&dir.join("soap_note.txt")).await;

        let audio_path = if dir.join("audio.wav").exists() {
            Some(dir.join("audio.wav").to_string_lossy().to_string())
        } else {
            None
        };

        // Read per-patient notes
        let patient_notes = self.read_patient_notes(&dir).await;

        Ok(ArchiveDetails {
            session_id: session_id.to_string(),
            metadata,
            transcript,
            soap_note,
            audio_path,
            patient_notes,
        })
    }

    /// Get session feedback
    pub async fn get_feedback(
        &self,
        physician_id: &str,
        session_id: &str,
    ) -> Result<Option<SessionFeedback>, ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;
        let path = dir.join("feedback.json");
        if !path.exists() {
            return Ok(None);
        }
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to read feedback: {e}")))?;
        let feedback: SessionFeedback = serde_json::from_str(&content)
            .map_err(|e| ApiError::Internal(format!("Failed to parse feedback: {e}")))?;
        Ok(Some(feedback))
    }

    /// Delete a session
    pub async fn delete_session(
        &self,
        physician_id: &str,
        session_id: &str,
    ) -> Result<(), ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;
        tokio::fs::remove_dir_all(&dir)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to delete session: {e}")))?;

        // Invalidate cache
        {
            let mut cache = self.session_cache.write().await;
            cache.remove(&(physician_id.to_string(), session_id.to_string()));
        }

        info!(physician_id, session_id, "Session deleted");
        Ok(())
    }

    /// Get transcript lines for split view
    pub async fn get_transcript_lines(
        &self,
        physician_id: &str,
        session_id: &str,
    ) -> Result<Vec<String>, ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;
        let path = dir.join("transcript.txt");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to read transcript: {e}")))?;
        Ok(content.lines().map(|l| l.to_string()).collect())
    }

    /// Split a session at a line boundary, returns new session_id
    pub async fn split_session(
        &self,
        physician_id: &str,
        session_id: &str,
        split_line: usize,
    ) -> Result<String, ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;
        let meta = self.read_metadata(&dir).await?;
        let date = parse_date_from_started_at(&meta.started_at)?;

        let transcript_path = dir.join("transcript.txt");
        let content = tokio::fs::read_to_string(&transcript_path)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to read transcript: {e}")))?;
        let lines: Vec<&str> = content.lines().collect();

        if split_line == 0 || split_line >= lines.len() {
            return Err(ApiError::BadRequest(format!(
                "Invalid split line {split_line}, transcript has {} lines",
                lines.len()
            )));
        }

        // Backup original transcript before modifying
        let backup_path = dir.join("transcript.txt.bak");
        tokio::fs::copy(&transcript_path, &backup_path)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to backup transcript: {e}")))?;

        let first_half = lines[..split_line].join("\n");
        let second_half = lines[split_line..].join("\n");

        // Update original session atomically
        atomic_write(&transcript_path, first_half.as_bytes()).await?;
        let mut first_meta = meta.clone();
        first_meta.word_count = first_half.split_whitespace().count();
        first_meta.has_soap_note = false;
        self.write_metadata(&dir, &first_meta).await?;
        // Remove SOAP (now invalid after split)
        let _ = tokio::fs::remove_file(dir.join("soap_note.txt")).await;

        // Create new session for second half
        let new_id = Uuid::new_v4().to_string();
        let new_dir = self.session_dir(physician_id, &date, &new_id)?;
        tokio::fs::create_dir_all(&new_dir)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to create new session dir: {e}")))?;

        atomic_write(&new_dir.join("transcript.txt"), second_half.as_bytes()).await?;

        let mut second_meta = meta;
        second_meta.session_id = new_id.clone();
        second_meta.word_count = second_half.split_whitespace().count();
        second_meta.has_soap_note = false;
        second_meta.has_audio = false;
        if let Some(enc) = second_meta.encounter_number {
            second_meta.encounter_number = Some(enc + 1);
        }
        self.write_metadata(&new_dir, &second_meta).await?;

        // Split succeeded — remove backup and update cache
        let _ = tokio::fs::remove_file(&backup_path).await;
        {
            let mut cache = self.session_cache.write().await;
            cache.insert(
                (physician_id.to_string(), new_id.clone()),
                new_dir,
            );
        }

        info!(
            physician_id,
            session_id,
            new_session_id = %new_id,
            split_line,
            "Session split"
        );
        Ok(new_id)
    }

    /// Merge two sessions
    pub async fn merge_sessions(
        &self,
        physician_id: &str,
        session_ids: &[String],
        date_str: &str,
    ) -> Result<String, ApiError> {
        if session_ids.len() != 2 {
            return Err(ApiError::BadRequest(
                "Merge requires exactly 2 session IDs".to_string(),
            ));
        }
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|e| ApiError::BadRequest(format!("Invalid date: {e}")))?;

        let dir_a = self.session_dir(physician_id, &date, &session_ids[0])?;
        let dir_b = self.session_dir(physician_id, &date, &session_ids[1])?;

        if !dir_a.exists() {
            return Err(ApiError::NotFound(format!(
                "Session not found: {}",
                session_ids[0]
            )));
        }
        if !dir_b.exists() {
            return Err(ApiError::NotFound(format!(
                "Session not found: {}",
                session_ids[1]
            )));
        }

        let meta_a = self.read_metadata(&dir_a).await?;
        let transcript_a = read_optional_file(&dir_a.join("transcript.txt"))
            .await
            .unwrap_or_default();
        let transcript_b = read_optional_file(&dir_b.join("transcript.txt"))
            .await
            .unwrap_or_default();

        // Backup both transcripts before modifying
        let backup_a = dir_a.join("transcript.txt.bak");
        if dir_a.join("transcript.txt").exists() {
            tokio::fs::copy(dir_a.join("transcript.txt"), &backup_a)
                .await
                .map_err(|e| ApiError::Internal(format!("Failed to backup transcript A: {e}")))?;
        }
        let backup_b = dir_b.join("transcript.txt.bak");
        if dir_b.join("transcript.txt").exists() {
            tokio::fs::copy(dir_b.join("transcript.txt"), &backup_b)
                .await
                .map_err(|e| ApiError::Internal(format!("Failed to backup transcript B: {e}")))?;
        }

        // Merge into first session atomically
        let merged_transcript = format!("{}\n{}", transcript_a.trim(), transcript_b.trim());
        atomic_write(&dir_a.join("transcript.txt"), merged_transcript.as_bytes()).await?;

        let mut merged_meta = meta_a;
        merged_meta.word_count = merged_transcript.split_whitespace().count();
        merged_meta.has_soap_note = false;
        self.write_metadata(&dir_a, &merged_meta).await?;
        let _ = tokio::fs::remove_file(dir_a.join("soap_note.txt")).await;

        // Delete second session
        let _ = tokio::fs::remove_dir_all(&dir_b).await;

        // Merge succeeded — remove backup and update cache
        let _ = tokio::fs::remove_file(&backup_a).await;
        {
            let mut cache = self.session_cache.write().await;
            cache.remove(&(physician_id.to_string(), session_ids[1].clone()));
        }

        info!(
            physician_id,
            kept = %session_ids[0],
            deleted = %session_ids[1],
            "Sessions merged"
        );
        Ok(session_ids[0].clone())
    }

    /// Renumber encounters for a date
    pub async fn renumber_encounters(
        &self,
        physician_id: &str,
        date_str: &str,
    ) -> Result<(), ApiError> {
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|e| ApiError::BadRequest(format!("Invalid date: {e}")))?;
        let date_dir = self.date_dir(physician_id, &date)?;

        if !date_dir.exists() {
            return Ok(());
        }

        let mut sessions = Vec::new();
        let mut entries = tokio::fs::read_dir(&date_dir)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to read date dir: {e}")))?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(meta) = self.read_metadata(&path).await {
                    sessions.push((path, meta));
                }
            }
        }

        // Sort by started_at
        sessions.sort_by(|a, b| a.1.started_at.cmp(&b.1.started_at));

        for (i, (dir, mut meta)) in sessions.into_iter().enumerate() {
            meta.encounter_number = Some((i + 1) as u32);
            self.write_metadata(&dir, &meta).await?;
        }

        info!(physician_id, date = date_str, "Encounters renumbered");
        Ok(())
    }

    /// List dates that have sessions for a physician
    pub async fn list_dates(
        &self,
        physician_id: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<Vec<String>, ApiError> {
        let phys_dir = self.physician_dir(physician_id)?;
        if !phys_dir.exists() {
            return Ok(Vec::new());
        }

        let from_date = from
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
        let to_date = to
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

        let mut dates = Vec::new();

        // Walk year/month/day directories
        let mut years = tokio::fs::read_dir(&phys_dir)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to read physician dir: {e}")))?;

        while let Ok(Some(year_entry)) = years.next_entry().await {
            let year_path = year_entry.path();
            if !year_path.is_dir() {
                continue;
            }
            let mut months = match tokio::fs::read_dir(&year_path).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            while let Ok(Some(month_entry)) = months.next_entry().await {
                let month_path = month_entry.path();
                if !month_path.is_dir() {
                    continue;
                }
                let mut days = match tokio::fs::read_dir(&month_path).await {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                while let Ok(Some(day_entry)) = days.next_entry().await {
                    let day_path = day_entry.path();
                    if !day_path.is_dir() {
                        continue;
                    }
                    // Check if this day has session subdirectories
                    let has_sessions = match tokio::fs::read_dir(&day_path).await {
                        Ok(mut entries) => entries.next_entry().await.ok().flatten().is_some(),
                        Err(_) => false,
                    };
                    if !has_sessions {
                        continue;
                    }

                    if let (Some(y), Some(m), Some(d)) = (
                        year_path.file_name().and_then(|n| n.to_str()),
                        month_path.file_name().and_then(|n| n.to_str()),
                        day_path.file_name().and_then(|n| n.to_str()),
                    ) {
                        let date_str = format!("{y}-{m}-{d}");
                        // Apply date range filter
                        if let Ok(date) = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
                            if let Some(ref f) = from_date {
                                if date < *f {
                                    continue;
                                }
                            }
                            if let Some(ref t) = to_date {
                                if date > *t {
                                    continue;
                                }
                            }
                        }
                        dates.push(date_str);
                    }
                }
            }
        }

        dates.sort();
        dates.reverse();
        Ok(dates)
    }

    /// List sessions for a specific date
    pub async fn list_sessions(
        &self,
        physician_id: &str,
        date_str: &str,
    ) -> Result<Vec<ArchiveSummary>, ApiError> {
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|e| ApiError::BadRequest(format!("Invalid date: {e}")))?;
        let date_dir = self.date_dir(physician_id, &date)?;

        if !date_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        let mut entries = tokio::fs::read_dir(&date_dir)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to read date dir: {e}")))?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if let Ok(meta) = self.read_metadata(&path).await {
                let feedback_path = path.join("feedback.json");
                let has_feedback = feedback_path.exists();
                let quality_rating = if has_feedback {
                    read_quality_rating(&feedback_path).await
                } else {
                    None
                };
                sessions.push(ArchiveSummary {
                    session_id: meta.session_id.clone(),
                    date: date_str.to_string(),
                    started_at: Some(meta.started_at.clone()),
                    duration_ms: meta.duration_ms,
                    word_count: meta.word_count,
                    has_soap_note: meta.has_soap_note,
                    has_audio: meta.has_audio,
                    auto_ended: meta.auto_ended,
                    charting_mode: meta.charting_mode,
                    encounter_number: meta.encounter_number,
                    patient_name: meta.patient_name,
                    likely_non_clinical: meta.likely_non_clinical,
                    has_feedback: Some(has_feedback),
                    quality_rating,
                    physician_name: meta.physician_name,
                    room_name: meta.room_name,
                    has_billing_record: meta.has_billing_record,
                });
            }
        }

        // Sort by started_at via encounter_number or session_id
        sessions.sort_by(|a, b| {
            a.encounter_number
                .cmp(&b.encounter_number)
                .then(a.session_id.cmp(&b.session_id))
        });

        Ok(sessions)
    }

    /// Get the audio file path for a session
    pub async fn get_audio_path(
        &self,
        physician_id: &str,
        session_id: &str,
    ) -> Result<PathBuf, ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;
        let path = dir.join("audio.wav");
        if !path.exists() {
            return Err(ApiError::NotFound("Audio file not found".to_string()));
        }
        Ok(path)
    }

    /// Save audio file for a session
    pub async fn save_audio(
        &self,
        physician_id: &str,
        session_id: &str,
        data: &[u8],
    ) -> Result<(), ApiError> {
        let dir = self.find_session_dir(physician_id, session_id).await?;
        let path = dir.join("audio.wav");
        tokio::fs::write(&path, data)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to write audio: {e}")))?;

        // Update metadata
        if let Ok(mut meta) = self.read_metadata(&dir).await {
            meta.has_audio = true;
            let _ = self.write_metadata(&dir, &meta).await;
        }

        info!(physician_id, session_id, bytes = data.len(), "Audio saved");
        Ok(())
    }

    /// Save an auxiliary file to a session directory
    pub async fn save_session_file(
        &self,
        physician_id: &str,
        session_id: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<(), ApiError> {
        if !is_allowed_session_file(filename) {
            return Err(ApiError::BadRequest(format!("File not allowed: {filename}")));
        }
        let dir = self.find_session_dir(physician_id, session_id).await?;
        let file_path = dir.join(filename);
        // Create parent directories for subdirectory files (e.g., screenshots/)
        if let Some(parent) = file_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        tokio::fs::write(&file_path, data)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to write {filename}: {e}")))?;
        info!(physician_id, session_id, filename, bytes = data.len(), "Session file saved");
        Ok(())
    }

    /// Read an auxiliary file from a session directory
    pub async fn get_session_file(
        &self,
        physician_id: &str,
        session_id: &str,
        filename: &str,
    ) -> Result<Vec<u8>, ApiError> {
        if !is_allowed_session_file(filename) {
            return Err(ApiError::BadRequest(format!("File not allowed: {filename}")));
        }
        let dir = self.find_session_dir(physician_id, session_id).await?;
        let path = dir.join(filename);
        if !path.exists() {
            return Err(ApiError::NotFound(format!("File not found: {filename}")));
        }
        tokio::fs::read(&path)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to read {filename}: {e}")))
    }

    /// Save day log to a date directory
    pub async fn save_day_log(
        &self,
        physician_id: &str,
        date_str: &str,
        data: &[u8],
    ) -> Result<(), ApiError> {
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|e| ApiError::BadRequest(format!("Invalid date: {e}")))?;
        let dir = self.date_dir(physician_id, &date)?;
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to create date dir: {e}")))?;
        tokio::fs::write(dir.join("day_log.jsonl"), data)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to write day_log: {e}")))?;
        info!(physician_id, date = date_str, bytes = data.len(), "Day log saved");
        Ok(())
    }

    /// Read day log from a date directory
    pub async fn get_day_log(
        &self,
        physician_id: &str,
        date_str: &str,
    ) -> Result<Vec<u8>, ApiError> {
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|e| ApiError::BadRequest(format!("Invalid date: {e}")))?;
        let dir = self.date_dir(physician_id, &date)?;
        let path = dir.join("day_log.jsonl");
        if !path.exists() {
            return Err(ApiError::NotFound("Day log not found".to_string()));
        }
        tokio::fs::read(&path)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to read day_log: {e}")))
    }

    // ── Helpers ────────────────────────────────────────────────────

    /// Find session directory by scanning date directories (session_id is unique).
    /// Results are cached in memory to avoid repeated directory walks.
    async fn find_session_dir(
        &self,
        physician_id: &str,
        session_id: &str,
    ) -> Result<PathBuf, ApiError> {
        validate_id(physician_id, "Physician ID")?;
        validate_id(session_id, "Session ID")?;

        let cache_key = (physician_id.to_string(), session_id.to_string());

        // Check cache first
        {
            let cache = self.session_cache.read().await;
            if let Some(cached) = cache.get(&cache_key) {
                if cached.exists() {
                    return Ok(cached.clone());
                }
                // Cached path no longer valid — fall through to walk
            }
        }

        let phys_dir = self.base_dir.join(physician_id);
        if !phys_dir.exists() {
            return Err(ApiError::NotFound(format!(
                "Session not found: {session_id}"
            )));
        }

        // Walk year/month/day to find the session
        let mut years = tokio::fs::read_dir(&phys_dir)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to read dir: {e}")))?;

        while let Ok(Some(year_entry)) = years.next_entry().await {
            let year_path = year_entry.path();
            if !year_path.is_dir() {
                continue;
            }
            let mut months = match tokio::fs::read_dir(&year_path).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            while let Ok(Some(month_entry)) = months.next_entry().await {
                let month_path = month_entry.path();
                if !month_path.is_dir() {
                    continue;
                }
                let mut days = match tokio::fs::read_dir(&month_path).await {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                while let Ok(Some(day_entry)) = days.next_entry().await {
                    let day_path = day_entry.path();
                    if !day_path.is_dir() {
                        continue;
                    }
                    let candidate = day_path.join(session_id);
                    if candidate.exists() && candidate.is_dir() {
                        // Cache the result
                        let mut cache = self.session_cache.write().await;
                        cache.insert(cache_key, candidate.clone());
                        return Ok(candidate);
                    }
                }
            }
        }

        // Remove stale entry if it was cached but not found on disk
        {
            let mut cache = self.session_cache.write().await;
            cache.remove(&cache_key);
        }

        Err(ApiError::NotFound(format!(
            "Session not found: {session_id}"
        )))
    }

    async fn read_metadata(&self, session_dir: &PathBuf) -> Result<ArchiveMetadata, ApiError> {
        let path = session_dir.join("metadata.json");
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to read metadata: {e}")))?;
        serde_json::from_str(&content)
            .map_err(|e| ApiError::Internal(format!("Failed to parse metadata: {e}")))
    }

    async fn write_metadata(
        &self,
        session_dir: &PathBuf,
        metadata: &ArchiveMetadata,
    ) -> Result<(), ApiError> {
        let json = serde_json::to_string_pretty(metadata)
            .map_err(|e| ApiError::Internal(format!("Failed to serialize metadata: {e}")))?;
        atomic_write(&session_dir.join("metadata.json"), &json).await
    }

    async fn read_patient_notes(&self, session_dir: &PathBuf) -> Option<Vec<ArchivedPatientNote>> {
        let labels_path = session_dir.join("patient_labels.json");
        if !labels_path.exists() {
            return None;
        }
        let labels_content = tokio::fs::read_to_string(&labels_path).await.ok()?;
        let labels: Vec<serde_json::Value> = serde_json::from_str(&labels_content).ok()?;

        let mut notes = Vec::new();
        for label_val in &labels {
            let index = label_val.get("index")?.as_u64()? as u32;
            let label = label_val.get("label")?.as_str()?.to_string();
            let file = format!("soap_patient_{}.txt", index);
            let content = tokio::fs::read_to_string(session_dir.join(&file))
                .await
                .unwrap_or_default();
            notes.push(ArchivedPatientNote {
                index,
                label,
                content,
            });
        }
        if notes.is_empty() {
            None
        } else {
            Some(notes)
        }
    }
}

/// Minimal shim that only deserializes `qualityRating` from feedback.json,
/// avoiding the full SessionFeedback allocation (patient_feedback Vec, comments)
/// when all we need is the rating for the session list row.
async fn read_quality_rating(path: &std::path::Path) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct QualityRatingOnly {
        #[serde(rename = "qualityRating")]
        quality_rating: Option<String>,
    }
    let content = tokio::fs::read_to_string(path).await.ok()?;
    serde_json::from_str::<QualityRatingOnly>(&content).ok()?.quality_rating
}

/// Parse a date from an RFC 3339 started_at string
fn parse_date_from_started_at(started_at: &str) -> Result<NaiveDate, ApiError> {
    // Try full RFC3339 first, fall back to date-only
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(started_at) {
        return Ok(dt.date_naive());
    }
    NaiveDate::parse_from_str(started_at, "%Y-%m-%d")
        .map_err(|e| ApiError::BadRequest(format!("Cannot parse date from started_at: {e}")))
}

/// Read file contents, returning None if file doesn't exist
async fn read_optional_file(path: &PathBuf) -> Option<String> {
    tokio::fs::read_to_string(path).await.ok()
}

/// Atomic write: write to a unique temp file, then rename into place.
/// Accepts any content that implements `AsRef<[u8]>` (covers both `&str` and `&[u8]`).
async fn atomic_write(path: impl AsRef<Path>, content: impl AsRef<[u8]>) -> Result<(), ApiError> {
    let path = path.as_ref();
    let tmp_path = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    tokio::fs::write(&tmp_path, content)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to write temp file: {e}")))?;
    tokio::fs::rename(&tmp_path, path)
        .await
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            ApiError::Internal(format!("Failed to rename: {e}"))
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_accepts_replay_artifacts() {
        assert!(is_allowed_session_file("pipeline_log.jsonl"));
        assert!(is_allowed_session_file("replay_bundle.json"));
        assert!(is_allowed_session_file("segments.jsonl"));
        assert!(is_allowed_session_file("billing.json"));
    }

    #[test]
    fn allowlist_accepts_patient_handout() {
        // Regression guard: handout was omitted from the allowlist until v0.10.34.
        assert!(is_allowed_session_file("patient_handout.txt"));
    }

    #[test]
    fn allowlist_accepts_clinician_notes() {
        // Per-encounter clinician-submitted notes sidecar (v0.10.57+).
        assert!(is_allowed_session_file("clinician_notes.json"));
    }

    #[test]
    fn allowlist_accepts_valid_screenshots() {
        assert!(is_allowed_session_file("screenshots/shot1.jpg"));
        assert!(is_allowed_session_file("screenshots/abc123.jpg"));
    }

    #[test]
    fn allowlist_rejects_screenshot_traversal() {
        assert!(!is_allowed_session_file("screenshots/../secret.jpg"));
        assert!(!is_allowed_session_file("screenshots/sub/nested.jpg"));
        assert!(!is_allowed_session_file("screenshots/file.png"));
    }

    #[test]
    fn allowlist_rejects_unknown_files() {
        assert!(!is_allowed_session_file("transcript.txt"));
        assert!(!is_allowed_session_file("metadata.json"));
        assert!(!is_allowed_session_file("arbitrary.file"));
        assert!(!is_allowed_session_file("../../etc/passwd"));
    }
}
