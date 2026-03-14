//! Shared encounter pipeline helpers for continuous mode.
//!
//! These functions are used by both the main detector loop and the
//! flush-on-stop path, eliminating duplication of clinical content checks,
//! non-clinical metadata updates, and orphaned SOAP recovery.

use chrono::Utc;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{info, warn};

use crate::encounter_detection::{
    build_clinical_content_check_prompt, parse_clinical_content_check, MIN_WORDS_FOR_CLINICAL_CHECK,
};
use crate::encounter_experiment::strip_hallucinations;
use crate::llm_client::{LLMClient, SoapFormat, SoapOptions};
use tauri::Emitter;
use crate::local_archive;
use crate::pipeline_log::PipelineLogger;

use crate::continuous_mode::effective_soap_detail_level;

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
                        let mut meta = log_extra.clone();
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
                let mut meta = log_extra.clone();
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
                let mut meta = log_extra.clone();
                if let Some(obj) = meta.as_object_mut() {
                    obj.insert("timeout".into(), serde_json::json!(true));
                }
                l.log_clinical_check(model, &cc_system, &cc_user, None, cc_latency, false, Some("timeout_30s"), meta);
            }
            warn!("Clinical content check timed out (30s)");
            true // fail-open
        }
    }
}

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

        let soap_opts = SoapOptions {
            detail_level: effective_soap_detail_level(soap_detail_level, word_count),
            format: SoapFormat::from_config_str(soap_format),
            ..Default::default()
        };
        info!(
            "Generating SOAP for orphaned session {} ({} words)",
            summary.session_id, word_count
        );

        let soap_start = Instant::now();
        let soap_future = client.generate_multi_patient_soap_note(
            soap_model,
            &filtered_text,
            None,
            Some(&soap_opts),
            None,
            None,
        );

        match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
            Ok(Ok(soap_result)) => {
                let soap_latency = soap_start.elapsed().as_millis() as u64;
                let soap_content = &soap_result
                    .notes
                    .iter()
                    .map(|n| n.content.clone())
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n");
                if let Ok(mut l) = logger.lock() {
                    l.log_soap(
                        soap_model,
                        "",
                        "",
                        Some(soap_content),
                        soap_latency,
                        true,
                        None,
                        serde_json::json!({
                            "stage": "orphaned_soap_recovery",
                            "session_id": summary.session_id,
                            "word_count": word_count,
                            "response_chars": soap_content.len(),
                        }),
                    );
                }
                // Use session's original date to avoid midnight routing errors
                let soap_date = chrono::DateTime::parse_from_rfc3339(&summary.date)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                if let Err(e) = local_archive::add_soap_note(
                    &summary.session_id,
                    &soap_date,
                    soap_content,
                    Some(soap_detail_level),
                    Some(soap_format),
                ) {
                    warn!(
                        "Failed to save recovered SOAP for {}: {}",
                        summary.session_id, e
                    );
                } else {
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
            }
            Ok(Err(e)) => {
                let soap_latency = soap_start.elapsed().as_millis() as u64;
                if let Ok(mut l) = logger.lock() {
                    l.log_soap(
                        soap_model,
                        "",
                        "",
                        None,
                        soap_latency,
                        false,
                        Some(&e.to_string()),
                        serde_json::json!({
                            "stage": "orphaned_soap_recovery",
                            "session_id": summary.session_id,
                        }),
                    );
                }
                warn!(
                    "Failed to generate recovered SOAP for {}: {}",
                    summary.session_id, e
                );
            }
            Err(_) => {
                warn!(
                    "SOAP generation timed out for orphaned session {}",
                    summary.session_id
                );
            }
        }
    }
}
