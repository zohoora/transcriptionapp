//! Shared encounter pipeline helpers for continuous mode.
//!
//! These functions are used by both the main detector loop and the
//! flush-on-stop path, eliminating duplication of clinical content checks,
//! non-clinical metadata updates, SOAP generation, merge checks, and
//! orphaned SOAP recovery.

use chrono::{DateTime, Datelike, Utc};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{info, warn};

use crate::encounter_detection::{
    build_clinical_content_check_prompt, parse_clinical_content_check,
    MultiPatientDetectionResult, MIN_WORDS_FOR_CLINICAL_CHECK,
};
use crate::encounter_experiment::strip_hallucinations;
use crate::encounter_merge::{build_encounter_merge_prompt, parse_merge_check, PrevMergeInput};
use crate::llm_client::{
    build_simple_soap_prompt, build_soap_user_content, LLMClient, MultiPatientSoapResult,
    SoapFormat, SoapOptions,
};
use crate::server_config::PromptTemplates;
use crate::local_archive;
use crate::pipeline_log::PipelineLogger;
use crate::continuous_mode::effective_soap_detail_level;
use crate::continuous_mode_events::ContinuousModeEvent;

// ── SOAP generation ──────────────────────────────────────────────────

/// Result of a SOAP generation + archive attempt.
pub enum SoapGenerationOutcome {
    /// SOAP generated and archived successfully.
    Success {
        result: MultiPatientSoapResult,
        /// Combined formatted SOAP text (with `=== Patient N ===` headers when
        /// multi-patient). Retained for legacy logging + uploads. New per-sibling
        /// callers should iterate `result.notes` and use each `note.content`.
        content: String,
        latency_ms: u64,
        /// Sibling session IDs when the multi-patient encounter was auto-split
        /// into separate sessions (anchor first, then siblings 1..N in
        /// patient-detection order). Empty for single-patient encounters or
        /// when the split fell back to the legacy combined-SOAP layout.
        /// Callers iterate this in pair with `result.notes` to drive per-patient
        /// billing extraction and per-sibling sync.
        sibling_ids: Vec<String>,
    },
    /// LLM call failed or timed out.
    Failed { latency_ms: u64, error: String },
}

/// Timeout for SOAP generation LLM calls (seconds).
const SOAP_GENERATION_TIMEOUT_SECS: u64 = 300;
/// Error string for SOAP timeout (avoids allocation in the timeout path).
const SOAP_TIMEOUT_ERROR: &str = "timeout_300s";

/// Timeout for the post-split clinical-content classifier LLM call. 2026-05-06:
/// raised from 30s to 90s after `fast-model` was repointed at a larger model.
/// The prompt is small (clinical / non-clinical classifier) so 90s is generous,
/// but the larger model still pushes some calls over the original 30s budget.
const CLINICAL_CONTENT_CHECK_TIMEOUT_SECS: u64 = 90;

/// Generate a SOAP note, archive it, and log the result.
///
/// Handles the full LLM call → timeout → archive → pipeline log pattern
/// shared across primary SOAP, merge SOAP regen, flush SOAP, and orphaned
/// recovery. Callers handle site-specific side effects (replay bundle, day log,
/// frontend events, error state) by matching on the returned outcome.
///
/// `log_extra` should contain caller-specific metadata (e.g. stage, session_id,
/// encounter_number). The function auto-adds `detail_level`, `format`,
/// `response_chars`, and `patient_count`.
/// Falls back to formatted `content` when `raw_response` is missing
/// (older code paths, stub backends).
fn build_soap_bundle_capture_success(
    soap_result: &mut crate::llm_client::MultiPatientSoapResult,
    system_prompt: &str,
    content: &str,
    latency_ms: u64,
    word_count: usize,
) -> crate::replay_bundle::SoapResult {
    let patient_count = soap_result.notes.len();
    let raw_for_bundle = soap_result
        .raw_response
        .take()
        .unwrap_or_else(|| content.to_string());
    crate::replay_bundle::SoapResult {
        ts: Utc::now().to_rfc3339(),
        latency_ms,
        success: true,
        word_count,
        error: None,
        patient_count: if patient_count > 1 { Some(patient_count) } else { None },
        system_prompt: Some(system_prompt.to_string()),
        user_prompt: soap_result.user_prompt.take(),
        response_raw: Some(raw_for_bundle),
    }
}

