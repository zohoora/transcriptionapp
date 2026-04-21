//! Medplum token proxy endpoint (v0.10.49+).
//!
//! `POST /medplum/token` returns a fresh `{access_token, expires_in}` by
//! running `client_credentials` against Medplum using the env-configured
//! `MEDPLUM_CLIENT_ID` + `MEDPLUM_CLIENT_SECRET`. Cached in memory by the
//! proxy between requests.

use crate::error::ApiError;
use crate::store::medplum_auth::TokenResponse;
use crate::store::AppState;
use axum::extract::State;
use axum::Json;
use std::sync::Arc;

pub async fn token(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TokenResponse>, ApiError> {
    let tok = state.medplum_auth.get_token().await?;
    Ok(Json(tok))
}
