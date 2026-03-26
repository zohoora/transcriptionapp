mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn create_and_list_rooms() {
    let app = TestApp::new();

    // Initially empty
    let resp = app.get("/rooms").await;
    resp.assert_ok();
    assert_eq!(resp.json().as_array().unwrap().len(), 0);

    // Create
    let resp = app
        .post_json(
            "/rooms",
            &serde_json::json!({ "name": "Room 6", "description": "Main exam room" }),
        )
        .await;
    resp.assert_ok();
    let created = resp.json();
    assert_eq!(created["name"], "Room 6");
    assert_eq!(created["description"], "Main exam room");
    assert!(created["id"].as_str().unwrap().len() > 0);

    // List
    let resp = app.get("/rooms").await;
    resp.assert_ok();
    assert_eq!(resp.json().as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn get_room_by_id() {
    let app = TestApp::new();

    let resp = app
        .post_json("/rooms", &serde_json::json!({ "name": "Room 2" }))
        .await;
    let id = resp.json()["id"].as_str().unwrap().to_string();

    let resp = app.get(&format!("/rooms/{id}")).await;
    resp.assert_ok();
    assert_eq!(resp.json()["name"], "Room 2");
}

#[tokio::test]
async fn get_nonexistent_room_returns_404() {
    let app = TestApp::new();
    let resp = app.get("/rooms/nonexistent-id").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_room() {
    let app = TestApp::new();

    let resp = app
        .post_json("/rooms", &serde_json::json!({ "name": "Old Room" }))
        .await;
    let id = resp.json()["id"].as_str().unwrap().to_string();

    let resp = app
        .put_json(
            &format!("/rooms/{id}"),
            &serde_json::json!({
                "name": "New Room",
                "presence_sensor_url": "http://172.16.100.37",
                "encounter_detection_mode": "hybrid"
            }),
        )
        .await;
    resp.assert_ok();
    let updated = resp.json();
    assert_eq!(updated["name"], "New Room");
    assert_eq!(updated["presence_sensor_url"], "http://172.16.100.37");
    assert_eq!(updated["encounter_detection_mode"], "hybrid");

    // Verify persistence
    let resp = app.get(&format!("/rooms/{id}")).await;
    resp.assert_ok();
    assert_eq!(resp.json()["name"], "New Room");
}

#[tokio::test]
async fn delete_room() {
    let app = TestApp::new();

    let resp = app
        .post_json("/rooms", &serde_json::json!({ "name": "Temp Room" }))
        .await;
    let id = resp.json()["id"].as_str().unwrap().to_string();

    let resp = app.delete(&format!("/rooms/{id}")).await;
    resp.assert_ok();

    let resp = app.get(&format!("/rooms/{id}")).await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_nonexistent_room_returns_404() {
    let app = TestApp::new();
    let resp = app.delete("/rooms/nonexistent-id").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}
