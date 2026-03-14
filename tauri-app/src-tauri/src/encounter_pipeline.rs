//! Shared encounter pipeline helpers for continuous mode.
//!
//! These functions are used by both the main detector loop and the
//! flush-on-stop path, eliminating duplication of clinical content checks,
//! non-clinical metadata updates, SOAP generation, merge checks, and
//! orphaned SOAP recovery.

use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{info, warn};

use crate::encounter_detection::{
    build_clinical_content_check_prompt, parse_clinical_content_check,
    MultiPatientDetectionResult, MIN_WORDS_FOR_CLINICAL_CHECK,
};
use crate::encounter_experiment::strip_hallucinations;
use crate::encounter_merge::{build_encounter_merge_prompt, parse_merge_check};
use crate::llm_client::{
    build_simple_soap_prompt, LLMClient, MultiPatientSoapResult, SoapFormat, SoapOptions,
};
use crate::local_archive;
use crate::pipeline_log::PipelineLogger;
use tauri::Emitter;

use crate::continuous_mode::effective_soap_detail_level;

// ── SOAP generation ──────────────────────────────────────────────────

/// Result of a SOAP generation + archive attempt.
pub enum SoapGenerationOutcome {
    /// SOAP generated and archived successfully.
    Success {
        result: MultiPatientSoapResult,
        content: String,
        latency_ms: u64,
    },
    /// LLM call failed or timed out.
    Failed { latency_ms: u64, error: String },
}

