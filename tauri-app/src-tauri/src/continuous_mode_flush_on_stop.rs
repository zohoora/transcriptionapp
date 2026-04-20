//! Continuous-mode shutdown pipeline.
//!
//! Runs on continuous_mode_stopped: cleans up spawned tasks, recovers orphaned
//! SOAPs + billing, flushes any remaining buffer as a final encounter (>100w),
//! runs a flush-time merge check against today's previous session, writes the
//! day's performance_summary.json, and emits the final `Stopped` event.
//!
//! LOGGER SESSION CONTRACT: on entry, the pipeline logger may or may not be
//! pointed at a session — the last encounter's session if the detector
//! finished cleanly, or unset. If a final flush session is produced, this
//! component redirects the logger to it. After exit, the logger state is
//! whatever this function left it at (no caller depends on it).
//!
//! COMPONENT: `continuous_mode_flush_on_stop`.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::continuous_mode::{
    head_words, tail_words, ContinuousModeHandle, ContinuousState, MERGE_EXCERPT_WORDS,
};
use crate::continuous_mode_events::ContinuousModeEvent;
use crate::day_log::DayLogger;
use crate::encounter_experiment::strip_hallucinations;
use crate::llm_client::LLMClient;
use crate::local_archive;
use crate::pipeline::PipelineHandle;
use crate::pipeline_log::PipelineLogger;
use crate::presence_sensor::PresenceSensor;
use crate::run_context::RunContext;
use crate::server_config::{BillingData, PromptTemplates};
use crate::server_sync::ServerSyncContext;

/// Long-lived dependency bundle. Constructed by the caller once before handing
/// off to `run`. All fields are moved into this struct; callers that still
/// need any of them after shutdown would have to clone first (none do today).
pub struct FlushOnStopDeps {
    pub handle: Arc<ContinuousModeHandle>,
    pub sync_ctx: ServerSyncContext,
    pub llm_client: Option<LLMClient>,
    pub soap_model: String,
    pub fast_model: String,
    pub soap_detail_level: u8,
    pub soap_format: String,
    pub soap_custom_instructions: String,
    pub logger: Arc<std::sync::Mutex<PipelineLogger>>,
    pub day_logger: Arc<Option<DayLogger>>,
    pub templates: Arc<PromptTemplates>,
    pub billing_data: Arc<BillingData>,
    pub min_words_for_clinical_check: usize,
    pub merge_enabled: bool,
    pub soap_generation_timeout_secs: u64,
    pub billing_extraction_timeout_secs: u64,
    pub billing_counselling_exhausted: bool,
}

/// Tasks + hardware handles that need to be stopped + joined before the flush
/// pipeline runs. Kept separate from `FlushOnStopDeps` because they are
/// resources produced by the orchestrator's main setup, not long-lived config.
pub struct FlushOnStopHandles {
    pub pipeline_handle: PipelineHandle,
    pub sensor_handle: Option<PresenceSensor>,
    pub consumer_task: JoinHandle<()>,
    pub detector_task: JoinHandle<()>,
    pub screenshot_task: Option<JoinHandle<()>>,
    pub shadow_task: Option<JoinHandle<()>>,
    pub sensor_monitor_task: Option<JoinHandle<()>>,
}