pub async fn generate_and_archive_soap(
    client: &LLMClient,
    soap_model: &str,
    filtered_text: &str,
    session_id: &str,
    session_date: &DateTime<Utc>,
    soap_detail_level: u8,
    soap_format: &str,
    soap_custom_instructions: &str,
    session_notes: String,
    word_count: usize,
    multi_patient_detection: Option<&MultiPatientDetectionResult>,
    logger: &Arc<Mutex<PipelineLogger>>,
    log_extra: serde_json::Value,
    templates: Option<&PromptTemplates>,
    // Optional server-configured timeout override. `None` falls back to the compiled
    // `SOAP_GENERATION_TIMEOUT_SECS` constant. Pass `None` from tests and pre-Phase-2
    // callers; production continuous mode passes `Some(thresholds.soap_generation_timeout_secs)`.
    soap_timeout_override: Option<u64>,
    // Optional replay bundle. When provided, the function writes a
    // `SoapResult` with the captured system_prompt + response on every
    // outcome path (Success / Failed / Timeout) so SOAP-experiment CLIs
    // can replay the prompt without re-issuing the LLM call. Schema v5+.
    bundle: Option<&Arc<Mutex<crate::replay_bundle::ReplayBundleBuilder>>>,
    // Optional deduped chart screenshots to attach as image_url parts.
    // When `Some` and non-empty, SOAP generation routes through `vision_model`
    // (vision-capable) instead of `soap_model`. Caller is responsible for
    // deduping via `screenshot_dedup::dedup_screenshots`.
    screenshot_paths: Option<&[PathBuf]>,
    // Vision-capable model alias used when `screenshot_paths` has images.
    // Pass the resolved `soap_model` alias (per ADR; `soap-model` has vision
    // capabilities). Ignored when `screenshot_paths` is None or empty.
    vision_model: &str,
) -> SoapGenerationOutcome {
    let soap_timeout = soap_timeout_override.unwrap_or(SOAP_GENERATION_TIMEOUT_SECS);
    let effective_detail = effective_soap_detail_level(soap_detail_level, word_count);
    let soap_opts = SoapOptions {
        detail_level: effective_detail,
        format: SoapFormat::from_config_str(soap_format),
        custom_instructions: soap_custom_instructions.to_string(),
        session_notes,
        ..Default::default()
    };
    let soap_system_prompt = build_simple_soap_prompt(&soap_opts, templates);
    // Built locally so all three replay-bundle outcome paths (Success / Failed /
    // Timeout) have user_prompt — the tokio::time::timeout Elapsed case can't
    // recover it from the cancelled future. Match `audio_events=None` and
    // `speaker_context=None` against the call below to keep the two in sync;
    // build_soap_user_content drops empty/whitespace session_notes itself.
    let soap_user_prompt = build_soap_user_content(
        filtered_text,
        None,
        Some(soap_opts.session_notes.as_str()),
        None,
    );

    let soap_start = Instant::now();
    let soap_future = client.generate_multi_patient_soap_note_timed(
        soap_model,
        filtered_text,
        None,
        Some(&soap_opts),
        None,
        multi_patient_detection,
        screenshot_paths,
        vision_model,
    );

    match tokio::time::timeout(tokio::time::Duration::from_secs(soap_timeout), soap_future).await {
        Ok((Ok(mut soap_result), call_metrics)) => {
            let latency_ms = soap_start.elapsed().as_millis() as u64;
            let content = soap_result.format_for_archive();

            // Multi-patient encounters auto-split into N sibling sessions, each
            // single-patient. Single-patient encounters take the legacy SOAP path.
            // If the sibling split fails (rare — disk error), fall back to the
            // legacy combined-SOAP layout so the encounter is still preserved
            // and clinicians can recover via the manual backfill button.
            let sibling_ids: Vec<String> = if soap_result.notes.len() > 1 {
                let date_str = session_date.format("%Y-%m-%d").to_string();
                let per_patient: Vec<(String, String)> = soap_result.notes.iter()
                    .map(|n| (n.patient_label.clone(), n.content.clone()))
                    .collect();
                match local_archive::split_into_siblings(session_id, &date_str, &per_patient) {
                    Ok(ids) => ids,
                    Err(e) => {
                        warn!(
                            session_id = %session_id,
                            error = %e,
                            "split_into_siblings failed; falling back to legacy combined-SOAP layout"
                        );
                        if let Err(e2) = local_archive::save_multi_patient_soap(
                            session_id,
                            session_date,
                            &soap_result.notes,
                            multi_patient_detection,
                        ) {
                            warn!("Legacy multi-patient SOAP fallback also failed for session {}: {}", session_id, e2);
                        }
                        Vec::new()
                    }
                }
            } else {
                if let Err(e) = local_archive::add_soap_note(
                    session_id,
                    session_date,
                    &content,
                    Some(effective_detail),
                    Some(soap_format),
                ) {
                    warn!("Failed to save SOAP for session {}: {}", session_id, e);
                }
                Vec::new()
            };

            if let Ok(mut l) = logger.lock() {
                let mut meta = log_extra;
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert("detail_level".into(), serde_json::json!(effective_detail));
                    obj.insert("format".into(), serde_json::json!(soap_format));
                    obj.insert(
                        "response_chars".into(),
                        serde_json::json!(content.len()),
                    );
                    obj.insert(
                        "patient_count".into(),
                        serde_json::json!(soap_result.notes.len()),
                    );
                }
                if let Some(m) = call_metrics {
                    m.attach_to(&mut meta);
                }
                l.log_soap(
                    soap_model,
                    &soap_system_prompt,
                    "",
                    Some(&content),
                    latency_ms,
                    true,
                    None,
                    meta,
                );
            }

            // Replay bundle: capture prompts + raw LLM JSON so
            // SOAP-experiment CLIs can replay through alternate prompts.
            if let Some(bundle_arc) = bundle {
                if let Ok(mut b) = bundle_arc.lock() {
                    let capture = build_soap_bundle_capture_success(
                        &mut soap_result,
                        &soap_system_prompt,
                        &content,
                        latency_ms,
                        word_count,
                    );
                    b.set_soap_result(capture);
                }
            }

            SoapGenerationOutcome::Success {
                result: soap_result,
                content,
                latency_ms,
                sibling_ids,
            }
        }
        Ok((Err(e), call_metrics)) => {
            let latency_ms = soap_start.elapsed().as_millis() as u64;
            if let Ok(mut l) = logger.lock() {
                let mut meta = log_extra;
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert("llm_error".into(), serde_json::json!(true));
                }
                if let Some(m) = call_metrics {
                    m.attach_to(&mut meta);
                }
                l.log_soap(
                    soap_model,
                    &soap_system_prompt,
                    "",
                    None,
                    latency_ms,
                    false,
                    Some(&e.to_string()),
                    meta,
                );
            }
            warn!("SOAP generation failed: {}", e);
            if let Some(bundle_arc) = bundle {
                if let Ok(mut b) = bundle_arc.lock() {
                    b.set_soap_result(crate::replay_bundle::SoapResult {
                        ts: Utc::now().to_rfc3339(),
                        latency_ms,
                        success: false,
                        word_count,
                        error: Some(e.to_string()),
                        patient_count: None,
                        system_prompt: Some(soap_system_prompt.clone()),
                        user_prompt: Some(soap_user_prompt.clone()),
                        response_raw: None,
                    });
                }
            }
            SoapGenerationOutcome::Failed {
                latency_ms,
                error: e.to_string(),
            }
        }
        Err(_) => {
            let latency_ms = soap_start.elapsed().as_millis() as u64;
            if let Ok(mut l) = logger.lock() {
                let mut meta = log_extra;
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert("timeout".into(), serde_json::json!(true));
                }
                l.log_soap(
                    soap_model,
                    &soap_system_prompt,
                    "",
                    None,
                    latency_ms,
                    false,
                    Some(SOAP_TIMEOUT_ERROR),
                    meta,
                );
            }
            warn!("SOAP generation timed out ({}s)", soap_timeout);
            if let Some(bundle_arc) = bundle {
                if let Ok(mut b) = bundle_arc.lock() {
                    b.set_soap_result(crate::replay_bundle::SoapResult {
                        ts: Utc::now().to_rfc3339(),
                        latency_ms,
                        success: false,
                        word_count,
                        error: Some(SOAP_TIMEOUT_ERROR.to_string()),
                        patient_count: None,
                        system_prompt: Some(soap_system_prompt.clone()),
                        user_prompt: Some(soap_user_prompt.clone()),
                        response_raw: None,
                    });
                }
            }
            SoapGenerationOutcome::Failed {
                latency_ms,
                error: SOAP_TIMEOUT_ERROR.to_string(),
            }
        }
    }
}

// ── Billing extraction ─────────────────────────────────────────────

/// Timeout for billing extraction LLM calls (seconds).
const BILLING_EXTRACTION_TIMEOUT_SECS: u64 = 300;

