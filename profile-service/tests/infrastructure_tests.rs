mod common;

use common::TestApp;

#[tokio::test]
async fn get_default_infrastructure_settings() {
    let app = TestApp::new();

    let resp = app.get("/infrastructure").await;
    resp.assert_ok();
    let json = resp.json();
    // All fields should be null by default
    assert!(json["llm_router_url"].is_null());
    assert!(json["whisper_server_url"].is_null());
    assert!(json["soap_model"].is_null());
}

#[tokio::test]
async fn partial_update_infrastructure() {
    let app = TestApp::new();

    // Update a subset of fields
    let resp = app
        .put_json(
            "/infrastructure",
            &serde_json::json!({
                "llm_router_url": "http://localhost:8080",
                "soap_model": "soap-model-fast"
            }),
        )
        .await;
    resp.assert_ok();
    let updated = resp.json();
    assert_eq!(updated["llm_router_url"], "http://localhost:8080");
    assert_eq!(updated["soap_model"], "soap-model-fast");
    // Other fields remain null
    assert!(updated["whisper_server_url"].is_null());
}

#[tokio::test]
async fn infrastructure_settings_persist() {
    let app = TestApp::new();

    // Set some values
    app.put_json(
        "/infrastructure",
        &serde_json::json!({
            "whisper_server_url": "http://stt:8001",
            "stt_alias": "medical-streaming"
        }),
    )
    .await
    .assert_ok();

    // Read back
    let resp = app.get("/infrastructure").await;
    resp.assert_ok();
    let json = resp.json();
    assert_eq!(json["whisper_server_url"], "http://stt:8001");
    assert_eq!(json["stt_alias"], "medical-streaming");
}

#[tokio::test]
async fn infrastructure_partial_update_preserves_existing() {
    let app = TestApp::new();

    // First update
    app.put_json(
        "/infrastructure",
        &serde_json::json!({
            "llm_router_url": "http://llm:8080",
            "soap_model": "soap-fast"
        }),
    )
    .await
    .assert_ok();

    // Second update: only change one field
    app.put_json(
        "/infrastructure",
        &serde_json::json!({
            "whisper_server_url": "http://stt:8001"
        }),
    )
    .await
    .assert_ok();

    // Verify both old and new values are present
    let resp = app.get("/infrastructure").await;
    resp.assert_ok();
    let json = resp.json();
    assert_eq!(json["llm_router_url"], "http://llm:8080");
    assert_eq!(json["soap_model"], "soap-fast");
    assert_eq!(json["whisper_server_url"], "http://stt:8001");
}
