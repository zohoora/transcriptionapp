//! Screenshot-based patient name extraction task for continuous mode.
//!
//! Periodically captures the screen, sends it to a vision model for patient name
//! extraction, and feeds votes into the PatientNameTracker. Runs as a spawned
//! tokio task; all shared state is passed via Arc clones.

use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::Notify;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::encounter_detection::SCREENSHOT_STALE_GRACE_SECS;
use crate::llm_client::{ContentPart, ImageUrlContent, LLMClient};
use crate::patient_name_tracker::PatientNameTracker;
use crate::pipeline_log::PipelineLogger;
use crate::replay_bundle::{ReplayBundleBuilder, VisionResult};

pub(crate) use crate::patient_name_tracker::{build_patient_name_prompt, parse_vision_response};

// ── Vision early-stop policy (Apr 17 2026) ─────────────────────────────────
//
// Calibrated from the Apr 16 Room 6 forensic analysis: 329 vision calls
// produced only ~52 unique names (80%+ redundancy). Sessions with stable
// names converged after 5 consecutive matching votes; the hard cap backstops
// pathological encounters that never stabilize (e.g. chart-switching noise).
//
// Expected savings per clinic day: ~78% reduction in vision calls,
// ~10 min of LLM wait freed up. Same downstream behavior because the
// weighted-majority name is unchanged on stable cases — see Option A
// rationale in the Apr 17 operational analysis.
// ────────────────────────────────────────────────────────────────────────────
/// Skip further vision calls for an encounter once this many consecutive
/// vision extractions all returned the same patient name.
const VISION_STREAK_CUTOFF: usize = 5;

/// Hard cap on total vision LLM calls per encounter. Backstops the rare
/// case where names keep flipping so `VISION_STREAK_CUTOFF` is never reached.
const VISION_MAX_CALLS_PER_ENCOUNTER: usize = 30;

/// All inputs needed by the screenshot task, gathered at spawn time.
pub struct ScreenshotTaskConfig {
    pub stop_flag: Arc<AtomicBool>,
    pub name_tracker: Arc<Mutex<PatientNameTracker>>,
    pub last_split_time: Arc<Mutex<DateTime<Utc>>>,
    pub vision_trigger: Arc<Notify>,
    pub vision_new_name: Arc<Mutex<Option<String>>>,
    pub vision_old_name: Arc<Mutex<Option<String>>>,
    pub debug_storage: bool,
    pub screenshot_interval: u64,
    pub llm_client: Option<LLMClient>,
    pub pipeline_logger: Arc<Mutex<PipelineLogger>>,
    pub replay_bundle: Arc<Mutex<ReplayBundleBuilder>>,
    /// Buffer for saving screenshots to session archive after encounter split
    pub screenshot_buffer: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    /// Transcript buffer used to gate captures: skip when no words are present
    pub transcript_buffer: Arc<std::sync::Mutex<crate::transcript_buffer::TranscriptBuffer>>,
}

