use crate::error::ApiError;
use crate::store::AppState;
use crate::types::{InfrastructureSettings, UpdateInfrastructureRequest};
use axum::extract::State;
use axum::Json;
use std::sync::Arc;

pub async fn get(
    State(state): State<Arc<AppState>>,
) -> Result<Json<InfrastructureSettings>, ApiError> {
    let store = state.infrastructure.read().await;
    Ok(Json(store.get()))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateInfrastructureRequest>,
) -> Result<Json<InfrastructureSettings>, ApiError> {
    let mut store = state.infrastructure.write().await;
    Ok(Json(store.update(req)?))
}
