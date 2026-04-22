//! OpenAI image generation proxy route (v0.10.54+).
//!
//! `POST /openai/image` — workstation sends `{prompt, quality, size?}`,
//! server makes the OpenAI call using the env-held `OPENAI_API_KEY` and
//! returns `{image_base64, model, quality}`. Keeps the secret on the
//! MacBook so new workstations don't need per-machine key plumbing.

use crate::error::ApiError;
use crate::store::openai_image::{OpenAIImageRequest, OpenAIImageResponse};
use crate::store::AppState;
use axum::extract::State;
use axum::Json;
use std::sync::Arc;

pub async fn generate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenAIImageRequest>,
) -> Result<Json<OpenAIImageResponse>, ApiError> {
    let resp = state.openai_image.generate(&req).await?;
    Ok(Json(resp))
}
