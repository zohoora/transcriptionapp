//! Post-split pipeline: clinical content check, SOAP generation, billing
//! extraction. Non-clinical encounters short-circuit (no SOAP, no billing).
//!
//! Runs after the splitter archives the encounter and redirects the pipeline
//! logger. Produces a `PostSplitOutcome` that downstream `merge_back` uses to
//! gate the standalone multi-patient safety-net check (via the
//! `pre_soap_found_multi_patient` flag).
//!
//! Most of this path already delegates to `encounter_pipeline.rs` helpers
//! (`check_clinical_content`, `generate_and_archive_soap`,
//! `extract_and_archive_billing`); this extraction is primarily composition +
//! structured tracing.
//!
//! LOGGER SESSION CONTRACT: on entry, the pipeline logger is pointed at the
//! encounter's session (set by the splitter). All `pipeline_log` writes land
//! in that session's `pipeline_log.jsonl`. Does not redirect.
//!
//! COMPONENT: `continuous_mode_post_split`.

use std::sync::{Arc, Mutex};

use tracing::{info, warn};

use crate::continuous_mode_events::ContinuousModeEvent;
use crate::continuous_mode_splitter::SplitContext;
use crate::day_log::DayLogger;
use crate::encounter_experiment::strip_hallucinations;
use crate::llm_client::LLMClient;
use crate::pipeline_log::PipelineLogger;
use crate::replay_bundle::ReplayBundleBuilder;
use crate::run_context::RunContext;
use crate::server_config::{BillingData, PromptTemplates};
use crate::server_sync::ServerSyncContext;

/// Long-lived dependency bundle. Built once per continuous-mode run before the
/// detector loop starts and borrowed into each call.
///
/// `llm_client` is borrowed by reference rather than owned here (unlike
/// `merge_back` which builds its own dedicated LLMClient). The detector task
/// already owns the primary `llm_client` for detection + clinical check; this
/// component just piggy-backs on the same instance.
pub struct PostSplitDeps {
    pub logger: Arc<Mutex<PipelineLogger>>,
    pub bundle: Arc<Mutex<ReplayBundleBuilder>>,
    pub day_logger: Arc<Option<DayLogger>>,
    pub sync_ctx: ServerSyncContext,
    pub last_error: Arc<Mutex<Option<String>>>,
    pub templates: Arc<PromptTemplates>,
    pub billing_data: Arc<BillingData>,
    pub fast_model: String,
    pub soap_model: String,
    /// Vision-capable model alias used for multimodal multi-patient detect +
    /// SOAP calls when deduped chart screenshots are available for the
    /// encounter (resolved from `operational.soap_model`, which per ADR is
    /// vision-capable). Falls back to text-only models when no screenshots
    /// are present at split time.
    pub vision_model: String,
    pub soap_detail_level: u8,
    pub soap_format: String,
    pub soap_custom_instructions: String,
    pub min_words_for_clinical_check: usize,
    pub multi_patient_detect_word_threshold: usize,
    pub soap_generation_timeout_secs: u64,
    pub billing_extraction_timeout_secs: u64,
    pub billing_counselling_exhausted: bool,
}

/// What happened in the post-split pipeline. Consumed by the caller for
/// `merge_back` wiring + the normal post-split tracking updates.
pub struct PostSplitOutcome {
    pub is_clinical: bool,
    /// True iff the pre-SOAP multi-patient detector returned `patient_count >=
    /// 2`. Used to gate the standalone multi-patient safety-net check in
    /// `merge_back` — no point re-detecting what we already confirmed.
    pub pre_soap_found_multi_patient: bool,
}

