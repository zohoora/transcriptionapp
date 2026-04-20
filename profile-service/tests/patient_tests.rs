mod common;

use axum::http::StatusCode;
use common::TestApp;

#[tokio::test]
async fn confirm_creates_then_is_idempotent() {
    let app = TestApp::new();

    let first = app
        .post_json(
            "/physicians/phys-1/patients/confirm",
            &serde_json::json!({
                "name": "Judie Joan Guest",
                "dob": "1945-04-08",
                "sessionId": "sess-a",
            }),
        )
        .await;
    first.assert_ok();
    let body = first.json();
    assert_eq!(body["created"], true);
    assert_eq!(body["record"]["name"], "Judie Joan Guest");
    assert_eq!(body["record"]["dob"], "1945-04-08");
    let pid_first = body["patientId"].as_str().unwrap().to_string();
    assert_eq!(
        body["record"]["sessionIds"],
        serde_json::json!(["sess-a"])
    );

    let second = app
        .post_json(
            "/physicians/phys-1/patients/confirm",
            &serde_json::json!({
                "name": "Judie Joan Guest",
                "dob": "1945-04-08",
                "sessionId": "sess-b",
            }),
        )
        .await;
    second.assert_ok();
    let body = second.json();
    assert_eq!(body["created"], false);
    assert_eq!(body["patientId"], pid_first);
    assert_eq!(
        body["record"]["sessionIds"],
        serde_json::json!(["sess-a", "sess-b"])
    );
}

#[tokio::test]
async fn confirm_normalizes_name_so_duplicates_merge() {
    let app = TestApp::new();

    app.post_json(
        "/physicians/phys-1/patients/confirm",
        &serde_json::json!({
            "name": "  john SMITH  ",
            "dob": "1970-01-01",
            "sessionId": "a",
        }),
    )
    .await
    .assert_ok();

    let resp = app
        .post_json(
            "/physicians/phys-1/patients/confirm",
            &serde_json::json!({
                "name": "John Smith",
                "dob": "1970-01-01",
                "sessionId": "b",
            }),
        )
        .await;
    resp.assert_ok();
    let body = resp.json();
    assert_eq!(body["created"], false, "normalization should match existing record");
    assert_eq!(
        body["record"]["sessionIds"],
        serde_json::json!(["a", "b"])
    );
}

#[tokio::test]
async fn confirm_reconciles_medplum_id_on_later_call() {
    let app = TestApp::new();

    app.post_json(
        "/physicians/phys-1/patients/confirm",
        &serde_json::json!({
            "name": "A",
            "dob": "1990-01-01",
            "sessionId": "a",
        }),
    )
    .await
    .assert_ok();

    let resp = app
        .post_json(
            "/physicians/phys-1/patients/confirm",
            &serde_json::json!({
                "name": "A",
                "dob": "1990-01-01",
                "sessionId": "b",
                "medplumPatientId": "mp-42",
            }),
        )
        .await;
    resp.assert_ok();
    assert_eq!(resp.json()["record"]["medplumPatientId"], "mp-42");
}

#[tokio::test]
async fn confirm_rejects_bad_dob_format() {
    let app = TestApp::new();
    let resp = app
        .post_json(
            "/physicians/phys-1/patients/confirm",
            &serde_json::json!({
                "name": "A",
                "dob": "04/08/1945",
                "sessionId": "a",
            }),
        )
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn search_returns_exact_match_or_empty() {
    let app = TestApp::new();

    app.post_json(
        "/physicians/phys-1/patients/confirm",
        &serde_json::json!({
            "name": "Judie Guest",
            "dob": "1945-04-08",
            "sessionId": "a",
        }),
    )
    .await
    .assert_ok();

    let hit = app
        .get("/physicians/phys-1/patients?name=Judie%20Guest&dob=1945-04-08")
        .await;
    hit.assert_ok();
    let list = hit.json();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["name"], "Judie Guest");

    let miss = app
        .get("/physicians/phys-1/patients?name=Other&dob=1945-04-08")
        .await;
    miss.assert_ok();
    assert_eq!(miss.json().as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn list_for_physician_returns_all() {
    let app = TestApp::new();

    for (name, dob) in [("A", "1970-01-01"), ("B", "1980-01-01"), ("C", "1990-01-01")] {
        app.post_json(
            "/physicians/phys-1/patients/confirm",
            &serde_json::json!({
                "name": name,
                "dob": dob,
                "sessionId": "s",
            }),
        )
        .await
        .assert_ok();
    }
    app.post_json(
        "/physicians/phys-2/patients/confirm",
        &serde_json::json!({
            "name": "D",
            "dob": "1960-01-01",
            "sessionId": "s",
        }),
    )
    .await
    .assert_ok();

    let resp = app.get("/physicians/phys-1/patients").await;
    resp.assert_ok();
    let list = resp.json();
    assert_eq!(list.as_array().unwrap().len(), 3);
    for r in list.as_array().unwrap() {
        assert_eq!(r["physicianId"], "phys-1");
    }
}

#[tokio::test]
async fn get_by_patient_id_returns_full_record() {
    let app = TestApp::new();

    let created = app
        .post_json(
            "/physicians/phys-1/patients/confirm",
            &serde_json::json!({
                "name": "Judie Guest",
                "dob": "1945-04-08",
                "sessionId": "sess-a",
                "medplumPatientId": "mp-7",
            }),
        )
        .await;
    created.assert_ok();
    let pid = created.json()["patientId"].as_str().unwrap().to_string();
    assert_eq!(pid, "mp-7", "medplumPatientId should seed patient_id");

    let resp = app
        .get(&format!("/physicians/phys-1/patients/{pid}"))
        .await;
    resp.assert_ok();
    assert_eq!(resp.json()["name"], "Judie Guest");
    assert_eq!(resp.json()["medplumPatientId"], "mp-7");
}

#[tokio::test]
async fn get_nonexistent_patient_returns_404() {
    let app = TestApp::new();
    let resp = app.get("/physicians/phys-1/patients/nope").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}
