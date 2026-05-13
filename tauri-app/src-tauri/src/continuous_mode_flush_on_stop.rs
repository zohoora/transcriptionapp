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
use crate::replay_bundle::ReplayBundleBuilder;
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
    /// Vision-capable model alias used for multimodal multi-patient detect +
    /// SOAP calls (see `PostSplitDeps::vision_model`).
    pub vision_model: String,
    pub soap_detail_level: u8,
    pub soap_format: String,
    pub soap_custom_instructions: String,
    pub logger: Arc<std::sync::Mutex<PipelineLogger>>,
    pub day_logger: Arc<Option<DayLogger>>,
    pub templates: Arc<PromptTemplates>,
    pub billing_data: Arc<BillingData>,
    /// Shared replay-bundle builder. Populated during the run; finalized here
    /// if the flush produces an encounter session (mirrors the splitter path).
    pub bundle: Arc<std::sync::Mutex<ReplayBundleBuilder>>,
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
        vision_model: flush_vision_model,
        soap_detail_level: flush_soap_detail_level,
        soap_format: flush_soap_format,
        soap_custom_instructions: flush_soap_custom_instructions,
        logger: logger_for_flush,
        day_logger: day_logger_for_flush,
        templates: flush_templates,
        billing_data: flush_billing_data,
        bundle: bundle_for_flush,
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
            &flush_vision_model,
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

                // Drain clinician notes from the handle atomically. We don't
                // know yet whether this buffer will merge back — the decision
                // comes from the flush merge check below. Keep the Vec in
                // scope and persist to the correct target once merge status
                // is known (flush session on no-merge, prev session on merge).
                let flush_drained_notes = handle.drain_encounter_notes();
                let flush_has_clinician_notes = !flush_drained_notes.is_empty();
                let flush_notes_text =
                    crate::local_archive::join_notes_for_prompt(&flush_drained_notes);

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
                                if flush_has_clinician_notes {
                                    metadata.has_clinician_notes = true;
                                }
                                // patient_name + patient_dob are populated
                                // by the SOAP path that runs immediately
                                // below (see `apply_soap_extracted_identity`
                                // inside `generate_and_archive_soap`). Leave
                                // both None here.
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

                                        // Prefer prev SOAP; fall back to the transcript tail
                                        // when prev has no usable SOAP.
                                        let prev_soap_ok = prev_details
                                            .soap_note
                                            .as_deref()
                                            .filter(|s| crate::llm_client::is_usable_soap(s));
                                        let prev_input = match prev_soap_ok {
                                            Some(s) => crate::encounter_merge::PrevMergeInput::SoapNote(s),
                                            None => crate::encounter_merge::PrevMergeInput::TranscriptTail(
                                                &filtered_prev_tail,
                                            ),
                                        };

                                        let merge_outcome =
                                            crate::encounter_pipeline::run_merge_check(
                                                client,
                                                &flush_fast_model,
                                                prev_input,
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
                                                // Migrate the flushed notes into the SURVIVING
                                                // session's clinician_notes.json so they live on
                                                // after this flush dir is cleaned up by
                                                // merge_encounters, and capture the combined
                                                // prev+flush list for the SOAP regen prompt.
                                                let flush_merge_notes = if flush_has_clinician_notes {
                                                    match crate::local_archive::append_clinician_notes_vec(
                                                        &prev_summary.session_id,
                                                        &now,
                                                        &flush_drained_notes,
                                                    ) {
                                                        Ok(Some(ref combined)) => {
                                                            crate::local_archive::join_notes_for_prompt(combined)
                                                        }
                                                        Ok(None) => flush_notes_text.clone(),
                                                        Err(e) => {
                                                            warn!(
                                                                event = "flush_merge_notes_persist_failed",
                                                                component = "continuous_mode_flush_on_stop",
                                                                prev_session_id = %prev_summary.session_id,
                                                                error = %e,
                                                                "Failed to append flushed clinician notes to surviving session"
                                                            );
                                                            flush_notes_text.clone()
                                                        }
                                                    }
                                                } else {
                                                    // No flushed notes to migrate — use prev's
                                                    // existing sidecar (if any) for the regen
                                                    // prompt so prev's original observations
                                                    // aren't dropped on the re-SOAP.
                                                    match crate::local_archive::read_clinician_notes(
                                                        &prev_summary.session_id,
                                                        &now,
                                                    ) {
                                                        Ok(Some(ref list)) if !list.is_empty() => {
                                                            crate::local_archive::join_notes_for_prompt(list)
                                                        }
                                                        _ => String::new(),
                                                    }
                                                };
                                                let prev_is_clinical =
                                                    prev_summary.likely_non_clinical != Some(true);
                                                crate::encounter_pipeline::regen_soap_after_merge(
                                                    client,
                                                    &merged_text,
                                                    &prev_summary.session_id,
                                                    &now,
                                                    prev_summary.patient_name.as_deref(),
                                                    &flush_soap_model,
                                                    &flush_vision_model,
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
                    // Non-merged flush: persist notes into this flushed
                    // session's own clinician_notes.json before SOAP runs,
                    // so the notes survive even if SOAP generation errors.
                    if flush_has_clinician_notes {
                        if let Err(e) = crate::local_archive::write_clinician_notes(
                            &session_id,
                            &ctx.now_utc(),
                            &flush_drained_notes,
                        ) {
                            warn!(
                                event = "flush_notes_persist_failed",
                                component = "continuous_mode_flush_on_stop",
                                session_id = %session_id,
                                error = %e,
                                "Failed to persist clinician_notes.json for flushed session"
                            );
                        }
                    }
                    let flush_notes = flush_notes_text.clone();

                    // One dedup, attached to both the multi-patient detect and
                    // SOAP calls below — consistent image set across both hops.
                    let flush_now = ctx.now_utc();
                    let flush_deduped_screenshots =
                        crate::screenshot_dedup::load_deduped_screenshots_for_session(
                            &session_id,
                            &flush_now,
                        );
                    let flush_screenshot_arg = Some(flush_deduped_screenshots.as_slice());

                    // Detect combined-patient sessions that never split before
                    // continuous-mode stopped (e.g. a long appointment that
                    // overlapped two patients in the same room). Without this
                    // the SOAP-LLM silently merges them into one note.
                    let multi_patient_detection = if word_count
                        >= crate::encounter_detection::MULTI_PATIENT_DETECT_WORD_THRESHOLD
                    {
                        info!(
                            event = "flush_multi_patient_detect",
                            component = "continuous_mode_flush_on_stop",
                            word_count,
                            screenshots_attached = flush_deduped_screenshots.len(),
                            "Running multi-patient detection on flush buffer"
                        );
                        crate::encounter_pipeline::run_pre_soap_multi_patient_detection(
                            client,
                            &flush_fast_model,
                            &filtered_text,
                            word_count,
                            "flush_on_stop",
                            &logger_for_flush,
                            &bundle_for_flush,
                            flush_screenshot_arg,
                            &flush_vision_model,
                        )
                        .await
                    } else {
                        None
                    };

                    info!(
                        event = "flush_soap_generating",
                        component = "continuous_mode_flush_on_stop",
                        word_count,
                        multi_patient = multi_patient_detection.is_some(),
                        screenshots_attached = flush_deduped_screenshots.len(),
                        "Generating SOAP for flushed buffer"
                    );
                    let outcome = crate::encounter_pipeline::generate_and_archive_soap(
                        client,
                        &flush_soap_model,
                        &filtered_text,
                        &session_id,
                        &flush_now,
                        flush_soap_detail_level,
                        &flush_soap_format,
                        &flush_soap_custom_instructions,
                        flush_notes,
                        word_count,
                        multi_patient_detection.as_ref(),
                        &logger_for_flush,
                        serde_json::json!({"stage": "flush_on_stop", "word_count": word_count}),
                        Some(&flush_templates),
                        Some(soap_generation_timeout_secs),
                        Some(&bundle_for_flush),
                        flush_screenshot_arg,
                        &flush_vision_model,
                    )
                    .await;
                    if let crate::encounter_pipeline::SoapGenerationOutcome::Success {
                        ref content, ref result, ..
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
                            // Patient name from the SOAP call we just ran
                            // (same extractor that wrote `metadata.patient_name`).
                            let flush_patient_name = result.notes.first()
                                .and_then(|n| n.extracted_patient_name.clone());
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
                                        transcript: Some(filtered_text.clone()),
                                        ..Default::default()
                                    },
                                    &logger_for_flush,
                                    Some(&flush_templates),
                                    Some(&flush_billing_data),
                                    Some(billing_extraction_timeout_secs),
                                    Some(&bundle_for_flush),
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

                // Finalize replay bundle for the flushed encounter. Mirrors
                // the splitter path (continuous_mode.rs:1779-1803) so this
                // session gets the same offline-replay coverage — fixes the
                // gap seen on 2026-04-21 where Spencer's flush-on-stop
                // session had no replay_bundle.json. If the flush was
                // merged into a previous session the surviving dir is gone,
                // so we clear without writing (matches the merge path's
                // fail-safe fallback).
                if let Ok(mut bundle) = bundle_for_flush.lock() {
                    bundle.set_split_decision(crate::replay_bundle::SplitDecision {
                        ts: ctx.now_utc().to_rfc3339(),
                        trigger: "flush".to_string(),
                        word_count,
                        cleaned_word_count: word_count,
                        end_segment_index: None,
                    });
                    bundle.set_outcome(crate::replay_bundle::Outcome {
                        session_id: session_id.clone(),
                        encounter_number: flush_encounter_number,
                        word_count,
                        is_clinical,
                        was_merged: flush_was_merged,
                        merged_into: None,
                        // Patient name is written into `metadata.patient_name`
                        // by the SOAP path. The replay bundle's outcome stays
                        // identity-free now that vision votes are gone.
                        patient_name: None,
                        detection_method: Some("flush".to_string()),
                    });
                    if flush_was_merged {
                        bundle.clear();
                    } else if let Ok(dir) =
                        local_archive::get_session_archive_dir(&session_id, &ctx.now_utc())
                    {
                        bundle.build_and_reset(&dir);
                    } else {
                        bundle.clear();
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

    // Negative-gap pair scan. Walks today's session
    // summaries looking for false-split signatures (next session started
    // <30s after prev session's recorded end AND tail <2500 words). Emits
    // an event so the frontend can show a one-click merge banner.
    let scan_date = ctx.now_local().format("%Y-%m-%d").to_string();
    if let Ok(today_sessions) = crate::local_archive::list_sessions_by_date(&scan_date) {
        let pairs = crate::local_archive::find_negative_gap_pairs(&today_sessions);
        if !pairs.is_empty() {
            info!(
                event = "negative_gap_pairs_found",
                component = "continuous_mode_flush_on_stop",
                date = %scan_date,
                pair_count = pairs.len(),
                "Negative-gap pair scan found false-split candidates"
            );
            ContinuousModeEvent::NegativeGapPairsFound {
                date: scan_date,
                pair_count: pairs.len(),
            }
            .emit_via(ctx);
        }
    }

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

#[cfg(test)]
mod multi_patient_threshold_tests {
    use crate::encounter_detection::MULTI_PATIENT_DETECT_WORD_THRESHOLD;

    /// Validates that the flush-on-stop multi-patient detection threshold
    /// matches the post-split path. Both paths must use the same gate so
    /// that a session ending via flush gets the same multi-patient scrutiny
    /// as a session ending via normal split. Regression guard for the
    /// silent-contamination cases (a long appointment that overlapped two
    /// patients in the same room produces a single-patient SOAP unless the
    /// flush path runs the same multi-patient gate as post_split).
    #[test]
    fn flush_on_stop_uses_same_multi_patient_threshold_as_post_split() {
        // Both paths read MULTI_PATIENT_DETECT_WORD_THRESHOLD directly.
        // Marlene's session was 3786 words, Ann's was 6430 — both far
        // above the 500-word threshold. If the threshold ever drifts above
        // a normal 30-min single-patient encounter (~3000 words at
        // ~100wpm), this constant assertion will start hiding real
        // multi-patient sessions.
        assert!(
            MULTI_PATIENT_DETECT_WORD_THRESHOLD <= 1500,
            "threshold {} words is too high — risks missing short multi-patient sessions",
            MULTI_PATIENT_DETECT_WORD_THRESHOLD
        );
        assert!(
            MULTI_PATIENT_DETECT_WORD_THRESHOLD > 0,
            "threshold must be positive"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::continuous_mode::ContinuousState;
    use crate::encounter_detection::MIN_WORDS_FOR_CLINICAL_CHECK;
    use crate::harness::test_env::{
        seed_transcript_buffer as seed_buffer, test_ctx_with_archive as make_ctx, ArchiveDirGuard,
    };
    use crate::local_archive::ArchiveMetadata;
    use crate::pipeline::PipelineHandle;
    use crate::replay_bundle::ReplayBundleBuilder;
    use chrono::{DateTime, Utc};
    use serial_test::serial;
    use std::path::{Path, PathBuf};

    fn make_deps(handle: Arc<ContinuousModeHandle>) -> FlushOnStopDeps {
        FlushOnStopDeps {
            handle,
            sync_ctx: ServerSyncContext::empty(),
            llm_client: None,
            soap_model: "soap-model-fast".to_string(),
            fast_model: "fast-model".to_string(),
            vision_model: "soap-model".to_string(),
            soap_detail_level: 5,
            soap_format: "comprehensive".to_string(),
            soap_custom_instructions: String::new(),
            logger: Arc::new(std::sync::Mutex::new(PipelineLogger::new())),
            day_logger: Arc::new(None),
            templates: Arc::new(PromptTemplates::default()),
            billing_data: Arc::new(BillingData::default()),
            bundle: Arc::new(std::sync::Mutex::new(ReplayBundleBuilder::new(
                serde_json::json!({}),
            ))),
            min_words_for_clinical_check: MIN_WORDS_FOR_CLINICAL_CHECK,
            merge_enabled: false,
            soap_generation_timeout_secs: 300,
            billing_extraction_timeout_secs: 60,
            billing_counselling_exhausted: false,
        }
    }

    fn make_handles() -> FlushOnStopHandles {
        FlushOnStopHandles {
            pipeline_handle: PipelineHandle::for_testing(),
            sensor_handle: None,
            consumer_task: tokio::spawn(async {}),
            detector_task: tokio::spawn(async {}),
            screenshot_task: None,
            shadow_task: None,
            sensor_monitor_task: None,
        }
    }

    /// Walk archive_root/YYYY/MM/DD/ and return any subdirectories (each one
    /// is a session UUID dir).
    fn list_session_dirs_today(archive_root: &Path) -> Vec<PathBuf> {
        let today_dir = archive_root.join(Utc::now().format("%Y/%m/%d").to_string());
        let Ok(read) = std::fs::read_dir(&today_dir) else {
            return Vec::new();
        };
        read.filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect()
    }

    fn last_event_type(
        events: &[crate::harness::captured_event::CapturedEvent],
    ) -> Option<String> {
        events.last().and_then(|e| {
            if e.event_name != "continuous_mode_event" {
                return None;
            }
            e.payload
                .get("type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
    }

    #[tokio::test]
    #[serial]
    async fn empty_buffer_emits_stopped_and_sets_idle() {
        let guard = ArchiveDirGuard::new();
        let ctx = make_ctx(guard.path());
        let handle = Arc::new(ContinuousModeHandle::new());

        run(&ctx, make_deps(Arc::clone(&handle)), make_handles())
            .await
            .expect("flush should succeed");

        let events = ctx.captured_events();
        assert_eq!(
            last_event_type(&events).as_deref(),
            Some("stopped"),
            "Stopped should be the final event"
        );

        let state = handle.state.lock().unwrap();
        assert!(matches!(*state, ContinuousState::Idle));

        // No buffer content → no archive activity at all.
        let session_dirs = list_session_dirs_today(guard.path());
        assert!(
            session_dirs.is_empty(),
            "empty buffer should not create flush session, found: {:?}",
            session_dirs
        );
    }

    #[tokio::test]
    #[serial]
    async fn buffer_under_100_words_skips_flush_session() {
        let guard = ArchiveDirGuard::new();
        let ctx = make_ctx(guard.path());
        let handle = Arc::new(ContinuousModeHandle::new());
        // 5 segments × 10 words = 50 words: above zero (so the buffer block
        // is entered), below the 100-word gate (so no session is created).
        seed_buffer(&handle, 10, 5);

        run(&ctx, make_deps(Arc::clone(&handle)), make_handles())
            .await
            .expect("flush");

        assert!(
            list_session_dirs_today(guard.path()).is_empty(),
            "buffer below the 100-word gate must not create a flush session"
        );
        assert_eq!(
            last_event_type(&ctx.captured_events()).as_deref(),
            Some("stopped")
        );
    }

    /// Helper: write a complete prior session to today's archive — transcript +
    /// metadata.json with `encounter_number` and `started_at` set. The ts arg
    /// drives `list_sessions_by_date`'s sort order (ascending by started_at),
    /// so the latest ts = the session the flush path picks via `.iter().rev()`.
    fn write_seed_session(session_id: &str, started_at_iso: &str, encounter_number: u32) {
        let date = DateTime::parse_from_rfc3339(started_at_iso)
            .expect("valid RFC3339 ts")
            .with_timezone(&Utc);
        crate::local_archive::save_session(
            session_id,
            "seed transcript text",
            60_000,
            None,
            false,
            None,
            Some(date),
            Some(1),
        )
        .expect("save seed session");
        let dir = crate::local_archive::get_session_archive_dir(session_id, &date)
            .expect("session dir");
        let raw = std::fs::read_to_string(dir.join("metadata.json")).expect("read meta");
        let mut meta: ArchiveMetadata = serde_json::from_str(&raw).expect("parse meta");
        meta.encounter_number = Some(encounter_number);
        meta.charting_mode = Some("continuous".to_string());
        let pretty = serde_json::to_string_pretty(&meta).unwrap();
        std::fs::write(dir.join("metadata.json"), pretty).expect("write meta");
    }

    #[tokio::test]
    #[serial]
    async fn flush_encounter_number_uses_latest_prev_plus_one() {
        // Regression guard for the Apr 16 2026 Grantham bug. The flush path
        // must compute encounter_number from the LATEST prev session's
        // encounter_number + 1 (not `sessions.len()`), so that mid-day
        // continuous-mode restarts don't bleed morning-run counts into the
        // evening run's flush.
        //
        // Seeded shape mirrors that day: a 3-encounter morning run (mid-day
        // stop+restart) followed by a 2-encounter evening run, then a flush.
        // Latest started_at is evening enc#2 → expected flush enc# = 3.
        // `sessions.len()`-based math would yield 5+1=6 (or 5), which would
        // fail the assert below.

        let guard = ArchiveDirGuard::new();
        let ctx = make_ctx(guard.path());
        let handle = Arc::new(ContinuousModeHandle::new());

        let today = Utc::now().format("%Y-%m-%d").to_string();
        // Morning continuous-mode run (3 encounters, ascending enc#)
        write_seed_session("morn-1", &format!("{today}T09:00:00Z"), 1);
        write_seed_session("morn-2", &format!("{today}T10:00:00Z"), 2);
        write_seed_session("morn-3", &format!("{today}T11:00:00Z"), 3);
        // App stopped mid-day, restarted in the afternoon → enc# resets
        write_seed_session("eve-1", &format!("{today}T17:00:00Z"), 1);
        write_seed_session("eve-2", &format!("{today}T18:00:00Z"), 2);
        // Latest started_at is `eve-2` with enc#2 → flush should be enc#3.
        let expected_flush_encounter_number: u32 = 3;

        seed_buffer(&handle, 30, 4); // 120 words → above the 100w gate

        run(&ctx, make_deps(Arc::clone(&handle)), make_handles())
            .await
            .expect("flush");

        let session_dirs = list_session_dirs_today(guard.path());
        // Among today_dir contents we have N seed sessions + 1 flush session.
        // Pick the one with detection_method="flush" (the others were seeded
        // without that field).
        let flush_dir = session_dirs
            .iter()
            .find(|p| {
                std::fs::read_to_string(p.join("metadata.json"))
                    .ok()
                    .and_then(|raw| serde_json::from_str::<ArchiveMetadata>(&raw).ok())
                    .and_then(|m| m.detection_method)
                    == Some("flush".to_string())
            })
            .expect("a flush-detected session should be present");

        let flush_meta: ArchiveMetadata = serde_json::from_str(
            &std::fs::read_to_string(flush_dir.join("metadata.json")).unwrap(),
        )
        .unwrap();

        assert_eq!(
            flush_meta.encounter_number,
            Some(expected_flush_encounter_number),
            "flush session must use prev.encounter_number + 1 (Apr 16 Grantham regression)"
        );
    }

    #[tokio::test]
    #[serial]
    async fn buffer_over_100_words_with_no_llm_archives_flush_session() {
        let guard = ArchiveDirGuard::new();
        let ctx = make_ctx(guard.path());
        let handle = Arc::new(ContinuousModeHandle::new());
        // 4 segments × 30 words = 120 words: above the 100-word gate, below
        // the 500-word multi-patient gate (which would call into LLM and
        // None-LLM blocks anyway). With llm_client=None, SOAP/billing/merge
        // all skip; the function still archives the transcript and writes
        // the metadata enrichment.
        seed_buffer(&handle, 30, 4);

        run(&ctx, make_deps(Arc::clone(&handle)), make_handles())
            .await
            .expect("flush");

        let session_dirs = list_session_dirs_today(guard.path());
        assert_eq!(
            session_dirs.len(),
            1,
            "expected exactly one flush session dir, got {:?}",
            session_dirs
        );
        let session_dir = &session_dirs[0];

        let metadata: ArchiveMetadata = serde_json::from_str(
            &std::fs::read_to_string(session_dir.join("metadata.json"))
                .expect("metadata.json"),
        )
        .expect("metadata parses");

        assert_eq!(metadata.charting_mode.as_deref(), Some("continuous"));
        assert_eq!(metadata.detection_method.as_deref(), Some("flush"));
        // No prior sessions seeded → encounter_number defaults to 1.
        assert_eq!(metadata.encounter_number, Some(1));

        // No SOAP generated when llm_client is None.
        assert!(!session_dir.join("soap_note.txt").exists());

        // Replay bundle finalized for the flush session.
        assert!(
            session_dir.join("replay_bundle.json").exists(),
            "replay_bundle.json should be finalized for flush sessions \
             (regression guard for the Apr 21 Spencer missing-bundle bug)"
        );
    }
}
