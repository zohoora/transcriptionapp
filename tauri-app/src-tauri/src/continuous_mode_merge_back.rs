//! Post-split safety nets: merge-back coordinator.
//!
//! Runs after the detector archives encounter N + SOAP, before the detector
//! loop moves on to encounter N+1. Consolidates four post-split safety nets
//! that used to live inline inside the detector task:
//!
//! 1. **Small-orphan auto-merge** — if the new encounter is short (<500 words)
//!    and the sensor confirms someone was present, auto-merge into the
//!    previous encounter without asking the LLM (this is almost certainly a
//!    post-procedure tail that was falsely split).
//! 2. **LLM merge check** — asks `fast-model` whether the new encounter is
//!    the same visit as the previous one; if yes, merge + regenerate SOAP.
//! 3. **Retrospective multi-patient split (post-merge)** — after an
//!    LLM-confirmed merge, if the merged transcript is large (>=2500 words),
//!    run multi-patient detection against it and re-split into per-patient
//!    SOAPs if the LLM finds a genuine two-patient encounter that had been
//!    falsely merged.
//! 4. **Standalone multi-patient check** — independent of merge, if the
//!    current encounter is clinical + large + the pre-SOAP multi-patient
//!    detection didn't already fire, run a second-pass check to catch
//!    missed multi-patient encounters.
//!
//! LOGGER SESSION CONTRACT:
//! - On entry, the pipeline logger is pointed at the NEW encounter's session
//!   directory (set by the detector at split time).
//! - On the LLM-confirmed merge path, `regen_soap_after_merge` re-points the
//!   logger at the surviving (prev) session's directory. The retrospective
//!   multi-patient block then explicitly calls `logger.set_session(&prev_dir)`
//!   again (defensive — regen already did it, but a future refactor mustn't
//!   drift). On exit the logger is at the surviving session's dir.
//! - On the non-merge and Skipped paths the logger stays on the new
//!   encounter's dir.
//! - Either way, the next detector iteration calls `logger.set_session(...)`
//!   on the newly archived encounter (continuous_mode.rs line ~1743), so
//!   the caller does not need to restore logger state.
//!
//! LOOP-STATE MUTATIONS: on merge, `loop_state.merge_back_count` is
//! incremented and `loop_state.encounter_number` is decremented (via
//! `saturating_sub` to be defensive against underflow). On the non-merge
//! / skipped paths, loop_state is untouched — the caller's own "split
//! stuck" block (line ~2697 of continuous_mode.rs) handles the reset.
//!
//! COMPONENT: `continuous_mode_merge_back`.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tracing::{info, warn};

use crate::continuous_mode::{
    finalize_merged_bundle, head_words, multi_patient_from_outcome, tail_words,
    ContinuousModeHandle, MERGE_EXCERPT_WORDS,
};
use crate::continuous_mode_events::ContinuousModeEvent;
use crate::continuous_mode_types::LoopState;
use crate::day_log::DayLogger;
use crate::encounter_experiment::strip_hallucinations;
use crate::llm_client::LLMClient;
use crate::local_archive;
use crate::pipeline_log::PipelineLogger;
use crate::replay_bundle::ReplayBundleBuilder;
use crate::run_context::RunContext;
use crate::segment_log::SegmentLogger;
use crate::server_config::{BillingData, PromptTemplates};
use crate::server_sync::ServerSyncContext;

/// Upper bound on prev-SOAP text fed to the merge-check LLM. Multi-patient
/// SOAPs can grow beyond this; truncating keeps the merge-prompt context
/// bounded while still carrying S/O/A/P structure for the first patient(s).
const PREV_SOAP_MERGE_CHAR_CAP: usize = 12_000;

/// Prepare a prev session's SOAP for use as the "previous-encounter" side of
/// the merge-check prompt. Returns `None` when the SOAP is missing, empty, or
/// a malformed-output placeholder — the caller then falls back to the tail path.
fn prepare_prev_soap_for_merge(raw: Option<&str>) -> Option<String> {
    let soap = raw?;
    if !crate::llm_client::is_usable_soap(soap) {
        return None;
    }
    let trimmed = soap.trim();
    if trimmed.len() <= PREV_SOAP_MERGE_CHAR_CAP {
        return Some(trimmed.to_string());
    }
    let cut = trimmed.ceil_char_boundary(PREV_SOAP_MERGE_CHAR_CAP);
    Some(format!("{}\n…[truncated]", &trimmed[..cut]))
}