/// Run the shutdown + flush pipeline. Always succeeds (errors are logged as
/// warnings; nothing here propagates a failure up — the orchestrator is
/// shutting down regardless).
pub async fn run<C: RunContext>(
    ctx: &C,
    deps: FlushOnStopDeps,
    handles: FlushOnStopHandles,
) -> Result<(), String> {
    let FlushOnStopDeps {
        handle,
        sync_ctx,
        llm_client: flush_llm_client,
        soap_model: flush_soap_model,
        fast_model: flush_fast_model,
        soap_detail_level: flush_soap_detail_level,
        soap_format: flush_soap_format,
        soap_custom_instructions: flush_soap_custom_instructions,
        logger: logger_for_flush,
        day_logger: day_logger_for_flush,
        templates: flush_templates,
        billing_data: flush_billing_data,
        min_words_for_clinical_check,
        merge_enabled,
        soap_generation_timeout_secs,
        billing_extraction_timeout_secs,
        billing_counselling_exhausted,
    } = deps;

    let FlushOnStopHandles {
        pipeline_handle,
        sensor_handle,
        consumer_task,
        detector_task,
        screenshot_task,
        shadow_task,
        sensor_monitor_task,
    } = handles;

    // Cleanup: stop presence sensor if active
    if let Some(mut sensor) = sensor_handle {
        info!(
            event = "flush_stop_sensor",
            component = "continuous_mode_flush_on_stop",
            "Stopping presence sensor"
        );
        sensor.stop();
    }

    // Cleanup: stop pipeline
    info!(
        event = "flush_stop_pipeline",
        component = "continuous_mode_flush_on_stop",
        "Stopping continuous mode pipeline"
    );
    pipeline_handle.stop();

    // Join pipeline handle in a blocking task to avoid Drop blocking the Tokio thread
    tokio::task::spawn_blocking(move || {
        pipeline_handle.join();
    })
    .await
    .ok();

    // Wait for tasks to finish
    let _ = consumer_task.await;
    detector_task.abort(); // Force stop the detector loop
    let _ = detector_task.await;
    if let Some(task) = screenshot_task {
        task.abort();
        let _ = task.await;
    }
    if let Some(task) = shadow_task {
        task.abort();
        let _ = task.await;
    }
    if let Some(task) = sensor_monitor_task {
        task.abort();
        let _ = task.await;
    }

    // ---- Orphaned SOAP recovery ----
    // Gated on a real Tauri AppHandle: the recovery helper emits events via
    // AppHandle directly. In test contexts (no AppHandle) the orchestrator
    // archive state can be verified directly without this recovery path.
    if let (Some(ref client), Some(app_handle)) = (flush_llm_client.as_ref(), ctx.raw_tauri_app()) {
        crate::encounter_pipeline::recover_orphaned_soap(
            client,
            &flush_soap_model,
            flush_soap_detail_level,
            &flush_soap_format,
            &flush_soap_custom_instructions,
            &logger_for_flush,
            &app_handle,
            &sync_ctx,
        )
        .await;
    }

    // ---- Orphaned billing recovery ----
    if let Some(ref client) = flush_llm_client {
        crate::encounter_pipeline::recover_orphaned_billing(
            client,
            &flush_fast_model,
            &logger_for_flush,
        )
        .await;
    }

    // Flush remaining buffer as final encounter check
    let (remaining_text, flush_encounter_start, flush_encounter_end, flush_segment_count) = {
        let buffer = handle
            .transcript_buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !buffer.is_empty() {
            (
                Some(buffer.full_text_with_speakers()),
                buffer.first_timestamp(),
                buffer.last_timestamp(),
                buffer.segment_count(),
            )
        } else {
            (None, None, None, 0)
        }
    };
    let mut flush_session_id_for_log: Option<String> = None;

    if let Some(text) = remaining_text {
        // Strip hallucinations before word count check and SOAP generation
        let (filtered_text, _) = strip_hallucinations(&text, 5);
        let word_count = filtered_text.split_whitespace().count();
        if let Ok(mut logger) = logger_for_flush.lock() {
            logger.log_hallucination_filter(serde_json::json!({
                "stage": "flush_on_stop",
                "original_words": text.split_whitespace().count(),
                "filtered_words": word_count,
            }));
        }
        if word_count > 100 {
            info!(
                event = "flush_final_encounter",
                component = "continuous_mode_flush_on_stop",
                word_count,
                "Flushing remaining buffer as final session"
            );
            let session_id = uuid::Uuid::new_v4().to_string();
            // Compute duration from first-to-last segment
            let flush_duration_ms_computed = match (flush_encounter_start, flush_encounter_end) {
                (Some(s), Some(e)) => (e - s).num_milliseconds().max(0) as u64,
                (Some(s), None) => (ctx.now_utc() - s).num_milliseconds().max(0) as u64,
                _ => 0,
            };
            if let Err(e) = local_archive::save_session(
                &session_id,
                &text, // Archive the raw text (preserve original for audit)
                flush_duration_ms_computed,
                None,
                false,
                Some("continuous_mode_stopped"),
                flush_encounter_start, // Actual encounter start time for started_at metadata
                Some(flush_segment_count),
            ) {
                warn!(
                    event = "flush_archive_failed",
                    component = "continuous_mode_flush_on_stop",
                    error = %e,
                    "Failed to archive final buffer"
                );
            } else {
                // Point logger to flush session's archive folder
                if let Ok(flush_session_dir) =
                    local_archive::get_session_archive_dir(&session_id, &ctx.now_utc())
                {
                    if let Ok(mut logger) = logger_for_flush.lock() {
                        logger.set_session(&flush_session_dir);
                    }
                    // Flush remaining screenshots
                    crate::screenshot_task::flush_screenshots_to_session(
                        &handle.screenshot_buffer,
                        &flush_session_dir,
                    );
                }

                // Track session ID for day log
                flush_session_id_for_log = Some(session_id.clone());

                // Cache today's sessions (used for encounter number + merge check)
                let flush_today_str = ctx.now_utc().format("%Y-%m-%d").to_string();
                let flush_today_sessions = local_archive::list_sessions_by_date(&flush_today_str).ok();

                // Update archive metadata with continuous mode info (match normal encounter path).
                //
                // Encounter number for a flushed session: previous session's encounter_number + 1.
                // Previously computed as `sessions.len()` which counts across continuous-mode
                // restarts (mid-day stops) and disagrees with the detector's per-run counter —
                // observed Apr 16 2026 where the evening run's flushed Grantham session was
                // labelled enc#10 even though the evening run only had 3 splits (Marchut #1 /
                // Nock #2 / Suglio #3), because the 6 morning sessions bled into the count.
                let flush_encounter_number = flush_today_sessions
                    .as_ref()
                    .and_then(|sessions| sessions.iter().rev().find(|s| s.session_id != session_id))
                    .and_then(|prev| prev.encounter_number.map(|n| n + 1))
                    .unwrap_or(1);
                if let Ok(session_dir) =
                    local_archive::get_session_archive_dir(&session_id, &ctx.now_utc())
                {
                    let meta_path = session_dir.join("metadata.json");
                    if meta_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&meta_path) {
                            if let Ok(mut metadata) =
                                serde_json::from_str::<local_archive::ArchiveMetadata>(&content)
                            {
                                metadata.charting_mode = Some("continuous".to_string());
                                metadata.encounter_number = Some(flush_encounter_number);
                                metadata.detection_method = Some("flush".to_string());
                                if let Ok(tracker) = handle.name_tracker.lock() {
                                    metadata.patient_name = tracker.majority_name();
                                    metadata.patient_dob = tracker.dob().map(|s| s.to_string());
                                } else {
                                    warn!(
                                        event = "flush_name_tracker_poisoned",
                                        component = "continuous_mode_flush_on_stop",
                                        "Name tracker lock poisoned during flush metadata enrichment"
                                    );
                                }
                                // Add physician/room context (multi-user)
                                sync_ctx.enrich_metadata(&mut metadata);
                                if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                                    let _ = std::fs::write(&meta_path, json);
                                }
                            }
                        }
                    }
                }

                // Server sync: upload flushed session (after metadata enrichment so server gets full metadata)
                {
                    let today = ctx.now_utc().format("%Y-%m-%d").to_string();
                    sync_ctx.sync_session(&session_id, &today);
                }

                // Clinical content check (shared with detector path)
                let is_clinical = if let Some(ref client) = flush_llm_client {
                    crate::encounter_pipeline::check_clinical_content(
                        client,
                        &flush_fast_model,
                        &text,
                        word_count,
                        &logger_for_flush,
                        serde_json::json!({
                            "stage": "flush_on_stop",
                            "word_count": word_count,
                        }),
                        Some(&flush_templates),
                        Some(min_words_for_clinical_check),
                    )
                    .await
                } else {
                    word_count >= min_words_for_clinical_check
                };

                if !is_clinical {
                    crate::encounter_pipeline::mark_non_clinical(&session_id);
                    // Re-sync so server gets the non-clinical flag
                    let today = ctx.now_utc().format("%Y-%m-%d").to_string();
                    sync_ctx.sync_session(&session_id, &today);
                }

                // ---- Flush merge check (runs BEFORE SOAP to avoid wasted generation) ----
                let mut flush_was_merged = false;
                if merge_enabled {
                    if let Some(ref client) = flush_llm_client {
                        if let Some(ref sessions) = flush_today_sessions {
                            // Pick the MOST RECENT session other than the one being flushed.
                            // `list_sessions_by_date` sorts ascending by started_at, so the
                            // plain `find` at the start of the vec returned the oldest —
                            // on a 2-run day this meant comparing the evening flush against
                            // a morning encounter 6 hours earlier (observed Apr 16 2026 for
                            // the Grantham flush at 19:59 which got compared against Pani
                            // from 13:24). Iterate in reverse to pick the temporal neighbor.
                            if let Some(prev_summary) =
                                sessions.iter().rev().find(|s| s.session_id != session_id)
                            {
                                if let Ok(prev_details) = local_archive::get_session(
                                    &prev_summary.session_id,
                                    &flush_today_str,
                                ) {
                                    if let Some(ref prev_transcript) = prev_details.transcript {
                                        let prev_tail =
                                            tail_words(prev_transcript, MERGE_EXCERPT_WORDS);
                                        let curr_head =
                                            head_words(&filtered_text, MERGE_EXCERPT_WORDS);
                                        let (filtered_prev_tail, _) =
                                            strip_hallucinations(&prev_tail, 5);
                                        let (filtered_curr_head, _) =
                                            strip_hallucinations(&curr_head, 5);

                                        let merge_outcome =
                                            crate::encounter_pipeline::run_merge_check(
                                                client,
                                                &flush_fast_model,
                                                &filtered_prev_tail,
                                                &filtered_curr_head,
                                                None, // No vision tracker at flush time
                                                &logger_for_flush,
                                                serde_json::json!({
                                                    "prev_session_id": prev_summary.session_id,
                                                    "curr_session_id": session_id,
                                                    "stage": "flush_on_stop",
                                                    "prev_tail_words": filtered_prev_tail.split_whitespace().count(),
                                                    "curr_head_words": filtered_curr_head.split_whitespace().count(),
                                                }),
                                                Some(&flush_templates),
                                            )
                                            .await;

                                        if merge_outcome.same_encounter == Some(true) {
                                            info!(
                                                event = "flush_merge_same",
                                                component = "continuous_mode_flush_on_stop",
                                                reason = ?merge_outcome.reason,
                                                curr_session_id = %session_id,
                                                prev_session_id = %prev_summary.session_id,
                                                "Flush merge check: same visit, merging"
                                            );
                                            let merged_text =
                                                format!("{}\n{}", prev_transcript, text);
                                            let merged_wc =
                                                merged_text.split_whitespace().count();
                                            let now = ctx.now_utc();
                                            if let Err(e) = local_archive::merge_encounters(
                                                &prev_summary.session_id,
                                                &session_id,
                                                &now,
                                                &merged_text,
                                                merged_wc,
                                                0, // Unknown duration for flush
                                                None, // No vision tracker at flush time
                                            ) {
                                                warn!(
                                                    event = "flush_merge_failed",
                                                    component = "continuous_mode_flush_on_stop",
                                                    error = %e,
                                                    "Failed to merge flushed encounter"
                                                );
                                            } else {
                                                // Sync merge to server
                                                {
                                                    let today =
                                                        ctx.now_utc().format("%Y-%m-%d").to_string();
                                                    sync_ctx.sync_merge(
                                                        &session_id,
                                                        &prev_summary.session_id,
                                                        &today,
                                                    );
                                                }
                                                flush_was_merged = true;
                                                // Regenerate SOAP for the merged encounter
                                                let prev_is_clinical =
                                                    prev_summary.likely_non_clinical != Some(true);
                                                let flush_merge_notes = handle
                                                    .encounter_notes
                                                    .lock()
                                                    .map(|n| n.clone())
                                                    .unwrap_or_default();
                                                crate::encounter_pipeline::regen_soap_after_merge(
                                                    client,
                                                    &merged_text,
                                                    &prev_summary.session_id,
                                                    &now,
                                                    prev_summary.patient_name.as_deref(),
                                                    &flush_soap_model,
                                                    flush_soap_detail_level,
                                                    &flush_soap_format,
                                                    &flush_soap_custom_instructions,
                                                    flush_merge_notes,
                                                    prev_is_clinical,
                                                    is_clinical,
                                                    &logger_for_flush,
                                                    &sync_ctx,
                                                    "flush_merge_soap_regen",
                                                    Some(&flush_fast_model),
                                                    0,
                                                    billing_counselling_exhausted,
                                                    Some(&flush_templates),
                                                    Some(&flush_billing_data),
                                                )
                                                .await;
                                                ContinuousModeEvent::EncounterMerged {
                                                    kept_session_id: None,
                                                    merged_into_session_id: Some(
                                                        prev_summary.session_id.clone(),
                                                    ),
                                                    removed_session_id: session_id.clone(),
                                                    reason: None,
                                                }
                                                .emit_via(ctx);
                                            }
                                        } else if let Some(false) = merge_outcome.same_encounter {
                                            info!(
                                                event = "flush_merge_separate",
                                                component = "continuous_mode_flush_on_stop",
                                                reason = ?merge_outcome.reason,
                                                "Flush merge check: different encounters"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Generate SOAP note (only if clinical AND not already merged)
                if !is_clinical {
                    info!(
                        event = "flush_soap_skipped_nonclinical",
                        component = "continuous_mode_flush_on_stop",
                        "Skipping SOAP for non-clinical flush encounter"
                    );
                } else if flush_was_merged {
                    info!(
                        event = "flush_soap_skipped_merged",
                        component = "continuous_mode_flush_on_stop",
                        "Skipping SOAP for flush encounter — already merged into previous session"
                    );
                } else if let Some(ref client) = flush_llm_client {
                    let flush_notes = handle
                        .encounter_notes
                        .lock()
                        .map(|n| n.clone())
                        .unwrap_or_default();
                    info!(
                        event = "flush_soap_generating",
                        component = "continuous_mode_flush_on_stop",
                        word_count,
                        "Generating SOAP for flushed buffer"
                    );
                    let outcome = crate::encounter_pipeline::generate_and_archive_soap(
                        client,
                        &flush_soap_model,
                        &filtered_text,
                        &session_id,
                        &ctx.now_utc(),
                        flush_soap_detail_level,
                        &flush_soap_format,
                        &flush_soap_custom_instructions,
                        flush_notes,
                        word_count,
                        None,
                        &logger_for_flush,
                        serde_json::json!({"stage": "flush_on_stop", "word_count": word_count}),
                        Some(&flush_templates),
                        Some(soap_generation_timeout_secs),
                    )
                    .await;
                    if let crate::encounter_pipeline::SoapGenerationOutcome::Success {
                        ref content, ..
                    } = outcome
                    {
                        sync_ctx.sync_soap(
                            &session_id,
                            content,
                            flush_soap_detail_level,
                            &flush_soap_format,
                        );
                        info!(
                            event = "flush_soap_generated",
                            component = "continuous_mode_flush_on_stop",
                            session_id = %session_id,
                            "SOAP generated for flushed buffer"
                        );
                        ContinuousModeEvent::SoapGenerated {
                            session_id: session_id.clone(),
                            patient_count: None,
                            recovered: None,
                        }
                        .emit_via(ctx);

                        // Billing extraction (fail-open)
                        {
                            let flush_duration_ms = flush_duration_ms_computed;
                            let flush_now = ctx.now_utc();
                            let flush_after_hours =
                                crate::encounter_pipeline::is_after_hours(&flush_now);
                            let flush_patient_name = handle
                                .name_tracker
                                .lock()
                                .ok()
                                .and_then(|t| t.majority_name());
                            let billing_start = std::time::Instant::now();
                            let billing_result =
                                crate::encounter_pipeline::extract_and_archive_billing(
                                    &client,
                                    &flush_fast_model,
                                    content,
                                    &filtered_text,
                                    "", // no physician-provided context in auto-extraction
                                    &session_id,
                                    &flush_now,
                                    flush_duration_ms,
                                    flush_patient_name.as_deref(),
                                    flush_after_hours,
                                    &crate::billing::RuleEngineContext {
                                        counselling_exhausted: billing_counselling_exhausted,
                                        ..Default::default()
                                    },
                                    &logger_for_flush,
                                    Some(&flush_templates),
                                    Some(&flush_billing_data),
                                    Some(billing_extraction_timeout_secs),
                                )
                                .await;
                            let billing_latency = billing_start.elapsed().as_millis() as u64;

                            match &billing_result {
                                Ok(record) => {
                                    if let Some(ref dl) = *day_logger_for_flush {
                                        dl.log(crate::day_log::DayEvent::BillingExtracted {
                                            ts: ctx.now_utc().to_rfc3339(),
                                            session_id: session_id.clone(),
                                            codes_count: record.codes.len() as u32,
                                            total_amount_cents: record.total_amount_cents,
                                            latency_ms: billing_latency,
                                            success: true,
                                        });
                                    }
                                    // Re-upload so server's has_billing_record catches up.
                                    let _ = record;
                                    let today = ctx.now_utc().format("%Y-%m-%d").to_string();
                                    sync_ctx.resync_session(&session_id, &today);
                                }
                                Err(e) => {
                                    warn!(
                                        event = "flush_billing_failed",
                                        component = "continuous_mode_flush_on_stop",
                                        error = %e,
                                        "Billing extraction failed for flush encounter"
                                    );
                                    if let Some(ref dl) = *day_logger_for_flush {
                                        dl.log(crate::day_log::DayEvent::BillingExtracted {
                                            ts: ctx.now_utc().to_rfc3339(),
                                            session_id: session_id.clone(),
                                            codes_count: 0,
                                            total_amount_cents: 0,
                                            latency_ms: billing_latency,
                                            success: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Log continuous mode stopped event
    if let Some(ref dl) = *day_logger_for_flush {
        dl.log(crate::day_log::DayEvent::ContinuousModeStopped {
            ts: ctx.now_utc().to_rfc3339(),
            total_encounters: handle.encounters_detected.load(Ordering::Relaxed),
            flush_session_id: flush_session_id_for_log,
        });
    }

    // Write the day's performance_summary.json after the stop event so it
    // reflects all events up to and including this run. On a multi-run day,
    // each stop overwrites the file with the cumulative aggregate. Cost is a
    // single pass over today's pipeline_log files (~sub-second on a normal
    // clinic day) and it runs only at stop, not on the hot path.
    crate::performance_summary::write_today_summary();

    // Set state to idle
    if let Ok(mut state) = handle.state.lock() {
        *state = ContinuousState::Idle;
    } else {
        warn!(
            event = "flush_state_lock_poisoned",
            component = "continuous_mode_flush_on_stop",
            "State lock poisoned while setting idle state on shutdown"
        );
    }

    ContinuousModeEvent::Stopped.emit_via(ctx);

    info!(
        event = "flush_complete",
        component = "continuous_mode_flush_on_stop",
        "Continuous mode stopped"
    );

    Ok(())
}
