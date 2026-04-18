//! Per-encounter equivalence tests.
//!
//! Each test loads a real production replay_bundle.json, drives the
//! orchestrator through RecordingRunContext, and asserts the produced archive
//! state matches the snapshot baseline sidecar.
//!
//! Snapshot semantics: first run (or HARNESS_RECORD_BASELINES=1) records the
//! `*.baseline.json` sidecar; subsequent runs verify against it.
//!
//! Tests are `#[serial]` because they share a process-wide
//! TRANSCRIPTIONAPP_ARCHIVE_DIR env var.

use serial_test::serial;
use transcription_app_lib::harness::EncounterHarness;

#[tokio::test(flavor = "current_thread", start_paused = true)]
#[serial]
async fn seed_bundle_03ffd0eb() {
    EncounterHarness::new("tests/fixtures/encounter_bundles/seed/2026-04-01_03ffd0eb.json")
        .run()
        .await
        .expect_equivalent();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
#[serial]
async fn seed_bundle_0e7b5a30() {
    EncounterHarness::new("tests/fixtures/encounter_bundles/seed/2026-04-01_0e7b5a30.json")
        .run()
        .await
        .expect_equivalent();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
#[serial]
async fn seed_bundle_573e02a8() {
    EncounterHarness::new("tests/fixtures/encounter_bundles/seed/2026-04-01_573e02a8.json")
        .run()
        .await
        .expect_equivalent();
}
