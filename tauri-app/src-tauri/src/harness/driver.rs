//! Harness driver — runs `run_continuous_mode` against a RecordingRunContext
//! seeded from a ReplayBundle, captures events and archive state, and returns
//! a handle to the captured output for later comparison.
//!
//! Phase 3 state: smoke-level. No comparator here — that arrives in Phase 4.
//! This file's job is to prove the orchestrator can spin up + shut down
//! cleanly from a test context without panicking.

use super::recording_run_context::RecordingRunContext;
use crate::config::Config;
use crate::continuous_mode::{run_continuous_mode, ContinuousModeHandle};
use crate::replay_bundle::ReplayBundle;
use crate::server_sync::ServerSyncContext;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

pub struct SmokeDriveOutcome {
    /// Tempdir holding the archive written by the orchestrator. Persisted
    /// until this struct is dropped.
    pub archive_dir: TempDir,
    /// The test context, holding captured events.
    pub ctx: RecordingRunContext,
    /// Any error returned by run_continuous_mode (vs panicking/timeout).
    pub run_result: Result<(), String>,
}

pub async fn drive_encounter_bundle_smoke(
    bundle: &ReplayBundle,
) -> Result<SmokeDriveOutcome, String> {
    // Tempdir for archive output. Set as env var so local_archive::get_archive_dir()
    // returns this path instead of the user's real ~/.transcriptionapp/archive.
    //
    // CAREFUL: this env var is process-wide. Tests that drive the orchestrator
    // must use #[serial] (from serial_test crate) to avoid interfering with
    // other tests that observe the default archive path. We save+restore the
    // prior value here so non-serial tests in the same binary run that happen
    // to be interleaved at the thread level see the original.
    let archive_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let prior_env = std::env::var("TRANSCRIPTIONAPP_ARCHIVE_DIR").ok();
    std::env::set_var("TRANSCRIPTIONAPP_ARCHIVE_DIR", archive_dir.path());

    // Build a test-safe Config: no live LLM/STT/sensor, sleep mode off.
    let config = harness_config();

    // RecordingRunContext: pre-loaded STT segments from the bundle,
    // replay LLM backend, virtual clock.
    let ctx = RecordingRunContext::from_bundle(bundle, archive_dir.path().to_path_buf());

    let handle = Arc::new(ContinuousModeHandle::new());
    let sync_ctx = ServerSyncContext::empty();

    // Spawn orchestrator. Clone ctx so we can still read captured events
    // after the task completes.
    let ctx_for_run = ctx.clone();
    let handle_for_run = handle.clone();
    let orch_task = tokio::spawn(async move {
        run_continuous_mode(ctx_for_run, handle_for_run, config, sync_ctx).await
    });

    // Advance virtual time so scheduled sleeps fire, letting the orchestrator
    // consume messages. The scripted pipeline drops its sender after loading,
    // so the orchestrator's rx.recv() returns None once drained and shutdown
    // proceeds naturally.
    //
    // 30s of virtual time is enough for the check interval + flush + recovery
    // paths without blowing the budget on tests that run pre-paused.
    tokio::time::advance(Duration::from_secs(30)).await;

    // Explicitly request stop to guarantee termination.
    handle.stop_flag.store(true, Ordering::Relaxed);

    // Further time advance to let stop propagate through loops.
    tokio::time::advance(Duration::from_secs(5)).await;

    // Wait for the task. Cap at 10s real wall-clock to guard against bugs.
    let run_result = match tokio::time::timeout(Duration::from_secs(10), orch_task).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => Err(format!("orchestrator task panicked: {}", e)),
        Err(_) => Err("orchestrator task did not complete within 10s wall time".into()),
    };

    // Restore prior env var so other tests in the same binary run observe
    // the default archive path. (The SmokeDriveOutcome's TempDir holds the
    // actual data; nothing else references the env var after this point.)
    match prior_env {
        Some(v) => std::env::set_var("TRANSCRIPTIONAPP_ARCHIVE_DIR", v),
        None => std::env::remove_var("TRANSCRIPTIONAPP_ARCHIVE_DIR"),
    }

    Ok(SmokeDriveOutcome {
        archive_dir,
        ctx,
        run_result,
    })
}

/// Build a Config suitable for the offline harness.
///
/// Key overrides vs production:
/// - LLM router URL empty → orchestrator's internal LLMClient::new fails
///   gracefully and sets flush_llm_client to None.
/// - STT router URL empty for the same reason.
/// - Sleep mode off.
/// - Sensor detection mode set to llm-only.
/// - No Gemini.
fn harness_config() -> Config {
    let mut c = Config::default();
    c.llm_router_url = String::new();
    c.whisper_server_url = String::new();
    c.gemini_api_key = String::new();
    c.sleep_mode_enabled = false;
    c.encounter_detection_mode = crate::config::EncounterDetectionMode::Llm;
    c.screen_capture_enabled = false;
    c
}

pub(crate) fn _orchestrator_dropped() -> PathBuf {
    // Placeholder for an eventual hook that runs orchestrator-cleanup assertions.
    PathBuf::new()
}

// NB: no unit test here. `drive_encounter_bundle_smoke` sets a process-wide
// env var (TRANSCRIPTIONAPP_ARCHIVE_DIR) to redirect archive writes; running
// it as a lib unit test would cause cross-contamination with other lib tests
// that observe the default archive path.
//
// Integration coverage lives at `tests/harness_per_encounter.rs`, which runs
// as a separate test binary — env vars there are isolated from the lib
// binary's test pool.
