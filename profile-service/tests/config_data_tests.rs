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
    // Category A extensions (Phase 3)
    assert_eq!(json["multi_patient_detect_word_threshold"], 500);
    assert_eq!(json["vision_skip_streak_k"], 5);
    assert_eq!(json["vision_skip_call_cap"], 30);
    assert_eq!(json["gemini_generation_timeout_secs"], 45);
}

#[tokio::test]
async fn detection_thresholds_back_compat_cat_a_extensions() {
    use profile_service::types::DetectionThresholds;

    // Old client caches / older server responses won't carry the new Cat A
    // fields. Per-field `#[serde(default = "fn")]` must fill them in cleanly.
    let parsed: DetectionThresholds =
        serde_json::from_str("{}").expect("parses empty object");
    assert_eq!(parsed.multi_patient_detect_word_threshold, 500);
    assert_eq!(parsed.vision_skip_streak_k, 5);
    assert_eq!(parsed.vision_skip_call_cap, 30);
    assert_eq!(parsed.gemini_generation_timeout_secs, 45);
    // Sanity-check an existing field — defense against accidental renames
    // collapsing back-compat for the whole struct.
    assert_eq!(parsed.force_check_word_threshold, 3000);
}

#[tokio::test]
async fn update_thresholds_persists_cat_a_extensions() {
    let app = TestApp::new();

    let resp = app
        .put_json(
            "/config/thresholds",
            &serde_json::json!({
                "multi_patient_detect_word_threshold": 750,
                "vision_skip_streak_k": 7,
                "vision_skip_call_cap": 50,
                "gemini_generation_timeout_secs": 60
            }),
        )
        .await;
    resp.assert_ok();

    let resp = app.get("/config/thresholds").await;
    resp.assert_ok();
    let json = resp.json();
    assert_eq!(json["multi_patient_detect_word_threshold"], 750);
    assert_eq!(json["vision_skip_streak_k"], 7);
    assert_eq!(json["vision_skip_call_cap"], 50);
    assert_eq!(json["gemini_generation_timeout_secs"], 60);
    // Untouched Phase 2 fields still hold defaults
    assert_eq!(json["force_check_word_threshold"], 3000);
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

// ── OperationalDefaults (Phase 3) ────────────────────────────────────

#[tokio::test]
async fn get_default_operational_defaults() {
    let app = TestApp::new();

    let resp = app.get("/config/defaults").await;
    resp.assert_ok();
    let json = resp.json();
    assert_eq!(json["sleep_start_hour"], 22);
    assert_eq!(json["sleep_end_hour"], 6);
    assert_eq!(json["thermal_hot_pixel_threshold_c"], 28.0);
    assert_eq!(json["co2_baseline_ppm"], 420.0);
    assert_eq!(json["encounter_check_interval_secs"], 120);
    assert_eq!(json["encounter_silence_trigger_secs"], 45);
    assert_eq!(json["soap_model"], "soap-model-fast");
    assert_eq!(json["soap_model_fast"], "soap-model-fast");
    assert_eq!(json["fast_model"], "fast-model");
    assert_eq!(json["encounter_detection_model"], "fast-model");
}

#[tokio::test]
async fn operational_defaults_back_compat_empty_object() {
    use profile_service::types::OperationalDefaults;

    // Deserializing `{}` must populate every field with its compiled default —
    // guarantees old client caches still parse after field additions.
    let parsed: OperationalDefaults = serde_json::from_str("{}").expect("parses empty object");
    assert_eq!(parsed, OperationalDefaults::default());
}

#[tokio::test]
async fn operational_defaults_round_trip() {
    use profile_service::types::OperationalDefaults;

    let original = OperationalDefaults {
        version: 0,
        sleep_start_hour: 21,
        sleep_end_hour: 7,
        thermal_hot_pixel_threshold_c: 29.5,
        co2_baseline_ppm: 450.0,
        encounter_check_interval_secs: 90,
        encounter_silence_trigger_secs: 30,
        soap_model: "custom-soap".to_string(),
        soap_model_fast: "custom-soap-fast".to_string(),
        fast_model: "custom-fast".to_string(),
        encounter_detection_model: "custom-detect".to_string(),
    };

    let serialized = serde_json::to_string(&original).expect("serializes");
    let round_tripped: OperationalDefaults =
        serde_json::from_str(&serialized).expect("deserializes");
    assert_eq!(round_tripped, original);
}

#[tokio::test]
async fn update_operational_defaults_and_read_back() {
    let app = TestApp::new();

    let resp = app
        .put_json(
            "/config/defaults",
            &serde_json::json!({
                "sleep_start_hour": 21,
                "sleep_end_hour": 7,
                "thermal_hot_pixel_threshold_c": 28.0,
                "co2_baseline_ppm": 420.0,
                "encounter_check_interval_secs": 60,
                "encounter_silence_trigger_secs": 30,
                "soap_model": "soap-model-fast",
                "soap_model_fast": "soap-model-fast",
                "fast_model": "fast-model",
                "encounter_detection_model": "fast-model"
            }),
        )
        .await;
    resp.assert_ok();
    let updated = resp.json();
    assert_eq!(updated["sleep_start_hour"], 21);
    assert_eq!(updated["encounter_check_interval_secs"], 60);

    // Read back to verify persistence
    let resp = app.get("/config/defaults").await;
    resp.assert_ok();
    let json = resp.json();
    assert_eq!(json["sleep_start_hour"], 21);
    assert_eq!(json["sleep_end_hour"], 7);
    assert_eq!(json["encounter_silence_trigger_secs"], 30);
}

#[tokio::test]
async fn update_operational_defaults_bumps_version() {
    let app = TestApp::new();

    let initial = app.get("/config/version").await.json()["version"]
        .as_u64()
        .unwrap();

    let resp = app
        .put_json(
            "/config/defaults",
            &serde_json::json!({
                "sleep_start_hour": 23,
                "sleep_end_hour": 5,
                "thermal_hot_pixel_threshold_c": 28.0,
                "co2_baseline_ppm": 420.0,
                "encounter_check_interval_secs": 120,
                "encounter_silence_trigger_secs": 45,
                "soap_model": "soap-model-fast",
                "soap_model_fast": "soap-model-fast",
                "fast_model": "fast-model",
                "encounter_detection_model": "fast-model"
            }),
        )
        .await;
    resp.assert_ok();
    // Returned body must carry the bumped version stamped into the struct —
    // parity with prompts/billing/thresholds so clients can cache payload +
    // version together and detect defaults-specific staleness.
    let body_version = resp.json()["version"].as_u64().unwrap();

    let after = app.get("/config/version").await.json()["version"]
        .as_u64()
        .unwrap();
    assert!(after > initial, "version must bump on update_defaults");
    assert_eq!(
        body_version, after,
        "update_defaults response must stamp the bumped shared version into the returned struct"
    );
}

#[tokio::test]
async fn operational_defaults_store_persists_across_reload() {
    use profile_service::store::config_data::ConfigDataStore;
    use profile_service::types::OperationalDefaults;

    let tmp = tempfile::tempdir().expect("tempdir");
    let mut store = ConfigDataStore::load(tmp.path()).expect("load");

    let mut new_defaults = OperationalDefaults::default();
    new_defaults.sleep_start_hour = 21;
    new_defaults.encounter_check_interval_secs = 300;
    new_defaults.soap_model = "my-soap".to_string();
    store.update_defaults(new_defaults).expect("update");

    // Reload from the same dir — values must round-trip via operational_defaults.json
    let store2 = ConfigDataStore::load(tmp.path()).expect("reload");
    let loaded = store2.get_defaults();
    assert_eq!(loaded.sleep_start_hour, 21);
    assert_eq!(loaded.encounter_check_interval_secs, 300);
    assert_eq!(loaded.soap_model, "my-soap");
    // Untouched fields still equal defaults
    assert_eq!(loaded.sleep_end_hour, 6);
    assert_eq!(loaded.fast_model, "fast-model");
}

#[tokio::test]
async fn operational_defaults_validate_rejects_bad_values() {
    use profile_service::types::OperationalDefaults;

    // sleep_start_hour > 23
    let mut bad = OperationalDefaults::default();
    bad.sleep_start_hour = 24;
    assert!(bad.validate().is_err());

    // sleep_end_hour > 23
    let mut bad = OperationalDefaults::default();
    bad.sleep_end_hour = 25;
    assert!(bad.validate().is_err());

    // thermal_hot_pixel_threshold_c below range
    let mut bad = OperationalDefaults::default();
    bad.thermal_hot_pixel_threshold_c = 10.0;
    assert!(bad.validate().is_err());

    // thermal_hot_pixel_threshold_c above range
    let mut bad = OperationalDefaults::default();
    bad.thermal_hot_pixel_threshold_c = 50.0;
    assert!(bad.validate().is_err());

    // co2_baseline_ppm below range
    let mut bad = OperationalDefaults::default();
    bad.co2_baseline_ppm = 100.0;
    assert!(bad.validate().is_err());

    // co2_baseline_ppm above range
    let mut bad = OperationalDefaults::default();
    bad.co2_baseline_ppm = 1000.0;
    assert!(bad.validate().is_err());

    // encounter_check_interval_secs below range
    let mut bad = OperationalDefaults::default();
    bad.encounter_check_interval_secs = 5;
    assert!(bad.validate().is_err());

    // encounter_check_interval_secs above range
    let mut bad = OperationalDefaults::default();
    bad.encounter_check_interval_secs = 5000;
    assert!(bad.validate().is_err());

    // encounter_silence_trigger_secs below range
    let mut bad = OperationalDefaults::default();
    bad.encounter_silence_trigger_secs = 1;
    assert!(bad.validate().is_err());

    // encounter_silence_trigger_secs above range
    let mut bad = OperationalDefaults::default();
    bad.encounter_silence_trigger_secs = 900;
    assert!(bad.validate().is_err());

    // Empty model strings
    let mut bad = OperationalDefaults::default();
    bad.soap_model = String::new();
    assert!(bad.validate().is_err());

    let mut bad = OperationalDefaults::default();
    bad.soap_model_fast = "   ".to_string();
    assert!(bad.validate().is_err());

    let mut bad = OperationalDefaults::default();
    bad.fast_model = String::new();
    assert!(bad.validate().is_err());

    let mut bad = OperationalDefaults::default();
    bad.encounter_detection_model = String::new();
    assert!(bad.validate().is_err());

    // sleep_start_hour == sleep_end_hour — rejected (otherwise the sleep window
    // collapses to either 0h or 24h, neither of which is a sensible pause window)
    let mut bad = OperationalDefaults::default();
    bad.sleep_start_hour = 10;
    bad.sleep_end_hour = 10;
    let err = bad.validate().expect_err("equal hours must error");
    let msg = format!("{err}");
    assert!(
        msg.contains("differ"),
        "expected equality error message, got: {msg}"
    );

    // Good values pass
    assert!(OperationalDefaults::default().validate().is_ok());
}

#[tokio::test]
async fn operational_defaults_validate_accepts_exact_boundaries() {
    use profile_service::types::OperationalDefaults;

    // Locks in inclusive-range semantics at both ends of every bounded field.
    // If a future refactor tightens a `..=` to `..`, these tests flip red.
    let boundary_cases: &[(&str, OperationalDefaults)] = &[
        (
            "thermal low",
            OperationalDefaults {
                thermal_hot_pixel_threshold_c: 20.0,
                ..OperationalDefaults::default()
            },
        ),
        (
            "thermal high",
            OperationalDefaults {
                thermal_hot_pixel_threshold_c: 40.0,
                ..OperationalDefaults::default()
            },
        ),
        (
            "co2 low",
            OperationalDefaults {
                co2_baseline_ppm: 300.0,
                ..OperationalDefaults::default()
            },
        ),
        (
            "co2 high",
            OperationalDefaults {
                co2_baseline_ppm: 600.0,
                ..OperationalDefaults::default()
            },
        ),
        (
            "check interval low",
            OperationalDefaults {
                encounter_check_interval_secs: 10,
                ..OperationalDefaults::default()
            },
        ),
        (
            "check interval high",
            OperationalDefaults {
                encounter_check_interval_secs: 3600,
                ..OperationalDefaults::default()
            },
        ),
        (
            "silence trigger low",
            OperationalDefaults {
                encounter_silence_trigger_secs: 5,
                ..OperationalDefaults::default()
            },
        ),
        (
            "silence trigger high",
            OperationalDefaults {
                encounter_silence_trigger_secs: 600,
                ..OperationalDefaults::default()
            },
        ),
        (
            "sleep_start_hour = 0",
            OperationalDefaults {
                sleep_start_hour: 0,
                sleep_end_hour: 6, // differ to dodge the equality rule
                ..OperationalDefaults::default()
            },
        ),
        (
            "sleep_start_hour = 23",
            OperationalDefaults {
                sleep_start_hour: 23,
                sleep_end_hour: 6,
                ..OperationalDefaults::default()
            },
        ),
    ];

    for (label, defaults) in boundary_cases {
        assert!(
            defaults.validate().is_ok(),
            "boundary case '{label}' must pass validate(), but got: {:?}",
            defaults.validate().unwrap_err()
        );
    }
}

#[tokio::test]
async fn update_operational_defaults_rejects_invalid_via_route() {
    let app = TestApp::new();

    let resp = app
        .put_json(
            "/config/defaults",
            &serde_json::json!({
                "sleep_start_hour": 24,
                "sleep_end_hour": 6,
                "thermal_hot_pixel_threshold_c": 28.0,
                "co2_baseline_ppm": 420.0,
                "encounter_check_interval_secs": 120,
                "encounter_silence_trigger_secs": 45,
                "soap_model": "soap-model-fast",
                "soap_model_fast": "soap-model-fast",
                "fast_model": "fast-model",
                "encounter_detection_model": "fast-model"
            }),
        )
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body = resp.json();
    assert!(
        body["error"].as_str().unwrap_or("").contains("sleep_start_hour"),
        "expected error mentioning sleep_start_hour, got: {body}"
    );

    // Empty model string — also 400
    let resp = app
        .put_json(
            "/config/defaults",
            &serde_json::json!({
                "sleep_start_hour": 22,
                "sleep_end_hour": 6,
                "thermal_hot_pixel_threshold_c": 28.0,
                "co2_baseline_ppm": 420.0,
                "encounter_check_interval_secs": 120,
                "encounter_silence_trigger_secs": 45,
                "soap_model": "",
                "soap_model_fast": "soap-model-fast",
                "fast_model": "fast-model",
                "encounter_detection_model": "fast-model"
            }),
        )
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_operational_defaults_partial_body_uses_defaults() {
    let app = TestApp::new();

    // Body with only one field — serde fills the rest with compiled defaults
    let resp = app
        .put_json(
            "/config/defaults",
            &serde_json::json!({"sleep_start_hour": 21}),
        )
        .await;
    resp.assert_ok();
    let updated = resp.json();
    assert_eq!(updated["sleep_start_hour"], 21);
    // Unspecified fields fall back to per-field defaults
    assert_eq!(updated["sleep_end_hour"], 6);
    assert_eq!(updated["soap_model"], "soap-model-fast");
    assert_eq!(updated["encounter_check_interval_secs"], 120);
}
