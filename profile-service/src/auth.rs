use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// API key authentication middleware.
///
/// - If `expected_key` is `None`, all requests pass through (backward compatible / dev mode).
/// - The `/health` endpoint is always exempt from auth.
/// - All other requests must provide a matching `X-API-Key` header.
pub async fn check_api_key(
    expected_key: Option<String>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // No key configured — pass everything through (dev / backward compat)
    let expected = match expected_key {
        Some(k) => k,
        None => return next.run(req).await,
    };

    // Health endpoint is always exempt
    if req.uri().path() == "/health" {
        return next.run(req).await;
    }

    // Check the X-API-Key header
    match req.headers().get("X-API-Key").and_then(|v| v.to_str().ok()) {
        Some(provided) if provided == expected => next.run(req).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid or missing API key" })),
        )
            .into_response(),
    }
}
