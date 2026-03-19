use crate::error::ApiError;
use crate::store::AppState;
use crate::types::{
    ArchiveDetails, ArchiveSummary, MergeSessionsRequest, RenumberRequest, SessionFeedback,
    SplitSessionRequest, UpdatePatientNameRequest, UpdateSoapRequest, UploadSessionRequest,
};
use axum::extract::{Multipart, Path, Query, State};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct DateQuery {
    pub date: String,
}

#[derive(Deserialize)]
pub struct DateRangeQuery {
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

pub async fn list_dates(
    State(state): State<Arc<AppState>>,
    Path(physician_id): Path<String>,
    Query(q): Query<DateRangeQuery>,
) -> Result<Json<Vec<String>>, ApiError> {
    let dates = state
        .sessions
        .list_dates(&physician_id, q.from.as_deref(), q.to.as_deref())
        .await?;
    Ok(Json(dates))
}

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Path(physician_id): Path<String>,
    Query(q): Query<DateQuery>,
) -> Result<Json<Vec<ArchiveSummary>>, ApiError> {
    let sessions = state.sessions.list_sessions(&physician_id, &q.date).await?;
    Ok(Json(sessions))
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id)): Path<(String, String)>,
) -> Result<Json<ArchiveDetails>, ApiError> {
    let details = state.sessions.get_session(&physician_id, &session_id).await?;
    Ok(Json(details))
}

pub async fn upload_session(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id)): Path<(String, String)>,
    Json(req): Json<UploadSessionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .sessions
        .upload_session(
            &physician_id,
            &session_id,
            &req.metadata,
            &req.transcript,
            req.soap_note.as_deref(),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn update_metadata(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id)): Path<(String, String)>,
    Json(metadata): Json<crate::types::ArchiveMetadata>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .sessions
        .update_metadata(&physician_id, &session_id, &metadata)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn update_soap(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id)): Path<(String, String)>,
    Json(req): Json<UpdateSoapRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .sessions
        .update_soap(
            &physician_id,
            &session_id,
            &req.content,
            req.detail_level,
            req.format.as_deref(),
        )
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn update_feedback(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id)): Path<(String, String)>,
    Json(feedback): Json<SessionFeedback>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .sessions
        .update_feedback(&physician_id, &session_id, &feedback)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn update_patient_name(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id)): Path<(String, String)>,
    Json(req): Json<UpdatePatientNameRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .sessions
        .update_patient_name(&physician_id, &session_id, &req.patient_name)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .sessions
        .delete_session(&physician_id, &session_id)
        .await?;
    Ok(Json(serde_json::json!({ "deleted": session_id })))
}

pub async fn get_transcript_lines(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id)): Path<(String, String)>,
) -> Result<Json<Vec<String>>, ApiError> {
    let lines = state
        .sessions
        .get_transcript_lines(&physician_id, &session_id)
        .await?;
    Ok(Json(lines))
}

pub async fn split_session(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id)): Path<(String, String)>,
    Json(req): Json<SplitSessionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let new_id = state
        .sessions
        .split_session(&physician_id, &session_id, req.split_line)
        .await?;
    Ok(Json(serde_json::json!({ "new_session_id": new_id })))
}

pub async fn merge_sessions(
    State(state): State<Arc<AppState>>,
    Path(physician_id): Path<String>,
    Json(req): Json<MergeSessionsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let kept_id = state
        .sessions
        .merge_sessions(&physician_id, &req.session_ids, &req.date)
        .await?;
    Ok(Json(serde_json::json!({ "kept_session_id": kept_id })))
}

pub async fn renumber_encounters(
    State(state): State<Arc<AppState>>,
    Path(physician_id): Path<String>,
    Json(req): Json<RenumberRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .sessions
        .renumber_encounters(&physician_id, &req.date)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn upload_audio(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id)): Path<(String, String)>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Multipart error: {e}")))?
    {
        let data = field
            .bytes()
            .await
            .map_err(|e| ApiError::BadRequest(format!("Failed to read field: {e}")))?;
        state
            .sessions
            .save_audio(&physician_id, &session_id, &data)
            .await?;
        return Ok(Json(serde_json::json!({ "ok": true, "bytes": data.len() })));
    }
    Err(ApiError::BadRequest("No file uploaded".to_string()))
}

pub async fn download_audio(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id)): Path<(String, String)>,
) -> Result<(axum::http::HeaderMap, Vec<u8>), ApiError> {
    let path = state
        .sessions
        .get_audio_path(&physician_id, &session_id)
        .await?;
    let data = tokio::fs::read(&path)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to read audio: {e}")))?;
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("content-type", "audio/wav".parse().unwrap());
    headers.insert(
        "content-disposition",
        format!("attachment; filename=\"{session_id}.wav\"")
            .parse()
            .unwrap(),
    );
    Ok((headers, data))
}

pub async fn upload_session_file(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id, filename)): Path<(String, String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .sessions
        .save_session_file(&physician_id, &session_id, &filename, &body)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn download_session_file(
    State(state): State<Arc<AppState>>,
    Path((physician_id, session_id, filename)): Path<(String, String, String)>,
) -> Result<axum::body::Bytes, ApiError> {
    let data = state
        .sessions
        .get_session_file(&physician_id, &session_id, &filename)
        .await?;
    Ok(axum::body::Bytes::from(data))
}

pub async fn upload_day_log(
    State(state): State<Arc<AppState>>,
    Path((physician_id, date)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .sessions
        .save_day_log(&physician_id, &date, &body)
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn download_day_log(
    State(state): State<Arc<AppState>>,
    Path((physician_id, date)): Path<(String, String)>,
) -> Result<axum::body::Bytes, ApiError> {
    let data = state
        .sessions
        .get_day_log(&physician_id, &date)
        .await?;
    Ok(axum::body::Bytes::from(data))
}
