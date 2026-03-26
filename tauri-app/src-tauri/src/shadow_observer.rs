//! Shadow observer task for dual detection comparison.
//!
//! In shadow mode, one detection method is "active" (controls actual splits) and
//! the other runs as a "shadow" observer, logging what it would have done for
//! comparison. This module extracts the shadow observer task spawn from
//! continuous_mode.rs.

use chrono::Utc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

use tauri::Emitter;

use crate::config::ShadowActiveMethod;
use crate::encounter_detection::{build_encounter_detection_prompt, parse_encounter_detection};
use crate::encounter_experiment::strip_hallucinations;
use crate::llm_client::LLMClient;
use crate::presence_sensor::PresenceState;
use crate::shadow_log::{ShadowCsvLogger, ShadowDecision, ShadowDecisionSummary, ShadowOutcome};
use crate::transcript_buffer::TranscriptBuffer;

/// Configuration for the shadow observer task.
pub struct ShadowObserverConfig {
    pub active_method: ShadowActiveMethod,
    pub csv_log_enabled: bool,
    pub detection_model: String,
    pub detection_nothink: bool,
    pub check_interval_secs: u32,
    pub llm_router_url: String,
    pub llm_api_key: String,
    pub llm_client_id: String,
}

/// Spawn the shadow observer task if shadow mode is active.
///
/// Returns a JoinHandle for the spawned task, or None if not in shadow mode
/// or if the required resources (sensor rx, LLM client) are unavailable.
pub fn spawn_shadow_observer(
    config: ShadowObserverConfig,
    stop_flag: Arc<AtomicBool>,
    transcript_buffer: Arc<Mutex<TranscriptBuffer>>,
    shadow_decisions: Arc<Mutex<Vec<ShadowDecisionSummary>>>,
    last_shadow_decision: Arc<Mutex<Option<ShadowDecision>>>,
    mut sensor_state_rx: Option<tokio::sync::watch::Receiver<PresenceState>>,
    silence_trigger: Arc<tokio::sync::Notify>,
    app: tauri::AppHandle,
) -> Option<tokio::task::JoinHandle<()>> {
    // Shadow runs the opposite of the active method
    let shadow_is_sensor = config.active_method == ShadowActiveMethod::Llm;
    let active_method = config.active_method;
    let shadow_method_str = if shadow_is_sensor { "sensor" } else { "llm" };
    info!(
        "Shadow mode: active={}, shadow={}",
        active_method, shadow_method_str
    );

    // Initialize shadow CSV logger
    let shadow_csv_logger: Option<Arc<Mutex<ShadowCsvLogger>>> = if config.csv_log_enabled {
        match ShadowCsvLogger::new() {
            Ok(logger) => Some(Arc::new(Mutex::new(logger))),
            Err(e) => {
                warn!("Failed to create shadow CSV logger: {}", e);
                None
            }
        }
    } else {
        None
    };

    let shadow_decisions_for_task = shadow_decisions;
    let last_shadow_for_task = last_shadow_decision;
    let stop_for_shadow = stop_flag;
    let app_for_shadow = app;
    let buffer_for_shadow = transcript_buffer;

    if shadow_is_sensor {
        // Active=LLM, Shadow=sensor — observe sensor state transitions
        if let Some(mut state_rx) = sensor_state_rx.take() {
            Some(tokio::spawn(async move {
                info!("Shadow sensor observer started (watch-based)");
                let mut prev_state = PresenceState::Unknown;
                loop {
                    if stop_for_shadow.load(Ordering::Relaxed) {
                        break;
                    }

                    if state_rx.changed().await.is_err() {
                        info!("Shadow sensor: watch channel closed");
                        break;
                    }

                    if stop_for_shadow.load(Ordering::Relaxed) {
                        break;
                    }

                    let new_state = *state_rx.borrow_and_update();

                    let outcome = match (prev_state, new_state) {
                        (PresenceState::Present, PresenceState::Absent) => {
                            ShadowOutcome::WouldSplit
                        }
                        (_, PresenceState::Present) => ShadowOutcome::WouldNotSplit,
                        _ => {
                            prev_state = new_state;
                            continue;
                        }
                    };

                    prev_state = new_state;

                    let (word_count, last_segment) = buffer_for_shadow
                        .lock()
                        .map(|b| (b.word_count(), b.last_index()))
                        .unwrap_or((0, None));

                    let decision = ShadowDecision {
                        timestamp: Utc::now(),
                        shadow_method: "sensor".to_string(),
                        active_method: active_method.to_string(),
                        outcome: outcome.clone(),
                        confidence: Some(1.0),
                        buffer_word_count: word_count,
                        buffer_last_segment: last_segment,
                    };

                    let outcome_str = outcome.as_str();

                    if let Some(ref logger) = shadow_csv_logger {
                        if let Ok(mut l) = logger.lock() {
                            l.write_decision(&decision);
                        }
                    }

                    let summary = ShadowDecisionSummary::from(&decision);
                    if let Ok(mut decisions) = shadow_decisions_for_task.lock() {
                        decisions.push(summary);
                    }
                    if let Ok(mut last) = last_shadow_for_task.lock() {
                        *last = Some(decision);
                    }

                    let _ = app_for_shadow.emit(
                        "continuous_mode_event",
                        serde_json::json!({
                            "type": "shadow_decision",
                            "shadow_method": "sensor",
                            "outcome": outcome_str,
                            "buffer_words": word_count,
                            "sensor_state": new_state.as_str()
                        }),
                    );

                    info!(
                        "Shadow sensor: {} (state: {}, buffer {} words)",
                        outcome_str,
                        new_state.as_str(),
                        word_count
                    );
                }
                info!("Shadow sensor observer stopped");
            }))
        } else {
            warn!("Shadow sensor observer: no sensor state receiver available (sensor failed to start)");
            None
        }
    } else {
        // Active=sensor, Shadow=LLM — run shadow LLM detection loop
        let silence_trigger_for_shadow = silence_trigger;
        let check_interval_shadow = config.check_interval_secs;
        let shadow_detection_model = config.detection_model.clone();
        let shadow_detection_nothink = config.detection_nothink;
        let shadow_llm_client = if !config.llm_router_url.is_empty() {
            LLMClient::new(
                &config.llm_router_url,
                &config.llm_api_key,
                &config.llm_client_id,
                &shadow_detection_model,
            )
            .ok()
        } else {
            None
        };

        Some(tokio::spawn(async move {
            info!("Shadow LLM observer started");
            loop {
                if stop_for_shadow.load(Ordering::Relaxed) {
                    break;
                }

                tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval_shadow as u64)) => {}
                    _ = silence_trigger_for_shadow.notified() => {
                        debug!("Shadow LLM: silence trigger received");
                    }
                }

                if stop_for_shadow.load(Ordering::Relaxed) {
                    break;
                }

                let (formatted, word_count, last_segment) = buffer_for_shadow
                    .lock()
                    .map(|b| {
                        (
                            b.format_for_detection(),
                            b.word_count(),
                            b.last_index(),
                        )
                    })
                    .unwrap_or_else(|_| (String::new(), 0, None));

                if word_count < 100 {
                    continue;
                }

                let outcome;
                let confidence;
                if let Some(ref client) = shadow_llm_client {
                    let (filtered, _) = strip_hallucinations(&formatted, 5);
                    let (system_prompt, user_prompt) =
                        build_encounter_detection_prompt(&filtered, None);
                    let system_prompt = if shadow_detection_nothink {
                        format!("/nothink\n{}", system_prompt)
                    } else {
                        system_prompt
                    };
                    let llm_future = client.generate(
                        &shadow_detection_model,
                        &system_prompt,
                        &user_prompt,
                        "shadow_encounter_detection",
                    );
                    match tokio::time::timeout(
                        tokio::time::Duration::from_secs(90),
                        llm_future,
                    )
                    .await
                    {
                        Ok(Ok(response)) => match parse_encounter_detection(&response) {
                            Ok(result) => {
                                if result.complete
                                    && result.confidence.unwrap_or(0.0) >= 0.7
                                {
                                    outcome = ShadowOutcome::WouldSplit;
                                } else {
                                    outcome = ShadowOutcome::WouldNotSplit;
                                }
                                confidence = result.confidence;
                            }
                            Err(e) => {
                                debug!("Shadow LLM: failed to parse detection: {}", e);
                                continue;
                            }
                        },
                        Ok(Err(e)) => {
                            debug!("Shadow LLM: detection call failed: {}", e);
                            continue;
                        }
                        Err(_) => {
                            debug!("Shadow LLM: detection timed out after 90s");
                            continue;
                        }
                    }
                } else {
                    continue;
                }

                let decision = ShadowDecision {
                    timestamp: Utc::now(),
                    shadow_method: "llm".to_string(),
                    active_method: active_method.to_string(),
                    outcome,
                    confidence,
                    buffer_word_count: word_count,
                    buffer_last_segment: last_segment,
                };

                if let Some(ref logger) = shadow_csv_logger {
                    if let Ok(mut l) = logger.lock() {
                        l.write_decision(&decision);
                    }
                }

                let outcome_str = decision.outcome.as_str().to_string();
                let summary = ShadowDecisionSummary::from(&decision);
                if let Ok(mut decisions) = shadow_decisions_for_task.lock() {
                    decisions.push(summary);
                }
                if let Ok(mut last) = last_shadow_for_task.lock() {
                    *last = Some(decision);
                }

                let _ = app_for_shadow.emit(
                    "continuous_mode_event",
                    serde_json::json!({
                        "type": "shadow_decision",
                        "shadow_method": "llm",
                        "outcome": outcome_str,
                        "confidence": confidence,
                        "buffer_words": word_count
                    }),
                );

                info!(
                    "Shadow LLM: {} (confidence={:?}, buffer {} words)",
                    outcome_str, confidence, word_count
                );
            }
            info!("Shadow LLM observer stopped");
        }))
    }
}