/// Extract billing codes from a completed SOAP note and save to archive.
///
/// Uses a two-stage approach:
/// 1. LLM extracts clinical features (constrained enums) from the SOAP content
/// 2. Deterministic rule engine maps features to OHIP billing codes
///
/// Fail-open: billing extraction errors are logged but never block encounter processing.
pub async fn extract_and_archive_billing(
    client: &LLMClient,
    model: &str,
    soap_content: &str,
    transcript: &str,
    context_hints: &str,
    session_id: &str,
    session_date: &DateTime<Utc>,
    duration_ms: u64,
    patient_name: Option<&str>,
    is_after_hours: bool,
    rule_ctx: &crate::billing::RuleEngineContext,
    logger: &Arc<Mutex<PipelineLogger>>,
    templates: Option<&PromptTemplates>,
    billing_data: Option<&crate::server_config::BillingData>,
    // Optional server-configured timeout override. `None` falls back to the
    // compiled `BILLING_EXTRACTION_TIMEOUT_SECS` constant.
    billing_timeout_override: Option<u64>,
    // Optional replay bundle. When provided, the function writes a
    // `BillingResult` with the captured system_prompt + user_prompt +
    // response_raw on every outcome path so billing-experiment CLIs can
    // replay the prompt without re-issuing the LLM call. Schema v5+.
    bundle: Option<&Arc<Mutex<crate::replay_bundle::ReplayBundleBuilder>>>,
) -> Result<crate::billing::BillingRecord, String> {
    use crate::billing::clinical_features::{build_billing_extraction_prompt, parse_billing_extraction};

    // 2026-04-30 Class D wiring: skip billing extraction entirely when SOAP
    // is non-substantive (all "Not documented" placeholders or similar).
    // Karin Smit's d7039e4c session generated A008A + Q310A + dx 311 against
    // a 522-word multi-language STT-noise transcript whose SOAP had every
    // section as "Not documented". An empty SOAP cannot support billing or
    // diagnostic-code resolution; emit a sentinel BillingRecord with an
    // explanatory note so downstream tools can audit.
    if !is_substantive_soap(soap_content) {
        info!(
            "[Class D] Skipping billing extraction for {} — SOAP is non-substantive (all-placeholder bullets)",
            session_id
        );
        if let Ok(mut l) = logger.lock() {
            l.log_event(
                "billing_skipped_empty_soap",
                serde_json::json!({
                    "session_id": session_id,
                    "reason": "non_substantive_soap",
                    "soap_chars": soap_content.len(),
                }),
            );
        }
        // Return an empty draft so callers don't write a billing.json
        return Err("non_substantive_soap".to_string());
    }

    let billing_timeout = billing_timeout_override.unwrap_or(BILLING_EXTRACTION_TIMEOUT_SECS);

    // Build prompt
    let (system_prompt, user_prompt) = build_billing_extraction_prompt(soap_content, transcript, context_hints, templates);

    // Call LLM with timeout
    let start = Instant::now();
    let result = tokio::time::timeout(
        tokio::time::Duration::from_secs(billing_timeout),
        client.generate_timed(model, &system_prompt, &user_prompt, "billing_extraction"),
    )
    .await;

    let latency_ms = start.elapsed().as_millis() as u64;

    // `call_metrics` is carried forward to the success-log path so per-step
    // scheduling_ms / network_ms / concurrent_at_start get attached to the
    // pipeline_log event.
    let (response, call_metrics) = match result {
        Ok((Ok(resp), m)) => (resp, m),
        Ok((Err(e), m)) => {
            warn!("Billing extraction LLM failed for {}: {}", session_id, e);
            if let Ok(mut l) = logger.lock() {
                let mut ctx = serde_json::json!({"session_id": session_id, "error": "llm_error"});
                m.attach_to(&mut ctx);
                l.log_llm_call(
                    "billing_extraction",
                    model,
                    &system_prompt,
                    &user_prompt,
                    None,
                    latency_ms,
                    false,
                    Some(&e.to_string()),
                    ctx,
                );
            }
            if let Some(bundle_arc) = bundle {
                if let Ok(mut b) = bundle_arc.lock() {
                    b.set_billing_result(crate::replay_bundle::BillingResult {
                        ts: Utc::now().to_rfc3339(),
                        latency_ms,
                        success: false,
                        codes_count: None,
                        total_amount_cents: None,
                        selected_codes: None,
                        error: Some(e.to_string()),
                        system_prompt: Some(system_prompt.clone()),
                        user_prompt: Some(user_prompt.clone()),
                        response_raw: None,
                    });
                }
            }
            return Err(format!("LLM error: {}", e));
        }
        Err(_) => {
            warn!("Billing extraction timed out for {} ({}s)", session_id, billing_timeout);
            if let Ok(mut l) = logger.lock() {
                l.log_llm_call(
                    "billing_extraction",
                    model,
                    &system_prompt,
                    &user_prompt,
                    None,
                    latency_ms,
                    false,
                    Some("timeout"),
                    serde_json::json!({"session_id": session_id, "error": "timeout"}),
                );
            }
            if let Some(bundle_arc) = bundle {
                if let Ok(mut b) = bundle_arc.lock() {
                    b.set_billing_result(crate::replay_bundle::BillingResult {
                        ts: Utc::now().to_rfc3339(),
                        latency_ms,
                        success: false,
                        codes_count: None,
                        total_amount_cents: None,
                        selected_codes: None,
                        error: Some("timeout".to_string()),
                        system_prompt: Some(system_prompt.clone()),
                        user_prompt: Some(user_prompt.clone()),
                        response_raw: None,
                    });
                }
            }
            return Err("Billing extraction timed out".to_string());
        }
    };

    // Parse clinical features from LLM response
    let mut features = parse_billing_extraction(&response)?;

    // Override after-hours from caller (more reliable than LLM)
    features.is_after_hours = is_after_hours;

    // SOAP-text keyword guard (v0.10.61). The LLM sometimes hallucinates
    // K-code conditions (diabetic_assessment, chf_management,
    // smoking_cessation) and even fabricates plausible evidence strings,
    // defeating the in-rule-engine evidence-text validator. Drop any such
    // condition whose required keyword is absent from the SOAP text itself.
    let (kept, dropped) = crate::billing::rule_engine::condition_keyword_guard(
        &features.conditions,
        soap_content,
    );
    if !dropped.is_empty() {
        warn!(
            "Billing extraction: dropped {} hallucinated condition(s) for {} — keyword absent from SOAP: {:?}",
            dropped.len(), session_id, dropped
        );
        for cond in &dropped {
            if let Some(key) = crate::billing::clinical_features::enum_to_snake_key(cond) {
                features.condition_evidence.remove(&key);
            }
        }
    }
    features.conditions = kept;

    // Population-specific visit-type guard (companion to condition guard
    // above) — drops prenatal/well-baby codes when the SOAP has no
    // matching keywords. See `visit_type_keyword_guard` for rationale.
    if let Some(downgraded_to) = crate::billing::rule_engine::visit_type_keyword_guard(
        &features.visit_type,
        soap_content,
    ) {
        warn!(
            "Billing extraction: downgraded {:?} → {:?} for {} — population keywords absent from SOAP",
            features.visit_type, downgraded_to, session_id
        );
        features.visit_type = downgraded_to;
    }

    // Resolve diagnostic code via tools-model + file_lookup retrieval (Stage 0).
    // Fails soft — `None` means the rule engine falls through to its existing
    // 4-stage pipeline. Primary motivation: fast-model confidently hallucinates
    // 3-digit OHIP codes it doesn't know (e.g., `315 developmental delays` for
    // fibromyalgia), whereas the 35B tools-model with a grounded reference
    // library is constrained to codes it can literally read back.
    let primary_for_tools = features.primary_diagnosis.clone().unwrap_or_default();
    let conditions_for_tools: Vec<String> = features
        .conditions
        .iter()
        .filter_map(crate::billing::clinical_features::enum_to_snake_key)
        .collect();
    let assessment_for_tools = extract_soap_assessment(soap_content);
    let tools_model_resolved = crate::billing::diagnostic_tools_model::resolve_via_tools_model(
        client,
        &primary_for_tools,
        &conditions_for_tools,
        &assessment_for_tools,
        logger,
        session_id,
    )
    .await;

    // Map features to billing codes via deterministic rule engine (with companion code context).
    let date_str = format!("{:04}-{:02}-{:02}", session_date.year(), session_date.month(), session_date.day());
    let mut record = crate::billing::map_features_to_billing_with_tools_model(
        &features,
        session_id,
        &date_str,
        duration_ms,
        patient_name,
        rule_ctx,
        billing_data,
        tools_model_resolved.as_ref(),
    );
    record.extraction_model = Some(model.to_string());
    record.suggestions = crate::billing::compute_upgrade_suggestions(&record, &features);

    local_archive::save_billing_record(session_id, session_date, &record)?;

    // Log success
    if let Ok(mut l) = logger.lock() {
        let mut ctx = serde_json::json!({
            "session_id": session_id,
            "codes_extracted": record.codes.len(),
            "total_cents": record.total_amount_cents,
            "total_shadow_cents": record.total_shadow_cents,
            "total_oob_cents": record.total_out_of_basket_cents,
            "total_time_cents": record.total_time_based_cents,
            "after_hours": is_after_hours,
            "clinical_features": serde_json::to_value(&features).unwrap_or_default(),
            "selected_codes": record.codes.iter().map(|c| c.code.clone()).collect::<Vec<_>>(),
            "time_entries": record.time_entries.iter().map(|t| serde_json::json!({
                "code": t.code,
                "minutes": t.minutes,
                "units": t.billable_units,
            })).collect::<Vec<_>>(),
        });
        call_metrics.attach_to(&mut ctx);
        l.log_llm_call(
            "billing_extraction",
            model,
            &system_prompt,
            &user_prompt,
            Some(&response),
            latency_ms,
            true,
            None,
            ctx,
        );
    }

    info!(
        session_id = %session_id,
        codes = record.codes.len(),
        total_cents = record.total_amount_cents,
        "Billing codes extracted and archived"
    );

    // Replay bundle: capture prompts + response so billing-experiment CLIs
    // can replay this encounter through alternate prompts. v5+.
    if let Some(bundle_arc) = bundle {
        if let Ok(mut b) = bundle_arc.lock() {
            b.set_billing_result(crate::replay_bundle::BillingResult {
                ts: Utc::now().to_rfc3339(),
                latency_ms,
                success: true,
                codes_count: Some(record.codes.len()),
                total_amount_cents: Some(record.total_amount_cents),
                selected_codes: Some(record.codes.iter().map(|c| c.code.clone()).collect()),
                error: None,
                system_prompt: Some(system_prompt.clone()),
                user_prompt: Some(user_prompt.clone()),
                response_raw: Some(response.clone()),
            });
        }
    }

    Ok(record)
}

/// Extract a single SOAP section between `start_marker` and `end_marker`
/// (or to end-of-string when no `end_marker`). Returns empty string when
/// `start_marker` is absent. Markers are sourced from `llm_client::SOAP_SECTION_*`
/// so renderer changes propagate. Tolerates marker variants with/without
/// trailing newline.
fn extract_soap_section(soap: &str, start_marker: &str, end_marker: Option<&str>) -> String {
    let start_trim = start_marker.trim_end_matches('\n');
    let Some(idx) = soap.find(start_marker).or_else(|| soap.find(start_trim)) else {
        return String::new();
    };
    let body_start = if soap[idx..].starts_with(start_marker) {
        idx + start_marker.len()
    } else {
        idx + start_trim.len()
    };
    let rest = &soap[body_start..];
    let end = match end_marker {
        Some(em) => rest.find(em.trim_end_matches('\n')).unwrap_or(rest.len()),
        None => rest.len(),
    };
    rest[..end].trim().to_string()
}

