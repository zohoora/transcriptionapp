use crate::error::ApiError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Status of a mobile processing job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Transcoding,
    Transcribing,
    Detecting,
    GeneratingSoap,
    Complete,
    Failed,
}

/// A session created by the processing CLI after splitting/processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedSession {
    pub session_id: String,
    pub encounter_number: u32,
    pub word_count: usize,
    pub has_soap: bool,
}

/// A mobile processing job — represents one uploaded recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileJob {
    pub job_id: String,
    pub physician_id: String,
    pub recording_id: String,
    pub started_at: String,
    pub duration_ms: u64,
    pub status: JobStatus,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub sessions_created: Vec<CreatedSession>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub device_info: Option<String>,
}

/// Request to update a job's status (used by the processing CLI).
#[derive(Debug, Deserialize)]
pub struct UpdateJobRequest {
    pub status: JobStatus,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub sessions_created: Option<Vec<CreatedSession>>,
}

/// In-memory job store backed by a JSON file on disk.
pub struct MobileJobStore {
    jobs: HashMap<String, MobileJob>,
    /// Recording ID → Job ID for idempotency checks.
    recording_index: HashMap<String, String>,
    persist_path: PathBuf,
    uploads_dir: PathBuf,
}

impl MobileJobStore {
    /// Load from disk or create empty.
    pub fn load(persist_path: PathBuf, uploads_dir: PathBuf) -> Result<Self, ApiError> {
        std::fs::create_dir_all(&uploads_dir)
            .map_err(|e| ApiError::Internal(format!("Failed to create uploads dir: {e}")))?;

        let jobs: HashMap<String, MobileJob> = if persist_path.exists() {
            let data = std::fs::read_to_string(&persist_path)
                .map_err(|e| ApiError::Internal(format!("Failed to read jobs file: {e}")))?;
            serde_json::from_str(&data)
                .map_err(|e| ApiError::Internal(format!("Failed to parse jobs file: {e}")))?
        } else {
            HashMap::new()
        };

        let recording_index: HashMap<String, String> = jobs
            .iter()
            .map(|(_, job)| (job.recording_id.clone(), job.job_id.clone()))
            .collect();

        Ok(Self {
            jobs,
            recording_index,
            persist_path,
            uploads_dir,
        })
    }

    /// Persist all jobs to disk atomically.
    fn save(&self) -> Result<(), ApiError> {
        if let Some(parent) = self.persist_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ApiError::Internal(format!("Failed to create parent dir: {e}")))?;
        }
        let content = serde_json::to_string_pretty(&self.jobs)
            .map_err(|e| ApiError::Internal(format!("Failed to serialize jobs: {e}")))?;
        let tmp = self
            .persist_path
            .with_extension(format!("{}.tmp", Uuid::new_v4()));
        std::fs::write(&tmp, &content)
            .map_err(|e| ApiError::Internal(format!("Failed to write temp file: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, &self.persist_path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            ApiError::Internal(format!("Failed to rename: {e}"))
        })?;
        Ok(())
    }

    /// Create a new job. Returns existing job if recording_id is a duplicate.
    pub fn create_job(
        &mut self,
        physician_id: String,
        recording_id: String,
        started_at: String,
        duration_ms: u64,
        device_info: Option<String>,
    ) -> Result<MobileJob, ApiError> {
        // Idempotency: return existing job if recording_id already seen
        if let Some(existing_id) = self.recording_index.get(&recording_id) {
            if let Some(existing) = self.jobs.get(existing_id) {
                return Ok(existing.clone());
            }
        }

        let now = chrono::Utc::now().to_rfc3339();
        let job = MobileJob {
            job_id: Uuid::new_v4().to_string(),
            physician_id,
            recording_id: recording_id.clone(),
            started_at,
            duration_ms,
            status: JobStatus::Queued,
            error: None,
            sessions_created: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            device_info,
        };

        self.recording_index
            .insert(recording_id, job.job_id.clone());
        self.jobs.insert(job.job_id.clone(), job.clone());
        self.save()?;
        Ok(job)
    }

    /// Get a job by ID.
    pub fn get_job(&self, job_id: &str) -> Result<MobileJob, ApiError> {
        self.jobs
            .get(job_id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("Job not found: {job_id}")))
    }

    /// List jobs, optionally filtered by physician_id and/or status.
    pub fn list_jobs(
        &self,
        physician_id: Option<&str>,
        status: Option<&str>,
    ) -> Vec<MobileJob> {
        let status_filter: Option<JobStatus> = status.and_then(|s| {
            serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
        });

        let mut jobs: Vec<MobileJob> = self
            .jobs
            .values()
            .filter(|j| {
                physician_id.map_or(true, |pid| j.physician_id == pid)
                    && status_filter
                        .as_ref()
                        .map_or(true, |sf| j.status == *sf)
            })
            .cloned()
            .collect();

        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        jobs
    }

    /// Update a job's status and optionally its error/sessions.
    pub fn update_job(
        &mut self,
        job_id: &str,
        req: UpdateJobRequest,
    ) -> Result<MobileJob, ApiError> {
        let job = self
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| ApiError::NotFound(format!("Job not found: {job_id}")))?;

        job.status = req.status;
        job.error = req.error;
        if let Some(sessions) = req.sessions_created {
            job.sessions_created = sessions;
        }
        job.updated_at = chrono::Utc::now().to_rfc3339();

        let result = job.clone();
        self.save()?;
        Ok(result)
    }

    /// Delete a job and its uploaded audio file.
    pub fn delete_job(&mut self, job_id: &str) -> Result<(), ApiError> {
        let job = self
            .jobs
            .remove(job_id)
            .ok_or_else(|| ApiError::NotFound(format!("Job not found: {job_id}")))?;

        self.recording_index.remove(&job.recording_id);

        // Clean up audio files
        let m4a = self.uploads_dir.join(format!("{}.m4a", job_id));
        let wav = self.uploads_dir.join(format!("{}.wav", job_id));
        let _ = std::fs::remove_file(m4a);
        let _ = std::fs::remove_file(wav);

        self.save()?;
        Ok(())
    }

    /// Validate a job_id doesn't contain path traversal characters.
    pub fn validate_job_id(job_id: &str) -> Result<(), ApiError> {
        if job_id.is_empty() {
            return Err(ApiError::BadRequest("job_id must not be empty".into()));
        }
        if job_id.contains('/') || job_id.contains('\\') {
            return Err(ApiError::BadRequest(
                "job_id must not contain path separators".into(),
            ));
        }
        if job_id.contains("..") {
            return Err(ApiError::BadRequest(
                "job_id must not contain '..'".into(),
            ));
        }
        if job_id.contains('\0') {
            return Err(ApiError::BadRequest(
                "job_id must not contain null bytes".into(),
            ));
        }
        Ok(())
    }

    /// Get the path where an uploaded audio file should be stored.
    pub fn upload_path(&self, job_id: &str) -> PathBuf {
        self.uploads_dir.join(format!("{}.m4a", job_id))
    }

    /// Check if an audio file exists for a job.
    pub fn audio_exists(&self, job_id: &str) -> bool {
        self.upload_path(job_id).exists()
    }
}