/// Long-lived dependency bundle. Built once per continuous-mode run before
/// the detector loop starts and borrowed into each merge-back call.
pub struct MergeBackDeps {
    pub handle: Arc<ContinuousModeHandle>,
    pub logger: Arc<std::sync::Mutex<PipelineLogger>>,
    pub bundle: Arc<std::sync::Mutex<ReplayBundleBuilder>>,
    pub segment_logger: Arc<std::sync::Mutex<SegmentLogger>>,
    pub day_logger: Arc<Option<DayLogger>>,
    pub sync_ctx: ServerSyncContext,
    pub llm_client: Option<LLMClient>,
    pub templates: Arc<PromptTemplates>,
    pub billing_data: Arc<BillingData>,
    pub fast_model: String,
    pub soap_model: String,
    pub soap_detail_level: u8,
    pub soap_format: String,
    pub soap_custom_instructions: String,
    pub multi_patient_check_word_threshold: usize,
    pub multi_patient_detect_word_threshold: usize,
    pub soap_generation_timeout_secs: u64,
    pub billing_counselling_exhausted: bool,
    pub merge_enabled: bool,
}

/// Per-encounter inputs to the merge-back pipeline.
pub struct MergeBackCall<'a> {
    pub session_id: &'a str,
    pub encounter_text: &'a str,
    pub encounter_text_rich: &'a str,
    pub encounter_word_count: usize,
    pub encounter_start: Option<DateTime<Utc>>,
    pub is_clinical: bool,
    pub pre_soap_found_multi_patient: bool,
    pub notes_text: &'a str,
    pub tracker_snapshot: (Option<String>, usize, Vec<String>),
    pub prev_encounter_session_id: Option<&'a str>,
    pub prev_encounter_text: Option<&'a str>,
    pub prev_encounter_text_rich: Option<&'a str>,
    pub prev_encounter_date: Option<&'a DateTime<Utc>>,
    pub prev_encounter_is_clinical: bool,
    pub prev_encounter_patient_name: Option<&'a str>,
    pub sensor_available: bool,
    /// True iff `prev_sensor_state == PresenceState::Present`.
    pub sensor_present_now: bool,
}

/// What happened during the merge-back pipeline. Drives caller updates to
/// its `prev_encounter_*` tracking variables.
pub enum MergeBackOutcome {
    /// Current encounter was merged back into the previous encounter.
    /// Caller should update its prev_* text/text_rich/is_clinical to the
    /// merged values and `continue` (skipping the finalize-and-update
    /// block that runs for non-merged encounters).
    ///
    /// `prev_encounter_session_id`, `prev_encounter_date`, and
    /// `prev_encounter_patient_name` should NOT be changed by the caller
    /// — the surviving session is the previous one, so those are already
    /// correct.
    Merged {
        merged_text: String,
        merged_text_rich: String,
        merged_is_clinical: bool,
    },
    /// No merge. Either the LLM said different encounters, or a multi-
    /// patient retroactive re-split happened, or the standalone multi-
    /// patient check ran. Caller falls through to the normal post-split
    /// flow (reset merge_back_count, finalize bundle, update prev_* to
    /// the current encounter).
    Separate,
    /// merge_enabled=false, or no prev encounter recorded yet. Same
    /// caller behavior as `Separate`.
    Skipped,
}

