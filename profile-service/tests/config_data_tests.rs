mod common;

use common::TestApp;

#[tokio::test]
async fn get_config_version_returns_initial() {
    let app = TestApp::new();

    let resp = app.get("/config/version").await;
    resp.assert_ok();
    let json = resp.json();
    assert_eq!(json["version"], 1);
    assert!(json["updated_at"].is_string());
}

#[tokio::test]
async fn get_default_thresholds() {
    let app = TestApp::new();

    let resp = app.get("/config/thresholds").await;
    resp.assert_ok();
    let json = resp.json();
    // Verify defaults match compiled constants
    assert_eq!(json["force_check_word_threshold"], 3000);
    assert_eq!(json["force_split_word_threshold"], 5000);
    assert_eq!(json["force_split_consecutive_limit"], 3);
    assert_eq!(json["absolute_word_cap"], 25000);
    assert_eq!(json["min_words_for_clinical_check"], 100);
    assert_eq!(json["screenshot_stale_grace_secs"], 90);
    assert_eq!(json["multi_patient_check_word_threshold"], 2500);
    assert_eq!(json["multi_patient_split_min_words"], 500);
    assert_eq!(json["confidence_base_short"], 0.85);
    assert_eq!(json["confidence_base_long"], 0.70);
    assert_eq!(json["soap_generation_timeout_secs"], 300);
    assert_eq!(json["daily_hour_limit"], 14.0);
    assert_eq!(json["monthly_hour_limit"], 240.0);
    assert_eq!(json["monthly_window_days"], 28);
}

#[tokio::test]
async fn update_thresholds_bumps_version() {
    let app = TestApp::new();

    // Get initial version
    let v1 = app.get("/config/version").await.json();
    let initial_version = v1["version"].as_u64().unwrap();

    // Update thresholds
    let resp = app
        .put_json(
            "/config/thresholds",
            &serde_json::json!({
                "force_check_word_threshold": 4000,
                "absolute_word_cap": 30000
            }),
        )
        .await;
    resp.assert_ok();
    let updated = resp.json();
    assert_eq!(updated["force_check_word_threshold"], 4000);
    assert_eq!(updated["absolute_word_cap"], 30000);

    // Version should have bumped
    let v2 = app.get("/config/version").await.json();
    assert!(v2["version"].as_u64().unwrap() > initial_version);

    // Read back to verify persistence
    let resp = app.get("/config/thresholds").await;
    resp.assert_ok();
    let json = resp.json();
    assert_eq!(json["force_check_word_threshold"], 4000);
    assert_eq!(json["absolute_word_cap"], 30000);
    // Unset fields keep defaults
    assert_eq!(json["force_split_word_threshold"], 5000);
}

#[tokio::test]
async fn get_default_prompts_returns_empty() {
    let app = TestApp::new();

    let resp = app.get("/config/prompts").await;
    resp.assert_ok();
    let json = resp.json();
    // Default prompts are all empty strings
    assert_eq!(json["soap_base_template"], "");
    assert_eq!(json["encounter_detection_system"], "");
    assert_eq!(json["clinical_content_check"], "");
    assert_eq!(json["billing_extraction"], "");
}

#[tokio::test]
async fn update_prompts_and_read_back() {
    let app = TestApp::new();

    let resp = app
        .put_json(
            "/config/prompts",
            &serde_json::json!({
                "encounter_detection_system": "You are analyzing a transcript...",
                "clinical_content_check": "Determine if this is clinical..."
            }),
        )
        .await;
    resp.assert_ok();

    let resp = app.get("/config/prompts").await;
    resp.assert_ok();
    let json = resp.json();
    assert_eq!(
        json["encounter_detection_system"],
        "You are analyzing a transcript..."
    );
    assert_eq!(
        json["clinical_content_check"],
        "Determine if this is clinical..."
    );
    // Other prompts remain empty
    assert_eq!(json["soap_base_template"], "");
}

#[tokio::test]
async fn get_default_billing_returns_empty() {
    let app = TestApp::new();

    let resp = app.get("/config/billing").await;
    resp.assert_ok();
    let json = resp.json();
    assert_eq!(json["ohip_codes"].as_array().unwrap().len(), 0);
    assert_eq!(json["diagnostic_codes"].as_array().unwrap().len(), 0);
    assert_eq!(json["exclusion_groups"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn update_billing_with_codes() {
    let app = TestApp::new();

    let resp = app
        .put_json(
            "/config/billing",
            &serde_json::json!({
                "ohip_codes": [
                    {
                        "code": "A001A",
                        "description": "Minor Assessment",
                        "ffs_rate_cents": 2680,
                        "basket": "in",
                        "shadow_pct": 30,
                        "category": "assessment",
                        "after_hours_eligible": true
                    },
                    {
                        "code": "A003A",
                        "description": "General Assessment",
                        "ffs_rate_cents": 9560,
                        "basket": "in",
                        "shadow_pct": 30,
                        "category": "assessment",
                        "after_hours_eligible": true
                    }
                ],
                "diagnostic_codes": [
                    {
                        "code": "250",
                        "description": "Diabetes mellitus",
                        "category": "Diabetes"
                    }
                ],
                "visit_type_mappings": {
                    "MinorAssessment": {"code": "A001A"},
                    "GeneralAssessment": {"code": "A003A"}
                }
            }),
        )
        .await;
    resp.assert_ok();

    let resp = app.get("/config/billing").await;
    resp.assert_ok();
    let json = resp.json();
    assert_eq!(json["ohip_codes"].as_array().unwrap().len(), 2);
    assert_eq!(json["ohip_codes"][0]["code"], "A001A");
    assert_eq!(json["ohip_codes"][0]["ffs_rate_cents"], 2680);
    assert_eq!(json["diagnostic_codes"].as_array().unwrap().len(), 1);
    assert_eq!(json["diagnostic_codes"][0]["code"], "250");
    assert_eq!(json["visit_type_mappings"]["MinorAssessment"]["code"], "A001A");
}

#[tokio::test]
async fn version_bumps_independently_per_category() {
    let app = TestApp::new();

    let v0 = app.get("/config/version").await.json()["version"]
        .as_u64()
        .unwrap();

    // Update prompts
    app.put_json(
        "/config/prompts",
        &serde_json::json!({"encounter_detection_system": "test"}),
    )
    .await
    .assert_ok();

    let v1 = app.get("/config/version").await.json()["version"]
        .as_u64()
        .unwrap();
    assert!(v1 > v0);

    // Update thresholds
    app.put_json(
        "/config/thresholds",
        &serde_json::json!({"force_check_word_threshold": 5000}),
    )
    .await
    .assert_ok();

    let v2 = app.get("/config/version").await.json()["version"]
        .as_u64()
        .unwrap();
    assert!(v2 > v1);

    // Update billing
    app.put_json(
        "/config/billing",
        &serde_json::json!({"ohip_codes": []}),
    )
    .await
    .assert_ok();

    let v3 = app.get("/config/version").await.json()["version"]
        .as_u64()
        .unwrap();
    assert!(v3 > v2);
}