/// Extract the SOAP Assessment section, or fall back to the first ~1500
/// bytes when the marker is absent.
fn extract_soap_assessment(soap: &str) -> String {
    use crate::llm_client::{SOAP_SECTION_ASSESSMENT, SOAP_SECTION_PLAN};
    let body = extract_soap_section(soap, SOAP_SECTION_ASSESSMENT, Some(SOAP_SECTION_PLAN));
    if !body.is_empty() {
        return body;
    }
    let mut end = soap.len().min(1500);
    while end > 0 && !soap.is_char_boundary(end) {
        end -= 1;
    }
    soap[..end].to_string()
}

/// 2026-04-30 Class D: detect SOAPs that parsed but contain only
/// "Not documented" placeholders. Karin Smit's session generated
/// fabricated billing on this fingerprint.
pub fn is_substantive_soap(soap: &str) -> bool {
    const PLACEHOLDERS: &[&str] = &[
        "not documented", "not documented.", "[see transcript]",
        "(no documentation provided)", "no documentation",
        "n/a", "none", "none documented", "no significant findings",
    ];
    fn is_placeholder(line: &str) -> bool {
        let trimmed = line
            .trim_start_matches(|c: char| c == '•' || c == '-' || c == '*' || c.is_whitespace())
            .trim()
            .to_lowercase();
        if trimmed.is_empty() { return true; }
        PLACEHOLDERS.iter().any(|p| trimmed == *p)
    }
    use crate::llm_client::{
        SOAP_SECTION_ASSESSMENT, SOAP_SECTION_OBJECTIVE, SOAP_SECTION_PLAN,
        SOAP_SECTION_SUBJECTIVE,
    };
    let markers = [
        SOAP_SECTION_SUBJECTIVE,
        SOAP_SECTION_OBJECTIVE.trim_start(),
        SOAP_SECTION_ASSESSMENT.trim_start(),
        SOAP_SECTION_PLAN.trim_start(),
    ];
    let mut substantive_sections = 0u32;
    for (i, marker) in markers.iter().enumerate() {
        let Some(start) = soap.find(marker) else { continue };
        let body_start = start + marker.len();
        let next_marker_idx = markers
            .iter()
            .skip(i + 1)
            .filter_map(|m| soap[body_start..].find(m).map(|x| body_start + x))
            .min()
            .unwrap_or(soap.len());
        let body = &soap[body_start..next_marker_idx];
        let has_substantive = body.lines().any(|line| !is_placeholder(line));
        if has_substantive { substantive_sections += 1; }
    }
    substantive_sections >= 2
}

/// Determine if a session started during after-hours (Ontario EST/EDT).
///
/// After-hours = weekends all day, weekdays before 8 AM or after 5 PM Eastern.
pub fn is_after_hours(started_at: &DateTime<Utc>) -> bool {
    use chrono::Timelike;
    let eastern = started_at.with_timezone(&chrono_tz::America::New_York);
    let hour = eastern.hour();
    let weekday = eastern.weekday();
    matches!(weekday, chrono::Weekday::Sat | chrono::Weekday::Sun)
        || hour < 8
        || hour >= 17
}

// ── Orphaned billing recovery ──────────────────────────────────────

/// Recover sessions that have SOAP but no billing record.
///
/// Mirrors the `recover_orphaned_soap` pattern. Called on continuous mode stop.
pub async fn recover_orphaned_billing(
    client: &LLMClient,
    model: &str,
    logger: &Arc<Mutex<PipelineLogger>>,
) {
    let today_str = Utc::now().format("%Y-%m-%d").to_string();
    let sessions = match local_archive::list_sessions_by_date(&today_str) {
        Ok(s) => s,
        Err(_) => return,
    };

    let orphaned: Vec<_> = sessions
        .iter()
        .filter(|s| s.has_soap_note)
        .filter(|s| s.likely_non_clinical != Some(true))
        .collect();

    // Check each session for missing billing
    let mut recovered = 0;
    for summary in orphaned {
        let details = match local_archive::get_session(&summary.session_id, &today_str) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Parse the date for billing record lookup (summary.date is "YYYY-MM-DD")
        let session_date = chrono::NaiveDate::parse_from_str(&summary.date, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(12, 0, 0))
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
            .unwrap_or_else(Utc::now);

        // Skip if billing already exists
        if let Ok(Some(_)) = local_archive::get_billing_record(&summary.session_id, &session_date) {
            continue;
        }

        let soap = match details.soap_note {
            Some(ref s) => s.clone(),
            None => continue,
        };

        let transcript = details.transcript
            .as_deref()
            .unwrap_or("");

        let duration_ms = summary.duration_ms.unwrap_or(0);
        let after_hours = is_after_hours(&session_date);

        info!(
            "Recovering billing for session {} (has SOAP but no billing)",
            summary.session_id
        );

        if let Err(e) = extract_and_archive_billing(
            client,
            model,
            &soap,
            transcript,
            "", // no physician-provided context in recovery
            &summary.session_id,
            &session_date,
            duration_ms,
            summary.patient_name.as_deref(),
            after_hours,
            &crate::billing::RuleEngineContext::default(), // office default for recovery
            logger,
            None,
            None,
            None, // recovery path uses compiled default timeout
            None, // recovery path has no replay bundle
        ).await {
            warn!("Failed to recover billing for {}: {}", summary.session_id, e);
        } else {
            recovered += 1;
        }
    }

    if recovered > 0 {
        info!("Recovered billing for {} orphaned sessions", recovered);
    }
}

// ── Merge check ──────────────────────────────────────────────────────

/// Result of a merge check LLM call.
///
/// Contains all data needed for caller-side replay bundle logging.
pub struct MergeCheckOutcome {
    /// `Some(true)` = same encounter, `Some(false)` = different, `None` = call failed.
    pub same_encounter: Option<bool>,
    /// LLM-provided reason (from parsed response).
    pub reason: Option<String>,
    pub latency_ms: u64,
    /// Error message if the call failed, timed out, or couldn't be parsed.
    pub error: Option<String>,
    /// The merge prompt system message (for replay bundle logging).
    pub prompt_system: String,
    /// The merge prompt user message (for replay bundle logging).
    pub prompt_user: String,
    /// Raw LLM response (for replay bundle logging).
    pub response_raw: Option<String>,
}

/// Run an LLM merge check between two encounter excerpts.
///
/// Builds the merge prompt, calls the LLM with a 60s timeout, parses the
/// response, and logs all outcomes to the pipeline logger. Returns the full
/// outcome for caller-side replay bundle and day log integration.
pub async fn run_merge_check<'a>(
    client: &LLMClient,
    model: &str,
    prev: PrevMergeInput<'a>,
    curr_head: &str,
    patient_name: Option<&str>,
    logger: &Arc<Mutex<PipelineLogger>>,
    mut log_extra: serde_json::Value,
    templates: Option<&PromptTemplates>,
) -> MergeCheckOutcome {
    // Record which prev-side input was fed to the LLM so replay bundles and
    // pipeline_log rows are debuggable after the fact.
    if let Some(obj) = log_extra.as_object_mut() {
        obj.insert("prev_source".into(), serde_json::json!(prev.source_tag()));
        obj.insert(
            "prev_excerpt_chars".into(),
            serde_json::json!(prev.content().len()),
        );
    }

    let (merge_system, merge_user) =
        build_encounter_merge_prompt(prev, curr_head, patient_name, templates);
    let merge_start = Instant::now();
    let merge_future = client.generate_timed(model, &merge_system, &merge_user, "encounter_merge");

    match tokio::time::timeout(tokio::time::Duration::from_secs(60), merge_future).await {
        Ok((Ok(merge_response), m)) => {
            let latency_ms = merge_start.elapsed().as_millis() as u64;
            match parse_merge_check(&merge_response) {
                Ok(merge_result) => {
                    if let Ok(mut l) = logger.lock() {
                        let mut meta = log_extra;
                        if let Some(obj) = meta.as_object_mut() {
                            obj.insert(
                                "same_encounter".into(),
                                serde_json::json!(merge_result.same_encounter),
                            );
                            obj.insert(
                                "reason".into(),
                                serde_json::json!(format!("{:?}", merge_result.reason)),
                            );
                        }
                        m.attach_to(&mut meta);
                        l.log_merge_check(
                            model,
                            &merge_system,
                            &merge_user,
                            Some(&merge_response),
                            latency_ms,
                            true,
                            None,
                            meta,
                        );
                    }
                    MergeCheckOutcome {
                        same_encounter: Some(merge_result.same_encounter),
                        reason: merge_result.reason,
                        latency_ms,
                        error: None,
                        prompt_system: merge_system,
                        prompt_user: merge_user,
                        response_raw: Some(merge_response),
                    }
                }
                Err(e) => {
                    if let Ok(mut l) = logger.lock() {
                        let mut meta = log_extra;
                        m.attach_to(&mut meta);
                        l.log_merge_check(
                            model,
                            &merge_system,
                            &merge_user,
                            Some(&merge_response),
                            latency_ms,
                            false,
                            Some(&format!("parse_error: {}", e)),
                            meta,
                        );
                    }
                    warn!("Failed to parse merge check response: {}", e);
                    MergeCheckOutcome {
                        same_encounter: None,
                        reason: None,
                        latency_ms,
                        error: Some(format!("parse_error: {}", e)),
                        prompt_system: merge_system,
                        prompt_user: merge_user,
                        response_raw: Some(merge_response),
                    }
                }
            }
        }
        Ok((Err(e), m)) => {
            let latency_ms = merge_start.elapsed().as_millis() as u64;
            if let Ok(mut l) = logger.lock() {
                let mut meta = log_extra;
                m.attach_to(&mut meta);
                l.log_merge_check(
                    model,
                    &merge_system,
                    &merge_user,
                    None,
                    latency_ms,
                    false,
                    Some(&e.to_string()),
                    meta,
                );
            }
            warn!("Merge check LLM call failed: {}", e);
            MergeCheckOutcome {
                same_encounter: None,
                reason: None,
                latency_ms,
                error: Some(e.to_string()),
                prompt_system: merge_system,
                prompt_user: merge_user,
                response_raw: None,
            }
        }
        Err(_) => {
            let latency_ms = merge_start.elapsed().as_millis() as u64;
            if let Ok(mut l) = logger.lock() {
                l.log_merge_check(
                    model,
                    &merge_system,
                    &merge_user,
                    None,
                    latency_ms,
                    false,
                    Some("timeout_60s"),
                    log_extra,
                );
            }
            warn!("Merge check timed out (60s)");
            MergeCheckOutcome {
                same_encounter: None,
                reason: None,
                latency_ms,
                error: Some("timeout_60s".to_string()),
                prompt_system: merge_system,
                prompt_user: merge_user,
                response_raw: None,
            }
        }
    }
}

