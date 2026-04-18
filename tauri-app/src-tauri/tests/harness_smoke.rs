//! Phase 3 smoke tests — the orchestrator drives real production bundles
//! through RecordingRunContext without panicking.
//!
//! No equivalence comparison yet (Phase 4 adds the comparator). These tests
//! only verify the harness can spin up + shut down cleanly on real inputs.

use transcription_app_lib::harness::driver::drive_encounter_bundle_smoke;
use transcription_app_lib::replay_bundle::ReplayBundle;

fn load(path: &str) -> ReplayBundle {
    let body = std::fs::read_to_string(path).expect("fixture exists");
    serde_json::from_str(&body).expect("fixture parses as ReplayBundle")
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn seed_bundle_98d5ee95() {
    let bundle = load("tests/fixtures/encounter_bundles/seed/2026-04-14_98d5ee95.json");
    let _outcome = drive_encounter_bundle_smoke(&bundle)
        .await
        .expect("smoke drive did not complete");
    // No assertion on outcome.run_result — Phase 3 only guards against panics.
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn seed_bundle_bfcf6574() {
    let bundle = load("tests/fixtures/encounter_bundles/seed/2026-04-14_bfcf6574.json");
    let _outcome = drive_encounter_bundle_smoke(&bundle)
        .await
        .expect("smoke drive did not complete");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn seed_bundle_73d178ff() {
    let bundle = load("tests/fixtures/encounter_bundles/seed/2026-04-14_73d178ff.json");
    let _outcome = drive_encounter_bundle_smoke(&bundle)
        .await
        .expect("smoke drive did not complete");
}
