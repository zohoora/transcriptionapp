use crate::error::ApiError;
use crate::store::AppState;
use crate::types::{CreatePhysicianRequest, PhysicianProfile, UpdatePhysicianRequest};
use axum::extract::{Path, State};
use axum::Json;
use std::sync::Arc;

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PhysicianProfile>>, ApiError> {
    let mgr = state.physicians.read().await;
    Ok(Json(mgr.list()))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<PhysicianProfile>, ApiError> {
    let mgr = state.physicians.read().await;
    Ok(Json(mgr.get(&id)?))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreatePhysicianRequest>,
) -> Result<Json<PhysicianProfile>, ApiError> {
    req.validate()?;
    let mut mgr = state.physicians.write().await;
    Ok(Json(mgr.create(req)?))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePhysicianRequest>,
) -> Result<Json<PhysicianProfile>, ApiError> {
    req.validate()?;
    let mut mgr = state.physicians.write().await;
    Ok(Json(mgr.update(&id, req)?))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut mgr = state.physicians.write().await;
    mgr.delete(&id)?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}
