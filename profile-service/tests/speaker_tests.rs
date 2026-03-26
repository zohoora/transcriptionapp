mod common;

use axum::http::StatusCode;
use common::TestApp;

fn sample_embedding() -> Vec<f32> {
    vec![0.1, 0.2, 0.3, 0.4, 0.5]
}

#[tokio::test]
async fn create_and_list_speakers() {
    let app = TestApp::new();

    // Initially empty
    let resp = app.get("/speakers").await;
    resp.assert_ok();
    assert_eq!(resp.json().as_array().unwrap().len(), 0);

    // Create
    let resp = app
        .post_json(
            "/speakers",
            &serde_json::json!({
                "name": "Dr. Voice",
                "role": "physician",
                "description": "Main physician",
                "embedding": sample_embedding()
            }),
        )
        .await;
    resp.assert_ok();
    let created = resp.json();
    assert_eq!(created["name"], "Dr. Voice");
    assert_eq!(created["role"], "physician");
    assert!(created["id"].as_str().unwrap().len() > 0);
    let emb = created["embedding"].as_array().unwrap();
    assert_eq!(emb.len(), 5);

    // List
    let resp = app.get("/speakers").await;
    resp.assert_ok();
    assert_eq!(resp.json().as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn get_speaker_by_id() {
    let app = TestApp::new();

    let resp = app
        .post_json(
            "/speakers",
            &serde_json::json!({
                "name": "Nurse A",
                "role": "rn",
                "description": "RN",
                "embedding": sample_embedding()
            }),
        )
        .await;
    let id = resp.json()["id"].as_str().unwrap().to_string();

    let resp = app.get(&format!("/speakers/{id}")).await;
    resp.assert_ok();
    assert_eq!(resp.json()["name"], "Nurse A");
}

#[tokio::test]
async fn get_nonexistent_speaker_returns_404() {
    let app = TestApp::new();
    let resp = app.get("/speakers/nonexistent-id").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_speaker() {
    let app = TestApp::new();

    let resp = app
        .post_json(
            "/speakers",
            &serde_json::json!({
                "name": "Old Name",
                "description": "desc",
                "embedding": sample_embedding()
            }),
        )
        .await;
    let id = resp.json()["id"].as_str().unwrap().to_string();

    let new_embedding: Vec<f32> = vec![0.9, 0.8, 0.7];
    let resp = app
        .put_json(
            &format!("/speakers/{id}"),
            &serde_json::json!({
                "name": "New Name",
                "embedding": new_embedding
            }),
        )
        .await;
    resp.assert_ok();
    let updated = resp.json();
    assert_eq!(updated["name"], "New Name");
    assert_eq!(updated["embedding"].as_array().unwrap().len(), 3);

    // Verify persistence
    let resp = app.get(&format!("/speakers/{id}")).await;
    resp.assert_ok();
    assert_eq!(resp.json()["name"], "New Name");
}

#[tokio::test]
async fn delete_speaker() {
    let app = TestApp::new();

    let resp = app
        .post_json(
            "/speakers",
            &serde_json::json!({
                "name": "Temp Speaker",
                "description": "",
                "embedding": sample_embedding()
            }),
        )
        .await;
    let id = resp.json()["id"].as_str().unwrap().to_string();

    let resp = app.delete(&format!("/speakers/{id}")).await;
    resp.assert_ok();

    let resp = app.get(&format!("/speakers/{id}")).await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_nonexistent_speaker_returns_404() {
    let app = TestApp::new();
    let resp = app.delete("/speakers/nonexistent-id").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}
