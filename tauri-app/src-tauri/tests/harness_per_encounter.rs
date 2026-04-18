//! Per-encounter equivalence tests.
//!
//! Each test loads a real production replay_bundle.json, drives the
//! orchestrator through RecordingRunContext, and asserts the produced archive
//! state matches the snapshot baseline sidecar (<bundle>.baseline.json).
//!
//! Snapshot semantics: first run (or HARNESS_RECORD_BASELINES=1) records the
//! baseline; subsequent runs verify the orchestrator still produces the same
//! archive state.
//!
//! Seed corpus: 10 hybrid_llm bundles from 2026-04-01 through 2026-04-17
//! spanning small (4–18 seg), medium (100–200 seg), and large (200–400 seg)
//! encounter sizes. Sensor-triggered bundles can't be replayed yet — the
//! harness doesn't inject a scripted sensor source (deferred).
//!
//! On failure: a structured MismatchReport is written to
//! target/harness-report/<test_id>.json and a one-liner printed to stderr.
//! Re-run with the printed drill-in command for focused debugging.
//!
//! Tests are `#[serial]` because they share a process-wide
//! TRANSCRIPTIONAPP_ARCHIVE_DIR env var.

use serial_test::serial;
use transcription_app_lib::harness::EncounterHarness;

macro_rules! harness_test {
    ($test_name:ident, $file:expr) => {
        #[tokio::test(flavor = "current_thread", start_paused = true)]
        #[serial]
        async fn $test_name() {
            EncounterHarness::new($file).run().await.expect_equivalent();
        }
    };
}

// --- 2026-04-01 ---
harness_test!(
    seed_2026_04_01_03ffd0eb,
    "tests/fixtures/encounter_bundles/seed/2026-04-01_03ffd0eb.json"
);
harness_test!(
    seed_2026_04_01_0e7b5a30,
    "tests/fixtures/encounter_bundles/seed/2026-04-01_0e7b5a30.json"
);
harness_test!(
    seed_2026_04_01_573e02a8,
    "tests/fixtures/encounter_bundles/seed/2026-04-01_573e02a8.json"
);
harness_test!(
    seed_2026_04_01_b75905e0,
    "tests/fixtures/encounter_bundles/seed/2026-04-01_b75905e0.json"
);

// --- 2026-04-08 ---
harness_test!(
    seed_2026_04_08_5c2a50a1,
    "tests/fixtures/encounter_bundles/seed/2026-04-08_5c2a50a1.json"
);
harness_test!(
    seed_2026_04_08_3a56b11f,
    "tests/fixtures/encounter_bundles/seed/2026-04-08_3a56b11f.json"
);

// --- 2026-04-09 ---
harness_test!(
    seed_2026_04_09_8d205075,
    "tests/fixtures/encounter_bundles/seed/2026-04-09_8d205075.json"
);
harness_test!(
    seed_2026_04_09_b25f029c,
    "tests/fixtures/encounter_bundles/seed/2026-04-09_b25f029c.json"
);

// --- 2026-04-14 ---
harness_test!(
    seed_2026_04_14_4b36a186,
    "tests/fixtures/encounter_bundles/seed/2026-04-14_4b36a186.json"
);

// --- 2026-04-17 ---
harness_test!(
    seed_2026_04_17_beba1f94,
    "tests/fixtures/encounter_bundles/seed/2026-04-17_beba1f94.json"
);