/// Generate a SOAP note, archive it, and log the result.
///
/// Handles the full LLM call → 120s timeout → archive → pipeline log pattern
/// shared across primary SOAP, merge SOAP regen, flush SOAP, and orphaned
/// recovery. Callers handle site-specific side effects (replay bundle, day log,
/// frontend events, error state) by matching on the returned outcome.
///
/// `log_extra` should contain caller-specific metadata (e.g. stage, session_id,
/// encounter_number). The function auto-adds `detail_level`, `format`,
/// `response_chars`, and `patient_count`.
pub async fn generate_and_archive_soap(
    client: &LLMClient,
    soap_model: &str,
    filtered_text: &str,
    session_id: &str,
    session_date: &DateTime<Utc>,
    soap_detail_level: u8,
    soap_format: &str,
    session_notes: String,
    word_count: usize,
    multi_patient_detection: Option<&MultiPatientDetectionResult>,
    logger: &Arc<Mutex<PipelineLogger>>,
    log_extra: serde_json::Value,
) -> SoapGenerationOutcome {
    let effective_detail = effective_soap_detail_level(soap_detail_level, word_count);
    let soap_opts = SoapOptions {
        detail_level: effective_detail,
        format: SoapFormat::from_config_str(soap_format),
        session_notes,
        ..Default::default()
    };
    let soap_system_prompt = build_simple_soap_prompt(&soap_opts);

    let soap_start = Instant::now();
    let soap_future = client.generate_multi_patient_soap_note(
        soap_model,
        filtered_text,
        None,
        Some(&soap_opts),
        None,
        multi_patient_detection,
    );

    match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
        Ok(Ok(soap_result)) => {
            let latency_ms = soap_start.elapsed().as_millis() as u64;
            let content = soap_result.format_for_archive();

            if let Err(e) = local_archive::add_soap_note(
                session_id,
                session_date,
                &content,
                Some(effective_detail),
                Some(soap_format),
            ) {
                warn!("Failed to save SOAP for session {}: {}", session_id, e);
            }

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

            SoapGenerationOutcome::Success {
                result: soap_result,
                content,
                latency_ms,
            }
        }
        Ok(Err(e)) => {
            let latency_ms = soap_start.elapsed().as_millis() as u64;
            if let Ok(mut l) = logger.lock() {
                let mut meta = log_extra;
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert("llm_error".into(), serde_json::json!(true));
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
                    Some("timeout_120s"),
                    meta,
                );
            }
            warn!("SOAP generation timed out (120s)");
            SoapGenerationOutcome::Failed {
                latency_ms,
                error: "timeout_120s".to_string(),
            }
        }
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
pub async fn run_merge_check(
    client: &LLMClient,
    model: &str,
    prev_tail: &str,
    curr_head: &str,
    patient_name: Option<&str>,
    logger: &Arc<Mutex<PipelineLogger>>,
    log_extra: serde_json::Value,
) -> MergeCheckOutcome {
    let (merge_system, merge_user) =
        build_encounter_merge_prompt(prev_tail, curr_head, patient_name);
    let merge_start = Instant::now();
    let merge_future = client.generate(model, &merge_system, &merge_user, "encounter_merge");

    match tokio::time::timeout(tokio::time::Duration::from_secs(60), merge_future).await {
        Ok(Ok(merge_response)) => {
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
                        l.log_merge_check(
                            model,
                            &merge_system,
                            &merge_user,
                            Some(&merge_response),
                            latency_ms,
                            false,
                            Some(&format!("parse_error: {}", e)),
                            log_extra,
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
        Ok(Err(e)) => {
            let latency_ms = merge_start.elapsed().as_millis() as u64;
            if let Ok(mut l) = logger.lock() {
                l.log_merge_check(
                    model,
                    &merge_system,
                    &merge_user,
                    None,
                    latency_ms,
                    false,
                    Some(&e.to_string()),
                    log_extra,
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

// ── Clinical content check ───────────────────────────────────────────

/// Run the two-pass clinical content check.
///
/// Returns `true` if the encounter is clinical, `false` if non-clinical.
/// Fail-open: LLM errors or timeouts default to clinical (true).
pub async fn check_clinical_content(
    client: &LLMClient,
    model: &str,
    transcript: &str,
    word_count: usize,
    logger: &Arc<Mutex<PipelineLogger>>,
    log_extra: serde_json::Value,
) -> bool {
    if word_count < MIN_WORDS_FOR_CLINICAL_CHECK {
        info!(
            "Encounter too small for clinical analysis ({} words < {} threshold) — treating as non-clinical",
            word_count, MIN_WORDS_FOR_CLINICAL_CHECK
        );
        return false;
    }

    let (cc_system, cc_user) = build_clinical_content_check_prompt(transcript);
    let cc_start = Instant::now();
    let cc_future = client.generate(model, &cc_system, &cc_user, "clinical_content_check");

    match tokio::time::timeout(tokio::time::Duration::from_secs(30), cc_future).await {
        Ok(Ok(cc_response)) => {
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
        Ok(Err(e)) => {
            let cc_latency = cc_start.elapsed().as_millis() as u64;
            if let Ok(mut l) = logger.lock() {
                let mut meta = log_extra;
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert("llm_error".into(), serde_json::json!(true));
                }
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
                    Some("timeout_30s"),
                    meta,
                );
            }
            warn!("Clinical content check timed out (30s)");
            true // fail-open
        }
    }
}

// ── Metadata helpers ─────────────────────────────────────────────────

/// Update metadata.json to mark a session as non-clinical.
pub fn mark_non_clinical(session_id: &str) {
    if let Ok(session_dir) = local_archive::get_session_archive_dir(session_id, &Utc::now()) {
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
    logger: &Arc<Mutex<PipelineLogger>>,
    app: &tauri::AppHandle,
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

        let outcome = generate_and_archive_soap(
            client,
            soap_model,
            &filtered_text,
            &summary.session_id,
            &soap_date,
            soap_detail_level,
            soap_format,
            String::new(),
            word_count,
            None,
            logger,
            serde_json::json!({
                "stage": "orphaned_soap_recovery",
                "session_id": summary.session_id,
                "word_count": word_count,
            }),
        )
        .await;

        match outcome {
            SoapGenerationOutcome::Success { .. } => {
                info!(
                    "Recovered SOAP for orphaned session {}",
                    summary.session_id
                );
                let _ = app.emit(
                    "continuous_mode_event",
                    serde_json::json!({
                        "type": "soap_generated",
                        "session_id": summary.session_id,
                        "recovered": true,
                    }),
                );
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
