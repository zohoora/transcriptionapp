use crate::error::ApiError;
use crate::store::mobile_jobs::{MobileJob, MobileJobStore, UpdateJobRequest};
use crate::store::AppState;
use axum::extract::{Multipart, Path, Query, State};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ListJobsQuery {
    pub physician_id: Option<String>,
    pub status: Option<String>,
}

/// POST /mobile/upload — Upload audio + metadata, create a processing job.
///
/// Accepts multipart/form-data with fields:
///   - audio: file (AAC/m4a binary)
///   - physician_id: string
///   - started_at: string (RFC3339)
///   - duration_ms: string (parsed as u64)
///   - recording_id: string (for idempotency)
///   - device_info: string (optional)
pub async fn upload(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<MobileJob>, ApiError> {
    let mut audio_data: Option<Vec<u8>> = None;
    let mut physician_id: Option<String> = None;
    let mut started_at: Option<String> = None;
    let mut duration_ms: Option<u64> = None;
    let mut recording_id: Option<String> = None;
    let mut device_info: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "audio" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("Failed to read audio: {e}")))?;
                audio_data = Some(bytes.to_vec());
            }
            "physician_id" => {
                physician_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::BadRequest(format!("Failed to read field: {e}")))?,
                );
            }
            "started_at" => {
                started_at = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::BadRequest(format!("Failed to read field: {e}")))?,
                );
            }
            "duration_ms" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("Failed to read field: {e}")))?;
                duration_ms = Some(
                    text.parse::<u64>()
                        .map_err(|_| ApiError::BadRequest("Invalid duration_ms".into()))?,
                );
            }
            "recording_id" => {
                recording_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::BadRequest(format!("Failed to read field: {e}")))?,
                );
            }
            "device_info" => {
                device_info = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| ApiError::BadRequest(format!("Failed to read field: {e}")))?,
                );
            }
            _ => {
                // Skip unknown fields
            }
        }
    }

    let audio_data =
        audio_data.ok_or_else(|| ApiError::BadRequest("Missing audio file".into()))?;
    let physician_id =
        physician_id.ok_or_else(|| ApiError::BadRequest("Missing physician_id".into()))?;
    let started_at =
        started_at.ok_or_else(|| ApiError::BadRequest("Missing started_at".into()))?;
    let duration_ms =
        duration_ms.ok_or_else(|| ApiError::BadRequest("Missing duration_ms".into()))?;
    let recording_id =
        recording_id.ok_or_else(|| ApiError::BadRequest("Missing recording_id".into()))?;

    // Validate physician exists
    {
        let physicians = state.physicians.read().await;
        physicians.get(&physician_id)?;
    }

    // Create job (idempotent — returns existing if recording_id is duplicate)
    let job = {
        let mut store = state.mobile_jobs.write().await;
        store.create_job(physician_id, recording_id, started_at, duration_ms, device_info)?
    };

    // Save audio file (only if not already saved — idempotency)
    let audio_path = {
        let store = state.mobile_jobs.read().await;
        store.upload_path(&job.job_id)
    };
    if !audio_path.exists() {
        tokio::fs::write(&audio_path, &audio_data)
            .await
            .map_err(|e| ApiError::Internal(format!("Failed to save audio: {e}")))?;
    }

    Ok(Json(job))
}

/// GET /mobile/jobs/:job_id — Get a single job's status.
pub async fn get_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<Json<MobileJob>, ApiError> {
    MobileJobStore::validate_job_id(&job_id)?;
    let store = state.mobile_jobs.read().await;
    let job = store.get_job(&job_id)?;
    Ok(Json(job))
}

/// GET /mobile/jobs — List jobs, optionally filtered by physician_id and/or status.
pub async fn list_jobs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<Vec<MobileJob>>, ApiError> {
    let store = state.mobile_jobs.read().await;
    let jobs = store.list_jobs(query.physician_id.as_deref(), query.status.as_deref());
    Ok(Json(jobs))
}

/// PUT /mobile/jobs/:job_id — Update a job's status (used by the processing CLI).
pub async fn update_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
    Json(req): Json<UpdateJobRequest>,
) -> Result<Json<MobileJob>, ApiError> {
    MobileJobStore::validate_job_id(&job_id)?;
    let mut store = state.mobile_jobs.write().await;
    let job = store.update_job(&job_id, req)?;
    Ok(Json(job))
}

/// DELETE /mobile/jobs/:job_id — Delete a job and its uploaded audio.
pub async fn delete_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    MobileJobStore::validate_job_id(&job_id)?;
    let mut store = state.mobile_jobs.write().await;
    store.delete_job(&job_id)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// GET /mobile/uploads/:job_id — Download the uploaded audio file (used by processing CLI).
pub async fn download_audio(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<(axum::http::HeaderMap, Vec<u8>), ApiError> {
    MobileJobStore::validate_job_id(&job_id)?;
    let store = state.mobile_jobs.read().await;
    store.get_job(&job_id)?;

    let path = store.upload_path(&job_id);
    if !path.exists() {
        return Err(ApiError::NotFound(format!(
            "Audio file not found for job: {job_id}"
        )));
    }

    let data = tokio::fs::read(&path)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to read audio: {e}")))?;

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("content-type", "audio/mp4".parse().unwrap());
    headers.insert(
        "content-disposition",
        format!("attachment; filename=\"{job_id}.m4a\"")
            .parse()
            .unwrap(),
    );
    Ok((headers, data))
}