/// Run the post-split safety-net pipeline.
///
/// See module-level docs for responsibilities, logger contract, and
/// loop-state mutation rules.
#[allow(clippy::too_many_arguments)]
pub async fn run<C: RunContext>(
    ctx: &C,
    deps: &MergeBackDeps,
    call: MergeBackCall<'_>,
    loop_state: &mut LoopState,
) -> MergeBackOutcome {
    let MergeBackCall {
        session_id,
        encounter_text,
        encounter_text_rich,
        encounter_word_count,
        encounter_start,
        is_clinical,
        pre_soap_found_multi_patient,
        notes_text,
        tracker_snapshot,
        prev_encounter_session_id,
        prev_encounter_text,
        prev_encounter_text_rich,
        prev_encounter_date,
        prev_encounter_is_clinical,
        prev_encounter_patient_name,
        sensor_available,
        sensor_present_now,
    } = call;

    let mut merged_outcome: Option<MergeBackOutcome> = None;

    // ---- Retrospective merge check ----
    // After archiving + SOAP for encounter N, check if it should merge with N-1.
    // Only runs when prev_encounter_session_id is set (i.e., a prior encounter
    // was split within this same continuous session). First encounters after
    // restart or manual "New Patient" trigger are never merge-checked — the
    // user's explicit action (restart / new patient) means a new session.
    if deps.merge_enabled {
        if let (Some(prev_id), Some(prev_text), Some(prev_date)) = (
            prev_encounter_session_id,
            prev_encounter_text,
            prev_encounter_date,
        ) {
            // ── Small-orphan auto-merge gate ──────────────────────────
            // If the new encounter is very short (<500 words) and the
            // sensor confirms someone was present, it's almost certainly
            // a post-procedure tail (aftercare, scheduling) that was
            // incorrectly split. Auto-merge without asking the LLM.
            // Requires sensor data — without it we can't distinguish a
            // short clinical tail from background noise / non-patient chatter.
            const SMALL_ORPHAN_WORD_THRESHOLD: usize = 500;
            let sensor_confirmed_present = sensor_available && sensor_present_now;

            if encounter_word_count < SMALL_ORPHAN_WORD_THRESHOLD && sensor_confirmed_present {
                info!(
                    event = "small_orphan_auto_merge",
                    component = "continuous_mode_merge_back",
                    session_id = %session_id,
                    prev_session_id = %prev_id,
                    word_count = encounter_word_count,
                    "Small-orphan auto-merge: short encounter with sensor present — merging without LLM check"
                );
                if let Ok(mut logger) = deps.logger.lock() {
                    logger.log_merge_check(
                        "auto_merge_small_orphan",
                        "",
                        "",
                        Some(&format!(
                            "{{\"same_encounter\": true, \"reason\": \"small orphan ({} words) with sensor present\"}}",
                            encounter_word_count
                        )),
                        0,
                        true,
                        None,
                        serde_json::json!({
                            "prev_session_id": prev_id,
                            "curr_session_id": session_id,
                            "encounter_word_count": encounter_word_count,
                            "sensor_present_now": sensor_present_now,
                            "gate": "small_orphan_auto_merge",
                        }),
                    );
                }

                // Log merge check to replay bundle
                if let Ok(mut bundle) = deps.bundle.lock() {
                    bundle.set_merge_check(crate::replay_bundle::MergeCheck {
                        ts: ctx.now_utc().to_rfc3339(),
                        prev_session_id: prev_id.to_string(),
                        prev_tail_excerpt: String::new(),
                        curr_head_excerpt: String::new(),
                        patient_name: None,
                        prompt_system: String::new(),
                        prompt_user: String::new(),
                        response_raw: None,
                        parsed_same_encounter: Some(true),
                        parsed_reason: Some(format!(
                            "small orphan ({} words) with sensor present",
                            encounter_word_count
                        )),
                        latency_ms: 0,
                        success: true,
                        auto_merge_gate: Some("small_orphan_auto_merge".to_string()),
                        prev_source: Some("auto_merge_small_orphan".to_string()),
                        prev_soap_excerpt: None,
                    });
                }
                if let Some(ref dl) = *deps.day_logger {
                    dl.log(crate::day_log::DayEvent::EncounterMerged {
                        ts: ctx.now_utc().to_rfc3339(),
                        new_session_id: session_id.to_string(),
                        prev_session_id: prev_id.to_string(),
                        reason: "small_orphan_auto_merge".to_string(),
                        gate_type: Some("small_orphan".to_string()),
                    });
                }

                // Perform the merge (same logic as LLM-confirmed merge below)
                let merged_text = format!("{}\n{}", prev_text, encounter_text);
                let merged_text_rich = match prev_encounter_text_rich {
                    Some(prev_rich) => format!("{}\n{}", prev_rich, encounter_text_rich),
                    None => format!("{}\n{}", prev_text, encounter_text_rich),
                };
                let merged_wc = merged_text.split_whitespace().count();
                let merged_duration = encounter_start
                    .map(|s| (ctx.now_utc() - s).num_milliseconds().max(0) as u64)
                    .unwrap_or(0);
                let merge_vision_name = deps
                    .handle
                    .name_tracker
                    .lock()
                    .ok()
                    .and_then(|t| t.majority_name());
                if let Err(e) = local_archive::merge_encounters(
                    prev_id,
                    session_id,
                    prev_date,
                    &merged_text,
                    merged_wc,
                    merged_duration,
                    merge_vision_name.as_deref(),
                ) {
                    warn!(
                        event = "small_orphan_merge_failed",
                        component = "continuous_mode_merge_back",
                        error = %e,
                        "Failed to auto-merge small orphan"
                    );
                } else {
                    // Sync merge to server: delete orphan, re-upload surviving session
                    {
                        let today = ctx.now_utc().format("%Y-%m-%d").to_string();
                        deps.sync_ctx.sync_merge(session_id, prev_id, &today);
                    }
                    // Regenerate SOAP for the merged encounter
                    if let Some(ref client) = deps.llm_client {
                        let merge_notes = deps
                            .handle
                            .encounter_notes
                            .lock()
                            .map(|n| n.clone())
                            .unwrap_or_default();
                        crate::encounter_pipeline::regen_soap_after_merge(
                            client,
                            &merged_text,
                            prev_id,
                            prev_date,
                            prev_encounter_patient_name,
                            &deps.soap_model,
                            deps.soap_detail_level,
                            &deps.soap_format,
                            &deps.soap_custom_instructions,
                            merge_notes,
                            prev_encounter_is_clinical,
                            is_clinical,
                            &deps.logger,
                            &deps.sync_ctx,
                            "auto_merge_soap_regen",
                            Some(&deps.fast_model),
                            merged_duration,
                            deps.billing_counselling_exhausted,
                            Some(&deps.templates),
                            Some(&deps.billing_data),
                        )
                        .await;
                    }

                    loop_state.merge_back_count += 1;
                    loop_state.encounter_number = loop_state.encounter_number.saturating_sub(1);
                    info!(
                        event = "small_orphan_merge_complete",
                        component = "continuous_mode_merge_back",
                        merge_back_count = loop_state.merge_back_count,
                        encounter_number = loop_state.encounter_number,
                        "Auto-merge complete"
                    );

                    // Remove merged session from recent encounters list
                    if let Ok(mut recent) = deps.handle.recent_encounters.lock() {
                        recent.retain(|e| e.session_id != session_id);
                    }

                    // Emit merge event to frontend
                    ContinuousModeEvent::EncounterMerged {
                        kept_session_id: Some(prev_id.to_string()),
                        merged_into_session_id: None,
                        removed_session_id: session_id.to_string(),
                        reason: Some(format!(
                            "small orphan ({} words) with sensor present",
                            encounter_word_count
                        )),
                    }
                    .emit_via(ctx);

                    finalize_merged_bundle(
                        &deps.bundle,
                        &deps.segment_logger,
                        &tracker_snapshot,
                        session_id,
                        loop_state.encounter_number + 1,
                        encounter_word_count,
                        is_clinical,
                        prev_id,
                        prev_date,
                    );

                    // Update prev tracking to the merged encounter
                    return MergeBackOutcome::Merged {
                        merged_text,
                        merged_text_rich,
                        merged_is_clinical: is_clinical || prev_encounter_is_clinical,
                    };
                }
            }

            // ── LLM merge check (normal path) ────────────────────────
            let prev_tail = tail_words(prev_text, MERGE_EXCERPT_WORDS);
            let curr_head = head_words(encounter_text, MERGE_EXCERPT_WORDS);

            if let Some(ref client) = deps.llm_client {
                let (filtered_prev_tail, _) = strip_hallucinations(&prev_tail, 5);
                let (filtered_curr_head, _) = strip_hallucinations(&curr_head, 5);
                let merge_patient_name = deps
                    .handle
                    .name_tracker
                    .lock()
                    .ok()
                    .and_then(|t| t.majority_name());

                // Prefer prev's SOAP (carries patient labels + plan the tail
                // lacks); fall back to transcript tail for non-clinical prev
                // or when SOAP generation produced the malformed placeholder.
                let prev_soap = prepare_prev_soap_for_merge(
                    local_archive::read_session_soap(prev_id, prev_date).as_deref(),
                );
                let prev_input = match prev_soap.as_deref() {
                    Some(s) => crate::encounter_merge::PrevMergeInput::SoapNote(s),
                    None => crate::encounter_merge::PrevMergeInput::TranscriptTail(
                        &filtered_prev_tail,
                    ),
                };

                let merge_outcome = crate::encounter_pipeline::run_merge_check(
                    client,
                    &deps.fast_model,
                    prev_input,
                    &filtered_curr_head,
                    merge_patient_name.as_deref(),
                    &deps.logger,
                    serde_json::json!({
                        "prev_session_id": prev_id,
                        "curr_session_id": session_id,
                        "patient_name": merge_patient_name,
                        "prev_tail_words": filtered_prev_tail.split_whitespace().count(),
                        "curr_head_words": filtered_curr_head.split_whitespace().count(),
                    }),
                    Some(&deps.templates),
                )
                .await;

                // Log to replay bundle
                if let Ok(mut bundle) = deps.bundle.lock() {
                    bundle.set_merge_check(crate::replay_bundle::MergeCheck {
                        ts: ctx.now_utc().to_rfc3339(),
                        prev_session_id: prev_id.to_string(),
                        prev_tail_excerpt: filtered_prev_tail.clone(),
                        curr_head_excerpt: filtered_curr_head.clone(),
                        patient_name: merge_patient_name.clone(),
                        prompt_system: merge_outcome.prompt_system.clone(),
                        prompt_user: merge_outcome.prompt_user.clone(),
                        response_raw: merge_outcome.response_raw.clone(),
                        parsed_same_encounter: merge_outcome.same_encounter,
                        parsed_reason: merge_outcome
                            .reason
                            .as_ref()
                            .map(|r| format!("{:?}", r)),
                        latency_ms: merge_outcome.latency_ms,
                        success: merge_outcome.error.is_none(),
                        auto_merge_gate: None,
                        prev_source: Some(prev_input.source_tag().to_string()),
                        prev_soap_excerpt: prev_soap.clone(),
                    });
                }

                if merge_outcome.same_encounter == Some(true) {
                    if let Some(ref dl) = *deps.day_logger {
                        dl.log(crate::day_log::DayEvent::EncounterMerged {
                            ts: ctx.now_utc().to_rfc3339(),
                            new_session_id: session_id.to_string(),
                            prev_session_id: prev_id.to_string(),
                            reason: format!("{:?}", merge_outcome.reason),
                            gate_type: None,
                        });
                    }
                    info!(
                        event = "llm_merge_same",
                        component = "continuous_mode_merge_back",
                        session_id = %session_id,
                        prev_session_id = %prev_id,
                        reason = ?merge_outcome.reason,
                        "Merge check: encounters are the same visit — merging"
                    );

                    let merged_text = format!("{}\n{}", prev_text, encounter_text);
                    let merged_text_rich = match prev_encounter_text_rich {
                        Some(prev_rich) => format!("{}\n{}", prev_rich, encounter_text_rich),
                        None => format!("{}\n{}", prev_text, encounter_text_rich),
                    };
                    let merged_wc = merged_text.split_whitespace().count();
                    let merged_duration = encounter_start
                        .map(|s| (ctx.now_utc() - s).num_milliseconds().max(0) as u64)
                        .unwrap_or(0);

                    let merge_vision_name = deps
                        .handle
                        .name_tracker
                        .lock()
                        .ok()
                        .and_then(|t| t.majority_name());
                    if let Err(e) = local_archive::merge_encounters(
                        prev_id,
                        session_id,
                        prev_date,
                        &merged_text,
                        merged_wc,
                        merged_duration,
                        merge_vision_name.as_deref(),
                    ) {
                        warn!(
                            event = "llm_merge_failed",
                            component = "continuous_mode_merge_back",
                            error = %e,
                            "Failed to merge encounters"
                        );
                    } else {
                        // Sync merge to server: delete merged-away session, re-upload surviving
                        {
                            let today = ctx.now_utc().format("%Y-%m-%d").to_string();
                            deps.sync_ctx.sync_merge(session_id, prev_id, &today);
                        }
                        // Regenerate SOAP for the merged encounter
                        if let Some(ref client) = deps.llm_client {
                            let merge_notes = deps
                                .handle
                                .encounter_notes
                                .lock()
                                .map(|n| n.clone())
                                .unwrap_or_default();
                            crate::encounter_pipeline::regen_soap_after_merge(
                                client,
                                &merged_text,
                                prev_id,
                                prev_date,
                                prev_encounter_patient_name,
                                &deps.soap_model,
                                deps.soap_detail_level,
                                &deps.soap_format,
                                &deps.soap_custom_instructions,
                                merge_notes,
                                prev_encounter_is_clinical,
                                is_clinical,
                                &deps.logger,
                                &deps.sync_ctx,
                                "merge_soap_regen",
                                Some(&deps.fast_model),
                                merged_duration,
                                deps.billing_counselling_exhausted,
                                Some(&deps.templates),
                                Some(&deps.billing_data),
                            )
                            .await;
                        }

                        loop_state.encounter_number =
                            loop_state.encounter_number.saturating_sub(1);

                        // Remove merged session from recent encounters list
                        if let Ok(mut recent) = deps.handle.recent_encounters.lock() {
                            recent.retain(|e| e.session_id != session_id);
                        }

                        ContinuousModeEvent::EncounterMerged {
                            kept_session_id: None,
                            merged_into_session_id: Some(prev_id.to_string()),
                            removed_session_id: session_id.to_string(),
                            reason: None,
                        }
                        .emit_via(ctx);

                        // Escalate confidence threshold for next detection
                        loop_state.merge_back_count += 1;
                        info!(
                            event = "llm_merge_back",
                            component = "continuous_mode_merge_back",
                            merge_back_count = loop_state.merge_back_count,
                            threshold_boost = loop_state.merge_back_count as f64 * 0.05,
                            "Merge-back: next confidence threshold escalated"
                        );

                        // Finalize the merged-away encounter's bundle
                        // BEFORE the retrospective multi-patient check
                        // below — that check's LLM call lands in the
                        // freshly-cleared builder and flows into the
                        // surviving encounter's bundle, matching
                        // production's pipeline_log routing.
                        finalize_merged_bundle(
                            &deps.bundle,
                            &deps.segment_logger,
                            &tracker_snapshot,
                            session_id,
                            loop_state.encounter_number + 1,
                            encounter_word_count,
                            is_clinical,
                            prev_id,
                            prev_date,
                        );

                        // ── Retrospective multi-patient check ──
                        if merged_wc >= deps.multi_patient_detect_word_threshold {
                            if let Some(ref client) = deps.llm_client {
                                info!(
                                    event = "retrospective_multi_patient_detect",
                                    component = "continuous_mode_merge_back",
                                    prev_session_id = %prev_id,
                                    merged_word_count = merged_wc,
                                    "Retrospective multi-patient detect"
                                );
                                // Log to the surviving session's pipeline log
                                let retro_outcome = client
                                    .run_multi_patient_detection(
                                        &deps.fast_model,
                                        &merged_text_rich,
                                    )
                                    .await;
                                if let Ok(mut logger) = deps.logger.lock() {
                                    // Point logger at surviving session dir so this entry is preserved
                                    if let Ok(prev_dir) = local_archive::get_session_archive_dir(
                                        prev_id, prev_date,
                                    ) {
                                        logger.set_session(&prev_dir);
                                    }
                                    let det_context = match &retro_outcome.detection {
                                        Some(d) => serde_json::json!({
                                            "stage": "retrospective",
                                            "patient_count": d.patient_count,
                                            "confidence": d.confidence,
                                            "reasoning": d.reasoning,
                                            "patients": d.patients.iter()
                                                .map(|p| serde_json::json!({"label": p.label, "summary": p.summary}))
                                                .collect::<Vec<_>>(),
                                            "word_count": merged_wc,
                                        }),
                                        None => serde_json::json!({
                                            "stage": "retrospective",
                                            "patient_count": 1,
                                            "word_count": merged_wc,
                                            "accepted": false,
                                        }),
                                    };
                                    logger.log_llm_call(
                                        "multi_patient_detect",
                                        &retro_outcome.model,
                                        &retro_outcome.system_prompt,
                                        &retro_outcome.user_prompt,
                                        retro_outcome.response_raw.as_deref(),
                                        retro_outcome.latency_ms,
                                        retro_outcome.success,
                                        retro_outcome.error.as_deref(),
                                        det_context,
                                    );
                                }
                                // Lands in the post-`finalize_merged_bundle`
                                // builder, so it flows into the surviving
                                // encounter's replay_bundle.json on its
                                // next finalization.
                                if let Ok(mut bundle) = deps.bundle.lock() {
                                    bundle.add_multi_patient_detection(
                                        multi_patient_from_outcome(
                                            &retro_outcome,
                                            crate::replay_bundle::MultiPatientStage::Retrospective,
                                            merged_wc,
                                        ),
                                    );
                                }
                                if let Some(detection) = retro_outcome.detection {
                                    info!(
                                        event = "retrospective_multi_patient_regen",
                                        component = "continuous_mode_merge_back",
                                        prev_session_id = %prev_id,
                                        patient_count = detection.patient_count,
                                        "Retrospective multi-patient SOAP regeneration"
                                    );
                                    let (filtered, _) = strip_hallucinations(&merged_text, 5);
                                    let regen_notes = deps
                                        .handle
                                        .encounter_notes
                                        .lock()
                                        .map(|n| n.clone())
                                        .unwrap_or_default();
                                    let regen_outcome =
                                        crate::encounter_pipeline::generate_and_archive_soap(
                                            client,
                                            &deps.soap_model,
                                            &filtered,
                                            prev_id,
                                            prev_date,
                                            deps.soap_detail_level,
                                            &deps.soap_format,
                                            &deps.soap_custom_instructions,
                                            regen_notes,
                                            merged_wc,
                                            Some(&detection),
                                            &deps.logger,
                                            serde_json::json!({
                                                "stage": "retrospective_multi_patient_soap",
                                                "session_id": prev_id,
                                                "detection_confidence": detection.confidence,
                                            }),
                                            Some(&deps.templates),
                                            Some(deps.soap_generation_timeout_secs),
                                        )
                                        .await;
                                    if let crate::encounter_pipeline::SoapGenerationOutcome::Success {
                                        ref result,
                                        ref content,
                                        ..
                                    } = regen_outcome
                                    {
                                        // Server sync: upload retrospective SOAP
                                        deps.sync_ctx.sync_soap(
                                            prev_id,
                                            content,
                                            deps.soap_detail_level,
                                            &deps.soap_format,
                                        );
                                        info!(
                                            event = "retrospective_multi_patient_regen_done",
                                            component = "continuous_mode_merge_back",
                                            prev_session_id = %prev_id,
                                            notes = result.notes.len(),
                                            chars = content.len(),
                                            "Retrospective per-patient SOAP regenerated"
                                        );
                                    }
                                }
                            }
                        }

                        // Return Merged so caller updates prev_* tracking
                        merged_outcome = Some(MergeBackOutcome::Merged {
                            merged_text,
                            merged_text_rich,
                            merged_is_clinical: is_clinical || prev_encounter_is_clinical,
                        });
                    }
                } else if let Some(false) = merge_outcome.same_encounter {
                    info!(
                        event = "llm_merge_separate",
                        component = "continuous_mode_merge_back",
                        reason = ?merge_outcome.reason,
                        "Merge check: different encounters"
                    );
                }
            }
        }
    }

    // If we merged above, return early — standalone multi-patient check
    // applies only to the current encounter (which no longer exists as a
    // standalone — it's been merged into the surviving session, which
    // already went through retrospective detection).
    if let Some(outcome) = merged_outcome {
        return outcome;
    }

    // ── Standalone multi-patient check for large encounters ──
    // Safety net: if the inline multi-patient detection (run before
    // SOAP at >=500 words) missed a multi-patient encounter, this
    // second pass catches it for large encounters (>=2,500 words).
    // Runs after the merge check to avoid wasted work on encounters
    // that will be merged back.
    //
    // Skip when pre_soap already confirmed 2+ patients — the pre_soap
    // and standalone prompts are identical, and re-running just burns
    // the LLM budget (Brown + Wicks in the Apr 16 audit each ran a
    // redundant standalone call that timed out at 30s after pre_soap
    // had already succeeded).
    if is_clinical
        && encounter_word_count >= deps.multi_patient_check_word_threshold
        && !pre_soap_found_multi_patient
    {
        if let Some(ref client) = deps.llm_client {
            info!(
                event = "standalone_multi_patient_detect",
                component = "continuous_mode_merge_back",
                encounter_number = loop_state.encounter_number,
                word_count = encounter_word_count,
                "Standalone multi-patient check"
            );
            let mp_outcome = client
                .run_multi_patient_detection(&deps.fast_model, encounter_text_rich)
                .await;
            if let Ok(mut logger) = deps.logger.lock() {
                let det_context = match &mp_outcome.detection {
                    Some(d) => serde_json::json!({
                        "stage": "standalone_multi_patient",
                        "patient_count": d.patient_count,
                        "confidence": d.confidence,
                        "word_count": encounter_word_count,
                    }),
                    None => serde_json::json!({
                        "stage": "standalone_multi_patient",
                        "patient_count": 1,
                        "word_count": encounter_word_count,
                    }),
                };
                logger.log_llm_call(
                    "multi_patient_detect",
                    &mp_outcome.model,
                    &mp_outcome.system_prompt,
                    &mp_outcome.user_prompt,
                    mp_outcome.response_raw.as_deref(),
                    mp_outcome.latency_ms,
                    mp_outcome.success,
                    mp_outcome.error.as_deref(),
                    det_context,
                );
            }
            if let Ok(mut bundle) = deps.bundle.lock() {
                bundle.add_multi_patient_detection(multi_patient_from_outcome(
                    &mp_outcome,
                    crate::replay_bundle::MultiPatientStage::Standalone,
                    encounter_word_count,
                ));
            }
            if let Some(detection) = mp_outcome.detection {
                info!(
                    event = "standalone_multi_patient_regen",
                    component = "continuous_mode_merge_back",
                    encounter_number = loop_state.encounter_number,
                    patient_count = detection.patient_count,
                    "Standalone multi-patient SOAP regeneration"
                );
                let (filtered, _) = strip_hallucinations(encounter_text, 5);
                let soap_now = ctx.now_utc();
                let regen_outcome = crate::encounter_pipeline::generate_and_archive_soap(
                    client,
                    &deps.soap_model,
                    &filtered,
                    session_id,
                    &soap_now,
                    deps.soap_detail_level,
                    &deps.soap_format,
                    &deps.soap_custom_instructions,
                    notes_text.to_string(),
                    encounter_word_count,
                    Some(&detection),
                    &deps.logger,
                    serde_json::json!({
                        "stage": "standalone_multi_patient_soap",
                        "session_id": session_id,
                        "encounter_number": loop_state.encounter_number,
                        "detection_confidence": detection.confidence,
                    }),
                    Some(&deps.templates),
                    Some(deps.soap_generation_timeout_secs),
                )
                .await;
                if let crate::encounter_pipeline::SoapGenerationOutcome::Success {
                    ref result,
                    ref content,
                    ..
                } = regen_outcome
                {
                    deps.sync_ctx.sync_soap(
                        session_id,
                        content,
                        deps.soap_detail_level,
                        &deps.soap_format,
                    );
                    info!(
                        event = "standalone_multi_patient_regen_done",
                        component = "continuous_mode_merge_back",
                        encounter_number = loop_state.encounter_number,
                        notes = result.notes.len(),
                        "Standalone multi-patient SOAP regenerated"
                    );
                }
            }
        }
    }

    if deps.merge_enabled {
        MergeBackOutcome::Separate
    } else {
        // Preserve the brief's distinction for future introspection; both
        // branches take the same fall-through path in the caller today.
        if prev_encounter_session_id.is_some() {
            MergeBackOutcome::Separate
        } else {
            MergeBackOutcome::Skipped
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_prev_soap_none_when_missing() {
        assert_eq!(prepare_prev_soap_for_merge(None), None);
    }

    #[test]
    fn prepare_prev_soap_none_when_blank() {
        assert_eq!(prepare_prev_soap_for_merge(Some("   \n\n")), None);
    }

    #[test]
    fn prepare_prev_soap_none_when_malformed_sentinel_present() {
        let placeholder = "S:\n- [SOAP generation produced malformed output — review transcript]";
        assert_eq!(prepare_prev_soap_for_merge(Some(placeholder)), None);
    }

    #[test]
    fn prepare_prev_soap_returns_trimmed_when_short() {
        let soap = "  S: cough\nO: clear\nA: viral\nP: rest  ";
        let out = prepare_prev_soap_for_merge(Some(soap)).unwrap();
        assert_eq!(out, "S: cough\nO: clear\nA: viral\nP: rest");
        assert!(!out.contains("[truncated]"));
    }

    #[test]
    fn prepare_prev_soap_truncates_when_over_cap() {
        let big = "S: ".to_string() + &"x".repeat(PREV_SOAP_MERGE_CHAR_CAP * 2);
        let out = prepare_prev_soap_for_merge(Some(&big)).unwrap();
        assert!(out.len() <= PREV_SOAP_MERGE_CHAR_CAP + 32);
        assert!(out.ends_with("…[truncated]"));
    }

    #[test]
    fn prepare_prev_soap_utf8_safe_at_boundary() {
        // Force a multi-byte char to straddle PREV_SOAP_MERGE_CHAR_CAP.
        let pad = "a".repeat(PREV_SOAP_MERGE_CHAR_CAP - 1);
        let soap = format!("{pad}é{}", "b".repeat(100));
        let out = prepare_prev_soap_for_merge(Some(&soap)).unwrap();
        // Must not panic on slicing + must still hold valid UTF-8.
        assert!(out.is_char_boundary(out.len()));
    }
}
