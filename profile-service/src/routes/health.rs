use crate::store::AppState;
use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn health(State(state): State<Arc<AppState>>) -> Json<Value> {
    let physicians = state.physicians.read().await.count();
    let rooms = state.rooms.read().await.count();
    let speakers = state.speakers.read().await.count();
    Json(json!({
        "healthy": true,
        "service": "profile-service",
        "physicians": physicians,
        "rooms": rooms,
        "speakers": speakers,
        "data_dir": state.data_dir.to_string_lossy(),
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}
