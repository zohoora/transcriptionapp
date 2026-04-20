//! Patient routes for the longitudinal patient memory (v0.10.46+).
//!
//! `POST /physicians/:physician_id/patients/confirm`
//!     idempotent upsert keyed on (name normalized, dob). Appends session_id
//!     to the record's session_ids on hit; creates on miss. Reconciles a
//!     prior UUID-fallback patient_id to the supplied Medplum FHIR ID.
//!
//! `GET  /physicians/:physician_id/patients?name=&dob=`
//!     exact-match lookup. Returns 404 if not found.
//!
//! `GET  /physicians/:physician_id/patients/:patient_id`
//!     lookup by canonical patient_id (Medplum FHIR ID or UUID fallback).
//!
//! `GET  /physicians/:physician_id/patients`  (no query params)
//!     list all confirmed patients for this physician.

use crate::error::ApiError;
use crate::store::AppState;
use crate::types::{
    ConfirmPatientRequest, ConfirmPatientResponse, PatientRecord, PatientSearchQuery,
};
use axum::extract::{Path, Query, State};
use axum::Json;
use std::sync::Arc;
use tracing::info;

pub async fn confirm(
    State(state): State<Arc<AppState>>,
    Path(physician_id): Path<String>,
    Json(req): Json<ConfirmPatientRequest>,
) -> Result<Json<ConfirmPatientResponse>, ApiError> {
    let mut mgr = state.patients.write().await;
    let (record, created) = mgr.confirm(
        &physician_id,
        &req.name,
        &req.dob,
        &req.session_id,
        req.medplum_patient_id,
    )?;
    info!(
        event = "patient_confirmed",
        physician_id = %physician_id,
        patient_id = %record.patient_id,
        created,
        session_count = record.session_ids.len(),
        "patient confirmed"
    );
    Ok(Json(ConfirmPatientResponse {
        patient_id: record.patient_id.clone(),
        created,
        record,
    }))
}

/// Handles both `?name=&dob=` lookup and unqualified list. When neither is
/// present, returns all patients for the physician.
pub async fn list_or_search(
    State(state): State<Arc<AppState>>,
    Path(physician_id): Path<String>,
    query: Option<Query<PatientSearchQuery>>,
) -> Result<Json<Vec<PatientRecord>>, ApiError> {
    let mgr = state.patients.read().await;
    let out = match query {
        Some(Query(q)) => mgr
            .get_by_name_dob(&physician_id, &q.name, &q.dob)
            .map(|r| vec![r])
            .unwrap_or_default(),
        None => mgr.list_for_physician(&physician_id),
    };
    Ok(Json(out))
}

pub async fn get_by_id(
    State(state): State<Arc<AppState>>,
    Path((physician_id, patient_id)): Path<(String, String)>,
) -> Result<Json<PatientRecord>, ApiError> {
    let mgr = state.patients.read().await;
    mgr.get_by_patient_id(&physician_id, &patient_id)
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("patient {patient_id}")))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path((physician_id, patient_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut mgr = state.patients.write().await;
    let removed = mgr.delete(&physician_id, &patient_id)?;
    if !removed {
        return Err(ApiError::NotFound(format!("patient {patient_id}")));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
