//! Encounter splitter: drain buffer, archive, enrich metadata, emit events.
//!
//! Runs once per detector decision to split. Produces a `SplitContext` that
//! flows downstream into `post_split` and `merge_back`. Consolidates the
//! buffer-drain + archival + metadata-enrichment + recent-encounters bookkeeping
//! that used to live inline inside the detector task.
//!
//! Responsibilities:
//!  1. Drain transcript segments through `end_index` from the shared buffer.
//!  2. Archive the encounter transcript via `local_archive::save_session`.
//!  3. Redirect pipeline + segment loggers to the new session's archive dir;
//!     flush buffered screenshots into it.
//!  4. Record the split decision on the replay bundle + day log.
//!  5. Enrich the newly-written `metadata.json` with continuous-mode fields
//!     (charting_mode, encounter_number, detection_method, patient name/DOB,
//!     shadow comparison, physician/room context).
//!  6. Kick a fire-and-forget server sync for the session.
//!  7. Snapshot the name tracker (majority name + full vote state) and reset it.
//!  8. Atomically read + clear encounter notes.
//!  9. Bump encounter counter, update `recent_encounters`, emit
//!     `EncounterDetected` event.
//!
//! LOGGER SESSION CONTRACT: on successful split, redirects the pipeline logger
//! to the newly-archived session's `pipeline_log.jsonl`. Caller does not
//! restore; the next encounter's splitter run re-redirects on its own.
//!
//! LOOP-STATE MUTATIONS: `loop_state.encounter_number` is read (after the
//! caller's pre-increment). Not mutated by this function.
//!
//! COMPONENT: `continuous_mode_splitter`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tracing::{info, warn};

use crate::config::ShadowActiveMethod;
use crate::continuous_mode::{ContinuousModeHandle, RecentEncounter};
use crate::continuous_mode_events::ContinuousModeEvent;
use crate::continuous_mode_types::LoopState;
use crate::day_log::DayLogger;
use crate::local_archive;
use crate::pipeline_log::PipelineLogger;
use crate::replay_bundle::ReplayBundleBuilder;
use crate::run_context::RunContext;
use crate::segment_log::SegmentLogger;
use crate::server_sync::ServerSyncContext;

/// Long-lived dependency bundle. Built once per continuous-mode run before
/// the detector loop starts and borrowed into each splitter call.
pub struct SplitterDeps {
    pub handle: Arc<ContinuousModeHandle>,
    pub logger: Arc<Mutex<PipelineLogger>>,
    pub bundle: Arc<Mutex<ReplayBundleBuilder>>,
    pub segment_logger: Arc<Mutex<SegmentLogger>>,
    pub day_logger: Arc<Option<DayLogger>>,
    pub sync_ctx: ServerSyncContext,
    /// Biomarker reset flag — set to true on every split so the pipeline clears
    /// per-encounter accumulators on the next consumer iteration.
    pub reset_bio_flag: Arc<AtomicBool>,
    /// Shadow-mode configuration (snapshot from config at run start).
    pub is_shadow_mode: bool,
    pub shadow_active_method: ShadowActiveMethod,
}

/// Per-encounter inputs derived from the detector's trigger evaluation.
pub struct SplitterCall<'a> {
    pub end_index: u64,
    pub detection_method: &'a str,
    pub cleaned_word_count: usize,
}

/// Everything downstream (`post_split`, `merge_back`, finalize block) needs
/// from the splitter. Returned by reference (the caller unpacks fields).
pub struct SplitContext {
    pub session_id: String,
    pub encounter_text: String,
    pub encounter_text_rich: String,
    pub encounter_word_count: usize,
    pub encounter_start: Option<DateTime<Utc>>,
    pub encounter_end: Option<DateTime<Utc>>,
    pub encounter_segment_count: usize,
    pub encounter_duration_ms: u64,
    pub notes_text: String,
    pub tracker_snapshot: (Option<String>, usize, Vec<String>),
    pub encounter_patient_name: Option<String>,
    pub detection_method: String,
    /// Resolved session archive directory for the new encounter. `None` if
    /// the archive path could not be resolved (rare — path derivation failure).
    pub session_dir: Option<PathBuf>,
}