// ── Multi-patient detection helper ───────────────────────────────────

/// Run the pre-SOAP multi-patient detection LLM call, log the outcome, and
/// add the result to the replay bundle. Shared between the post-split path
/// and the flush-on-stop path so both produce identical pipeline_log
/// entries and replay_bundle records.
///
/// `stage_label` is logged as the `stage` key in the LLM context (e.g.
/// `"post_split"` or `"flush_on_stop"`) and surfaces in the multi-patient
/// detection record's debugging output.
pub async fn run_pre_soap_multi_patient_detection(
    client: &LLMClient,
    fast_model: &str,
    transcript: &str,
    word_count: usize,
    stage_label: &str,
    logger: &Arc<Mutex<PipelineLogger>>,
    bundle: &Arc<Mutex<crate::replay_bundle::ReplayBundleBuilder>>,
    // Optional deduped screenshots. When non-empty, routes the detect call
    // through `vision_model` for chart-aware multi-patient detection.
    screenshot_paths: Option<&[PathBuf]>,
    vision_model: &str,
) -> Option<MultiPatientDetectionResult> {
    let outcome = client
        .run_multi_patient_detection(fast_model, transcript, screenshot_paths, vision_model)
        .await;
    if let Ok(mut l) = logger.lock() {
        let mut det_context = match &outcome.detection {
            Some(d) => serde_json::json!({
                "stage": stage_label,
                "patient_count": d.patient_count,
                "confidence": d.confidence,
                "reasoning": d.reasoning,
                "patients": d.patients.iter()
                    .map(|p| serde_json::json!({"label": p.label, "summary": p.summary}))
                    .collect::<Vec<_>>(),
                "word_count": word_count,
            }),
            None => serde_json::json!({
                "stage": stage_label,
                "patient_count": 1,
                "word_count": word_count,
                "accepted": false,
            }),
        };
        if let Some(m) = outcome.call_metrics {
            m.attach_to(&mut det_context);
        }
        l.log_llm_call(
            "multi_patient_detect",
            &outcome.model,
            &outcome.system_prompt,
            &outcome.user_prompt,
            outcome.response_raw.as_deref(),
            outcome.latency_ms,
            outcome.success,
            outcome.error.as_deref(),
            det_context,
        );
    }
    if let Ok(mut b) = bundle.lock() {
        b.add_multi_patient_detection(crate::continuous_mode::multi_patient_from_outcome(
            &outcome,
            crate::replay_bundle::MultiPatientStage::PreSoap,
            word_count,
        ));
    }
    outcome.detection
}

// ── Clinical content check ───────────────────────────────────────────

/// Run the two-pass clinical content check.
///
/// Returns `true` if the encounter is clinical, `false` if non-clinical.
/// Fail-open: LLM errors or timeouts default to clinical (true).
///
/// `min_words_override`: when `Some`, replaces the compiled `MIN_WORDS_FOR_CLINICAL_CHECK`
/// constant — lets callers inject server-configured `DetectionThresholds.min_words_for_clinical_check`.
/// Pass `None` to use the compiled default (tests, fallback paths, or pre-Phase-2 callers).
pub async fn check_clinical_content(
    client: &LLMClient,
    model: &str,
    transcript: &str,
    word_count: usize,
    logger: &Arc<Mutex<PipelineLogger>>,
    log_extra: serde_json::Value,
    templates: Option<&PromptTemplates>,
    min_words_override: Option<usize>,
) -> bool {
    let min_words = min_words_override.unwrap_or(MIN_WORDS_FOR_CLINICAL_CHECK);
    if word_count < min_words {
        info!(
            "Encounter too small for clinical analysis ({} words < {} threshold) — treating as non-clinical",
            word_count, min_words
        );
        return false;
    }

    let (cc_system, cc_user) = build_clinical_content_check_prompt(transcript, templates);
    let cc_start = Instant::now();
    let cc_future = client.generate_timed(model, &cc_system, &cc_user, "clinical_content_check");

    match tokio::time::timeout(tokio::time::Duration::from_secs(CLINICAL_CONTENT_CHECK_TIMEOUT_SECS), cc_future).await {
        Ok((Ok(cc_response), m)) => {
            let cc_latency = cc_start.elapsed().as_millis() as u64;
            match parse_clinical_content_check(&cc_response) {
                Ok(cc_result) => {
                    if let Ok(mut l) = logger.lock() {
                        let mut meta = log_extra.clone();
                        if let Some(obj) = meta.as_object_mut() {
                            obj.insert(
                                "is_clinical".into(),
                                serde_json::json!(cc_result.clinical),
                            );
                            obj.insert("reason".into(), serde_json::json!(cc_result.reason));
                        }
                        m.attach_to(&mut meta);
                        l.log_clinical_check(
                            model,
                            &cc_system,
                            &cc_user,
                            Some(&cc_response),
                            cc_latency,
                            true,
                            None,
                            meta,
                        );
                    }
                    if !cc_result.clinical {
                        info!("Encounter flagged as non-clinical: {:?}", cc_result.reason);
                        return false;
                    }
                    info!("Encounter confirmed clinical: {:?}", cc_result.reason);
                    true
                }
                Err(e) => {
                    if let Ok(mut l) = logger.lock() {
                        let mut meta = log_extra;
                        if let Some(obj) = meta.as_object_mut() {
                            obj.insert("parse_error".into(), serde_json::json!(true));
                        }
                        m.attach_to(&mut meta);
                        l.log_clinical_check(
                            model,
                            &cc_system,
                            &cc_user,
                            Some(&cc_response),
                            cc_latency,
                            false,
                            Some(&e),
                            meta,
                        );
                    }
                    warn!("Failed to parse clinical content check: {}", e);
                    true // fail-open
                }
            }
        }
        Ok((Err(e), m)) => {
            let cc_latency = cc_start.elapsed().as_millis() as u64;
            if let Ok(mut l) = logger.lock() {
                let mut meta = log_extra;
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert("llm_error".into(), serde_json::json!(true));
                }
                m.attach_to(&mut meta);
                l.log_clinical_check(
                    model,
                    &cc_system,
                    &cc_user,
                    None,
                    cc_latency,
                    false,
                    Some(&e.to_string()),
                    meta,
                );
            }
            warn!("Clinical content check LLM call failed: {}", e);
            true // fail-open
        }
        Err(_) => {
            let cc_latency = cc_start.elapsed().as_millis() as u64;
            let timeout_tag = format!("timeout_{}s", CLINICAL_CONTENT_CHECK_TIMEOUT_SECS);
            if let Ok(mut l) = logger.lock() {
                let mut meta = log_extra;
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert("timeout".into(), serde_json::json!(true));
                }
                l.log_clinical_check(
                    model,
                    &cc_system,
                    &cc_user,
                    None,
                    cc_latency,
                    false,
                    Some(&timeout_tag),
                    meta,
                );
            }
            warn!("Clinical content check timed out ({}s)", CLINICAL_CONTENT_CHECK_TIMEOUT_SECS);
            true // fail-open
        }
    }
}

