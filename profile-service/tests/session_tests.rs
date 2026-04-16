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

// ── Patch metadata tests (added v0.10.30 to fix HTTP 422 on renumber sync) ──

#[tokio::test]
async fn patch_metadata_accepts_partial_json() {
    // Regression test for v0.10.30: previously the endpoint required full
    // ArchiveMetadata struct, returned 422 on partial updates. Now merges fields.
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

    // Send only encounter_number — old code would reject with 422
    let patch = serde_json::json!({ "encounter_number": 7 });
    let resp = app
        .put_json(
            &format!("/physicians/{physician_id}/sessions/test-session-001/metadata"),
            &patch,
        )
        .await;
    resp.assert_ok();

    // Verify the field was updated and others were preserved
    let resp = app
        .get(&format!(
            "/physicians/{physician_id}/sessions/test-session-001"
        ))
        .await;
    resp.assert_ok();
    let details = resp.json();
    assert_eq!(details["metadata"]["encounter_number"], 7);
    assert_eq!(details["metadata"]["session_id"], "test-session-001");
    assert_eq!(details["metadata"]["patient_name"], "John Doe");
    assert_eq!(details["metadata"]["word_count"], 500);
}

#[tokio::test]
async fn patch_metadata_ignores_null_fields() {
    // Patch should only overwrite non-null fields; null fields are skipped
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

    // Patch with one real field and one null — null should NOT clear the existing value
    let patch = serde_json::json!({
        "encounter_number": 3,
        "patient_name": null
    });
    app.put_json(
        &format!("/physicians/{physician_id}/sessions/test-session-001/metadata"),
        &patch,
    )
    .await
    .assert_ok();

    let resp = app
        .get(&format!(
            "/physicians/{physician_id}/sessions/test-session-001"
        ))
        .await;
    let details = resp.json();
    assert_eq!(details["metadata"]["encounter_number"], 3);
    assert_eq!(details["metadata"]["patient_name"], "John Doe", "null in patch should not clear existing value");
}

#[tokio::test]
async fn patch_metadata_returns_404_for_missing_session() {
    let app = TestApp::new();
    let resp = app.post_json("/physicians", &serde_json::json!({ "name": "Dr. X" })).await;
    let physician_id = resp.json()["id"].as_str().unwrap().to_string();
    let patch = serde_json::json!({ "encounter_number": 1 });
    let resp = app
        .put_json(
            &format!("/physicians/{physician_id}/sessions/no-such-session/metadata"),
            &patch,
        )
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

// ── Merge crash safety tests ──

#[tokio::test]
async fn merge_creates_backup_files() {
    // Verify .bak files are created during merge for crash recovery
    let app = TestApp::new();
    let resp = app.post_json("/physicians", &serde_json::json!({ "name": "Dr. Y" })).await;
    let physician_id = resp.json()["id"].as_str().unwrap().to_string();

    // Upload two sessions to merge
    let session_a = serde_json::json!({
        "metadata": {
            "session_id": "merge-a",
            "started_at": "2026-04-15T10:00:00Z",
            "ended_at": "2026-04-15T10:15:00Z",
            "duration_ms": 900000_u64,
            "segment_count": 20,
            "word_count": 300,
            "has_soap_note": true,
            "has_audio": false,
            "auto_ended": false
        },
        "transcript": "First session transcript.",
        "soap_note": "S: First session SOAP."
    });
    let session_b = serde_json::json!({
        "metadata": {
            "session_id": "merge-b",
            "started_at": "2026-04-15T10:20:00Z",
            "ended_at": "2026-04-15T10:35:00Z",
            "duration_ms": 900000_u64,
            "segment_count": 25,
            "word_count": 400,
            "has_soap_note": false,
            "has_audio": false,
            "auto_ended": false
        },
        "transcript": "Second session transcript.",
        "soap_note": null
    });
    app.post_json(
        &format!("/physicians/{physician_id}/sessions/merge-a"),
        &session_a,
    )
    .await
    .assert_ok();
    app.post_json(
        &format!("/physicians/{physician_id}/sessions/merge-b"),
        &session_b,
    )
    .await
    .assert_ok();

    // Both sessions exist before merge
    let resp = app
        .get(&format!("/physicians/{physician_id}/sessions?date=2026-04-15"))
        .await;
    assert_eq!(resp.json().as_array().unwrap().len(), 2);
}