/// Run the clinical-check -> multi-patient-detect -> SOAP -> billing pipeline.
#[allow(clippy::too_many_arguments)]
pub async fn run<C: RunContext>(
    ctx: &C,
    deps: &PostSplitDeps,
    llm_client: &Option<LLMClient>,
    split: &SplitContext,
    encounter_number: u32,
) -> PostSplitOutcome {
    let SplitContext {
        session_id,
        encounter_text,
        encounter_text_rich,
        encounter_word_count,
        encounter_duration_ms,
        notes_text,
        encounter_patient_name,
        ..
    } = split;
    let encounter_word_count = *encounter_word_count;
    let encounter_duration_ms = *encounter_duration_ms;

    // Clinical content check: flag non-clinical encounters
    let is_clinical = if let Some(ref client) = llm_client {
        crate::encounter_pipeline::check_clinical_content(
            client,
            &deps.fast_model,
            encounter_text,
            encounter_word_count,
            &deps.logger,
            serde_json::json!({
                "encounter_number": encounter_number,
                "word_count": encounter_word_count,
            }),
            Some(&deps.templates),
            Some(deps.min_words_for_clinical_check),
        )
        .await
    } else {
        encounter_word_count >= deps.min_words_for_clinical_check
    };

    if !is_clinical {
        crate::encounter_pipeline::mark_non_clinical(session_id);
        // Sync non-clinical status to server (initial upload didn't have it)
        let today = ctx.now_utc().format("%Y-%m-%d").to_string();
        deps.sync_ctx.sync_session(session_id, &today);
    }

    // Log clinical check result to replay bundle + day log
    if let Ok(mut bundle) = deps.bundle.lock() {
        bundle.set_clinical_check(crate::replay_bundle::ClinicalCheck {
            ts: ctx.now_utc().to_rfc3339(),
            is_clinical,
            latency_ms: 0, // Clinical check latency already logged via pipeline_logger
            success: true,
            error: None,
        });
    }
    if let Some(ref dl) = *deps.day_logger {
        dl.log(crate::day_log::DayEvent::ClinicalCheckResult {
            ts: ctx.now_utc().to_rfc3339(),
            session_id: session_id.clone(),
            is_clinical,
        });
    }

    // Whether the pre-SOAP multi-patient detector already found 2+ patients
    // for this encounter. Used later to gate the "standalone safety net"
    // check — no point re-detecting what we already confirmed (saves a
    // 30s-timeout LLM call on long encounters, as seen in the Apr 16 2026
    // Room 6 audit where Brown + Wicks each ran a redundant standalone
    // check that burned the full 30s timeout after pre_soap had already
    // succeeded).
    let mut pre_soap_found_multi_patient = false;

    // Generate SOAP note (with 120s timeout — SOAP is heavier than detection)
    // Skip SOAP for non-clinical encounters to prevent hallucinated clinical content
    if !is_clinical {
        info!(
            event = "post_split_skip_soap_non_clinical",
            component = "continuous_mode_post_split",
            encounter_number,
            "Skipping SOAP for non-clinical encounter"
        );
    } else if let Some(ref client) = llm_client {
        // One dedup, attached to BOTH the multi-patient detect call AND the
        // SOAP call below — the model sees a consistent chart-image set across
        // both hops. Screenshots are flushed to disk by `continuous_mode_splitter`
        // BEFORE this function fires.
        let soap_now = ctx.now_utc();
        let deduped_screenshots =
            crate::screenshot_dedup::load_deduped_screenshots_for_session(session_id, &soap_now);
        let screenshot_arg = Some(deduped_screenshots.as_slice());

        // Detect couples/family visits before SOAP so the per-patient SOAP
        // path can fire when patient_count > 1.
        let multi_patient_detection = if encounter_word_count >= deps.multi_patient_detect_word_threshold {
            info!(
                event = "post_split_multi_patient_detect",
                component = "continuous_mode_post_split",
                encounter_number,
                word_count = encounter_word_count,
                screenshots_attached = deduped_screenshots.len(),
                "Running multi-patient detection"
            );
            crate::encounter_pipeline::run_pre_soap_multi_patient_detection(
                client,
                &deps.fast_model,
                encounter_text_rich,
                encounter_word_count,
                "post_split",
                &deps.logger,
                &deps.bundle,
                screenshot_arg,
                &deps.vision_model,
            )
            .await
        } else {
            None
        };

        // Strip hallucinated repetitions before SOAP generation
        let (filtered_encounter_text, soap_filter_report) = strip_hallucinations(encounter_text, 5);
        if !soap_filter_report.repetitions.is_empty()
            || !soap_filter_report.phrase_repetitions.is_empty()
        {
            if let Ok(mut logger) = deps.logger.lock() {
                logger.log_hallucination_filter(serde_json::json!({
                    "call_site": "soap_prep",
                    "original_words": soap_filter_report.original_word_count,
                    "cleaned_words": soap_filter_report.cleaned_word_count,
                    "single_word_reps": soap_filter_report.repetitions.iter()
                        .map(|r| &r.word).collect::<Vec<_>>(),
                    "phrase_reps": soap_filter_report.phrase_repetitions.iter()
                        .map(|r| &r.phrase).collect::<Vec<_>>(),
                }));
            }
        }

        // Propagate pre_soap's multi-patient result into the outer scope so
        // the standalone safety-net check can skip.
        pre_soap_found_multi_patient = multi_patient_detection
            .as_ref()
            .map_or(false, |d| d.patient_count >= 2);

        info!(
            event = "post_split_soap_generate",
            component = "continuous_mode_post_split",
            encounter_number,
            screenshots_attached = deduped_screenshots.len(),
            "Generating SOAP"
        );
        let soap_outcome = crate::encounter_pipeline::generate_and_archive_soap(
            client,
            &deps.soap_model,
            &filtered_encounter_text,
            session_id,
            &soap_now,
            deps.soap_detail_level,
            &deps.soap_format,
            &deps.soap_custom_instructions,
            notes_text.clone(),
            encounter_word_count,
            multi_patient_detection.as_ref(),
            &deps.logger,
            serde_json::json!({
                "encounter_number": encounter_number,
                "word_count": encounter_word_count,
                "has_notes": !notes_text.is_empty(),
            }),
            Some(&deps.templates),
            Some(deps.soap_generation_timeout_secs),
            Some(&deps.bundle),
            screenshot_arg,
            &deps.vision_model,
        )
        .await;

        match soap_outcome {
            crate::encounter_pipeline::SoapGenerationOutcome::Success {
                ref result,
                ref content,
                latency_ms,
                ref sibling_ids,
            } => {
                let patient_count = result.notes.len();
                ContinuousModeEvent::SoapGenerated {
                    session_id: session_id.clone(),
                    patient_count: Some(patient_count),
                    recovered: None,
                }
                .emit_via(ctx);
                info!(
                    event = "post_split_soap_success",
                    component = "continuous_mode_post_split",
                    encounter_number,
                    patient_count,
                    sibling_count = sibling_ids.len(),
                    "SOAP generated"
                );
                if let Some(ref dl) = *deps.day_logger {
                    dl.log(crate::day_log::DayEvent::SoapGenerated {
                        ts: ctx.now_utc().to_rfc3339(),
                        session_id: session_id.clone(),
                        latency_ms,
                        success: true,
                    });
                }

                let after_hours = crate::encounter_pipeline::is_after_hours(&soap_now);
                let today = ctx.now_utc().format("%Y-%m-%d").to_string();

                // Single-patient encounters bill once on the source session_id with the
                // combined `content`. Multi-patient encounters were auto-split into
                // siblings — bill each sibling independently with its per-patient SOAP
                // and a Q310A duration prorated by SOAP word count. Sibling billing
                // calls run concurrently (each is a 5-30s LLM round-trip; serial would
                // double the encounter tail latency for a 2-patient visit).
                let billing_inputs: Vec<(String, String, u64, Option<String>)> = if sibling_ids.is_empty() {
                    deps.sync_ctx.sync_soap(session_id, content, deps.soap_detail_level, &deps.soap_format);
                    vec![(
                        session_id.clone(),
                        content.clone(),
                        encounter_duration_ms,
                        encounter_patient_name.clone(),
                    )]
                } else {
                    info!(
                        event = "post_split_sibling_split",
                        component = "continuous_mode_post_split",
                        encounter_number,
                        sibling_count = sibling_ids.len(),
                        "Multi-patient encounter auto-split into siblings"
                    );
                    let per_patient_soaps: Vec<crate::local_archive::PerPatientSplitInput> = result.notes.iter()
                        .map(|n| crate::local_archive::PerPatientSplitInput {
                            label: n.patient_label.clone(),
                            soap_text: n.content.clone(),
                            extracted_name: n.extracted_patient_name.clone(),
                            extracted_dob: n.extracted_patient_dob.clone(),
                        })
                        .collect();
                    let durations = crate::local_archive::prorate_durations_by_soap_words(
                        &per_patient_soaps, encounter_duration_ms,
                    );
                    sibling_ids.iter()
                        .zip(result.notes.iter())
                        .zip(durations.into_iter())
                        .map(|((child_id, note), child_dur)| (
                            child_id.clone(),
                            note.content.clone(),
                            child_dur,
                            Some(note.patient_label.clone()),
                        ))
                        .collect()
                };

                let rule_ctx = crate::billing::RuleEngineContext {
                    counselling_exhausted: deps.billing_counselling_exhausted,
                    transcript: Some(filtered_encounter_text.clone()),
                    ..Default::default()
                };
                let model_ref = &deps.fast_model;
                let transcript_ref = filtered_encounter_text.as_str();
                let soap_now_ref = &soap_now;
                let rule_ctx_ref = &rule_ctx;
                let logger_ref = &deps.logger;
                let templates_ref = &deps.templates;
                let billing_data_ref = &deps.billing_data;
                let bundle_ref = &deps.bundle;
                let billing_timeout = deps.billing_extraction_timeout_secs;
                let billing_futures = billing_inputs.iter().map(|(sid, soap, dur, label)| async move {
                    let start = std::time::Instant::now();
                    let r = crate::encounter_pipeline::extract_and_archive_billing(
                        client, model_ref, soap, transcript_ref, "",
                        sid, soap_now_ref, *dur, label.as_deref(), after_hours,
                        rule_ctx_ref, logger_ref,
                        Some(templates_ref), Some(billing_data_ref),
                        Some(billing_timeout), Some(bundle_ref),
                    ).await;
                    (sid.clone(), r, start.elapsed().as_millis() as u64)
                });
                let results = futures_util::future::join_all(billing_futures).await;
                for (target_sid, billing_result, billing_latency) in results {
                    match &billing_result {
                        Ok(record) => {
                            if let Some(ref dl) = *deps.day_logger {
                                dl.log(crate::day_log::DayEvent::BillingExtracted {
                                    ts: ctx.now_utc().to_rfc3339(),
                                    session_id: target_sid.clone(),
                                    codes_count: record.codes.len() as u32,
                                    total_amount_cents: record.total_amount_cents,
                                    latency_ms: billing_latency,
                                    success: true,
                                });
                            }
                            // Single resync per target: uploads metadata, transcript,
                            // soap_note, billing.json, has_billing_record=true. For
                            // siblings this also creates the new sibling sessions
                            // server-side (anchor already existed pre-split).
                            deps.sync_ctx.resync_session(&target_sid, &today);
                        }
                        Err(e) => {
                            warn!(
                                event = "post_split_billing_failed",
                                component = "continuous_mode_post_split",
                                encounter_number,
                                target_session_id = %target_sid,
                                error = %e,
                                "Billing extraction failed"
                            );
                            if let Some(ref dl) = *deps.day_logger {
                                dl.log(crate::day_log::DayEvent::BillingExtracted {
                                    ts: ctx.now_utc().to_rfc3339(),
                                    session_id: target_sid,
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
            crate::encounter_pipeline::SoapGenerationOutcome::Failed {
                latency_ms,
                ref error,
            } => {
                if let Ok(mut err) = deps.last_error.lock() {
                    *err = Some(format!("SOAP generation failed: {}", error));
                } else {
                    warn!(
                        event = "post_split_last_error_poisoned",
                        component = "continuous_mode_post_split",
                        "Last error lock poisoned, error state not updated"
                    );
                }
                ContinuousModeEvent::SoapFailed {
                    session_id: session_id.clone(),
                    error: error.clone(),
                }
                .emit_via(ctx);
                if let Some(ref dl) = *deps.day_logger {
                    dl.log(crate::day_log::DayEvent::SoapGenerated {
                        ts: ctx.now_utc().to_rfc3339(),
                        session_id: session_id.clone(),
                        latency_ms,
                        success: false,
                    });
                }
            }
        }
    }

    PostSplitOutcome {
        is_clinical,
        pre_soap_found_multi_patient,
    }
}