// ── Metadata helpers ─────────────────────────────────────────────────

/// Update metadata.json to mark a session as non-clinical.
pub fn mark_non_clinical(session_id: &str) {
    if let Ok(session_dir) = local_archive::get_session_archive_dir(session_id, &Utc::now()) {
        mark_non_clinical_at(&session_dir);
    }
}

/// Core implementation: mark a session as non-clinical given its directory path.
///
/// No-op if `metadata.json` does not exist or cannot be parsed.
pub fn mark_non_clinical_at(session_dir: &std::path::Path) {
    let meta_path = session_dir.join("metadata.json");
    if meta_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if let Ok(mut metadata) =
                serde_json::from_str::<local_archive::ArchiveMetadata>(&content)
            {
                metadata.likely_non_clinical = Some(true);
                if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                    let _ = std::fs::write(&meta_path, json);
                }
            }
        }
    }
}

// ── Orphaned SOAP recovery ──────────────────────────────────────────

/// Recover orphaned sessions that were archived but never got SOAP notes.
///
/// This happens when `detector_task.abort()` kills in-flight SOAP generation.
/// Scans today's sessions for `has_soap_note == false` and regenerates.
pub async fn recover_orphaned_soap(
    client: &LLMClient,
    soap_model: &str,
    soap_detail_level: u8,
    soap_format: &str,
    soap_custom_instructions: &str,
    logger: &Arc<Mutex<PipelineLogger>>,
    app: &tauri::AppHandle,
    sync_ctx: &crate::server_sync::ServerSyncContext,
    // Vision-capable model alias for orphan-SOAP recovery (see
    // `generate_and_archive_soap` `vision_model` docs). When the orphan
    // session has screenshots on disk, recovery goes through the vision path.
    vision_model: &str,
) {
    let today_str = Utc::now().format("%Y-%m-%d").to_string();
    let sessions = match local_archive::list_sessions_by_date(&today_str) {
        Ok(s) => s,
        Err(_) => return,
    };

    let orphaned: Vec<_> = sessions
        .iter()
        .filter(|s| !s.has_soap_note && s.word_count > 100)
        .filter(|s| s.likely_non_clinical != Some(true))
        .collect();

    if orphaned.is_empty() {
        return;
    }
    info!(
        "Found {} orphaned sessions without SOAP notes, recovering",
        orphaned.len()
    );

    for summary in orphaned {
        let details = match local_archive::get_session(&summary.session_id, &today_str) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let transcript = match details.transcript {
            Some(ref t) => t,
            None => continue,
        };

        let (filtered_text, _) = strip_hallucinations(transcript, 5);
        let word_count = filtered_text.split_whitespace().count();
        if word_count < 100 {
            info!(
                "Orphaned session {} has only {} words after filtering, skipping SOAP",
                summary.session_id, word_count
            );
            continue;
        }

        info!(
            "Generating SOAP for orphaned session {} ({} words)",
            summary.session_id, word_count
        );

        // Use session's original date to avoid midnight routing errors
        let soap_date = chrono::DateTime::parse_from_rfc3339(&summary.date)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        let orphan_deduped = crate::screenshot_dedup::load_deduped_screenshots_for_session(
            &summary.session_id,
            &soap_date,
        );
        let orphan_screenshot_arg = Some(orphan_deduped.as_slice());

        let outcome = generate_and_archive_soap(
            client,
            soap_model,
            &filtered_text,
            &summary.session_id,
            &soap_date,
            soap_detail_level,
            soap_format,
            soap_custom_instructions,
            String::new(),
            word_count,
            None,
            logger,
            serde_json::json!({
                "stage": "orphaned_soap_recovery",
                "session_id": summary.session_id,
                "word_count": word_count,
            }),
            None,
            None, // recovery path uses compiled default timeout
            None, // orphan recovery has no replay bundle
            orphan_screenshot_arg,
            vision_model,
        )
        .await;

        match outcome {
            SoapGenerationOutcome::Success { .. } => {
                info!(
                    "Recovered SOAP for orphaned session {}",
                    summary.session_id
                );
                // Sync recovered SOAP to server
                sync_ctx.sync_session(&summary.session_id, &today_str);
                ContinuousModeEvent::SoapGenerated {
                    session_id: summary.session_id.clone(),
                    patient_count: None,
                    recovered: Some(true),
                }.emit(app);
            }
            SoapGenerationOutcome::Failed { ref error, .. } => {
                warn!(
                    "Failed to recover SOAP for {}: {}",
                    summary.session_id, error
                );
            }
        }
    }
}

// ── Merge-execute helpers ──────────────────────────────────────────────

/// Clear the `likely_non_clinical` flag on a session's metadata.
///
/// Used after merging a clinical and non-clinical encounter — the merged
/// result should not be flagged non-clinical.
pub fn clear_non_clinical_flag(session_id: &str, session_date: &DateTime<Utc>) {
    let session_dir = match local_archive::get_session_archive_dir(session_id, session_date) {
        Ok(d) => d,
        Err(e) => {
            warn!("clear_non_clinical_flag: failed to resolve session dir for {}: {}", session_id, e);
            return;
        }
    };
    clear_non_clinical_flag_at(&session_dir);
}