/// Runs the screenshot capture + vision extraction loop.
/// Called via `tokio::spawn(run_screenshot_task(config))`.
pub async fn run_screenshot_task(cfg: ScreenshotTaskConfig) {
    info!(
        "Screenshot name extraction task started (interval: {}s)",
        cfg.screenshot_interval
    );

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(cfg.screenshot_interval)).await;

        if cfg.stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // Skip capture when no speech in buffer (no active encounter)
        let buffer_words = cfg.transcript_buffer.lock()
            .map(|b| b.word_count())
            .unwrap_or(0);
        if buffer_words == 0 {
            debug!("Screenshot: no words in buffer, skipping capture");
            continue;
        }

        // Capture screen (blocking CoreGraphics call)
        let capture_result =
            tokio::task::spawn_blocking(|| crate::screenshot::capture_to_base64(1150)).await;

        let capture = match capture_result {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                debug!("Screenshot capture failed (may not have permission): {}", e);
                continue;
            }
            Err(e) => {
                debug!("Screenshot capture task panicked: {}", e);
                continue;
            }
        };

        if capture.likely_blank {
            warn!("Screenshot appears blank — screen recording permission likely not granted. Skipping vision analysis. Grant permission in System Settings → Privacy & Security → Screen Recording.");
            continue;
        }

        let image_base64 = capture.base64;

        // Save debug screenshot if enabled
        if cfg.debug_storage {
            save_debug_screenshot(&image_base64);
        }

        // Buffer screenshot for session archive (decoded from base64 to raw JPEG)
        if let Ok(jpeg_bytes) = base64_decode(&image_base64) {
            let ts = Utc::now().to_rfc3339();
            if let Ok(mut buf) = cfg.screenshot_buffer.lock() {
                buf.push((ts, jpeg_bytes));
            }
        }

        let client = match &cfg.llm_client {
            Some(c) => c,
            None => {
                debug!("No LLM client for screenshot name extraction");
                continue;
            }
        };

        // Vision early-stop gate (Apr 17 2026). Screenshots are already buffered
        // above, so the audit trail is preserved — only the vision LLM call is
        // skipped. Skip fires when either:
        //   • the tracker has accumulated K consecutive matching votes (confident)
        //   • the encounter has already burned the per-encounter cap
        // Both state fields are reset by `PatientNameTracker::reset()` on split.
        let should_skip = cfg.name_tracker.lock().ok()
            .map(|t| t.should_skip_vision(VISION_STREAK_CUTOFF, VISION_MAX_CALLS_PER_ENCOUNTER))
            .unwrap_or(false);
        if should_skip {
            if let Ok(t) = cfg.name_tracker.lock() {
                debug!(
                    "Vision early-stop: streak={} attempts={} — skipping LLM call",
                    t.streak_count(), t.vision_calls_attempted()
                );
            }
            continue;
        }

        // Bump the attempt counter BEFORE invoking vision so the cap counts
        // failures too (a timed-out or unparseable response still burned LLM budget).
        if let Ok(mut t) = cfg.name_tracker.lock() {
            t.note_vision_attempt();
        }

        let (system_prompt, user_text) = build_patient_name_prompt(None);
        let system_prompt_log = system_prompt.clone();
        let user_text_log = user_text.clone();
        let content_parts = vec![
            ContentPart::Text { text: user_text },
            ContentPart::ImageUrl {
                image_url: ImageUrlContent {
                    url: format!("data:image/jpeg;base64,{}", image_base64),
                },
            },
        ];

        let vision_start = Instant::now();
        // v0.10.38: switch to generate_vision_timed to get per-call CallMetrics
        // (scheduling_ms / network_ms / concurrent_at_start / retry_count). The
        // metrics are merged into the vision_extraction pipeline_log context via
        // `m.attach_to(&mut ctx)` — same facility the 4 text-LLM paths got in
        // v0.10.36.
        let vision_future = client.generate_vision_timed(
            "vision-model",
            &system_prompt,
            content_parts,
            "patient_name_extraction",
            Some(0.1),
            Some(100),
            None,
            None,
        );

        match tokio::time::timeout(tokio::time::Duration::from_secs(30), vision_future).await {
            Ok((Ok(response), metrics)) => {
                let vision_latency = vision_start.elapsed().as_millis() as u64;
                let (parsed_name, parsed_dob) = parse_vision_response(&response);

                // Store DOB if found (most recent extraction wins)
                if let Some(ref dob) = parsed_dob {
                    if let Ok(mut tracker) = cfg.name_tracker.lock() {
                        tracker.set_dob(dob.clone());
                    }
                }

                if let Some(ref name) = parsed_name {
                    info!("Vision extracted patient name: {}", name);

                    let is_stale = check_stale_vote(
                        name,
                        &cfg.last_split_time,
                        &cfg.name_tracker,
                    );

                    if let Ok(mut logger) = cfg.pipeline_logger.lock() {
                        let mut ctx = serde_json::json!({
                            "parsed_name": name,
                            "parsed_dob": parsed_dob,
                            "screenshot_blank": false,
                            "is_stale": is_stale,
                        });
                        metrics.attach_to(&mut ctx);
                        logger.log_vision(
                            "vision-model",
                            &system_prompt_log,
                            &user_text_log,
                            Some(&response),
                            vision_latency,
                            true,
                            None,
                            ctx,
                        );
                    }
                    if let Ok(mut bundle) = cfg.replay_bundle.lock() {
                        bundle.add_vision_result(VisionResult {
                            ts: Utc::now().to_rfc3339(),
                            parsed_name: Some(name.clone()),
                            is_stale,
                            is_blank: false,
                            latency_ms: vision_latency,
                        });
                    }

                    if is_stale {
                        info!(
                            "Skipping stale screenshot vote '{}' — matches previous encounter name and within {}s grace period",
                            name, SCREENSHOT_STALE_GRACE_SECS
                        );
                        continue;
                    }

                    if let Ok(mut tracker) = cfg.name_tracker.lock() {
                        let (changed, old_name, new_name) =
                            tracker.record_and_check_change(&name);
                        if changed {
                            info!(
                                "Vision detected patient name change: {:?} → {:?} — accelerating detection",
                                old_name, new_name
                            );
                            if let Ok(mut n) = cfg.vision_new_name.lock() {
                                *n = new_name;
                            }
                            if let Ok(mut o) = cfg.vision_old_name.lock() {
                                *o = old_name;
                            }
                            cfg.vision_trigger.notify_one();
                        }
                    } else {
                        warn!(
                            "Name tracker lock poisoned, patient name vote dropped: {}",
                            name
                        );
                    }
                } else {
                    if let Ok(mut logger) = cfg.pipeline_logger.lock() {
                        let mut ctx = serde_json::json!({
                            "parsed_name": serde_json::Value::Null,
                            "screenshot_blank": false,
                            "not_found": true,
                        });
                        metrics.attach_to(&mut ctx);
                        logger.log_vision(
                            "vision-model",
                            &system_prompt_log,
                            &user_text_log,
                            Some(&response),
                            vision_latency,
                            true,
                            None,
                            ctx,
                        );
                    }
                    if let Ok(mut bundle) = cfg.replay_bundle.lock() {
                        bundle.add_vision_result(VisionResult::failed(vision_latency));
                    }
                    debug!("Vision did not find a patient name on screen");
                }
            }
            Ok((Err(e), metrics)) => {
                let vision_latency = vision_start.elapsed().as_millis() as u64;
                if let Ok(mut logger) = cfg.pipeline_logger.lock() {
                    let mut ctx = serde_json::json!({"llm_error": true});
                    metrics.attach_to(&mut ctx);
                    logger.log_vision(
                        "vision-model",
                        &system_prompt_log,
                        &user_text_log,
                        None,
                        vision_latency,
                        false,
                        Some(&e.to_string()),
                        ctx,
                    );
                }
                if let Ok(mut bundle) = cfg.replay_bundle.lock() {
                    bundle.add_vision_result(VisionResult::failed(vision_latency));
                }
                debug!("Vision name extraction failed: {}", e);
            }
            Err(_) => {
                let vision_latency = vision_start.elapsed().as_millis() as u64;
                if let Ok(mut logger) = cfg.pipeline_logger.lock() {
                    logger.log_vision(
                        "vision-model",
                        &system_prompt_log,
                        &user_text_log,
                        None,
                        vision_latency,
                        false,
                        Some("timeout_30s"),
                        serde_json::json!({"timeout": true}),
                    );
                }
                if let Ok(mut bundle) = cfg.replay_bundle.lock() {
                    bundle.add_vision_result(VisionResult::failed(vision_latency));
                }
                debug!("Vision name extraction timed out after 30s");
            }
        }
    }

    info!("Screenshot name extraction task stopped");
}