/// Run the split pipeline and return a `SplitContext` for the downstream
/// post_split + merge_back stages. Returns `Err` only if the buffer lock is
/// poisoned at drain time — the caller should `continue` the detector loop.
pub async fn split_encounter<C: RunContext>(
    ctx: &C,
    deps: &SplitterDeps,
    call: SplitterCall<'_>,
    loop_state: &LoopState,
) -> Result<SplitContext, String> {
    let SplitterCall {
        end_index,
        detection_method,
        cleaned_word_count,
    } = call;
    let detection_method_str = detection_method.to_string();

    // Extract encounter segments from buffer
    let (encounter_text, encounter_text_rich, encounter_word_count, encounter_start, encounter_end, encounter_segment_count) = {
        let mut buffer = deps
            .handle
            .transcript_buffer
            .lock()
            .map_err(|e| format!("transcript buffer lock poisoned: {}", e))?;
        let drained = buffer.drain_through(end_index);
        let seg_count = drained.len();
        let text: String = drained
            .iter()
            .map(|s| {
                if s.speaker_id.is_some() {
                    let label = crate::transcript_buffer::format_speaker_label(
                        s.speaker_id.as_deref(),
                        s.speaker_confidence,
                    );
                    format!("{}: {}", label, s.text)
                } else {
                    s.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let text_rich = crate::transcript_buffer::format_segments_for_detection(&drained);
        let wc = text.split_whitespace().count();
        let start = drained.first().map(|s| s.started_at);
        let end = drained.last().map(|s| s.started_at);
        (text, text_rich, wc, start, end, seg_count)
    };

    // Generate session ID for this encounter
    let session_id = uuid::Uuid::new_v4().to_string();

    // Compute duration from first-to-last segment (not Utc::now which includes LLM processing time)
    let encounter_duration_ms = match (encounter_start, encounter_end) {
        (Some(s), Some(e)) => (e - s).num_milliseconds().max(0) as u64,
        (Some(s), None) => (ctx.now_utc() - s).num_milliseconds().max(0) as u64,
        _ => 0,
    };

    // Archive the encounter transcript (pass actual start time for accurate duration)
    if let Err(e) = local_archive::save_session(
        &session_id,
        &encounter_text,
        encounter_duration_ms,
        None, // No per-encounter audio in continuous mode
        false,
        None,
        encounter_start, // actual encounter start time for started_at metadata
        Some(encounter_segment_count),
    ) {
        warn!(
            event = "splitter_archive_failed",
            component = "continuous_mode_splitter",
            error = %e,
            "Failed to archive encounter"
        );
    }

    // Resolve session archive dir once — used for logger set_session,
    // metadata rewrite, and returned in SplitContext for downstream use.
    let session_dir = local_archive::get_session_archive_dir(&session_id, &ctx.now_utc()).ok();

    // Set pipeline logger and segment logger to write to this session's archive folder
    if let Some(ref dir) = session_dir {
        if let Ok(mut logger) = deps.logger.lock() {
            logger.set_session(dir);
        }
        if let Ok(mut sl) = deps.segment_logger.lock() {
            sl.set_session(dir);
        }
        // Flush buffered screenshots to session archive
        crate::screenshot_task::flush_screenshots_to_session(
            &deps.handle.screenshot_buffer,
            dir,
        );
    }

    // Set split decision on replay bundle
    if let Ok(mut bundle) = deps.bundle.lock() {
        bundle.set_split_decision(crate::replay_bundle::SplitDecision {
            ts: ctx.now_utc().to_rfc3339(),
            trigger: detection_method_str.clone(),
            word_count: encounter_word_count,
            cleaned_word_count,
            end_segment_index: Some(end_index),
        });
    }

    // Log encounter split to day log
    if let Some(ref dl) = *deps.day_logger {
        dl.log(crate::day_log::DayEvent::EncounterSplit {
            ts: ctx.now_utc().to_rfc3339(),
            session_id: session_id.clone(),
            encounter_number: loop_state.encounter_number,
            trigger: detection_method_str.clone(),
            word_count: encounter_word_count,
            detection_method: detection_method_str.clone(),
        });
    }

    // Update archive metadata with continuous mode info
    if let Some(ref dir) = session_dir {
        let date_path = dir.join("metadata.json");
        if date_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&date_path) {
                if let Ok(mut metadata) = serde_json::from_str::<local_archive::ArchiveMetadata>(&content) {
                    metadata.charting_mode = Some("continuous".to_string());
                    metadata.encounter_number = Some(loop_state.encounter_number);
                    // Record how this encounter was detected (reuse pre-computed value)
                    metadata.detection_method = Some(detection_method_str.clone());
                    // Add patient name and DOB from vision extraction
                    if let Ok(tracker) = deps.handle.name_tracker.lock() {
                        metadata.patient_name = tracker.majority_name();
                        metadata.patient_dob = tracker.dob().map(|s| s.to_string());
                    } else {
                        warn!(
                            event = "splitter_name_tracker_poisoned",
                            component = "continuous_mode_splitter",
                            "Name tracker lock poisoned, patient name/dob not written to metadata"
                        );
                    }
                    // Add shadow comparison data if in shadow mode
                    if deps.is_shadow_mode {
                        let shadow_method = if deps.shadow_active_method == ShadowActiveMethod::Sensor {
                            "llm"
                        } else {
                            "sensor"
                        };
                        let decisions: Vec<crate::shadow_log::ShadowDecisionSummary> = deps
                            .handle
                            .shadow_decisions
                            .lock()
                            .unwrap_or_else(|e| {
                                warn!(
                                    event = "splitter_shadow_decisions_poisoned",
                                    component = "continuous_mode_splitter",
                                    "Shadow decisions lock poisoned, recovering data"
                                );
                                e.into_inner()
                            })
                            .clone();

                        let active_split_at = ctx.now_utc().to_rfc3339();

                        // Check if shadow agreed: any "would_split" decision in last 5 minutes
                        let now = ctx.now_utc();
                        let shadow_agreed = if decisions.is_empty() {
                            None
                        } else {
                            let agreed = decisions.iter().any(|d| {
                                d.outcome == "would_split" && {
                                    chrono::DateTime::parse_from_rfc3339(&d.timestamp)
                                        .map(|ts| (now - ts.with_timezone(&Utc)).num_seconds().abs() < 300)
                                        .unwrap_or(false)
                                }
                            });
                            Some(agreed)
                        };

                        metadata.shadow_comparison = Some(crate::shadow_log::ShadowEncounterComparison {
                            shadow_method: shadow_method.to_string(),
                            decisions,
                            active_split_at,
                            shadow_agreed,
                        });
                    }

                    // Add physician/room context (multi-user)
                    deps.sync_ctx.enrich_metadata(&mut metadata);

                    if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                        let _ = std::fs::write(&date_path, json);
                    }
                }
            }
        }
    }

    // Server sync: upload session to profile server
    {
        let today = ctx.now_utc().format("%Y-%m-%d").to_string();
        deps.sync_ctx.sync_session(&session_id, &today);
    }

    // Clear shadow decisions for next encounter (if in shadow mode)
    if deps.is_shadow_mode {
        if let Ok(mut decisions) = deps.handle.shadow_decisions.lock() {
            decisions.clear();
        }
    }

    // Extract patient name and full tracker state before resetting.
    // The replay bundle needs this data too — capturing after reset
    // would see an empty tracker (was the cause of replay_bundle
    // always showing majority_name=None, vote_count=0).
    let (encounter_patient_name, tracker_snapshot) = match deps.handle.name_tracker.lock() {
        Ok(mut tracker) => {
            let name = tracker.majority_name();
            let votes: usize = tracker.vote_count();
            let unique: Vec<String> = tracker.votes().keys().cloned().collect();
            tracker.reset();
            (name.clone(), (name, votes, unique))
        }
        Err(e) => {
            warn!(
                event = "splitter_name_tracker_poisoned",
                component = "continuous_mode_splitter",
                error = %e,
                "Name tracker lock poisoned"
            );
            (None, (None, 0, vec![]))
        }
    };

    // Record split timestamp (for stale screenshot detection)
    if let Ok(mut t) = deps.handle.last_split_time.lock() {
        *t = ctx.now_utc();
    }

    // Read encounter notes AND clear atomically (SOAP generation needs them)
    let notes_text = match deps.handle.encounter_notes.lock() {
        Ok(mut notes) => {
            let text = notes.clone();
            notes.clear();
            text
        }
        Err(e) => {
            warn!(
                event = "splitter_notes_poisoned",
                component = "continuous_mode_splitter",
                error = %e,
                "Encounter notes lock poisoned, using recovered value"
            );
            let mut notes = e.into_inner();
            let text = notes.clone();
            notes.clear();
            text
        }
    };

    // Reset biomarker accumulators for the new encounter
    deps.reset_bio_flag.store(true, Ordering::SeqCst);

    // Update stats
    deps.handle.encounters_detected.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut recent) = deps.handle.recent_encounters.lock() {
        recent.insert(0, RecentEncounter {
            session_id: session_id.clone(),
            time: ctx.now_utc().to_rfc3339(),
            patient_name: encounter_patient_name.clone(),
        });
        recent.truncate(3); // Keep only the 3 most recent
    } else {
        warn!(
            event = "splitter_recent_encounters_poisoned",
            component = "continuous_mode_splitter",
            "Recent encounters lock poisoned, stats not updated"
        );
    }

    // Emit encounter detected event
    ContinuousModeEvent::EncounterDetected {
        session_id: session_id.clone(),
        word_count: encounter_word_count,
        patient_name: encounter_patient_name.clone(),
    }
    .emit_via(ctx);

    info!(
        event = "splitter_complete",
        component = "continuous_mode_splitter",
        session_id = %session_id,
        encounter_number = loop_state.encounter_number,
        word_count = encounter_word_count,
        detection_method = %detection_method_str,
        "Encounter split complete"
    );

    Ok(SplitContext {
        session_id,
        encounter_text,
        encounter_text_rich,
        encounter_word_count,
        encounter_start,
        encounter_end,
        encounter_segment_count,
        encounter_duration_ms,
        notes_text,
        tracker_snapshot,
        encounter_patient_name,
        detection_method: detection_method_str,
        session_dir,
    })
}
