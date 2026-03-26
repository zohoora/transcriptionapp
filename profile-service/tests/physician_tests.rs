mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn create_and_list_physicians() {
    let app = TestApp::new();

    // Initially empty
    let resp = app.get("/physicians").await;
    resp.assert_ok();
    assert_eq!(resp.json().as_array().unwrap().len(), 0);

    // Create
    let resp = app
        .post_json(
            "/physicians",
            &serde_json::json!({ "name": "Dr. Smith", "specialty": "Cardiology" }),
        )
        .await;
    resp.assert_ok();
    let created = resp.json();
    assert_eq!(created["name"], "Dr. Smith");
    assert_eq!(created["specialty"], "Cardiology");
    assert!(created["id"].as_str().unwrap().len() > 0);
    assert!(created["created_at"].as_str().is_some());

    // List
    let resp = app.get("/physicians").await;
    resp.assert_ok();
    let list = resp.json();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["name"], "Dr. Smith");
}

#[tokio::test]
async fn get_physician_by_id() {
    let app = TestApp::new();

    let resp = app
        .post_json(
            "/physicians",
            &serde_json::json!({ "name": "Dr. Jones" }),
        )
        .await;
    resp.assert_ok();
    let id = resp.json()["id"].as_str().unwrap().to_string();

    let resp = app.get(&format!("/physicians/{id}")).await;
    resp.assert_ok();
    assert_eq!(resp.json()["name"], "Dr. Jones");
}

#[tokio::test]
async fn get_nonexistent_physician_returns_404() {
    let app = TestApp::new();
    let resp = app.get("/physicians/nonexistent-id").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_physician() {
    let app = TestApp::new();

    let resp = app
        .post_json(
            "/physicians",
            &serde_json::json!({ "name": "Dr. Old" }),
        )
        .await;
    let id = resp.json()["id"].as_str().unwrap().to_string();

    let resp = app
        .put_json(
            &format!("/physicians/{id}"),
            &serde_json::json!({ "name": "Dr. New", "soap_detail_level": 7 }),
        )
        .await;
    resp.assert_ok();
    let updated = resp.json();
    assert_eq!(updated["name"], "Dr. New");
    assert_eq!(updated["soap_detail_level"], 7);

    // Verify persistence
    let resp = app.get(&format!("/physicians/{id}")).await;
    resp.assert_ok();
    assert_eq!(resp.json()["name"], "Dr. New");
}

#[tokio::test]
async fn delete_physician() {
    let app = TestApp::new();

    let resp = app
        .post_json(
            "/physicians",
            &serde_json::json!({ "name": "Dr. Delete" }),
        )
        .await;
    let id = resp.json()["id"].as_str().unwrap().to_string();

    let resp = app.delete(&format!("/physicians/{id}")).await;
    resp.assert_ok();

    // Verify gone
    let resp = app.get(&format!("/physicians/{id}")).await;
    resp.assert_status(StatusCode::NOT_FOUND);

    // List empty
    let resp = app.get("/physicians").await;
    resp.assert_ok();
    assert_eq!(resp.json().as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn delete_nonexistent_physician_returns_404() {
    let app = TestApp::new();
    let resp = app.delete("/physicians/nonexistent-id").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}