/// Check if a name vote is stale (matches previous encounter's name within grace period).
fn check_stale_vote(
    name: &str,
    last_split_time: &Arc<Mutex<DateTime<Utc>>>,
    name_tracker: &Arc<Mutex<PatientNameTracker>>,
) -> bool {
    if let Ok(split_time) = last_split_time.lock() {
        let secs_since_split = (Utc::now() - *split_time).num_seconds();
        if secs_since_split < SCREENSHOT_STALE_GRACE_SECS {
            if let Ok(tracker) = name_tracker.lock() {
                return tracker.previous_name() == Some(name);
            }
        }
    }
    false
}

/// Save a debug screenshot to disk.
fn save_debug_screenshot(image_base64: &str) {
    use base64::Engine;
    if let Ok(config_dir) = Config::config_dir() {
        let debug_dir = config_dir.join("debug").join("continuous-screenshots");
        let _ = std::fs::create_dir_all(&debug_dir);
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let filename = debug_dir.join(format!("{}.jpg", timestamp));
        match base64::engine::general_purpose::STANDARD.decode(image_base64) {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(&filename, &bytes) {
                    warn!("Failed to save debug screenshot: {}", e);
                } else {
                    debug!("Debug screenshot saved: {:?}", filename);
                }
            }
            Err(e) => {
                warn!(
                    "Failed to decode screenshot base64 for debug save: {}",
                    e
                );
            }
        }
    }
}

/// Decode base64-encoded image data to raw bytes.
fn base64_decode(data: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(data)
}

/// Flush buffered screenshots to a session's archive directory.
/// Called after encounter split when the session_dir is known.
pub fn flush_screenshots_to_session(
    buffer: &Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    session_dir: &std::path::Path,
) {
    let screenshots = match buffer.lock() {
        Ok(mut buf) => std::mem::take(&mut *buf),
        Err(e) => {
            warn!("Screenshot buffer lock poisoned: {e}");
            return;
        }
    };
    if screenshots.is_empty() {
        return;
    }
    let dir = session_dir.join("screenshots");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        warn!("Failed to create screenshots dir: {e}");
        return;
    }
    for (i, (ts, jpeg)) in screenshots.iter().enumerate() {
        // Use index + truncated timestamp for filename (avoids colons in filenames)
        let safe_ts = ts.replace(':', "").replace('+', "").chars().take(15).collect::<String>();
        let filename = format!("{:03}_{}.jpg", i, safe_ts);
        if let Err(e) = std::fs::write(dir.join(&filename), jpeg) {
            warn!("Failed to save screenshot {}: {e}", filename);
        }
    }
    info!("Saved {} screenshots to {}", screenshots.len(), dir.display());
}
