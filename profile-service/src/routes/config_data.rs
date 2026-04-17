use crate::error::ApiError;
use crate::store::AppState;
use crate::types::{
    BillingData, ConfigVersion, DetectionThresholds, OperationalDefaults, PromptTemplates,
};
use axum::extract::State;
use axum::Json;
use std::sync::Arc;

pub async fn get_version(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ConfigVersion>, ApiError> {
    let store = state.config_data.read().await;
    Ok(Json(store.get_version()))
}

pub async fn get_prompts(
    State(state): State<Arc<AppState>>,
) -> Result<Json<PromptTemplates>, ApiError> {
    let store = state.config_data.read().await;
    Ok(Json(store.get_prompts()))
}

pub async fn update_prompts(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PromptTemplates>,
) -> Result<Json<PromptTemplates>, ApiError> {
    let mut store = state.config_data.write().await;
    Ok(Json(store.update_prompts(req)?))
}

pub async fn get_billing(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BillingData>, ApiError> {
    let store = state.config_data.read().await;
    Ok(Json(store.get_billing()))
}

pub async fn update_billing(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BillingData>,
) -> Result<Json<BillingData>, ApiError> {
    let mut store = state.config_data.write().await;
    Ok(Json(store.update_billing(req)?))
}

pub async fn get_thresholds(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DetectionThresholds>, ApiError> {
    let store = state.config_data.read().await;
    Ok(Json(store.get_thresholds()))
}

pub async fn update_thresholds(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DetectionThresholds>,
) -> Result<Json<DetectionThresholds>, ApiError> {
    let mut store = state.config_data.write().await;
    Ok(Json(store.update_thresholds(req)?))
}

pub async fn get_defaults(
    State(state): State<Arc<AppState>>,
) -> Result<Json<OperationalDefaults>, ApiError> {
    let store = state.config_data.read().await;
    Ok(Json(store.get_defaults()))
}

pub async fn update_defaults(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OperationalDefaults>,
) -> Result<Json<OperationalDefaults>, ApiError> {
    let mut store = state.config_data.write().await;
    // `update_defaults` validates internally and returns ApiError::BadRequest
    // on violation, which `IntoResponse` maps to 400.
    Ok(Json(store.update_defaults(req)?))
}