/// Core implementation: clear the non-clinical flag given a session directory path.
///
/// No-op if `metadata.json` does not exist or cannot be parsed, or if
/// `likely_non_clinical` is already `None`.
pub fn clear_non_clinical_flag_at(session_dir: &std::path::Path) {
    let meta_path = session_dir.join("metadata.json");
    let content = match std::fs::read_to_string(&meta_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    match serde_json::from_str::<local_archive::ArchiveMetadata>(&content) {
        Ok(mut metadata) => {
            metadata.likely_non_clinical = None;
            match serde_json::to_string_pretty(&metadata) {
                Ok(json) => {
                    let _ = std::fs::write(&meta_path, json);
                }
                Err(e) => warn!("clear_non_clinical_flag_at: failed to serialize metadata: {}", e),
            }
        }
        Err(e) => warn!("clear_non_clinical_flag_at: failed to parse metadata: {}", e),
    }
}

/// Regenerate SOAP for a surviving session after a merge.
///
/// Strips hallucinations from the merged text, generates a new SOAP note,
/// syncs to server, and clears the non-clinical flag if clinical status
/// differs between the merged encounters. Returns true if SOAP was generated.
pub async fn regen_soap_after_merge(
    client: &LLMClient,
    merged_text: &str,
    surviving_session_id: &str,
    surviving_date: &DateTime<Utc>,
    // Vision-extracted patient name on the surviving session — needed so the
    // re-extracted billing.json keeps patientName populated rather than going null.
    surviving_patient_name: Option<&str>,
    soap_model: &str,
    // Vision-capable model alias for the merge-regen SOAP path (see
    // `generate_and_archive_soap` `vision_model` docs).
    vision_model: &str,
    soap_detail_level: u8,
    soap_format: &str,
    soap_custom_instructions: &str,
    notes: String,
    prev_is_clinical: bool,
    curr_is_clinical: bool,
    logger: &Arc<Mutex<PipelineLogger>>,
    sync_ctx: &crate::server_sync::ServerSyncContext,
    log_stage: &str,
    // Billing extraction context (pass fast_model for LLM extraction)
    billing_fast_model: Option<&str>,
    billing_duration_ms: u64,
    billing_counselling_exhausted: bool,
    billing_templates: Option<&PromptTemplates>,
    billing_data: Option<&crate::server_config::BillingData>,
) -> bool {
    if !(prev_is_clinical || curr_is_clinical) {
        info!("Skipping SOAP regeneration for merged non-clinical encounters");
        return false;
    }

    let (filtered_merged, _) = strip_hallucinations(merged_text, 5);
    let filtered_wc = filtered_merged.split_whitespace().count();

    // Re-point the logger at the SURVIVING session's directory before regen calls.
    // Without this, the logger still points at the merged-away encounter's directory
    // (which is about to be deleted as part of the merge), so the merge-regen SOAP
    // and billing pipeline_log events get written to a deleted path and lost.
    // Observed during the Apr 16 2026 Room 6 audit for the Wicks/3eaa2d79 merge.
    if let Ok(surviving_dir) = crate::local_archive::get_session_archive_dir(surviving_session_id, surviving_date) {
        if let Ok(mut l) = logger.lock() {
            l.set_session(&surviving_dir);
        }
    }

    // Dedup the surviving session's screenshots for the merge-regen SOAP call.
    let merge_deduped = crate::screenshot_dedup::load_deduped_screenshots_for_session(
        surviving_session_id,
        surviving_date,
    );
    let merge_screenshot_arg = Some(merge_deduped.as_slice());

    let outcome = generate_and_archive_soap(
        client,
        soap_model,
        &filtered_merged,
        surviving_session_id,
        surviving_date,
        soap_detail_level,
        soap_format,
        soap_custom_instructions,
        notes,
        filtered_wc,
        None,
        logger,
        serde_json::json!({
            "stage": log_stage,
            "merged_into": surviving_session_id,
        }),
        None,
        None, // merge-regen uses compiled default timeout; caller could thread from ServerConfig if desired
        None, // merge-regen path: bundle of merging-into session is finalized later via build_merged_and_reset
        merge_screenshot_arg,
        vision_model,
    )
    .await;

    if let SoapGenerationOutcome::Success { ref content, .. } = outcome {
        sync_ctx.sync_soap(surviving_session_id, content, soap_detail_level, soap_format);
        if prev_is_clinical != curr_is_clinical {
            clear_non_clinical_flag(surviving_session_id, surviving_date);
        }
        info!("Regenerated SOAP for merged encounter {}", surviving_session_id);

        // Extract billing for the merged encounter (fail-open)
        if let Some(fast_model) = billing_fast_model {
            let after_hours = is_after_hours(&Utc::now());
            let rule_ctx = crate::billing::RuleEngineContext {
                counselling_exhausted: billing_counselling_exhausted,
                transcript: Some(filtered_merged.clone()),
                ..Default::default()
            };
            match extract_and_archive_billing(
                client, fast_model, content, &filtered_merged, "",
                surviving_session_id, surviving_date, billing_duration_ms,
                surviving_patient_name, after_hours, &rule_ctx, logger,
                billing_templates, billing_data,
                None, // merge-path billing uses compiled default timeout
                None, // merge-path: bundle finalized via build_merged_and_reset
            ).await {
                Ok(record) => info!(
                    "Billing extracted after merge for {} ({} codes)",
                    surviving_session_id, record.codes.len()
                ),
                Err(e) => warn!("Billing extraction failed after merge for {}: {e}", surviving_session_id),
            }
        }

        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::continuous_mode_events::ContinuousModeEvent;

    // ====== 2026-04-30 forensic review validation tests ======

    #[test]
    fn class_d_2026_04_30_karin_empty_soap_not_substantive() {
        let soap = "S:\n• Not documented\n\n\
                    O:\n• Not documented\n\n\
                    A:\n• Not documented\n\n\
                    P:\n• Not documented";
        assert!(!is_substantive_soap(soap));
    }

    #[test]
    fn class_d_2026_04_30_real_soap_is_substantive() {
        let soap = "S:\n• Patient reports knee pain\n\n\
                    O:\n• Not documented\n\n\
                    A:\n• Bilateral knee OA\n\n\
                    P:\n• Scheduled blood draw for PRP";
        assert!(is_substantive_soap(soap));
    }

    // ====== end 2026-04-30 validation tests ======

    // ── Helper: write a minimal ArchiveMetadata JSON to a temp dir ──

    fn write_metadata(dir: &std::path::Path, metadata: &local_archive::ArchiveMetadata) {
        let json = serde_json::to_string_pretty(metadata).unwrap();
        std::fs::write(dir.join("metadata.json"), json).unwrap();
    }

    fn read_metadata(dir: &std::path::Path) -> local_archive::ArchiveMetadata {
        let content = std::fs::read_to_string(dir.join("metadata.json")).unwrap();
        serde_json::from_str(&content).unwrap()
    }

    fn sample_metadata() -> local_archive::ArchiveMetadata {
        local_archive::ArchiveMetadata {
            session_id: "test-session".into(),
            started_at: "2026-03-26T10:00:00Z".into(),
            ended_at: Some("2026-03-26T10:15:00Z".into()),
            duration_ms: Some(900_000),
            segment_count: 50,
            word_count: 500,
            has_soap_note: true,
            has_audio: false,
            auto_ended: false,
            auto_end_reason: None,
            soap_detail_level: Some(5),
            soap_format: Some("problem_based".into()),
            charting_mode: Some("continuous".into()),
            encounter_number: Some(3),
            patient_name: Some("Jane Doe".into()),
            patient_dob: None,
            detection_method: Some("llm".into()),
            shadow_comparison: None,
            likely_non_clinical: None,
            patient_count: None,
            physician_id: None,
            physician_name: None,
            room_name: None,
            has_patient_handout: None,
            has_billing_record: None,
            patient_confirmed_at: None,
            medplum_patient_id: None,
            has_clinician_notes: false,
            soap_prompt_version: None,
            billing_prompt_version: None,
            sibling_group_id: None,
            sibling_index: None,
            sibling_group_size: None,
        }
    }

    // ── mark_non_clinical_at tests ──

    #[test]
    fn mark_non_clinical_at_sets_flag() {
        let dir = tempfile::tempdir().unwrap();
        let mut meta = sample_metadata();
        meta.likely_non_clinical = None;
        write_metadata(dir.path(), &meta);

        mark_non_clinical_at(dir.path());

        let result = read_metadata(dir.path());
        assert_eq!(result.likely_non_clinical, Some(true));
    }

    #[test]
    fn mark_non_clinical_at_noop_without_metadata() {
        let dir = tempfile::tempdir().unwrap();
        // No metadata.json — should not panic
        mark_non_clinical_at(dir.path());
        assert!(!dir.path().join("metadata.json").exists());
    }

    #[test]
    fn mark_non_clinical_at_preserves_fields() {
        let dir = tempfile::tempdir().unwrap();
        let meta = sample_metadata();
        write_metadata(dir.path(), &meta);

        mark_non_clinical_at(dir.path());

        let result = read_metadata(dir.path());
        assert_eq!(result.patient_name, Some("Jane Doe".into()));
        assert_eq!(result.encounter_number, Some(3));
        assert_eq!(result.charting_mode, Some("continuous".into()));
        assert_eq!(result.session_id, "test-session");
        assert_eq!(result.word_count, 500);
    }

    #[test]
    fn mark_non_clinical_at_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let mut meta = sample_metadata();
        meta.likely_non_clinical = Some(true);
        write_metadata(dir.path(), &meta);

        mark_non_clinical_at(dir.path());

        let result = read_metadata(dir.path());
        assert_eq!(result.likely_non_clinical, Some(true));
    }

    // ── clear_non_clinical_flag_at tests ──

    #[test]
    fn clear_non_clinical_flag_at_clears_flag() {
        let dir = tempfile::tempdir().unwrap();
        let mut meta = sample_metadata();
        meta.likely_non_clinical = Some(true);
        write_metadata(dir.path(), &meta);

        clear_non_clinical_flag_at(dir.path());

        let result = read_metadata(dir.path());
        assert_eq!(result.likely_non_clinical, None);
    }

    #[test]
    fn clear_non_clinical_flag_at_noop_when_already_none() {
        let dir = tempfile::tempdir().unwrap();
        let mut meta = sample_metadata();
        meta.likely_non_clinical = None;
        write_metadata(dir.path(), &meta);

        clear_non_clinical_flag_at(dir.path());

        let result = read_metadata(dir.path());
        assert_eq!(result.likely_non_clinical, None);
    }

    #[test]
    fn clear_non_clinical_flag_at_noop_without_metadata() {
        let dir = tempfile::tempdir().unwrap();
        // No metadata.json — should not panic
        clear_non_clinical_flag_at(dir.path());
        assert!(!dir.path().join("metadata.json").exists());
    }

    #[test]
    fn clear_non_clinical_flag_at_preserves_fields() {
        let dir = tempfile::tempdir().unwrap();
        let mut meta = sample_metadata();
        meta.likely_non_clinical = Some(true);
        write_metadata(dir.path(), &meta);

        clear_non_clinical_flag_at(dir.path());

        let result = read_metadata(dir.path());
        assert_eq!(result.patient_name, Some("Jane Doe".into()));
        assert_eq!(result.encounter_number, Some(3));
        assert_eq!(result.charting_mode, Some("continuous".into()));
        assert_eq!(result.session_id, "test-session");
        assert_eq!(result.word_count, 500);
    }

    // ── Constant verification ──

    #[test]
    fn min_words_for_clinical_check_is_100() {
        assert_eq!(MIN_WORDS_FOR_CLINICAL_CHECK, 100);
    }

    // ── Type construction tests ──

    #[test]
    fn build_soap_bundle_capture_carries_user_prompt_and_response() {
        let mut soap_result = crate::llm_client::MultiPatientSoapResult {
            notes: vec![crate::llm_client::PatientSoapNote {
                patient_label: "Combined".into(),
                speaker_id: "All".into(),
                content: "S: Headache".into(),
            }],
            physician_speaker: None,
            generated_at: "2026-05-08T00:00:00Z".into(),
            model_used: "soap-model-fast".into(),
            raw_response: Some(r#"{"subjective":["headache"]}"#.into()),
            user_prompt: Some("TRANSCRIPT:\nDr Z (77%): How is your head?".into()),
        };

        let captured = build_soap_bundle_capture_success(
            &mut soap_result,
            "you are a medical scribe...",
            "S: Headache",
            5_000,
            42,
        );

        assert_eq!(captured.success, true);
        assert_eq!(captured.latency_ms, 5_000);
        assert_eq!(captured.word_count, 42);
        assert_eq!(captured.system_prompt.as_deref(), Some("you are a medical scribe..."));
        assert_eq!(
            captured.user_prompt.as_deref(),
            Some("TRANSCRIPT:\nDr Z (77%): How is your head?"),
        );
        assert_eq!(
            captured.response_raw.as_deref(),
            Some(r#"{"subjective":["headache"]}"#)
        );
        assert!(soap_result.user_prompt.is_none());
        assert!(soap_result.raw_response.is_none());
    }

    #[test]
    fn build_soap_bundle_capture_falls_back_to_content_when_raw_missing() {
        let mut soap_result = crate::llm_client::MultiPatientSoapResult {
            notes: vec![],
            physician_speaker: None,
            generated_at: "2026-05-08T00:00:00Z".into(),
            model_used: "soap-model-fast".into(),
            raw_response: None,
            user_prompt: Some("TRANSCRIPT:\nx".into()),
        };
        let captured = build_soap_bundle_capture_success(
            &mut soap_result,
            "sys",
            "FORMATTED CONTENT",
            1,
            1,
        );
        assert_eq!(captured.response_raw.as_deref(), Some("FORMATTED CONTENT"));
        assert_eq!(captured.user_prompt.as_deref(), Some("TRANSCRIPT:\nx"));
    }

    #[test]
    fn soap_generation_outcome_success_fields() {
        let result = crate::llm_client::MultiPatientSoapResult {
            notes: vec![],
            physician_speaker: None,
            generated_at: "2026-03-26T10:00:00Z".into(),
            model_used: "soap-model-fast".into(),
            raw_response: None,
            user_prompt: None,
        };
        let outcome = SoapGenerationOutcome::Success {
            result,
            content: "test content".into(),
            latency_ms: 1500,
            sibling_ids: Vec::new(),
        };
        match outcome {
            SoapGenerationOutcome::Success { content, latency_ms, .. } => {
                assert_eq!(content, "test content");
                assert_eq!(latency_ms, 1500);
            }
            _ => panic!("Expected Success variant"),
        }
    }

    #[test]
    fn soap_generation_outcome_failed_fields() {
        let outcome = SoapGenerationOutcome::Failed {
            latency_ms: 120_000,
            error: "timeout_120s".into(),
        };
        match outcome {
            SoapGenerationOutcome::Failed { latency_ms, error } => {
                assert_eq!(latency_ms, 120_000);
                assert_eq!(error, "timeout_120s");
            }
            _ => panic!("Expected Failed variant"),
        }
    }

    #[test]
    fn merge_check_outcome_construction() {
        let outcome = MergeCheckOutcome {
            same_encounter: Some(true),
            reason: Some("same patient".into()),
            latency_ms: 800,
            error: None,
            prompt_system: "system".into(),
            prompt_user: "user".into(),
            response_raw: Some("raw".into()),
        };
        assert_eq!(outcome.same_encounter, Some(true));
        assert_eq!(outcome.reason.as_deref(), Some("same patient"));
        assert_eq!(outcome.latency_ms, 800);
        assert!(outcome.error.is_none());
    }

    #[test]
    fn merge_check_outcome_failed() {
        let outcome = MergeCheckOutcome {
            same_encounter: None,
            reason: None,
            latency_ms: 60_000,
            error: Some("timeout_60s".into()),
            prompt_system: "system".into(),
            prompt_user: "user".into(),
            response_raw: None,
        };
        assert!(outcome.same_encounter.is_none());
        assert_eq!(outcome.error.as_deref(), Some("timeout_60s"));
    }

    // ── ContinuousModeEvent serialization tests (encounter_pipeline-specific variants) ──

    #[test]
    fn event_soap_generated_matches_inline_json() {
        // Mirrors: json!({"type": "soap_generated", "session_id": ..., "patient_count": ...})
        let event = ContinuousModeEvent::SoapGenerated {
            session_id: "sess-001".into(),
            patient_count: Some(2),
            recovered: None,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        let expected = serde_json::json!({
            "type": "soap_generated",
            "session_id": "sess-001",
            "patient_count": 2,
        });
        assert_eq!(json, expected);
    }

    #[test]
    fn event_soap_generated_recovered_matches_inline_json() {
        // Mirrors: json!({"type": "soap_generated", "session_id": ..., "recovered": true})
        let event = ContinuousModeEvent::SoapGenerated {
            session_id: "sess-orphan".into(),
            patient_count: None,
            recovered: Some(true),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        let expected = serde_json::json!({
            "type": "soap_generated",
            "session_id": "sess-orphan",
            "recovered": true,
        });
        assert_eq!(json, expected);
    }

    #[test]
    fn event_soap_failed_matches_inline_json() {
        let event = ContinuousModeEvent::SoapFailed {
            session_id: "sess-fail".into(),
            error: "timeout_120s".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        let expected = serde_json::json!({
            "type": "soap_failed",
            "session_id": "sess-fail",
            "error": "timeout_120s",
        });
        assert_eq!(json, expected);
    }

    #[test]
    fn event_encounter_merged_with_kept_matches_inline_json() {
        let event = ContinuousModeEvent::EncounterMerged {
            kept_session_id: Some("prev-123".into()),
            merged_into_session_id: None,
            removed_session_id: "curr-456".into(),
            reason: Some("small orphan (150 words) with sensor present".into()),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        let expected = serde_json::json!({
            "type": "encounter_merged",
            "kept_session_id": "prev-123",
            "removed_session_id": "curr-456",
            "reason": "small orphan (150 words) with sensor present",
        });
        assert_eq!(json, expected);
    }

    #[test]
    fn event_encounter_merged_with_merged_into_matches_inline_json() {
        let event = ContinuousModeEvent::EncounterMerged {
            kept_session_id: None,
            merged_into_session_id: Some("prev-789".into()),
            removed_session_id: "curr-012".into(),
            reason: None,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        let expected = serde_json::json!({
            "type": "encounter_merged",
            "merged_into_session_id": "prev-789",
            "removed_session_id": "curr-012",
        });
        assert_eq!(json, expected);
    }

    #[test]
    fn event_encounter_detected_matches_inline_json() {
        let event = ContinuousModeEvent::EncounterDetected {
            session_id: "sess-new".into(),
            word_count: 750,
            patient_name: Some("John Smith".into()),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        let expected = serde_json::json!({
            "type": "encounter_detected",
            "session_id": "sess-new",
            "word_count": 750,
            "patient_name": "John Smith",
        });
        assert_eq!(json, expected);
    }
}
