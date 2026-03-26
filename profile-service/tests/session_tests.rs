mod common;

use axum::http::StatusCode;
use common::TestApp;

fn create_physician_and_session_data() -> (serde_json::Value, serde_json::Value) {
    let physician_body = serde_json::json!({ "name": "Dr. Test" });
    let session_body = serde_json::json!({
        "metadata": {
            "session_id": "test-session-001",
            "started_at": "2026-03-26T10:00:00Z",
            "ended_at": "2026-03-26T10:30:00Z",
            "duration_ms": 1800000_u64,
            "segment_count": 50,
            "word_count": 500,
            "has_soap_note": false,
            "has_audio": false,
            "auto_ended": false,
            "charting_mode": "continuous",
            "encounter_number": 1,
            "patient_name": "John Doe"
        },
        "transcript": "Patient presents with chest pain.\nDuration approximately two days.",
        "soap_note": "S: Patient presents with chest pain for 2 days."
    });
    (physician_body, session_body)
}

#[tokio::test]
async fn upload_and_retrieve_session() {
    let app = TestApp::new();

    // Create physician first
    let (phys_body, session_body) = create_physician_and_session_data();
    let resp = app.post_json("/physicians", &phys_body).await;
    resp.assert_ok();
    let physician_id = resp.json()["id"].as_str().unwrap().to_string();

    // Upload session
    let resp = app
        .post_json(
            &format!("/physicians/{physician_id}/sessions/test-session-001"),
            &session_body,
        )
        .await;
    resp.assert_ok();

    // Retrieve session
    let resp = app
        .get(&format!("/physicians/{physician_id}/sessions/test-session-001"))
        .await;
    resp.assert_ok();
    let details = resp.json();
    assert_eq!(details["session_id"], "test-session-001");
    assert_eq!(details["metadata"]["patient_name"], "John Doe");
    assert!(details["transcript"]
        .as_str()
        .unwrap()
        .contains("chest pain"));
    assert!(details["soap_note"].as_str().is_some());
}

#[tokio::test]
async fn list_session_dates() {
    let app = TestApp::new();

    let (phys_body, session_body) = create_physician_and_session_data();
    let resp = app.post_json("/physicians", &phys_body).await;
    let physician_id = resp.json()["id"].as_str().unwrap().to_string();

    // Upload session
    app.post_json(
        &format!("/physicians/{physician_id}/sessions/test-session-001"),
        &session_body,
    )
    .await
    .assert_ok();

    // List dates
    let resp = app
        .get(&format!("/physicians/{physician_id}/sessions/dates"))
        .await;
    resp.assert_ok();
    let dates = resp.json();
    let dates_arr = dates.as_array().unwrap();
    assert_eq!(dates_arr.len(), 1);
    assert_eq!(dates_arr[0], "2026-03-26");
}

#[tokio::test]
async fn list_sessions_by_date() {
    let app = TestApp::new();

    let (phys_body, session_body) = create_physician_and_session_data();
    let resp = app.post_json("/physicians", &phys_body).await;
    let physician_id = resp.json()["id"].as_str().unwrap().to_string();

    app.post_json(
        &format!("/physicians/{physician_id}/sessions/test-session-001"),
        &session_body,
    )
    .await
    .assert_ok();

    let resp = app
        .get(&format!(
            "/physicians/{physician_id}/sessions?date=2026-03-26"
        ))
        .await;
    resp.assert_ok();
    let sessions = resp.json();
    let sessions_arr = sessions.as_array().unwrap();
    assert_eq!(sessions_arr.len(), 1);
    assert_eq!(sessions_arr[0]["session_id"], "test-session-001");
    assert_eq!(sessions_arr[0]["patient_name"], "John Doe");
}

#[tokio::test]
async fn delete_session() {
    let app = TestApp::new();

    let (phys_body, session_body) = create_physician_and_session_data();
    let resp = app.post_json("/physicians", &phys_body).await;
    let physician_id = resp.json()["id"].as_str().unwrap().to_string();

    app.post_json(
        &format!("/physicians/{physician_id}/sessions/test-session-001"),
        &session_body,
    )
    .await
    .assert_ok();

    // Delete
    let resp = app
        .delete(&format!(
            "/physicians/{physician_id}/sessions/test-session-001"
        ))
        .await;
    resp.assert_ok();

    // Verify gone
    let resp = app
        .get(&format!(
            "/physicians/{physician_id}/sessions/test-session-001"
        ))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_soap_note() {
    let app = TestApp::new();

    let (phys_body, session_body) = create_physician_and_session_data();
    let resp = app.post_json("/physicians", &phys_body).await;
    let physician_id = resp.json()["id"].as_str().unwrap().to_string();

    app.post_json(
        &format!("/physicians/{physician_id}/sessions/test-session-001"),
        &session_body,
    )
    .await
    .assert_ok();

    // Update SOAP
    let resp = app
        .put_json(
            &format!("/physicians/{physician_id}/sessions/test-session-001/soap"),
            &serde_json::json!({
                "content": "Updated SOAP note content",
                "detail_level": 5,
                "format": "problem_based"
            }),
        )
        .await;
    resp.assert_ok();

    // Verify updated
    let resp = app
        .get(&format!(
            "/physicians/{physician_id}/sessions/test-session-001"
        ))
        .await;
    resp.assert_ok();
    let details = resp.json();
    assert_eq!(details["soap_note"], "Updated SOAP note content");
    assert_eq!(details["metadata"]["has_soap_note"], true);
    assert_eq!(details["metadata"]["soap_detail_level"], 5);
}

#[tokio::test]
async fn get_nonexistent_session_returns_404() {
    let app = TestApp::new();

    let resp = app.post_json("/physicians", &serde_json::json!({ "name": "Dr. X" })).await;
    let physician_id = resp.json()["id"].as_str().unwrap().to_string();

    let resp = app
        .get(&format!("/physicians/{physician_id}/sessions/no-such-session"))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn upload_session_idempotent() {
    let app = TestApp::new();

    let (phys_body, session_body) = create_physician_and_session_data();
    let resp = app.post_json("/physicians", &phys_body).await;
    let physician_id = resp.json()["id"].as_str().unwrap().to_string();

    // Upload twice — should succeed (idempotent overwrite)
    app.post_json(
        &format!("/physicians/{physician_id}/sessions/test-session-001"),
        &session_body,
    )
    .await
    .assert_ok();

    app.post_json(
        &format!("/physicians/{physician_id}/sessions/test-session-001"),
        &session_body,
    )
    .await
    .assert_ok();

    // Still only one session in the date listing
    let resp = app
        .get(&format!(
            "/physicians/{physician_id}/sessions?date=2026-03-26"
        ))
        .await;
    resp.assert_ok();
    assert_eq!(resp.json().as_array().unwrap().len(), 1);
}
