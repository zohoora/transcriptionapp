mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn health_requires_no_auth() {
    let app = TestApp::with_auth("secret-key-123");
    let resp = app.get("/health").await;
    resp.assert_ok();
    let json = resp.json();
    assert_eq!(json["healthy"], true);
}

#[tokio::test]
async fn protected_route_rejects_missing_key() {
    let app = TestApp::with_auth("secret-key-123");
    let resp = app.get("/physicians").await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
    let json = resp.json();
    assert!(json["error"].as_str().unwrap().contains("API key"));
}

#[tokio::test]
async fn protected_route_rejects_wrong_key() {
    let app = TestApp::with_auth("secret-key-123");
    let resp = app.get_authed("/physicians", "wrong-key").await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn protected_route_accepts_correct_key() {
    let app = TestApp::with_auth("secret-key-123");
    let resp = app.get_authed("/physicians", "secret-key-123").await;
    resp.assert_ok();
    let json = resp.json();
    assert!(json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn no_auth_configured_allows_all() {
    let app = TestApp::new();
    // No API key set — all routes should be open
    let resp = app.get("/physicians").await;
    resp.assert_ok();

    let resp = app.get("/health").await;
    resp.assert_ok();

    let resp = app.get("/rooms").await;
    resp.assert_ok();
}
