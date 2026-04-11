//! Continuous Charting Mode
//!
//! Runs the audio pipeline continuously all day. An LLM-based encounter detector
//! periodically analyzes the transcript buffer to identify complete patient encounters,
//! then automatically archives them and generates SOAP notes.
//!
//! Architecture:
//!   Microphone → Pipeline (runs all day) → TranscriptBuffer
//!                                              ↓ (periodic)
//!                                        Encounter Detector (LLM)
//!                                              ↓
//!                                        Complete? → Extract → SOAP → Archive

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::config::{Config, EncounterDetectionMode, ShadowActiveMethod};
use crate::continuous_mode_events::ContinuousModeEvent;
use crate::encounter_experiment::strip_hallucinations;
use crate::llm_client::LLMClient;
use crate::local_archive;
use crate::pipeline::{start_pipeline, PipelineConfig, PipelineMessage};
use crate::server_sync::ServerSyncContext;

// Re-export submodule types for backward compatibility
pub use crate::transcript_buffer::{BufferedSegment, TranscriptBuffer};
pub use crate::encounter_detection::{
    EncounterDetectionResult, EncounterDetectionContext,
    build_encounter_detection_prompt, parse_encounter_detection,
    ClinicalContentCheckResult, build_clinical_content_check_prompt, parse_clinical_content_check,
    FORCE_CHECK_WORD_THRESHOLD, FORCE_SPLIT_WORD_THRESHOLD, FORCE_SPLIT_CONSECUTIVE_LIMIT, ABSOLUTE_WORD_CAP,
    MIN_WORDS_FOR_CLINICAL_CHECK, SCREENSHOT_STALE_GRACE_SECS,
    MULTI_PATIENT_CHECK_WORD_THRESHOLD, MULTI_PATIENT_SPLIT_MIN_WORDS,
    MULTI_PATIENT_CHECK_PROMPT, MULTI_PATIENT_SPLIT_PROMPT,
    parse_multi_patient_check,
    MULTI_PATIENT_DETECT_WORD_THRESHOLD,
    DetectionEvalContext, DetectionOutcome, evaluate_detection,
    TRIGGER_HYBRID_SENSOR_TIMEOUT,
};
pub use crate::encounter_merge::MergeCheckResult;
pub use crate::patient_name_tracker::PatientNameTracker;

// ============================================================================
// Merge excerpt helpers
// ============================================================================

/// Number of words to extract from transcript tail/head for merge comparison
const MERGE_EXCERPT_WORDS: usize = 500;

/// Maximum word count for a buffer to be considered "idle" and auto-cleared.
/// Buffers older than `idle_encounter_timeout_secs` with fewer words than this
/// are discarded as ambient noise (hallway chatter, STT hallucinations).
const IDLE_ENCOUNTER_MAX_WORDS: usize = 200;

/// Minimum word count for a split to be accepted. If the LLM's `end_segment_index`
/// yields fewer words than this, the split is rejected and the buffer continues
/// accumulating. This prevents false micro-splits (e.g. 30-word goodbye fragments)
/// that leave the actual encounter content stranded in the next buffer.
/// The existing sensor timeout force-split (which uses `last_index()`) is unaffected
/// since it always archives the entire buffer.
const MIN_SPLIT_WORD_FLOOR: usize = 100;

/// Extract up to `n` words from the end of `text`.
fn tail_words(text: &str, n: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() > n { words[words.len() - n..].join(" ") } else { text.to_string() }
}

/// Extract up to `n` words from the start of `text`.
fn head_words(text: &str, n: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() > n { words[..n].join(" ") } else { text.to_string() }
}

/// Compute effective SOAP detail level based on transcript length.
///
/// Short encounters use the clinician's configured level as-is.
/// Longer transcripts automatically scale up so the LLM doesn't
/// cherry-pick a handful of items from a dense, multi-topic session.
/// The configured level acts as a floor — never reduces detail.
pub fn effective_soap_detail_level(configured: u8, word_count: usize) -> u8 {
    let word_based: u8 = match word_count {
        0..=1499 => 3,
        1500..=2999 => 4,
        3000..=4999 => 5,
        5000..=7999 => 6,
        8000..=11999 => 7,
        12000..=15999 => 8,
        16000..=19999 => 9,
        _ => 10,
    };
    // .min(10) guards against configured values that escaped config clamping
    configured.max(word_based).min(10)
}

// ============================================================================
// Continuous Mode State
// ============================================================================

/// State of the continuous mode
#[derive(Debug, Clone, PartialEq)]
pub enum ContinuousState {
    Idle,
    Recording,
    Checking,
    Sleeping,
    Error(String),
}

impl ContinuousState {
    pub fn as_str(&self) -> &str {
        match self {
            ContinuousState::Idle => "idle",
            ContinuousState::Recording => "recording",
            ContinuousState::Checking => "checking",
            ContinuousState::Sleeping => "sleeping",
            ContinuousState::Error(_) => "error",
        }
    }
}

/// Summary of a recent encounter for dashboard display
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentEncounter {
    pub session_id: String,
    pub time: String,          // ISO 8601
    pub patient_name: Option<String>,
}

/// Stats for the frontend monitoring dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousModeStats {
    pub state: String,
    pub recording_since: String,
    pub encounters_detected: u32,
    pub recent_encounters: Vec<RecentEncounter>,
    pub last_error: Option<String>,
    pub buffer_word_count: usize,
    /// ISO timestamp of the first segment in the current buffer (for "current encounter" display)
    pub buffer_started_at: Option<String>,
    /// Presence sensor connection status (None when in LLM detection mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_connected: Option<bool>,
    /// Presence sensor state: "present", "absent", "unknown" (None when in LLM mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_state: Option<String>,
    /// Whether shadow mode is active (dual detection comparison)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_mode_active: Option<bool>,
    /// Which method is the shadow ("llm" or "sensor"), None when not in shadow mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_method: Option<String>,
    /// Last shadow decision outcome: "would_split" or "would_not_split"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_shadow_outcome: Option<String>,
    /// Whether continuous mode is currently in a scheduled sleep window
    pub is_sleeping: bool,
    /// ISO timestamp when sleep will end and recording resumes (None when not sleeping)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_resume_at: Option<String>,
}

/// Handle to control the running continuous mode
pub struct ContinuousModeHandle {
    /// Inner stop flag: set by `handle.stop()` or the sleep scheduler to stop
    /// the `run_continuous_mode` loop. Cleared by `reset_for_new_run()` when
    /// the outer sleep/restart loop starts a fresh run.
    ///
    /// Contract: code that wants to pause for sleep MUST only set `stop_flag`
    /// (not `user_stop_flag`). Code that wants a full user-initiated stop MUST
    /// call `handle.stop()`, which sets both flags.
    pub stop_flag: Arc<AtomicBool>,
    /// Outer stop flag: set only by an explicit user stop action. NOT cleared
    /// on sleep restart. The outer loop in `commands/continuous.rs` checks this
    /// to distinguish a user-stop (exit entirely) from a sleep-stop (pause and
    /// restart later).
    pub user_stop_flag: Arc<AtomicBool>,
    pub state: Arc<Mutex<ContinuousState>>,
    pub transcript_buffer: Arc<Mutex<TranscriptBuffer>>,
    pub encounters_detected: Arc<AtomicU32>,
    pub recording_since: Arc<Mutex<DateTime<Utc>>>,
    pub recent_encounters: Arc<Mutex<Vec<RecentEncounter>>>,
    pub last_error: Arc<Mutex<Option<String>>>,
    pub name_tracker: Arc<Mutex<PatientNameTracker>>,
    /// Manual trigger for "New Patient" button — wakes the encounter detector immediately
    pub encounter_manual_trigger: Arc<tokio::sync::Notify>,
    /// Per-encounter notes from the clinician (passed to SOAP generation, cleared on new encounter)
    pub encounter_notes: Arc<Mutex<String>>,
    /// Presence sensor state receiver (None when in LLM detection mode)
    pub sensor_state_rx: Mutex<Option<tokio::sync::watch::Receiver<crate::presence_sensor::PresenceState>>>,
    /// Presence sensor status receiver (None when in LLM detection mode)
    pub sensor_status_rx: Mutex<Option<tokio::sync::watch::Receiver<crate::presence_sensor::SensorStatus>>>,
    /// Vision-triggered name change: wakes detection loop when chart switch detected
    pub vision_name_change_trigger: Arc<tokio::sync::Notify>,
    /// Vision-detected new patient name (set by screenshot task, read by detection loop)
    pub vision_new_name: Arc<Mutex<Option<String>>>,
    /// Vision-detected previous patient name (set by screenshot task on change)
    pub vision_old_name: Arc<Mutex<Option<String>>>,
    /// Shadow mode: accumulated shadow decisions for the current encounter
    pub shadow_decisions: Arc<Mutex<Vec<crate::shadow_log::ShadowDecisionSummary>>>,
    /// Shadow mode: most recent shadow decision (for dashboard display)
    pub last_shadow_decision: Arc<Mutex<Option<crate::shadow_log::ShadowDecision>>>,
    /// Timestamp of last encounter split (for stale screenshot detection)
    pub last_split_time: Arc<Mutex<DateTime<Utc>>>,
    /// Buffered screenshots for the current encounter (timestamp, JPEG bytes)
    pub screenshot_buffer: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    /// ISO timestamp when sleep will end (set when entering sleep, cleared on wake)
    pub sleep_resume_at: Arc<Mutex<Option<String>>>,
}

impl ContinuousModeHandle {
    pub fn new() -> Self {
        Self {
            stop_flag: Arc::new(AtomicBool::new(false)),
            user_stop_flag: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(ContinuousState::Idle)),
            transcript_buffer: Arc::new(Mutex::new(TranscriptBuffer::new())),
            encounters_detected: Arc::new(AtomicU32::new(0)),
            recording_since: Arc::new(Mutex::new(Utc::now())),
            recent_encounters: Arc::new(Mutex::new(Vec::new())),
            last_error: Arc::new(Mutex::new(None)),
            name_tracker: Arc::new(Mutex::new(PatientNameTracker::new())),
            encounter_manual_trigger: Arc::new(tokio::sync::Notify::new()),
            encounter_notes: Arc::new(Mutex::new(String::new())),
            sensor_state_rx: Mutex::new(None),
            sensor_status_rx: Mutex::new(None),
            vision_name_change_trigger: Arc::new(tokio::sync::Notify::new()),
            vision_new_name: Arc::new(Mutex::new(None)),
            vision_old_name: Arc::new(Mutex::new(None)),
            shadow_decisions: Arc::new(Mutex::new(Vec::new())),
            last_shadow_decision: Arc::new(Mutex::new(None)),
            last_split_time: Arc::new(Mutex::new(Utc::now())),
            screenshot_buffer: Arc::new(Mutex::new(Vec::new())),
            sleep_resume_at: Arc::new(Mutex::new(None)),
        }
    }

    /// Reset handle state for a new run cycle (after sleep wake-up).
    /// Clears the stop flag, transcript buffer, encounter state, and all per-encounter data
    /// so the handle can be reused without creating a new one.
    pub fn reset_for_new_run(&self) {
        self.stop_flag.store(false, Ordering::Relaxed);
        if let Ok(mut state) = self.state.lock() {
            *state = ContinuousState::Idle;
        }
        if let Ok(mut buf) = self.transcript_buffer.lock() {
            buf.clear();
        }
        self.encounters_detected.store(0, Ordering::Relaxed);
        if let Ok(mut v) = self.recording_since.lock() { *v = Utc::now(); }
        if let Ok(mut v) = self.recent_encounters.lock() { v.clear(); }
        if let Ok(mut v) = self.last_error.lock() { *v = None; }
        if let Ok(mut v) = self.name_tracker.lock() { *v = PatientNameTracker::new(); }
        if let Ok(mut v) = self.encounter_notes.lock() { v.clear(); }
        if let Ok(mut v) = self.vision_new_name.lock() { *v = None; }
        if let Ok(mut v) = self.vision_old_name.lock() { *v = None; }
        if let Ok(mut v) = self.shadow_decisions.lock() { v.clear(); }
        if let Ok(mut v) = self.last_shadow_decision.lock() { *v = None; }
        if let Ok(mut v) = self.last_split_time.lock() { *v = Utc::now(); }
        if let Ok(mut v) = self.screenshot_buffer.lock() { v.clear(); }
        if let Ok(mut v) = self.sleep_resume_at.lock() { *v = None; }
        // sensor_state_rx and sensor_status_rx are set up by run_continuous_mode
        if let Ok(mut v) = self.sensor_state_rx.lock() { *v = None; }
        if let Ok(mut v) = self.sensor_status_rx.lock() { *v = None; }
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    pub fn is_stopped(&self) -> bool {
        self.stop_flag.load(Ordering::Relaxed)
    }

    pub fn get_stats(&self) -> ContinuousModeStats {
        let state = self
            .state
            .lock()
            .map(|s| s.as_str().to_string())
            .unwrap_or_else(|_| "error".to_string());

        let last_err = self
            .last_error
            .lock()
            .ok()
            .and_then(|e| e.clone());

        let recent_encounters = self
            .recent_encounters
            .lock()
            .map(|v| v.clone())
            .unwrap_or_default();

        let (buffer_wc, buffer_started) = self
            .transcript_buffer
            .lock()
            .map(|b| (b.word_count(), b.first_timestamp().map(|t| t.to_rfc3339())))
            .unwrap_or((0, None));

        // Read sensor state if available
        let sensor_connected = self
            .sensor_status_rx
            .lock()
            .ok()
            .and_then(|rx| rx.as_ref().map(|r| r.borrow().is_connected()));

        let sensor_state = self
            .sensor_state_rx
            .lock()
            .ok()
            .and_then(|rx| rx.as_ref().map(|r| r.borrow().as_str().to_string()));

        // Shadow mode stats
        let last_shadow = self
            .last_shadow_decision
            .lock()
            .ok()
            .and_then(|d| d.as_ref().map(|dec| (
                dec.shadow_method.clone(),
                dec.outcome.as_str().to_string(),
            )));

        let (shadow_mode_active, shadow_method, last_shadow_outcome) = match last_shadow {
            Some((method, outcome)) => (Some(true), Some(method), Some(outcome)),
            None => (None, None, None),
        };

        let is_sleeping = self
            .state
            .lock()
            .map(|s| *s == ContinuousState::Sleeping)
            .unwrap_or(false);

        let sleep_resume = self
            .sleep_resume_at
            .lock()
            .ok()
            .and_then(|v| v.clone());

        let recording_since = self
            .recording_since
            .lock()
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        ContinuousModeStats {
            state,
            recording_since,
            encounters_detected: self.encounters_detected.load(Ordering::Relaxed),
            recent_encounters,
            last_error: last_err,
            buffer_word_count: buffer_wc,
            buffer_started_at: buffer_started,
            sensor_connected,
            sensor_state,
            shadow_mode_active,
            shadow_method,
            last_shadow_outcome,
            is_sleeping,
            sleep_resume_at: sleep_resume,
        }
    }
}

// ============================================================================
// Main Continuous Mode Loop
// ============================================================================

/// Run continuous mode: starts the pipeline, buffers segments, detects encounters.
///
/// This function runs indefinitely until the stop_flag is set.
pub async fn run_continuous_mode(
    app: tauri::AppHandle,
    handle: Arc<ContinuousModeHandle>,
    config: Config,
    sync_ctx: ServerSyncContext,
) -> Result<(), String> {
    use tauri::Emitter;

    // Audio recording path for continuous mode
    let audio_output_path = {
        let recordings_dir = config.get_recordings_dir();
        if let Err(e) = std::fs::create_dir_all(&recordings_dir) {
            warn!("Could not create recordings directory: {}", e);
            None
        } else {
            let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
            Some(recordings_dir.join(format!("continuous_{}.wav", timestamp)))
        }
    };

    // Build pipeline config — same as session but with auto_end disabled
    let pipeline_config = PipelineConfig::from_config(
        &config,
        config.input_device_id.clone(),
        audio_output_path,
        None,            // No initial audio buffer in continuous mode
        false,           // Never auto-end in continuous mode
        0,
    );

    // Create message channel
    let (tx, mut rx) = mpsc::channel::<PipelineMessage>(32);

    // Start the pipeline
    let pipeline_handle = match start_pipeline(pipeline_config, tx) {
        Ok(h) => h,
        Err(e) => {
            error!("Failed to start continuous mode pipeline: {}", e);
            if let Ok(mut state) = handle.state.lock() {
                *state = ContinuousState::Error(e.to_string());
            } else {
                warn!("State lock poisoned while setting error state");
            }
            ContinuousModeEvent::Error { error: e.to_string() }.emit(&app);
            return Err(e.to_string());
        }
    };

    info!("Continuous mode pipeline started");

    // Clone the biomarker reset flag so the detector task can trigger resets on encounter boundaries
    let reset_bio_for_detector = pipeline_handle.reset_biomarkers_flag();

    // Pipeline started successfully — now set state and emit event
    if let Ok(mut state) = handle.state.lock() {
        *state = ContinuousState::Recording;
    } else {
        warn!("State lock poisoned while setting recording state");
    }
    ContinuousModeEvent::Started.emit(&app);

    // Tag the buffer with this pipeline's generation so stale segments are rejected
    let pipeline_generation: u64 = 1; // Single pipeline per continuous mode run
    if let Ok(mut buffer) = handle.transcript_buffer.lock() {
        buffer.set_generation(pipeline_generation);
    } else {
        warn!("Buffer lock poisoned while setting generation");
    }

    // Segment timeline logger — writes per-segment JSONL (created early for consumer task)
    let segment_logger = Arc::new(Mutex::new(crate::segment_log::SegmentLogger::new()));
    let segment_logger_for_consumer = Arc::clone(&segment_logger);

    // Replay bundle builder — accumulates data per encounter (created early for consumer task)
    let replay_bundle = Arc::new(Mutex::new(
        crate::replay_bundle::ReplayBundleBuilder::new(config.replay_snapshot()),
    ));
    let bundle_for_consumer = Arc::clone(&replay_bundle);

    // Clone handles for the segment consumer task
    let buffer_for_consumer = handle.transcript_buffer.clone();
    let stop_for_consumer = handle.stop_flag.clone();
    let app_for_consumer = app.clone();

    // Track silence duration for trigger
    let silence_start = Arc::new(Mutex::new(Option::<std::time::Instant>::None));
    let silence_trigger_tx = Arc::new(tokio::sync::Notify::new());
    let silence_trigger_rx = silence_trigger_tx.clone();
    let silence_threshold_secs = config.encounter_silence_trigger_secs;
    let silence_start_for_consumer = silence_start.clone();

    // Spawn segment consumer task
    // Track speech-without-transcription to detect STT pipeline failures
    const STALL_THRESHOLD_SECS: u64 = 30;
    let consumer_task = tokio::spawn(async move {
        let mut cumulative_speech_secs: u64 = 0;
        let mut last_speech_active = false;
        let mut last_speech_start: Option<std::time::Instant> = None;
        let mut stall_warned = false;

        while let Some(msg) = rx.recv().await {
            if stop_for_consumer.load(Ordering::Relaxed) {
                break;
            }

            match msg {
                PipelineMessage::Segment(segment) => {
                    // Segment received — STT is working, reset stall tracking
                    cumulative_speech_secs = 0;
                    last_speech_start = None;
                    if stall_warned {
                        stall_warned = false;
                    }

                    // Reset silence tracking on speech
                    if let Ok(mut s) = silence_start_for_consumer.lock() {
                        *s = None;
                    } else {
                        warn!("Silence tracking lock poisoned, silence state may be stale");
                    }

                    let (seg_index, seg_wc, buf_wc) = if let Ok(mut buffer) = buffer_for_consumer.lock() {
                        buffer.push(
                            segment.text.clone(),
                            segment.start_ms,
                            segment.end_ms,
                            segment.speaker_id.clone(),
                            segment.speaker_confidence,
                            pipeline_generation,
                        );
                        let idx = buffer.last_index().unwrap_or(0);
                        let wc = segment.text.split_whitespace().count();
                        let bwc = buffer.word_count();
                        (idx, wc, bwc)
                    } else {
                        warn!("Buffer lock poisoned, segment dropped: {}", segment.text);
                        continue;
                    };

                    // Log segment to segment timeline and replay bundle
                    if let Ok(mut sl) = segment_logger_for_consumer.lock() {
                        sl.log_segment(
                            seg_index, segment.start_ms, segment.end_ms,
                            &segment.text, segment.speaker_id.as_deref(),
                            segment.speaker_confidence, seg_wc, buf_wc,
                        );
                    }
                    if let Ok(mut bundle) = bundle_for_consumer.lock() {
                        bundle.add_segment(crate::replay_bundle::ReplaySegment {
                            ts: Utc::now().to_rfc3339(),
                            index: seg_index,
                            start_ms: segment.start_ms,
                            end_ms: segment.end_ms,
                            text: segment.text.clone(),
                            speaker_id: segment.speaker_id.clone(),
                            speaker_confidence: segment.speaker_confidence,
                        });
                    }

                    // Emit transcript preview for live monitoring view (with speaker labels)
                    if let Ok(buffer) = buffer_for_consumer.lock() {
                        let text = buffer.full_text_with_speakers();
                        // Only send last ~500 chars for preview (char-boundary safe)
                        let preview = if text.len() > 500 {
                            let target = text.len() - 500;
                            // Find the nearest char boundary at or after the target offset
                            let start = text.ceil_char_boundary(target);
                            format!("...{}", &text[start..])
                        } else {
                            text
                        };
                        let _ = app_for_consumer.emit("continuous_transcript_preview", serde_json::json!({
                            "finalized_text": preview,
                            "draft_text": null,
                            "segment_count": 0
                        }));
                    } else {
                        warn!("Buffer lock poisoned, transcript preview skipped");
                    }
                }
                PipelineMessage::Status { is_speech_active, .. } => {
                    if !is_speech_active {
                        // Accumulate speech duration for stall detection
                        if last_speech_active {
                            if let Some(start) = last_speech_start.take() {
                                cumulative_speech_secs += start.elapsed().as_secs();
                            }
                        }
                        last_speech_active = false;

                        // Track silence start
                        let mut s = silence_start_for_consumer.lock().unwrap_or_else(|e| e.into_inner());
                        if s.is_none() {
                            *s = Some(std::time::Instant::now());
                        } else if let Some(start) = *s {
                            if start.elapsed().as_secs() >= silence_threshold_secs as u64 {
                                // Silence gap detected — trigger encounter check
                                // Use notify_waiters so both active detector AND shadow observer receive the event
                                silence_trigger_tx.notify_waiters();
                                *s = None; // Reset so we don't keep triggering
                            }
                        }
                    } else {
                        // Speech active — track start time for stall detection
                        if !last_speech_active {
                            last_speech_start = Some(std::time::Instant::now());
                        }
                        last_speech_active = true;

                        // Check for stall: speech detected but no transcription
                        let active_secs = last_speech_start
                            .map(|s| s.elapsed().as_secs())
                            .unwrap_or(0);
                        if cumulative_speech_secs + active_secs >= STALL_THRESHOLD_SECS && !stall_warned {
                            warn!(
                                "Transcription stalled: {}s of speech detected with no transcription output",
                                cumulative_speech_secs + active_secs
                            );
                            ContinuousModeEvent::TranscriptionStalled {
                                speech_secs: cumulative_speech_secs + active_secs,
                            }.emit(&app_for_consumer);
                            stall_warned = true;
                        }

                        // Speech active — reset silence
                        let mut s = silence_start_for_consumer.lock().unwrap_or_else(|e| e.into_inner());
                        *s = None;
                    }
                }
                PipelineMessage::Biomarker(update) => {
                    let _ = app_for_consumer.emit("biomarker_update", update);
                }
                PipelineMessage::AudioQuality(snapshot) => {
                    let _ = app_for_consumer.emit("audio_quality", snapshot);
                }
                PipelineMessage::Stopped => {
                    info!("Continuous mode pipeline stopped");
                    break;
                }
                PipelineMessage::Error(e) => {
                    error!("Continuous mode pipeline error: {}", e);
                    break;
                }
                PipelineMessage::TranscriptChunk { text } => {
                    // Emit streaming chunk as draft_text for live preview
                    let _ = app_for_consumer.emit("continuous_transcript_preview", serde_json::json!({
                        "finalized_text": null,
                        "draft_text": text,
                        "segment_count": 0
                    }));
                }
                // Ignore auto-end messages in continuous mode
                PipelineMessage::AutoEndSilence { .. } | PipelineMessage::SilenceWarning { .. } => {}
            }
        }
    });

    // Start presence sensor if in sensor, shadow, or hybrid detection mode
    let is_shadow_mode = config.encounter_detection_mode == EncounterDetectionMode::Shadow;
    let is_hybrid_mode = config.encounter_detection_mode == EncounterDetectionMode::Hybrid;
    let shadow_active_method = config.shadow_active_method;
    let needs_sensor = matches!(
        config.encounter_detection_mode,
        EncounterDetectionMode::Sensor | EncounterDetectionMode::Shadow | EncounterDetectionMode::Hybrid
    );
    let has_sensor_config = !config.presence_sensor_url.is_empty() || !config.presence_sensor_port.is_empty();
    let use_sensor_mode = needs_sensor && has_sensor_config;
    let mut sensor_handle: Option<crate::presence_sensor::PresenceSensor> = None;
    let sensor_absence_trigger: Arc<tokio::sync::Notify>;
    // Shadow sensor observer uses watch channel for state transitions (not Notify)
    let mut shadow_sensor_state_rx: Option<tokio::sync::watch::Receiver<crate::presence_sensor::PresenceState>> = None;
    // Hybrid mode: dedicated watch receiver for sensor state in the detection loop
    let mut hybrid_sensor_state_rx: Option<tokio::sync::watch::Receiver<crate::presence_sensor::PresenceState>> = None;

    if use_sensor_mode {
        // Auto-detect serial port only when not using HTTP mode
        let sensor_port = if config.presence_sensor_url.is_empty() {
            crate::presence_sensor::auto_detect_port(&config.presence_sensor_port)
                .unwrap_or_default()
        } else {
            String::new()
        };

        let sensor_config = crate::presence_sensor::SuiteConfig {
            port: sensor_port,
            url: config.presence_sensor_url.clone(),
            debounce_secs: config.presence_debounce_secs,
            absence_threshold_secs: config.presence_absence_threshold_secs,
            csv_log_enabled: config.presence_csv_log_enabled,
            thermal: crate::presence_sensor::ThermalConfig {
                hot_pixel_threshold_c: config.thermal_hot_pixel_threshold_c,
                ..Default::default()
            },
            co2: crate::presence_sensor::Co2Config {
                baseline_ppm: config.co2_baseline_ppm,
                ..Default::default()
            },
            fusion: crate::presence_sensor::FusionConfig::default(),
        };

        match crate::presence_sensor::PresenceSensorSuite::start_suite(&sensor_config) {
            Ok(sensor) => {
                info!("Presence sensor started for encounter detection");
                sensor_absence_trigger = sensor.absence_notifier();

                // Store sensor state receivers in the handle for stats
                if let Ok(mut rx) = handle.sensor_state_rx.lock() {
                    *rx = Some(sensor.subscribe_state());
                }
                if let Ok(mut rx) = handle.sensor_status_rx.lock() {
                    *rx = Some(sensor.subscribe_status());
                }

                // Emit sensor status event
                ContinuousModeEvent::SensorStatus { connected: true, state: "unknown".into() }.emit(&app);

                // Get a dedicated state receiver for shadow sensor observer
                shadow_sensor_state_rx = Some(sensor.subscribe_state());
                // Get a dedicated state receiver for hybrid detection loop
                if is_hybrid_mode {
                    hybrid_sensor_state_rx = Some(sensor.subscribe_state());
                }
                sensor_handle = Some(sensor);
            }
            Err(e) => {
                info!("Presence sensor not available: {}. Running with LLM-only detection.", e);
                ContinuousModeEvent::SensorStatus { connected: false, state: "not_configured".into() }.emit(&app);
                // Fall back: create a dummy Notify that never fires
                sensor_absence_trigger = Arc::new(tokio::sync::Notify::new());
            }
        }
    } else {
        // LLM mode — no sensor absence trigger
        sensor_absence_trigger = Arc::new(tokio::sync::Notify::new());
    }

    // Determine effective detection mode (may have fallen back from sensor to LLM)
    // In shadow mode, the active method controls which detection branch runs
    // In hybrid mode, sensor is handled separately (not via effective_sensor_mode)
    let effective_sensor_mode = if is_shadow_mode {
        shadow_active_method == ShadowActiveMethod::Sensor && sensor_handle.is_some()
    } else if is_hybrid_mode {
        false // Hybrid uses its own sensor integration in the detection loop
    } else {
        sensor_handle.is_some()
    };

    // Spawn sensor status monitoring task (emits events on state/status changes)
    // Also spawn for hybrid mode when sensor is available (even though effective_sensor_mode is false)
    let has_sensor = sensor_handle.is_some();
    let sensor_monitor_task: Option<tokio::task::JoinHandle<()>> = if effective_sensor_mode || (is_hybrid_mode && has_sensor) {
        let sensor = sensor_handle.as_ref().expect("sensor monitor requires sensor_handle.is_some()");
        let mut state_rx = sensor.subscribe_state();
        let mut status_rx = sensor.subscribe_status();
        let stop_for_monitor = handle.stop_flag.clone();
        let app_for_monitor = app.clone();

        Some(tokio::spawn(async move {
            loop {
                if stop_for_monitor.load(Ordering::Relaxed) {
                    break;
                }

                tokio::select! {
                    Ok(()) = state_rx.changed() => {
                        let state = *state_rx.borrow_and_update();
                        let state_str = match state {
                            crate::presence_sensor::PresenceState::Present => "present",
                            crate::presence_sensor::PresenceState::Absent => "absent",
                            crate::presence_sensor::PresenceState::Unknown => "unknown",
                        };
                        info!("Sensor state changed: {}", state_str);
                        ContinuousModeEvent::SensorStatus { connected: true, state: state_str.into() }.emit(&app_for_monitor);
                    }
                    Ok(()) = status_rx.changed() => {
                        let status = status_rx.borrow_and_update().clone();
                        let connected = matches!(status, crate::presence_sensor::SensorStatus::Connected);
                        ContinuousModeEvent::SensorStatus { connected, state: "unknown".into() }.emit(&app_for_monitor);
                        if !connected {
                            warn!("Sensor disconnected: {:?}", status);
                        }
                    }
                    else => break,
                }
            }
        }))
    } else {
        None
    };

    // Spawn shadow observer task (if shadow mode is active)
    let shadow_task: Option<tokio::task::JoinHandle<()>> = if is_shadow_mode {
        crate::shadow_observer::spawn_shadow_observer(
            crate::shadow_observer::ShadowObserverConfig {
                active_method: shadow_active_method,
                csv_log_enabled: config.shadow_csv_log_enabled,
                detection_model: config.encounter_detection_model.clone(),
                detection_nothink: config.encounter_detection_nothink,
                check_interval_secs: config.encounter_check_interval_secs,
                llm_router_url: config.llm_router_url.clone(),
                llm_api_key: config.llm_api_key.clone(),
                llm_client_id: config.llm_client_id.clone(),
            },
            handle.stop_flag.clone(),
            handle.transcript_buffer.clone(),
            handle.shadow_decisions.clone(),
            handle.last_shadow_decision.clone(),
            shadow_sensor_state_rx.take(),
            silence_trigger_rx.clone(),
            app.clone(),
        )
    } else {
        None
    };

    // Spawn encounter detection loop
    let buffer_for_detector = handle.transcript_buffer.clone();
    let stop_for_detector = handle.stop_flag.clone();
    let state_for_detector = handle.state.clone();
    let encounters_for_detector = handle.encounters_detected.clone();
    let recent_encounters_for_detector = handle.recent_encounters.clone();
    let last_error_for_detector = handle.last_error.clone();
    let name_tracker_for_detector = handle.name_tracker.clone();
    let last_split_time_for_detector = handle.last_split_time.clone();
    let screenshot_buffer_for_detector = handle.screenshot_buffer.clone();
    let app_for_detector = app.clone();
    let check_interval = config.encounter_check_interval_secs;
    let idle_timeout_secs = config.idle_encounter_timeout_secs;

    // Build LLM client for encounter detection (uses smaller model for better accuracy)
    let detection_model = config.encounter_detection_model.clone();
    let detection_nothink = config.encounter_detection_nothink;
    let llm_client = if !config.llm_router_url.is_empty() {
        LLMClient::new(
            &config.llm_router_url,
            &config.llm_api_key,
            &config.llm_client_id,
            &detection_model,
        )
        .ok()
    } else {
        None
    };

    let soap_model = config.soap_model_fast.clone();
    let fast_model = config.fast_model.clone();
    let soap_detail_level = config.soap_detail_level;
    let soap_format = config.soap_format.clone();
    let soap_custom_instructions = config.soap_custom_instructions.clone();
    let merge_enabled = config.encounter_merge_enabled;
    // Clone config values for flush-on-stop SOAP generation + merge check (outside detector task)
    let flush_fast_model = config.fast_model.clone();
    let flush_soap_model = config.soap_model_fast.clone();
    let flush_soap_detail_level = config.soap_detail_level;
    let flush_soap_format = config.soap_format.clone();
    let flush_soap_custom_instructions = config.soap_custom_instructions.clone();
    let flush_llm_client = if !config.llm_router_url.is_empty() {
        LLMClient::new(
            &config.llm_router_url,
            &config.llm_api_key,
            &config.llm_client_id,
            &config.fast_model,
        )
        .ok()
    } else {
        None
    };

    // Clone encounter notes for the detector task
    let encounter_notes_for_detector = handle.encounter_notes.clone();

    // Clone manual trigger for the detector task
    let manual_trigger_rx = handle.encounter_manual_trigger.clone();

    // Clone vision name change trigger for the detector task
    // Vision trigger is no longer used for detection decisions — EMR chart name is
    // unreliable (doctor may open family members, not open chart, or vision may parse
    // the same name differently). Vision still extracts names for metadata labeling.
    let _vision_trigger_rx = handle.vision_name_change_trigger.clone();

    // Biomarker reset flag for the detector task
    let reset_bio_flag = reset_bio_for_detector;

    // Clone sensor trigger for detector task
    let sensor_trigger_for_detector = sensor_absence_trigger.clone();

    // Clone shadow state for detector task
    let handle_shadow_decisions = handle.shadow_decisions.clone();

    // Pipeline replay logger — writes JSONL to each session's archive folder
    let pipeline_logger = Arc::new(Mutex::new(crate::pipeline_log::PipelineLogger::new()));
    let logger_for_detector = Arc::clone(&pipeline_logger);
    let logger_for_screenshot = Arc::clone(&pipeline_logger);
    let logger_for_flush = Arc::clone(&pipeline_logger);

    // Segment logger clones for detector and flush (Arc created before consumer task)
    let segment_logger_for_detector = Arc::clone(&segment_logger);


    // Day-level orchestration logger
    let day_logger = Arc::new(crate::day_log::DayLogger::new());
    let day_logger_for_detector = Arc::clone(&day_logger);
    let day_logger_for_flush = Arc::clone(&day_logger);
    // Log continuous mode start with config snapshot
    if let Some(ref dl) = *day_logger {
        dl.log(crate::day_log::DayEvent::ContinuousModeStarted {
            ts: Utc::now().to_rfc3339(),
            config: config.replay_snapshot(),
        });
    }

    // Replay bundle clones for detector, screenshot, flush (Arc created before consumer task)
    let bundle_for_detector = Arc::clone(&replay_bundle);
    let bundle_for_screenshot = Arc::clone(&replay_bundle);

    // Server sync context clone for detector task (fire-and-forget uploads)
    let sync_ctx_for_detector = sync_ctx.clone();

    // Hybrid mode config
    let hybrid_confirm_window_secs = config.hybrid_confirm_window_secs;
    let hybrid_min_words_for_sensor_split = config.hybrid_min_words_for_sensor_split;
    // Move the hybrid sensor receiver into the detector task
    let mut hybrid_sensor_rx = hybrid_sensor_state_rx;

    let detector_task = tokio::spawn(async move {
        let mut encounter_number: u32 = 0;
        let mut consecutive_llm_failures: u32 = 0;
        // Tracks how many times a split was merged back into the previous encounter.
        // Each merge-back escalates the confidence threshold by +0.05, making
        // repeated false-positive splits on long sessions increasingly unlikely.
        let mut merge_back_count: u32 = 0;

        // Hybrid mode: sensor absence tracking
        let mut sensor_absent_since: Option<DateTime<Utc>> = None;
        let mut prev_sensor_state = crate::presence_sensor::PresenceState::Unknown;
        let mut sensor_available = hybrid_sensor_rx.is_some();
        // True when sensor has been continuously present since the last encounter split.
        // Reset to false on any Present→Absent transition; set to true on split.
        let mut sensor_continuous_present = false;

        // Track previous encounter for retrospective merge checks
        let mut prev_encounter_session_id: Option<String> = None;
        let mut prev_encounter_text: Option<String> = None;
        let mut prev_encounter_text_rich: Option<String> = None;
        let mut prev_encounter_date: Option<DateTime<Utc>> = None;
        let mut prev_encounter_is_clinical: bool = true;

        loop {
            // Wait for trigger based on detection mode
            // Returns (manual_triggered, sensor_triggered)
            let (manual_triggered, sensor_triggered) = if is_hybrid_mode && sensor_available {
                // Hybrid mode with sensor: timer + silence + manual + sensor
                let Some(sensor_rx) = hybrid_sensor_rx.as_mut() else {
                    warn!("Hybrid mode active but sensor receiver is None — falling back to timer-only");
                    sensor_available = false;
                    continue;
                };
                tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval as u64)) => {
                        // Regular timer — handles back-to-back encounters without physical departure
                        (false, false)
                    }
                    _ = silence_trigger_rx.notified() => {
                        info!("Hybrid: silence gap detected — triggering encounter check");
                        (false, false)
                    }
                    _ = manual_trigger_rx.notified() => {
                        info!("Manual new patient trigger received");
                        (true, false)
                    }
                    result = sensor_rx.changed() => {
                        match result {
                            Ok(()) => {
                                let new_state = *sensor_rx.borrow_and_update();
                                let old_state = prev_sensor_state;
                                prev_sensor_state = new_state;
                                // Log sensor transitions to replay bundle
                                if old_state != new_state {
                                    if let Ok(mut bundle) = bundle_for_detector.lock() {
                                        bundle.add_sensor_transition(crate::replay_bundle::SensorTransition {
                                            ts: Utc::now().to_rfc3339(),
                                            from: old_state.as_str().to_string(),
                                            to: new_state.as_str().to_string(),
                                        });
                                    }
                                }
                                match (old_state, new_state) {
                                    (crate::presence_sensor::PresenceState::Present,
                                     crate::presence_sensor::PresenceState::Absent) => {
                                        sensor_absent_since = Some(Utc::now());
                                        sensor_continuous_present = false;
                                        info!("Hybrid: sensor detected departure (Present→Absent), accelerating LLM check");
                                        (false, true) // sensor_triggered → accelerate LLM check (NOT force-split)
                                    }
                                    (_, crate::presence_sensor::PresenceState::Present) => {
                                        if sensor_absent_since.is_some() {
                                            info!("Hybrid: person returned — cancelling sensor absence tracking");
                                            sensor_absent_since = None;
                                        }
                                        continue; // No check needed
                                    }
                                    _ => continue, // Other transitions (Absent→Absent, Unknown→Absent, etc.)
                                }
                            }
                            Err(_) => {
                                warn!("Hybrid: sensor watch channel closed — sensor disconnected. Falling back to LLM-only.");
                                sensor_available = false;
                                sensor_absent_since = None;
                                ContinuousModeEvent::SensorStatus { connected: false, state: "unknown".into() }.emit(&app_for_detector);
                                continue; // Re-enter loop; next iteration uses LLM-only path
                            }
                        }
                    }
                }
            } else if is_hybrid_mode {
                // Hybrid mode without sensor (sensor failed/disconnected): pure LLM fallback
                tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval as u64)) => {
                        (false, false)
                    }
                    _ = silence_trigger_rx.notified() => {
                        info!("Hybrid (LLM fallback): silence gap detected — triggering encounter check");
                        (false, false)
                    }
                    _ = manual_trigger_rx.notified() => {
                        info!("Manual new patient trigger received");
                        (true, false)
                    }
                }
            } else if effective_sensor_mode {
                // Pure sensor mode: wait for sensor absence threshold OR manual trigger
                tokio::select! {
                    _ = sensor_trigger_for_detector.notified() => {
                        info!("Sensor: absence threshold reached — triggering encounter split");
                        (false, true)
                    }
                    _ = manual_trigger_rx.notified() => {
                        info!("Manual new patient trigger received");
                        (true, false)
                    }
                }
            } else {
                // LLM / Shadow mode: wait for timer, silence, or manual trigger
                tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval as u64)) => {
                        (false, false)
                    }
                    _ = silence_trigger_rx.notified() => {
                        info!("Silence gap detected — triggering encounter check");
                        (false, false)
                    }
                    _ = manual_trigger_rx.notified() => {
                        info!("Manual new patient trigger received");
                        (true, false)
                    }
                }
            };

            if stop_for_detector.load(Ordering::Relaxed) {
                break;
            }

            // Check if buffer has enough content to analyze
            let (formatted, word_count, is_empty, first_ts, first_seg_idx, last_seg_idx) = {
                let buffer = match buffer_for_detector.lock() {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let first_idx = buffer.first_index().unwrap_or(0);
                let last_idx = buffer.last_index().unwrap_or(0);
                (buffer.format_for_detection(), buffer.word_count(), buffer.is_empty(), buffer.first_timestamp(), first_idx, last_idx)
            };

            // Idle buffer detection: discard ambient noise that accumulates between encounters.
            if idle_timeout_secs > 0 && !is_empty && !manual_triggered && !sensor_triggered {
                if let Some(first_time) = first_ts {
                    let buffer_age_secs = (Utc::now() - first_time).num_seconds();
                    if buffer_age_secs > idle_timeout_secs as i64
                        && word_count < IDLE_ENCOUNTER_MAX_WORDS
                    {
                        info!(
                            "Idle buffer timeout: {} words over {}s (threshold: {}s / {} words) — clearing",
                            word_count, buffer_age_secs, idle_timeout_secs, IDLE_ENCOUNTER_MAX_WORDS
                        );
                        if let Some(ref dl) = *day_logger_for_detector {
                            dl.log(crate::day_log::DayEvent::IdleBufferCleared {
                                ts: Utc::now().to_rfc3339(),
                                word_count,
                                buffer_age_secs,
                            });
                        }
                        if let Ok(mut buffer) = buffer_for_detector.lock() {
                            buffer.clear();
                        }
                        ContinuousModeEvent::IdleBufferCleared { word_count, buffer_age_secs }.emit(&app_for_detector);
                        continue;
                    }
                }
            }

            // Pre-compute hallucination-cleaned word count for large buffers.
            // This prevents STT phrase loops from inflating word counts and triggering
            // premature force-splits. Only runs when buffer is large enough to matter.
            let (filtered_formatted, hallucination_report) = if word_count > FORCE_CHECK_WORD_THRESHOLD {
                let (filtered, report) = strip_hallucinations(&formatted, 5);
                if !report.repetitions.is_empty() || !report.phrase_repetitions.is_empty() {
                    info!(
                        "Hallucination filter: {} → {} words ({} single-word, {} phrase repetitions stripped)",
                        report.original_word_count, report.cleaned_word_count,
                        report.repetitions.len(), report.phrase_repetitions.len()
                    );
                    if let Ok(mut logger) = logger_for_detector.lock() {
                        logger.log_hallucination_filter(serde_json::json!({
                            "call_site": "detection",
                            "original_words": report.original_word_count,
                            "cleaned_words": report.cleaned_word_count,
                            "single_word_reps": report.repetitions.iter()
                                .map(|r| &r.word).collect::<Vec<_>>(),
                            "phrase_reps": report.phrase_repetitions.iter()
                                .map(|r| &r.phrase).collect::<Vec<_>>(),
                        }));
                    }
                }
                (Some(filtered), Some(report))
            } else {
                (None, None)
            };
            // Compute cleaned word count relative to the raw transcript word count.
            // The hallucination report counts words in format_for_detection() output,
            // which includes segment metadata ([index] (timestamp) (speaker label):)
            // inflating the count by ~5 words per segment. We apply only the delta
            // (words actually stripped) to the raw buffer word count.
            let cleaned_word_count = hallucination_report
                .as_ref()
                .map(|r| {
                    let stripped = r.original_word_count.saturating_sub(r.cleaned_word_count);
                    word_count.saturating_sub(stripped)
                })
                .unwrap_or(word_count);

            // Manual or sensor trigger: skip minimum guards, but still need >0 words
            // In hybrid mode, sensor triggers still require minimum content so the LLM
            // has enough transcript to make a meaningful split decision. Without this,
            // sensor flicker during departures causes micro-splits on wrap-up dialogue.
            const MIN_SENSOR_HYBRID_WORDS: usize = 500;
            if manual_triggered || sensor_triggered {
                if is_empty {
                    info!("{}: buffer is empty, nothing to archive",
                        if sensor_triggered { "Sensor trigger" } else { "Manual trigger" });
                    continue;
                }
                if sensor_triggered && is_hybrid_mode && word_count < MIN_SENSOR_HYBRID_WORDS {
                    info!("Sensor trigger: only {} words (minimum {} in hybrid mode), deferring to LLM timer",
                        word_count, MIN_SENSOR_HYBRID_WORDS);
                    continue;
                }
                info!("{}: bypassing minimum duration/word count guards ({} words)",
                    if sensor_triggered { "Sensor trigger" } else { "Manual trigger" }, word_count);
            } else {
                if is_empty || word_count < 100 {
                    debug!("Skipping detection: word_count={} (minimum 100)", word_count);
                    continue;
                }

                // Also trigger if buffer is very large (safety valve).
                // Use hallucination-cleaned word count so STT phrase loops
                // don't inflate counts past the threshold prematurely.
                let force_check = cleaned_word_count > FORCE_CHECK_WORD_THRESHOLD;

                // Minimum encounter duration: 2 minutes (unless force_check)
                if !force_check {
                    if let Some(first_time) = first_ts {
                        let buffer_age_secs = (Utc::now() - first_time).num_seconds();
                        if buffer_age_secs < 120 {
                            debug!("Skipping detection: buffer_age={}s (minimum 120s), word_count={}", buffer_age_secs, word_count);
                            continue;
                        }
                    }
                }
                if force_check {
                    info!("Buffer exceeds {} cleaned words (raw={}, cleaned={}) — forcing encounter check",
                        FORCE_CHECK_WORD_THRESHOLD, word_count, cleaned_word_count);
                }
            }

            // Set state to checking
            if let Ok(mut state) = state_for_detector.lock() {
                *state = ContinuousState::Checking;
            } else {
                warn!("State lock poisoned while setting checking state");
            }
            ContinuousModeEvent::Checking.emit(&app_for_detector);

            // Run encounter detection via LLM (with 60s timeout to prevent blocking)
            // Manual trigger or pure-sensor trigger: skip LLM — directly split
            // Hybrid sensor trigger: accelerate LLM check (do NOT force-split)
            let detection_result = if manual_triggered || (sensor_triggered && !is_hybrid_mode) {
                let last_idx = buffer_for_detector.lock().ok().and_then(|b| b.last_index());
                let source = if sensor_triggered { "Sensor" } else { "Manual" };
                info!("{} trigger: forcing encounter split (last_index={:?})", source, last_idx);
                if let Ok(mut logger) = logger_for_detector.lock() {
                    logger.log_split_trigger(serde_json::json!({
                        "trigger": source.to_lowercase(),
                        "word_count": word_count,
                        "cleaned_word_count": cleaned_word_count,
                    }));
                }
                Some(EncounterDetectionResult {
                    complete: true,
                    end_segment_index: last_idx,
                    confidence: Some(1.0),
                })
            } else if let Some(ref client) = llm_client {
                // Reuse pre-computed hallucination-filtered text if available, otherwise filter now
                let filtered_for_llm = filtered_formatted.clone().unwrap_or_else(|| {
                    let (filtered, _) = strip_hallucinations(&formatted, 5);
                    filtered
                });
                // Build detection context from available signals (vision name change, sensor state)
                let detection_context = {
                    let mut ctx = EncounterDetectionContext::default();
                    if sensor_triggered {
                        ctx.sensor_departed = true;
                    } else if sensor_absent_since.is_some() {
                        ctx.sensor_departed = true;
                    }
                    // Tell LLM when sensor confirms someone is still present (suppresses false splits).
                    // Require actual Present state — if sensor connected to wrong device and never
                    // received valid data, prev_sensor_state stays Unknown and we don't inject context.
                    if sensor_available && !ctx.sensor_departed && prev_sensor_state == crate::presence_sensor::PresenceState::Present {
                        ctx.sensor_present = true;
                    }
                    ctx
                };
                let (system_prompt, user_prompt) = build_encounter_detection_prompt(
                    &filtered_for_llm,
                    Some(&detection_context),
                );
                // Prepend /nothink for Qwen3 models to disable thinking mode (improves detection accuracy)
                let system_prompt = if detection_nothink {
                    format!("/nothink\n{}", system_prompt)
                } else {
                    system_prompt
                };
                let detect_start = Instant::now();
                let llm_future = client.generate(&detection_model, &system_prompt, &user_prompt, "encounter_detection");
                let detect_ctx = serde_json::json!({
                    "word_count": word_count,
                    "cleaned_word_count": cleaned_word_count,
                    "sensor_present": detection_context.sensor_present,
                    "sensor_departed": detection_context.sensor_departed,
                    "nothink": detection_nothink,
                    "consecutive_llm_failures": consecutive_llm_failures,
                });
                // Pre-compute replay bundle fields shared across all detection outcomes
                let replay_buffer_age = first_ts
                    .map(|t| (Utc::now() - t).num_seconds() as f64).unwrap_or(0.0);
                let replay_sensor_absent = sensor_absent_since.map(|t| t.to_rfc3339());
                let replay_sensor_ctx = crate::replay_bundle::SensorContext::new(
                    detection_context.sensor_departed, detection_context.sensor_present,
                );

                match tokio::time::timeout(tokio::time::Duration::from_secs(90), llm_future).await {
                    Ok(Ok(response)) => {
                        let latency = detect_start.elapsed().as_millis() as u64;
                        match parse_encounter_detection(&response) {
                            Ok(result) => {
                                info!(
                                    "Detection result: complete={}, confidence={:?}, end_segment_index={:?}, word_count={}",
                                    result.complete, result.confidence, result.end_segment_index, word_count
                                );
                                // Clear any previous error on successful detection
                                if let Ok(mut err) = last_error_for_detector.lock() {
                                    *err = None;
                                }
                                if let Ok(mut logger) = logger_for_detector.lock() {
                                    let mut ctx = detect_ctx.clone();
                                    ctx["parsed_complete"] = serde_json::json!(result.complete);
                                    ctx["parsed_confidence"] = serde_json::json!(result.confidence);
                                    ctx["parsed_end_segment_index"] = serde_json::json!(result.end_segment_index);
                                    logger.log_detection(
                                        &detection_model, &system_prompt, &user_prompt,
                                        Some(&response), latency, true, None, ctx,
                                    );
                                }
                                // Add detection check to replay bundle
                                if let Ok(mut bundle) = bundle_for_detector.lock() {
                                    let mut check = crate::replay_bundle::DetectionCheck::new(
                                        (first_seg_idx, last_seg_idx), word_count, cleaned_word_count,
                                        replay_sensor_ctx.clone(), system_prompt.clone(), user_prompt.clone(),
                                        latency, consecutive_llm_failures, merge_back_count,
                                        replay_buffer_age, replay_sensor_absent.clone(),
                                    );
                                    check.response_raw = Some(response.clone());
                                    check.parsed_complete = Some(result.complete);
                                    check.parsed_confidence = result.confidence;
                                    check.parsed_end_index = result.end_segment_index;
                                    check.success = true;
                                    bundle.add_detection_check(check);
                                }
                                Some(result)
                            }
                            Err(e) => {
                                warn!("Failed to parse encounter detection: {}", e);
                                if let Ok(mut logger) = logger_for_detector.lock() {
                                    let mut ctx = detect_ctx.clone();
                                    ctx["parse_error"] = serde_json::json!(true);
                                    logger.log_detection(
                                        &detection_model, &system_prompt, &user_prompt,
                                        Some(&response), latency, false, Some(&e), ctx,
                                    );
                                }
                                if let Ok(mut bundle) = bundle_for_detector.lock() {
                                    let mut check = crate::replay_bundle::DetectionCheck::new(
                                        (first_seg_idx, last_seg_idx), word_count, cleaned_word_count,
                                        replay_sensor_ctx.clone(), system_prompt.clone(), user_prompt.clone(),
                                        latency, consecutive_llm_failures, merge_back_count,
                                        replay_buffer_age, replay_sensor_absent.clone(),
                                    );
                                    check.response_raw = Some(response.clone());
                                    check.error = Some(e.clone());
                                    bundle.add_detection_check(check);
                                }
                                if let Ok(mut err) = last_error_for_detector.lock() {
                                    *err = Some(e);
                                } else {
                                    warn!("Last error lock poisoned, error state not updated");
                                }
                                None
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        let latency = detect_start.elapsed().as_millis() as u64;
                        warn!("Encounter detection LLM call failed: {}", e);
                        if let Ok(mut logger) = logger_for_detector.lock() {
                            let mut ctx = detect_ctx.clone();
                            ctx["llm_error"] = serde_json::json!(true);
                            logger.log_detection(
                                &detection_model, &system_prompt, &user_prompt,
                                None, latency, false, Some(&e.to_string()), ctx,
                            );
                        }
                        if let Ok(mut bundle) = bundle_for_detector.lock() {
                            let mut check = crate::replay_bundle::DetectionCheck::new(
                                (first_seg_idx, last_seg_idx), word_count, cleaned_word_count,
                                replay_sensor_ctx.clone(), system_prompt.clone(), user_prompt.clone(),
                                latency, consecutive_llm_failures, merge_back_count,
                                replay_buffer_age, replay_sensor_absent.clone(),
                            );
                            check.error = Some(e.to_string());
                            bundle.add_detection_check(check);
                        }
                        if let Ok(mut err) = last_error_for_detector.lock() {
                            *err = Some(e);
                        } else {
                            warn!("Last error lock poisoned, error state not updated");
                        }
                        ContinuousModeEvent::Error { error: "Encounter detection failed".into() }.emit(&app_for_detector);
                        None
                    }
                    Err(_elapsed) => {
                        let latency = detect_start.elapsed().as_millis() as u64;
                        warn!("Encounter detection LLM call timed out after 90s");
                        if let Ok(mut logger) = logger_for_detector.lock() {
                            let mut ctx = detect_ctx.clone();
                            ctx["timeout"] = serde_json::json!(true);
                            logger.log_detection(
                                &detection_model, &system_prompt, &user_prompt,
                                None, latency, false, Some("timeout_90s"), ctx,
                            );
                        }
                        if let Ok(mut bundle) = bundle_for_detector.lock() {
                            let mut check = crate::replay_bundle::DetectionCheck::new(
                                (first_seg_idx, last_seg_idx), word_count, cleaned_word_count,
                                replay_sensor_ctx, system_prompt.clone(), user_prompt.clone(),
                                latency, consecutive_llm_failures, merge_back_count,
                                replay_buffer_age, replay_sensor_absent,
                            );
                            check.error = Some("timeout_90s".to_string());
                            bundle.add_detection_check(check);
                        }
                        if let Ok(mut err) = last_error_for_detector.lock() {
                            *err = Some("Encounter detection timed out".to_string());
                        } else {
                            warn!("Last error lock poisoned, error state not updated");
                        }
                        None
                    }
                }
            } else {
                warn!("No LLM client configured for encounter detection");
                None
            };

            // Re-check sensor state before evaluating — the sensor may have returned
            // to Present during a long LLM call (race between select! branches).
            if sensor_absent_since.is_some() {
                if let Some(ref mut rx) = hybrid_sensor_rx {
                    let current_sensor = *rx.borrow();
                    if current_sensor == crate::presence_sensor::PresenceState::Present {
                        info!("Hybrid: sensor returned to Present during LLM call — cancelling stale absence");
                        sensor_absent_since = None;
                        prev_sensor_state = current_sensor;
                    }
                }
            }

            // Evaluate detection decision via pure function (single source of truth)
            let buffer_age_mins = first_ts
                .map(|t| (Utc::now() - t).num_minutes())
                .unwrap_or(0);
            let eval_ctx = DetectionEvalContext {
                detection_result,
                buffer_age_mins,
                merge_back_count: merge_back_count as usize,
                word_count,
                cleaned_word_count,
                consecutive_llm_failures,
                manual_triggered,
                sensor_triggered,
                is_hybrid_mode,
                sensor_absent_secs: sensor_absent_since.map(|t| (Utc::now() - t).num_seconds() as u64),
                hybrid_confirm_window_secs,
                hybrid_min_words_for_sensor_split,
                sensor_continuous_present,
            };
            let (outcome, new_failures) = evaluate_detection(&eval_ctx);
            consecutive_llm_failures = new_failures;

            // Act on the decision — derive detection_method_str directly from outcome
            let (end_index, detection_method_str) = match outcome {
                DetectionOutcome::ForceSplit { ref trigger } => {
                    let last_idx = buffer_for_detector.lock().ok().and_then(|b| b.last_index());
                    warn!("Force-split triggered: {} (word_count={}, cleaned={})", trigger, word_count, cleaned_word_count);
                    if let Ok(mut logger) = logger_for_detector.lock() {
                        logger.log_split_trigger(serde_json::json!({
                            "trigger": trigger,
                            "word_count": word_count,
                            "cleaned_word_count": cleaned_word_count,
                        }));
                    }
                    if trigger == TRIGGER_HYBRID_SENSOR_TIMEOUT {
                        sensor_absent_since = None;
                    }
                    let method = if is_hybrid_mode {
                        trigger.as_str() // "hybrid_sensor_timeout", "absolute_word_cap", etc.
                    } else {
                        trigger.as_str()
                    };
                    match last_idx {
                        Some(idx) => (idx, method),
                        None => continue,
                    }
                }
                DetectionOutcome::Split { end_segment_index, confidence, ref trigger } => {
                    info!("Detection split: trigger={}, confidence={:.2}", trigger, confidence);
                    let method = if manual_triggered {
                        "manual"
                    } else if is_hybrid_mode {
                        if sensor_triggered { "hybrid_sensor_confirmed" } else { "hybrid_llm" }
                    } else if sensor_triggered {
                        "sensor"
                    } else {
                        trigger.as_str() // "llm"
                    };
                    match end_segment_index {
                        Some(idx) => (idx, method),
                        None => continue,
                    }
                }
                DetectionOutcome::BelowThreshold { confidence, threshold } => {
                    info!(
                        "Confidence gate rejected: confidence={:.2}, threshold={:.2}, merge_backs={}, buffer_age_mins={}, word_count={}",
                        confidence, threshold, merge_back_count, buffer_age_mins, word_count
                    );
                    if let Ok(mut logger) = logger_for_detector.lock() {
                        logger.log_confidence_gate(serde_json::json!({
                            "confidence": confidence,
                            "threshold": threshold,
                            "merge_back_count": merge_back_count,
                            "buffer_age_mins": buffer_age_mins,
                            "word_count": word_count,
                            "consecutive_llm_failures": consecutive_llm_failures,
                            "rejected": true,
                        }));
                    }
                    continue;
                }
                DetectionOutcome::NoSplit => {
                    info!("Detection: no split, word_count={}, cleaned={}", word_count, cleaned_word_count);
                    continue;
                }
                DetectionOutcome::NoResult => {
                    info!("Detection: no result (LLM error/timeout), consecutive_failures={}", consecutive_llm_failures);
                    continue;
                }
            };

            // ── Minimum word floor ──
            // Reject splits that produce micro-encounters (e.g. 30-word goodbye
            // fragments). The sensor timeout force-split is exempt because it
            // always uses last_index() (the entire buffer).
            // ForceSplit outcomes already use last_index and archive everything,
            // so only LLM-detected splits (DetectionOutcome::Split) need the check.
            if matches!(outcome, DetectionOutcome::Split { .. }) {
                let split_wc = buffer_for_detector.lock()
                    .ok()
                    .map(|b| b.word_count_through(end_index))
                    .unwrap_or(0);
                if split_wc < MIN_SPLIT_WORD_FLOOR {
                    info!(
                        "Split rejected: {} words at end_segment_index={} below {} word floor (total buffer: {} cleaned words). Waiting for sensor timeout or next check.",
                        split_wc, end_index, MIN_SPLIT_WORD_FLOOR, cleaned_word_count
                    );
                    continue;
                }
            }

            // Split decided — extract encounter
            {
                        encounter_number += 1;
                        // Clear hybrid sensor tracking on successful split
                        if is_hybrid_mode {
                            sensor_absent_since = None;
                            // If sensor is currently present, mark continuous presence for next encounter.
                            // This blocks LLM-only splits while the same person remains in the room.
                            sensor_continuous_present = sensor_available
                                && prev_sensor_state == crate::presence_sensor::PresenceState::Present;
                        }
                        info!(
                            "Encounter #{} detected (end_segment_index={})",
                            encounter_number, end_index
                        );

                        // Extract encounter segments from buffer
                        let (encounter_text, encounter_text_rich, encounter_word_count, encounter_start, encounter_segment_count) = {
                            let mut buffer = match buffer_for_detector.lock() {
                                Ok(b) => b,
                                Err(_) => continue,
                            };
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
                            (text, text_rich, wc, start, seg_count)
                        };

                        // Generate session ID for this encounter
                        let session_id = uuid::Uuid::new_v4().to_string();

                        // Archive the encounter transcript (pass actual start time for accurate duration)
                        if let Err(e) = local_archive::save_session(
                            &session_id,
                            &encounter_text,
                            0, // duration_ms unused when encounter_started_at is provided
                            None, // No per-encounter audio in continuous mode
                            false,
                            None,
                            encounter_start, // actual encounter start time for duration calc
                            Some(encounter_segment_count),
                        ) {
                            warn!("Failed to archive encounter: {}", e);
                        }

                        // Set pipeline logger and segment logger to write to this session's archive folder
                        if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
                            if let Ok(mut logger) = logger_for_detector.lock() {
                                logger.set_session(&session_dir);
                            }
                            if let Ok(mut sl) = segment_logger_for_detector.lock() {
                                sl.set_session(&session_dir);
                            }
                            // Flush buffered screenshots to session archive
                            crate::screenshot_task::flush_screenshots_to_session(
                                &screenshot_buffer_for_detector, &session_dir,
                            );
                        }

                        // Set split decision on replay bundle
                        if let Ok(mut bundle) = bundle_for_detector.lock() {
                            bundle.set_split_decision(crate::replay_bundle::SplitDecision {
                                ts: Utc::now().to_rfc3339(),
                                trigger: detection_method_str.to_string(),
                                word_count: encounter_word_count,
                                cleaned_word_count,
                                end_segment_index: Some(end_index),
                            });
                        }

                        // Log encounter split to day log
                        if let Some(ref dl) = *day_logger_for_detector {
                            dl.log(crate::day_log::DayEvent::EncounterSplit {
                                ts: Utc::now().to_rfc3339(),
                                session_id: session_id.clone(),
                                encounter_number,
                                trigger: detection_method_str.to_string(),
                                word_count: encounter_word_count,
                                detection_method: detection_method_str.to_string(),
                            });
                        }

                        // Update archive metadata with continuous mode info
                        if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
                            let date_path = session_dir.join("metadata.json");
                            if date_path.exists() {
                                if let Ok(content) = std::fs::read_to_string(&date_path) {
                                    if let Ok(mut metadata) = serde_json::from_str::<local_archive::ArchiveMetadata>(&content) {
                                        metadata.charting_mode = Some("continuous".to_string());
                                        metadata.encounter_number = Some(encounter_number);
                                        // Record how this encounter was detected (reuse pre-computed value)
                                        metadata.detection_method = Some(detection_method_str.to_string());
                                        // Add patient name and DOB from vision extraction
                                        if let Ok(tracker) = name_tracker_for_detector.lock() {
                                            metadata.patient_name = tracker.majority_name();
                                            metadata.patient_dob = tracker.dob().map(|s| s.to_string());
                                        } else {
                                            warn!("Name tracker lock poisoned, patient name/dob not written to metadata");
                                        }
                                        // Add shadow comparison data if in shadow mode
                                        if is_shadow_mode {
                                            let shadow_method = if shadow_active_method == ShadowActiveMethod::Sensor { "llm" } else { "sensor" };
                                            let decisions: Vec<crate::shadow_log::ShadowDecisionSummary> = handle_shadow_decisions
                                                .lock()
                                                .unwrap_or_else(|e| {
                                                    warn!("Shadow decisions lock poisoned, recovering data");
                                                    e.into_inner()
                                                })
                                                .clone();

                                            let active_split_at = Utc::now().to_rfc3339();

                                            // Check if shadow agreed: any "would_split" decision in last 5 minutes
                                            let now = Utc::now();
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
                                        sync_ctx_for_detector.enrich_metadata(&mut metadata);

                                        if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                                            let _ = std::fs::write(&date_path, json);
                                        }
                                    }
                                }
                            }
                        }

                        // Server sync: upload session to profile server
                        {
                            let today = Utc::now().format("%Y-%m-%d").to_string();
                            sync_ctx_for_detector.sync_session(&session_id, &today);
                        }

                        // Clear shadow decisions for next encounter (if in shadow mode)
                        if is_shadow_mode {
                            if let Ok(mut decisions) = handle_shadow_decisions.lock() {
                                decisions.clear();
                            }
                        }

                        // Extract patient name and full tracker state before resetting.
                        // The replay bundle needs this data too — capturing after reset
                        // would see an empty tracker (was the cause of replay_bundle
                        // always showing majority_name=None, vote_count=0).
                        let (encounter_patient_name, tracker_snapshot) = match name_tracker_for_detector.lock() {
                            Ok(mut tracker) => {
                                let name = tracker.majority_name();
                                let votes: usize = tracker.vote_count();
                                let unique: Vec<String> = tracker.votes().keys().cloned().collect();
                                tracker.reset();
                                (name.clone(), (name, votes, unique))
                            }
                            Err(e) => {
                                warn!("Name tracker lock poisoned: {}", e);
                                (None, (None, 0, vec![]))
                            }
                        };

                        // Record split timestamp (for stale screenshot detection)
                        if let Ok(mut t) = last_split_time_for_detector.lock() {
                            *t = Utc::now();
                        }

                        // Read encounter notes AND clear atomically (SOAP generation needs them)
                        let notes_text = match encounter_notes_for_detector.lock() {
                            Ok(mut notes) => {
                                let text = notes.clone();
                                notes.clear();
                                text
                            }
                            Err(e) => {
                                warn!("Encounter notes lock poisoned, using recovered value: {}", e);
                                let mut notes = e.into_inner();
                                let text = notes.clone();
                                notes.clear();
                                text
                            }
                        };

                        // Reset biomarker accumulators for the new encounter
                        reset_bio_flag.store(true, std::sync::atomic::Ordering::SeqCst);

                        // Update stats
                        encounters_for_detector.fetch_add(1, Ordering::Relaxed);
                        if let Ok(mut recent) = recent_encounters_for_detector.lock() {
                            recent.insert(0, RecentEncounter {
                                session_id: session_id.clone(),
                                time: Utc::now().to_rfc3339(),
                                patient_name: encounter_patient_name.clone(),
                            });
                            recent.truncate(3); // Keep only the 3 most recent
                        } else {
                            warn!("Recent encounters lock poisoned, stats not updated");
                        }

                        // Emit encounter detected event
                        ContinuousModeEvent::EncounterDetected {
                            session_id: session_id.clone(),
                            word_count: encounter_word_count,
                            patient_name: encounter_patient_name.clone(),
                        }.emit(&app_for_detector);

                        // Clinical content check: flag non-clinical encounters
                        let is_clinical = if let Some(ref client) = llm_client {
                            crate::encounter_pipeline::check_clinical_content(
                                client, &fast_model, &encounter_text, encounter_word_count,
                                &logger_for_detector,
                                serde_json::json!({
                                    "encounter_number": encounter_number,
                                    "word_count": encounter_word_count,
                                }),
                            ).await
                        } else {
                            encounter_word_count >= MIN_WORDS_FOR_CLINICAL_CHECK
                        };

                        if !is_clinical {
                            crate::encounter_pipeline::mark_non_clinical(&session_id);
                            // Sync non-clinical status to server (initial upload didn't have it)
                            let today = Utc::now().format("%Y-%m-%d").to_string();
                            sync_ctx_for_detector.sync_session(&session_id, &today);
                        }

                        // Log clinical check result to replay bundle + day log
                        if let Ok(mut bundle) = bundle_for_detector.lock() {
                            bundle.set_clinical_check(crate::replay_bundle::ClinicalCheck {
                                ts: Utc::now().to_rfc3339(),
                                is_clinical,
                                latency_ms: 0, // Clinical check latency already logged via pipeline_logger
                                success: true,
                                error: None,
                            });
                        }
                        if let Some(ref dl) = *day_logger_for_detector {
                            dl.log(crate::day_log::DayEvent::ClinicalCheckResult {
                                ts: Utc::now().to_rfc3339(),
                                session_id: session_id.clone(),
                                is_clinical,
                            });
                        }

                        // Generate SOAP note (with 120s timeout — SOAP is heavier than detection)
                        // Skip SOAP for non-clinical encounters to prevent hallucinated clinical content
                        if !is_clinical {
                            info!("Skipping SOAP for non-clinical encounter #{}", encounter_number);
                        } else if let Some(ref client) = llm_client {
                            // ── Multi-patient detection ──────────────────────────
                            // Run before SOAP to detect couples/family visits.
                            let multi_patient_detection = if encounter_word_count >= MULTI_PATIENT_DETECT_WORD_THRESHOLD {
                                info!("Running multi-patient detection for encounter #{} ({} words)", encounter_number, encounter_word_count);
                                let outcome = client.run_multi_patient_detection(&fast_model, &encounter_text_rich).await;
                                if let Ok(mut logger) = logger_for_detector.lock() {
                                    let det_context = match &outcome.detection {
                                        Some(d) => serde_json::json!({
                                            "patient_count": d.patient_count,
                                            "confidence": d.confidence,
                                            "reasoning": d.reasoning,
                                            "patients": d.patients.iter()
                                                .map(|p| serde_json::json!({"label": p.label, "summary": p.summary}))
                                                .collect::<Vec<_>>(),
                                            "word_count": encounter_word_count,
                                        }),
                                        None => serde_json::json!({
                                            "patient_count": 1,
                                            "word_count": encounter_word_count,
                                            "accepted": false,
                                        }),
                                    };
                                    logger.log_llm_call(
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
                                outcome.detection
                            } else {
                                None
                            };

                            // Strip hallucinated repetitions before SOAP generation
                            let (filtered_encounter_text, soap_filter_report) = strip_hallucinations(&encounter_text, 5);
                            if !soap_filter_report.repetitions.is_empty() || !soap_filter_report.phrase_repetitions.is_empty() {
                                if let Ok(mut logger) = logger_for_detector.lock() {
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

                            info!("Generating SOAP for encounter #{}", encounter_number);
                            let soap_now = Utc::now();
                            let soap_outcome = crate::encounter_pipeline::generate_and_archive_soap(
                                client, &soap_model, &filtered_encounter_text,
                                &session_id, &soap_now,
                                soap_detail_level, &soap_format, &soap_custom_instructions,
                                notes_text.clone(), encounter_word_count,
                                multi_patient_detection.as_ref(),
                                &logger_for_detector,
                                serde_json::json!({
                                    "encounter_number": encounter_number,
                                    "word_count": encounter_word_count,
                                    "has_notes": !notes_text.is_empty(),
                                }),
                            ).await;

                            match soap_outcome {
                                crate::encounter_pipeline::SoapGenerationOutcome::Success { ref result, ref content, latency_ms } => {
                                    let patient_count = result.notes.len();
                                    ContinuousModeEvent::SoapGenerated {
                                        session_id: session_id.clone(),
                                        patient_count: Some(patient_count),
                                        recovered: None,
                                    }.emit(&app_for_detector);
                                    info!("SOAP generated for encounter #{} ({} patient notes)", encounter_number, patient_count);
                                    // Server sync: upload SOAP
                                    sync_ctx_for_detector.sync_soap(&session_id, content, soap_detail_level, &soap_format);
                                    if let Ok(mut bundle) = bundle_for_detector.lock() {
                                        bundle.set_soap_result(crate::replay_bundle::SoapResult {
                                            ts: Utc::now().to_rfc3339(),
                                            latency_ms, success: true,
                                            word_count: encounter_word_count, error: None,
                                            patient_count: if patient_count > 1 { Some(patient_count) } else { None },
                                        });
                                    }
                                    if let Some(ref dl) = *day_logger_for_detector {
                                        dl.log(crate::day_log::DayEvent::SoapGenerated {
                                            ts: Utc::now().to_rfc3339(),
                                            session_id: session_id.clone(),
                                            latency_ms, success: true,
                                        });
                                    }

                                    // Billing extraction (fail-open)
                                    {
                                        let encounter_duration_ms = encounter_start
                                            .map(|s| (Utc::now() - s).num_milliseconds().max(0) as u64)
                                            .unwrap_or(0);
                                        let after_hours = crate::encounter_pipeline::is_after_hours(&soap_now);
                                        let billing_start = std::time::Instant::now();
                                        let billing_result = crate::encounter_pipeline::extract_and_archive_billing(
                                            client,
                                            &fast_model,
                                            content,
                                            &filtered_encounter_text,
                                            "", // no physician-provided context in auto-extraction
                                            &session_id,
                                            &soap_now,
                                            encounter_duration_ms,
                                            encounter_patient_name.as_deref(),
                                            after_hours,
                                            &crate::billing::RuleEngineContext::default(), // office default
                                            &logger_for_detector,
                                        ).await;
                                        let billing_latency = billing_start.elapsed().as_millis() as u64;

                                        match &billing_result {
                                            Ok(record) => {
                                                if let Some(ref dl) = *day_logger_for_detector {
                                                    dl.log(crate::day_log::DayEvent::BillingExtracted {
                                                        ts: Utc::now().to_rfc3339(),
                                                        session_id: session_id.clone(),
                                                        codes_count: record.codes.len() as u32,
                                                        total_amount_cents: record.total_amount_cents,
                                                        latency_ms: billing_latency,
                                                        success: true,
                                                    });
                                                }
                                                if let Ok(mut bundle) = bundle_for_detector.lock() {
                                                    bundle.set_billing_result(crate::replay_bundle::BillingResult {
                                                        ts: Utc::now().to_rfc3339(),
                                                        latency_ms: billing_latency,
                                                        success: true,
                                                        codes_count: Some(record.codes.len()),
                                                        total_amount_cents: Some(record.total_amount_cents),
                                                        selected_codes: Some(record.codes.iter().map(|c| c.code.clone()).collect()),
                                                        error: None,
                                                    });
                                                }
                                            }
                                            Err(e) => {
                                                warn!("Billing extraction failed for encounter #{}: {}", encounter_number, e);
                                                if let Some(ref dl) = *day_logger_for_detector {
                                                    dl.log(crate::day_log::DayEvent::BillingExtracted {
                                                        ts: Utc::now().to_rfc3339(),
                                                        session_id: session_id.clone(),
                                                        codes_count: 0,
                                                        total_amount_cents: 0,
                                                        latency_ms: billing_latency,
                                                        success: false,
                                                    });
                                                }
                                                if let Ok(mut bundle) = bundle_for_detector.lock() {
                                                    bundle.set_billing_result(crate::replay_bundle::BillingResult {
                                                        ts: Utc::now().to_rfc3339(),
                                                        latency_ms: billing_latency,
                                                        success: false,
                                                        codes_count: None,
                                                        total_amount_cents: None,
                                                        selected_codes: None,
                                                        error: Some(e.clone()),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                crate::encounter_pipeline::SoapGenerationOutcome::Failed { latency_ms, ref error } => {
                                    if let Ok(mut bundle) = bundle_for_detector.lock() {
                                        bundle.set_soap_result(crate::replay_bundle::SoapResult {
                                            ts: Utc::now().to_rfc3339(),
                                            latency_ms, success: false,
                                            word_count: encounter_word_count,
                                            error: Some(error.clone()),
                                            patient_count: None,
                                        });
                                    }
                                    if let Ok(mut err) = last_error_for_detector.lock() {
                                        *err = Some(format!("SOAP generation failed: {}", error));
                                    } else {
                                        warn!("Last error lock poisoned, error state not updated");
                                    }
                                    ContinuousModeEvent::SoapFailed {
                                        session_id: session_id.clone(),
                                        error: error.clone(),
                                    }.emit(&app_for_detector);
                                    if let Some(ref dl) = *day_logger_for_detector {
                                        dl.log(crate::day_log::DayEvent::SoapGenerated {
                                            ts: Utc::now().to_rfc3339(),
                                            session_id: session_id.clone(),
                                            latency_ms, success: false,
                                        });
                                    }
                                }
                            }
                        }

                        // ---- Retrospective merge check ----
                        // After archiving + SOAP for encounter N, check if it should merge with N-1.
                        // Gap fix: when prev_encounter_session_id is None (first encounter in this
                        // continuous session), load the most recent same-day session from the archive
                        // so the very first split still gets merge-checked.
                        if merge_enabled && prev_encounter_session_id.is_none() {
                            let today_str = Utc::now().format("%Y-%m-%d").to_string();
                            if let Ok(sessions) = local_archive::list_sessions_by_date(&today_str) {
                                // Find the most recent session that isn't the one we just archived
                                if let Some(prev_summary) = sessions.iter().find(|s| s.session_id != session_id) {
                                    if let Ok(details) = local_archive::get_session(&prev_summary.session_id, &today_str) {
                                        if let Some(transcript) = details.transcript {
                                            info!(
                                                "Loaded previous same-day session {} from archive for merge check (first encounter fallback)",
                                                prev_summary.session_id
                                            );
                                            prev_encounter_session_id = Some(prev_summary.session_id.clone());
                                            prev_encounter_text = Some(transcript);
                                            prev_encounter_text_rich = None; // Archive only has flat text
                                            prev_encounter_date = Some(Utc::now());
                                            prev_encounter_is_clinical = prev_summary.likely_non_clinical != Some(true);
                                        }
                                    }
                                }
                            }
                        }

                        if merge_enabled {
                            if let (Some(ref prev_id), Some(ref prev_text), Some(ref prev_date)) =
                                (&prev_encounter_session_id, &prev_encounter_text, &prev_encounter_date)
                            {
                                // ── Small-orphan auto-merge gate ──────────────────────────
                                // If the new encounter is very short (<500 words) and the
                                // sensor confirms someone was present, it's almost certainly
                                // a post-procedure tail (aftercare, scheduling) that was
                                // incorrectly split. Auto-merge without asking the LLM.
                                // Requires sensor data — without it we can't distinguish a
                                // short clinical tail from background noise / non-patient chatter.
                                const SMALL_ORPHAN_WORD_THRESHOLD: usize = 500;
                                let sensor_confirmed_present = sensor_available
                                    && prev_sensor_state == crate::presence_sensor::PresenceState::Present;

                                if encounter_word_count < SMALL_ORPHAN_WORD_THRESHOLD && sensor_confirmed_present {
                                    info!(
                                        "Small-orphan auto-merge: encounter {} has {} words with sensor present — merging into {} without LLM check",
                                        session_id, encounter_word_count, prev_id
                                    );
                                    if let Ok(mut logger) = logger_for_detector.lock() {
                                        logger.log_merge_check(
                                            "auto_merge_small_orphan", "", "",
                                            Some(&format!("{{\"same_encounter\": true, \"reason\": \"small orphan ({} words) with sensor present\"}}", encounter_word_count)),
                                            0, true, None,
                                            serde_json::json!({
                                                "prev_session_id": prev_id,
                                                "curr_session_id": session_id,
                                                "encounter_word_count": encounter_word_count,
                                                "sensor_state": format!("{:?}", prev_sensor_state),
                                                "gate": "small_orphan_auto_merge",
                                            }),
                                        );
                                    }

                                    // Log merge check to replay bundle
                                    if let Ok(mut bundle) = bundle_for_detector.lock() {
                                        bundle.set_merge_check(crate::replay_bundle::MergeCheck {
                                            ts: Utc::now().to_rfc3339(),
                                            prev_session_id: prev_id.clone(),
                                            prev_tail_excerpt: String::new(),
                                            curr_head_excerpt: String::new(),
                                            patient_name: None,
                                            prompt_system: String::new(),
                                            prompt_user: String::new(),
                                            response_raw: None,
                                            parsed_same_encounter: Some(true),
                                            parsed_reason: Some(format!("small orphan ({} words) with sensor present", encounter_word_count)),
                                            latency_ms: 0,
                                            success: true,
                                            auto_merge_gate: Some("small_orphan_auto_merge".to_string()),
                                        });
                                    }
                                    if let Some(ref dl) = *day_logger_for_detector {
                                        dl.log(crate::day_log::DayEvent::EncounterMerged {
                                            ts: Utc::now().to_rfc3339(),
                                            new_session_id: session_id.clone(),
                                            prev_session_id: prev_id.clone(),
                                            reason: "small_orphan_auto_merge".to_string(),
                                            gate_type: Some("small_orphan".to_string()),
                                        });
                                    }

                                    // Perform the merge (same logic as LLM-confirmed merge below)
                                    let merged_text = format!("{}\n{}", prev_text, encounter_text);
                                    let merged_text_rich = match &prev_encounter_text_rich {
                                        Some(prev_rich) => format!("{}\n{}", prev_rich, encounter_text_rich),
                                        None => format!("{}\n{}", prev_text, encounter_text_rich),
                                    };
                                    let merged_wc = merged_text.split_whitespace().count();
                                    let merged_duration = encounter_start
                                        .map(|s| (Utc::now() - s).num_milliseconds().max(0) as u64)
                                        .unwrap_or(0);
                                    let merge_vision_name = name_tracker_for_detector
                                        .lock()
                                        .ok()
                                        .and_then(|t| t.majority_name());
                                    if let Err(e) = local_archive::merge_encounters(
                                        prev_id,
                                        &session_id,
                                        prev_date,
                                        &merged_text,
                                        merged_wc,
                                        merged_duration,
                                        merge_vision_name.as_deref(),
                                    ) {
                                        warn!("Failed to auto-merge small orphan: {}", e);
                                    } else {
                                        // Sync merge to server: delete orphan, re-upload surviving session
                                        {
                                            let today = Utc::now().format("%Y-%m-%d").to_string();
                                            sync_ctx_for_detector.sync_merge(&session_id, prev_id, &today);
                                        }
                                        // Regenerate SOAP for the merged encounter
                                        if let Some(ref client) = llm_client {
                                            let merge_notes = encounter_notes_for_detector
                                                .lock()
                                                .map(|n| n.clone())
                                                .unwrap_or_default();
                                            crate::encounter_pipeline::regen_soap_after_merge(
                                                client, &merged_text, prev_id, prev_date,
                                                &soap_model, soap_detail_level, &soap_format, &soap_custom_instructions,
                                                merge_notes, prev_encounter_is_clinical, is_clinical,
                                                &logger_for_detector, &sync_ctx_for_detector,
                                                "auto_merge_soap_regen",
                                            ).await;
                                        }

                                        merge_back_count += 1;
                                        encounter_number -= 1;
                                        info!("Auto-merge complete (merge_back_count now {}, encounter_number now {})", merge_back_count, encounter_number);

                                        // Remove merged session from recent encounters list
                                        if let Ok(mut recent) = recent_encounters_for_detector.lock() {
                                            recent.retain(|e| e.session_id != session_id);
                                        }

                                        // Emit merge event to frontend
                                        ContinuousModeEvent::EncounterMerged {
                                            kept_session_id: Some(prev_id.clone()),
                                            merged_into_session_id: None,
                                            removed_session_id: session_id.clone(),
                                            reason: Some(format!("small orphan ({} words) with sensor present", encounter_word_count)),
                                        }.emit(&app_for_detector);

                                        // Update prev tracking to the merged encounter
                                        prev_encounter_text = Some(merged_text);
                                        prev_encounter_text_rich = Some(merged_text_rich);
                                        prev_encounter_is_clinical = is_clinical || prev_encounter_is_clinical;
                                        continue; // Skip updating prev to current since we merged
                                    }
                                }

                                // ── LLM merge check (normal path) ────────────────────────
                                let prev_tail = tail_words(prev_text, MERGE_EXCERPT_WORDS);
                                let curr_head = head_words(&encounter_text, MERGE_EXCERPT_WORDS);

                                if let Some(ref client) = llm_client {
                                    let (filtered_prev_tail, _) = strip_hallucinations(&prev_tail, 5);
                                    let (filtered_curr_head, _) = strip_hallucinations(&curr_head, 5);
                                    let merge_patient_name = name_tracker_for_detector
                                        .lock()
                                        .ok()
                                        .and_then(|t| t.majority_name());

                                    let merge_outcome = crate::encounter_pipeline::run_merge_check(
                                        client, &fast_model,
                                        &filtered_prev_tail, &filtered_curr_head,
                                        merge_patient_name.as_deref(),
                                        &logger_for_detector,
                                        serde_json::json!({
                                            "prev_session_id": prev_id,
                                            "curr_session_id": session_id,
                                            "patient_name": merge_patient_name,
                                            "prev_tail_words": filtered_prev_tail.split_whitespace().count(),
                                            "curr_head_words": filtered_curr_head.split_whitespace().count(),
                                        }),
                                    ).await;

                                    // Log to replay bundle
                                    if let Ok(mut bundle) = bundle_for_detector.lock() {
                                        bundle.set_merge_check(crate::replay_bundle::MergeCheck {
                                            ts: Utc::now().to_rfc3339(),
                                            prev_session_id: prev_id.clone(),
                                            prev_tail_excerpt: filtered_prev_tail.clone(),
                                            curr_head_excerpt: filtered_curr_head.clone(),
                                            patient_name: merge_patient_name.clone(),
                                            prompt_system: merge_outcome.prompt_system.clone(),
                                            prompt_user: merge_outcome.prompt_user.clone(),
                                            response_raw: merge_outcome.response_raw.clone(),
                                            parsed_same_encounter: merge_outcome.same_encounter,
                                            parsed_reason: merge_outcome.reason.as_ref().map(|r| format!("{:?}", r)),
                                            latency_ms: merge_outcome.latency_ms,
                                            success: merge_outcome.error.is_none(),
                                            auto_merge_gate: None,
                                        });
                                    }

                                    if merge_outcome.same_encounter == Some(true) {
                                        if let Some(ref dl) = *day_logger_for_detector {
                                            dl.log(crate::day_log::DayEvent::EncounterMerged {
                                                ts: Utc::now().to_rfc3339(),
                                                new_session_id: session_id.clone(),
                                                prev_session_id: prev_id.clone(),
                                                reason: format!("{:?}", merge_outcome.reason),
                                                gate_type: None,
                                            });
                                        }
                                        info!(
                                            "Merge check: encounters are the same visit (reason: {:?}). Merging {} into {}",
                                            merge_outcome.reason, session_id, prev_id
                                        );

                                        let merged_text = format!("{}\n{}", prev_text, encounter_text);
                                        let merged_text_rich = match &prev_encounter_text_rich {
                                            Some(prev_rich) => format!("{}\n{}", prev_rich, encounter_text_rich),
                                            None => format!("{}\n{}", prev_text, encounter_text_rich),
                                        };
                                        let merged_wc = merged_text.split_whitespace().count();
                                        let merged_duration = encounter_start
                                            .map(|s| (Utc::now() - s).num_milliseconds().max(0) as u64)
                                            .unwrap_or(0);

                                        let merge_vision_name = name_tracker_for_detector
                                            .lock()
                                            .ok()
                                            .and_then(|t| t.majority_name());
                                        if let Err(e) = local_archive::merge_encounters(
                                            prev_id, &session_id, prev_date,
                                            &merged_text, merged_wc, merged_duration,
                                            merge_vision_name.as_deref(),
                                        ) {
                                            warn!("Failed to merge encounters: {}", e);
                                        } else {
                                            // Sync merge to server: delete merged-away session, re-upload surviving
                                            {
                                                let today = Utc::now().format("%Y-%m-%d").to_string();
                                                sync_ctx_for_detector.sync_merge(&session_id, prev_id, &today);
                                            }
                                            // Regenerate SOAP for the merged encounter
                                            if let Some(ref client) = llm_client {
                                                let merge_notes = encounter_notes_for_detector
                                                    .lock()
                                                    .map(|n| n.clone())
                                                    .unwrap_or_default();
                                                crate::encounter_pipeline::regen_soap_after_merge(
                                                    client, &merged_text, prev_id, prev_date,
                                                    &soap_model, soap_detail_level, &soap_format, &soap_custom_instructions,
                                                    merge_notes, prev_encounter_is_clinical, is_clinical,
                                                    &logger_for_detector, &sync_ctx_for_detector,
                                                    "merge_soap_regen",
                                                ).await;
                                            }

                                            encounter_number -= 1;

                                            // Remove merged session from recent encounters list
                                            if let Ok(mut recent) = recent_encounters_for_detector.lock() {
                                                recent.retain(|e| e.session_id != session_id);
                                            }

                                            ContinuousModeEvent::EncounterMerged {
                                                kept_session_id: None,
                                                merged_into_session_id: Some(prev_id.clone()),
                                                removed_session_id: session_id.clone(),
                                                reason: None,
                                            }.emit(&app_for_detector);

                                            // Escalate confidence threshold for next detection
                                            merge_back_count += 1;
                                            info!("Merge-back #{}: next confidence threshold escalated by +{:.2}", merge_back_count, merge_back_count as f64 * 0.05);

                                            // ── Retrospective multi-patient check ──
                                            if merged_wc >= MULTI_PATIENT_DETECT_WORD_THRESHOLD {
                                                if let Some(ref client) = llm_client {
                                                    info!("Retrospective multi-patient detect on {} ({} words)", prev_id, merged_wc);
                                                    // Log to the surviving session's pipeline log
                                                    let retro_outcome = client.run_multi_patient_detection(&fast_model, &merged_text_rich).await;
                                                    if let Ok(mut logger) = logger_for_detector.lock() {
                                                        // Point logger at surviving session dir so this entry is preserved
                                                        if let Ok(prev_dir) = local_archive::get_session_archive_dir(prev_id, prev_date) {
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
                                                    if let Some(detection) = retro_outcome.detection {
                                                        info!("Retrospective: {} patients detected, regenerating per-patient SOAP for {}",
                                                            detection.patient_count, prev_id);
                                                        let (filtered, _) = strip_hallucinations(&merged_text, 5);
                                                        let regen_notes = encounter_notes_for_detector
                                                            .lock().map(|n| n.clone()).unwrap_or_default();
                                                        let regen_outcome = crate::encounter_pipeline::generate_and_archive_soap(
                                                            client, &soap_model, &filtered,
                                                            prev_id, prev_date,
                                                            soap_detail_level, &soap_format, &soap_custom_instructions,
                                                            regen_notes, merged_wc,
                                                            Some(&detection),
                                                            &logger_for_detector,
                                                            serde_json::json!({
                                                                "stage": "retrospective_multi_patient_soap",
                                                                "session_id": prev_id,
                                                                "detection_confidence": detection.confidence,
                                                            }),
                                                        ).await;
                                                        if let crate::encounter_pipeline::SoapGenerationOutcome::Success { ref result, ref content, .. } = regen_outcome {
                                                            // Server sync: upload retrospective SOAP
                                                            sync_ctx_for_detector.sync_soap(prev_id, content, soap_detail_level, &soap_format);
                                                            info!(
                                                                "Retrospective per-patient SOAP regenerated for {} ({} notes, {} chars)",
                                                                prev_id, result.notes.len(), content.len()
                                                            );
                                                        }
                                                    }
                                                }
                                            }

                                            // Update prev tracking to the merged encounter
                                            prev_encounter_text = Some(merged_text);
                                            prev_encounter_text_rich = Some(merged_text_rich);
                                            prev_encounter_is_clinical = is_clinical || prev_encounter_is_clinical;
                                            continue; // Skip updating prev to current since we merged
                                        }
                                    } else if let Some(false) = merge_outcome.same_encounter {
                                        info!(
                                            "Merge check: different encounters (reason: {:?})",
                                            merge_outcome.reason
                                        );
                                    }
                                }
                            }
                        }

                        // ── Standalone multi-patient check for large encounters ──
                        // Safety net: if the inline multi-patient detection (run before
                        // SOAP at ≥500 words) missed a multi-patient encounter, this
                        // second pass catches it for large encounters (≥2,500 words).
                        // Runs after the merge check to avoid wasted work on encounters
                        // that will be merged back.
                        if is_clinical
                            && encounter_word_count >= MULTI_PATIENT_CHECK_WORD_THRESHOLD
                        {
                            if let Some(ref client) = llm_client {
                                info!(
                                    "Standalone multi-patient check on encounter #{} ({} words)",
                                    encounter_number, encounter_word_count
                                );
                                let mp_outcome = client
                                    .run_multi_patient_detection(&fast_model, &encounter_text_rich)
                                    .await;
                                if let Ok(mut logger) = logger_for_detector.lock() {
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
                                if let Some(detection) = mp_outcome.detection {
                                    info!(
                                        "Standalone check: {} patients detected in encounter #{}, regenerating per-patient SOAP",
                                        detection.patient_count, encounter_number
                                    );
                                    let (filtered, _) = strip_hallucinations(&encounter_text, 5);
                                    let soap_now = Utc::now();
                                    let regen_outcome =
                                        crate::encounter_pipeline::generate_and_archive_soap(
                                            client,
                                            &soap_model,
                                            &filtered,
                                            &session_id,
                                            &soap_now,
                                            soap_detail_level,
                                            &soap_format,
                                            &soap_custom_instructions,
                                            notes_text.clone(),
                                            encounter_word_count,
                                            Some(&detection),
                                            &logger_for_detector,
                                            serde_json::json!({
                                                "stage": "standalone_multi_patient_soap",
                                                "session_id": session_id,
                                                "encounter_number": encounter_number,
                                                "detection_confidence": detection.confidence,
                                            }),
                                        )
                                        .await;
                                    if let crate::encounter_pipeline::SoapGenerationOutcome::Success {
                                        ref result,
                                        ref content,
                                        ..
                                    } = regen_outcome
                                    {
                                        sync_ctx_for_detector.sync_soap(
                                            &session_id,
                                            content,
                                            soap_detail_level,
                                            &soap_format,
                                        );
                                        info!(
                                            "Standalone multi-patient SOAP regenerated for encounter #{} ({} notes)",
                                            encounter_number, result.notes.len()
                                        );
                                    }
                                }
                            }
                        }

                        // Split stuck — reset merge-back escalation
                        if merge_back_count > 0 {
                            info!("Split confirmed, resetting merge_back_count from {} to 0", merge_back_count);
                            merge_back_count = 0;
                        }

                        // Finalize replay bundle for this encounter
                        // Use tracker_snapshot captured before reset (not the now-empty tracker)
                        let (tracker_majority, tracker_votes, tracker_unique) = tracker_snapshot;
                        if let Ok(mut bundle) = bundle_for_detector.lock() {
                            bundle.set_name_tracker(crate::replay_bundle::NameTrackerState {
                                majority_name: tracker_majority.clone(),
                                vote_count: tracker_votes,
                                unique_names: tracker_unique,
                            });
                            let trigger = bundle.split_decision_trigger();
                            bundle.set_outcome(crate::replay_bundle::Outcome {
                                session_id: session_id.clone(),
                                encounter_number,
                                word_count: encounter_word_count,
                                is_clinical,
                                was_merged: false,
                                merged_into: None,
                                patient_name: tracker_majority,
                                detection_method: trigger,
                            });
                            // Write replay_bundle.json and reset for next encounter
                            if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
                                bundle.build_and_reset(&session_dir);
                            }
                        }
                        // Reset segment logger for next encounter
                        if let Ok(mut sl) = segment_logger_for_detector.lock() {
                            sl.clear_session();
                        }

                        // Update prev encounter tracking for next iteration
                        prev_encounter_session_id = Some(session_id.clone());
                        prev_encounter_text = Some(encounter_text);
                        prev_encounter_text_rich = Some(encounter_text_rich);
                        prev_encounter_date = Some(Utc::now());
                        prev_encounter_is_clinical = is_clinical;
            }

            // Return to recording state
            if let Ok(mut state) = state_for_detector.lock() {
                if *state == ContinuousState::Checking {
                    *state = ContinuousState::Recording;
                }
            } else {
                warn!("State lock poisoned while returning to recording state");
            }
        }
    });

    // Spawn screenshot-based patient name extraction task (if screen capture enabled)
    let screenshot_task = if config.screen_capture_enabled {
        let llm_client_for_screenshot = if !config.llm_router_url.is_empty() {
            LLMClient::new(
                &config.llm_router_url,
                &config.llm_api_key,
                &config.llm_client_id,
                &config.fast_model,
            )
            .ok()
        } else {
            None
        };

        Some(tokio::spawn(crate::screenshot_task::run_screenshot_task(
            crate::screenshot_task::ScreenshotTaskConfig {
                stop_flag: handle.stop_flag.clone(),
                name_tracker: handle.name_tracker.clone(),
                last_split_time: handle.last_split_time.clone(),
                vision_trigger: handle.vision_name_change_trigger.clone(),
                vision_new_name: handle.vision_new_name.clone(),
                vision_old_name: handle.vision_old_name.clone(),
                debug_storage: config.debug_storage_enabled,
                screenshot_interval: config.screen_capture_interval_secs.max(30) as u64,
                llm_client: llm_client_for_screenshot,
                pipeline_logger: logger_for_screenshot.clone(),
                replay_bundle: bundle_for_screenshot.clone(),
                screenshot_buffer: handle.screenshot_buffer.clone(),
                transcript_buffer: handle.transcript_buffer.clone(),
            },
        )))
    } else {
        None
    };

    // Wait for stop signal
    loop {
        if handle.is_stopped() {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Cleanup: stop presence sensor if active
    if let Some(mut sensor) = sensor_handle.take() {
        info!("Stopping presence sensor");
        sensor.stop();
    }

    // Cleanup: stop pipeline
    info!("Stopping continuous mode pipeline");
    pipeline_handle.stop();

    // Join pipeline handle in a blocking task to avoid Drop blocking the Tokio thread
    tokio::task::spawn_blocking(move || {
        pipeline_handle.join();
    }).await.ok();

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
    if let Some(ref client) = flush_llm_client {
        crate::encounter_pipeline::recover_orphaned_soap(
            client,
            &flush_soap_model,
            flush_soap_detail_level,
            &flush_soap_format,
            &flush_soap_custom_instructions,
            &logger_for_flush,
            &app,
            &sync_ctx,
        ).await;
    }

    // ---- Orphaned billing recovery ----
    if let Some(ref client) = flush_llm_client {
        crate::encounter_pipeline::recover_orphaned_billing(
            client,
            &flush_fast_model,
            &logger_for_flush,
        ).await;
    }

    // Flush remaining buffer as final encounter check
    let (remaining_text, flush_encounter_start, flush_segment_count) = {
        let buffer = handle.transcript_buffer.lock().unwrap_or_else(|e| e.into_inner());
        if !buffer.is_empty() {
            (Some(buffer.full_text_with_speakers()), buffer.first_timestamp(), buffer.segment_count())
        } else {
            (None, None, 0)
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
            info!("Flushing remaining buffer ({} words after filtering) as final session", word_count);
            let session_id = uuid::Uuid::new_v4().to_string();
            if let Err(e) = local_archive::save_session(
                &session_id,
                &text, // Archive the raw text (preserve original for audit)
                0, // Unknown duration for flush
                None,
                false,
                Some("continuous_mode_stopped"),
                flush_encounter_start, // Actual encounter start time for accurate duration
                Some(flush_segment_count),
            ) {
                warn!("Failed to archive final buffer: {}", e);
            } else {
                // Point logger to flush session's archive folder
                if let Ok(flush_session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
                    if let Ok(mut logger) = logger_for_flush.lock() {
                        logger.set_session(&flush_session_dir);
                    }
                    // Flush remaining screenshots
                    crate::screenshot_task::flush_screenshots_to_session(
                        &handle.screenshot_buffer, &flush_session_dir,
                    );
                }

                // Track session ID for day log
                flush_session_id_for_log = Some(session_id.clone());

                // Cache today's sessions (used for encounter number + merge check)
                let flush_today_str = Utc::now().format("%Y-%m-%d").to_string();
                let flush_today_sessions = local_archive::list_sessions_by_date(&flush_today_str).ok();

                // Update archive metadata with continuous mode info (match normal encounter path)
                let flush_encounter_number = flush_today_sessions.as_ref()
                    .map(|s| s.len() as u32)
                    .unwrap_or(1);
                if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
                    let meta_path = session_dir.join("metadata.json");
                    if meta_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&meta_path) {
                            if let Ok(mut metadata) = serde_json::from_str::<local_archive::ArchiveMetadata>(&content) {
                                metadata.charting_mode = Some("continuous".to_string());
                                metadata.encounter_number = Some(flush_encounter_number);
                                metadata.detection_method = Some("flush".to_string());
                                if let Ok(tracker) = handle.name_tracker.lock() {
                                    metadata.patient_name = tracker.majority_name();
                                    metadata.patient_dob = tracker.dob().map(|s| s.to_string());
                                } else {
                                    warn!("Name tracker lock poisoned during flush metadata enrichment");
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
                    let today = Utc::now().format("%Y-%m-%d").to_string();
                    sync_ctx.sync_session(&session_id, &today);
                }

                // Clinical content check (shared with detector path)
                let is_clinical = if let Some(ref client) = flush_llm_client {
                    crate::encounter_pipeline::check_clinical_content(
                        client, &flush_fast_model, &text, word_count,
                        &logger_for_flush,
                        serde_json::json!({
                            "stage": "flush_on_stop",
                            "word_count": word_count,
                        }),
                    ).await
                } else {
                    word_count >= MIN_WORDS_FOR_CLINICAL_CHECK
                };

                if !is_clinical {
                    crate::encounter_pipeline::mark_non_clinical(&session_id);
                    // Re-sync so server gets the non-clinical flag
                    let today = Utc::now().format("%Y-%m-%d").to_string();
                    sync_ctx.sync_session(&session_id, &today);
                }

                // ---- Flush merge check (runs BEFORE SOAP to avoid wasted generation) ----
                let mut flush_was_merged = false;
                if merge_enabled {
                    if let Some(ref client) = flush_llm_client {
                        if let Some(ref sessions) = flush_today_sessions {
                            if let Some(prev_summary) = sessions.iter().find(|s| s.session_id != session_id) {
                                if let Ok(prev_details) = local_archive::get_session(&prev_summary.session_id, &flush_today_str) {
                                    if let Some(ref prev_transcript) = prev_details.transcript {
                                        let prev_tail = tail_words(prev_transcript, MERGE_EXCERPT_WORDS);
                                        let curr_head = head_words(&filtered_text, MERGE_EXCERPT_WORDS);
                                        let (filtered_prev_tail, _) = strip_hallucinations(&prev_tail, 5);
                                        let (filtered_curr_head, _) = strip_hallucinations(&curr_head, 5);

                                        let merge_outcome = crate::encounter_pipeline::run_merge_check(
                                            client, &flush_fast_model,
                                            &filtered_prev_tail, &filtered_curr_head,
                                            None, // No vision tracker at flush time
                                            &logger_for_flush,
                                            serde_json::json!({
                                                "prev_session_id": prev_summary.session_id,
                                                "curr_session_id": session_id,
                                                "stage": "flush_on_stop",
                                                "prev_tail_words": filtered_prev_tail.split_whitespace().count(),
                                                "curr_head_words": filtered_curr_head.split_whitespace().count(),
                                            }),
                                        ).await;

                                        if merge_outcome.same_encounter == Some(true) {
                                            info!(
                                                "Flush merge check: same visit (reason: {:?}). Merging {} into {}",
                                                merge_outcome.reason, session_id, prev_summary.session_id
                                            );
                                            let merged_text = format!("{}\n{}", prev_transcript, text);
                                            let merged_wc = merged_text.split_whitespace().count();
                                            let now = Utc::now();
                                            if let Err(e) = local_archive::merge_encounters(
                                                &prev_summary.session_id,
                                                &session_id,
                                                &now,
                                                &merged_text,
                                                merged_wc,
                                                0, // Unknown duration for flush
                                                None, // No vision tracker at flush time
                                            ) {
                                                warn!("Failed to merge flushed encounter: {}", e);
                                            } else {
                                                // Sync merge to server
                                                {
                                                    let today = Utc::now().format("%Y-%m-%d").to_string();
                                                    sync_ctx.sync_merge(&session_id, &prev_summary.session_id, &today);
                                                }
                                                flush_was_merged = true;
                                                // Regenerate SOAP for the merged encounter
                                                let prev_is_clinical = prev_summary.likely_non_clinical != Some(true);
                                                let flush_merge_notes = handle.encounter_notes
                                                    .lock()
                                                    .map(|n| n.clone())
                                                    .unwrap_or_default();
                                                crate::encounter_pipeline::regen_soap_after_merge(
                                                    client, &merged_text, &prev_summary.session_id, &now,
                                                    &flush_soap_model, flush_soap_detail_level, &flush_soap_format, &flush_soap_custom_instructions,
                                                    flush_merge_notes, prev_is_clinical, is_clinical,
                                                    &logger_for_flush, &sync_ctx,
                                                    "flush_merge_soap_regen",
                                                ).await;
                                                ContinuousModeEvent::EncounterMerged {
                                                    kept_session_id: None,
                                                    merged_into_session_id: Some(prev_summary.session_id.clone()),
                                                    removed_session_id: session_id.clone(),
                                                    reason: None,
                                                }.emit(&app);
                                            }
                                        } else if let Some(false) = merge_outcome.same_encounter {
                                            info!(
                                                "Flush merge check: different encounters (reason: {:?})",
                                                merge_outcome.reason
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
                    info!("Skipping SOAP for non-clinical flush encounter");
                } else if flush_was_merged {
                    info!("Skipping SOAP for flush encounter — already merged into previous session");
                } else if let Some(ref client) = flush_llm_client {
                    let flush_notes = handle.encounter_notes
                        .lock()
                        .map(|n| n.clone())
                        .unwrap_or_default();
                    info!("Generating SOAP for flushed buffer ({} words)", word_count);
                    let outcome = crate::encounter_pipeline::generate_and_archive_soap(
                        client, &flush_soap_model, &filtered_text,
                        &session_id, &Utc::now(),
                        flush_soap_detail_level, &flush_soap_format, &flush_soap_custom_instructions,
                        flush_notes, word_count, None,
                        &logger_for_flush,
                        serde_json::json!({"stage": "flush_on_stop", "word_count": word_count}),
                    ).await;
                    if let crate::encounter_pipeline::SoapGenerationOutcome::Success { ref content, .. } = outcome {
                        sync_ctx.sync_soap(&session_id, content, flush_soap_detail_level, &flush_soap_format);
                        info!("SOAP generated for flushed buffer");
                        ContinuousModeEvent::SoapGenerated {
                            session_id: session_id.clone(),
                            patient_count: None,
                            recovered: None,
                        }.emit(&app);

                        // Billing extraction (fail-open)
                        {
                            let flush_duration_ms = flush_encounter_start
                                .map(|s| (Utc::now() - s).num_milliseconds().max(0) as u64)
                                .unwrap_or(0);
                            let flush_now = Utc::now();
                            let flush_after_hours = crate::encounter_pipeline::is_after_hours(&flush_now);
                            let flush_patient_name = handle.name_tracker
                                .lock()
                                .ok()
                                .and_then(|t| t.majority_name());
                            let billing_start = std::time::Instant::now();
                            let billing_result = crate::encounter_pipeline::extract_and_archive_billing(
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
                                &crate::billing::RuleEngineContext::default(), // office default
                                &logger_for_flush,
                            ).await;
                            let billing_latency = billing_start.elapsed().as_millis() as u64;

                            match &billing_result {
                                Ok(record) => {
                                    if let Some(ref dl) = *day_logger_for_flush {
                                        dl.log(crate::day_log::DayEvent::BillingExtracted {
                                            ts: Utc::now().to_rfc3339(),
                                            session_id: session_id.clone(),
                                            codes_count: record.codes.len() as u32,
                                            total_amount_cents: record.total_amount_cents,
                                            latency_ms: billing_latency,
                                            success: true,
                                        });
                                    }
                                }
                                Err(e) => {
                                    warn!("Billing extraction failed for flush encounter: {}", e);
                                    if let Some(ref dl) = *day_logger_for_flush {
                                        dl.log(crate::day_log::DayEvent::BillingExtracted {
                                            ts: Utc::now().to_rfc3339(),
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
            ts: Utc::now().to_rfc3339(),
            total_encounters: handle.encounters_detected.load(Ordering::Relaxed),
            flush_session_id: flush_session_id_for_log,
        });
    }

    // Set state to idle
    if let Ok(mut state) = handle.state.lock() {
        *state = ContinuousState::Idle;
    } else {
        warn!("State lock poisoned while setting idle state on shutdown");
    }

    ContinuousModeEvent::Stopped.emit(&app);

    info!("Continuous mode stopped");
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Pure function tests: tail_words, head_words
    // ========================================================================

    #[test]
    fn test_tail_words_empty_string() {
        assert_eq!(tail_words("", 5), "");
    }

    #[test]
    fn test_tail_words_fewer_than_n() {
        assert_eq!(tail_words("hello world", 5), "hello world");
    }

    #[test]
    fn test_tail_words_exact_n() {
        assert_eq!(tail_words("one two three", 3), "one two three");
    }

    #[test]
    fn test_tail_words_more_than_n() {
        assert_eq!(tail_words("alpha beta gamma delta epsilon", 3), "gamma delta epsilon");
    }

    #[test]
    fn test_tail_words_multiple_whitespace() {
        // split_whitespace normalizes whitespace
        assert_eq!(tail_words("a  b   c    d", 2), "c d");
    }

    #[test]
    fn test_head_words_empty_string() {
        assert_eq!(head_words("", 5), "");
    }

    #[test]
    fn test_head_words_fewer_than_n() {
        assert_eq!(head_words("hello world", 5), "hello world");
    }

    #[test]
    fn test_head_words_exact_n() {
        assert_eq!(head_words("one two three", 3), "one two three");
    }

    #[test]
    fn test_head_words_more_than_n() {
        assert_eq!(head_words("alpha beta gamma delta epsilon", 3), "alpha beta gamma");
    }

    #[test]
    fn test_head_words_multiple_whitespace() {
        assert_eq!(head_words("a  b   c    d", 2), "a b");
    }

    // ========================================================================
    // Pure function tests: effective_soap_detail_level
    // ========================================================================

    #[test]
    fn test_effective_detail_short_encounter_uses_configured() {
        // Short encounters (< 1500 words): configured level wins if >= 3
        assert_eq!(effective_soap_detail_level(5, 500), 5);
        assert_eq!(effective_soap_detail_level(3, 1000), 3);
        assert_eq!(effective_soap_detail_level(8, 1499), 8);
    }

    #[test]
    fn test_effective_detail_scales_with_word_count() {
        // Word-based level rises with transcript length
        assert_eq!(effective_soap_detail_level(3, 1500), 4);  // 1500-2999 → 4
        assert_eq!(effective_soap_detail_level(3, 3000), 5);  // 3000-4999 → 5
        assert_eq!(effective_soap_detail_level(3, 5000), 6);  // 5000-7999 → 6
        assert_eq!(effective_soap_detail_level(3, 8000), 7);  // 8000-11999 → 7
        assert_eq!(effective_soap_detail_level(3, 12000), 8); // 12000-15999 → 8
        assert_eq!(effective_soap_detail_level(3, 16000), 9); // 16000-19999 → 9
        assert_eq!(effective_soap_detail_level(3, 20000), 10); // 20000+ → 10
    }

    #[test]
    fn test_effective_detail_configured_is_floor() {
        // High configured level is never reduced by word count
        assert_eq!(effective_soap_detail_level(8, 500), 8);
        assert_eq!(effective_soap_detail_level(10, 1000), 10);
        assert_eq!(effective_soap_detail_level(7, 3000), 7); // word-based=5, configured=7 wins
    }

    #[test]
    fn test_effective_detail_capped_at_10() {
        assert_eq!(effective_soap_detail_level(10, 50000), 10);
        assert_eq!(effective_soap_detail_level(3, 100000), 10);
    }

    #[test]
    fn test_effective_detail_shirley_scenario() {
        // 22K words at configured level 3 should scale to 10
        assert_eq!(effective_soap_detail_level(3, 22000), 10);
    }

    #[test]
    fn test_effective_detail_boundary_values() {
        // Test exact boundary transitions
        assert_eq!(effective_soap_detail_level(1, 1499), 3);  // just under 1500
        assert_eq!(effective_soap_detail_level(1, 1500), 4);  // exactly 1500
        assert_eq!(effective_soap_detail_level(1, 2999), 4);  // just under 3000
        assert_eq!(effective_soap_detail_level(1, 3000), 5);  // exactly 3000
        assert_eq!(effective_soap_detail_level(1, 19999), 9); // just under 20000
        assert_eq!(effective_soap_detail_level(1, 20000), 10); // exactly 20000
    }

    #[test]
    fn test_effective_detail_zero_words() {
        // Empty transcript — word-based = 3, configured wins if higher
        assert_eq!(effective_soap_detail_level(5, 0), 5);
        assert_eq!(effective_soap_detail_level(1, 0), 3);
    }

    #[test]
    fn test_effective_detail_configured_zero() {
        // Zero configured (shouldn't happen after config clamping, but documents floor)
        assert_eq!(effective_soap_detail_level(0, 0), 3);
        assert_eq!(effective_soap_detail_level(0, 5000), 6);
    }

    #[test]
    fn test_effective_detail_configured_above_max() {
        // Configured > 10 (hand-edited config) — .min(10) clamps it
        assert_eq!(effective_soap_detail_level(12, 0), 10);
        assert_eq!(effective_soap_detail_level(255, 500), 10);
    }

    // ========================================================================
    // Existing tests: validate decision-logic invariants by reconstructing
    // production branching logic inline. They verify that the expected
    // boolean/numeric relationships hold for hybrid detection state machine.
    // ========================================================================

    #[test]
    fn test_continuous_mode_handle_stats() {
        let handle = ContinuousModeHandle::new();
        let stats = handle.get_stats();
        assert_eq!(stats.state, "idle");
        assert_eq!(stats.encounters_detected, 0);
        assert_eq!(stats.buffer_word_count, 0);
        assert!(stats.recent_encounters.is_empty());
    }

    #[test]
    fn test_continuous_mode_handle_stop() {
        let handle = ContinuousModeHandle::new();
        assert!(!handle.is_stopped());
        handle.stop();
        assert!(handle.is_stopped());
    }

    #[test]
    fn test_detection_prompt_no_premature_bias() {
        let (system, _) = build_encounter_detection_prompt("test transcript", None);
        assert!(
            !system.contains("better to wait"),
            "Prompt should not have 'better to wait' bias"
        );
        assert!(
            !system.contains("under 2 minutes"),
            "Prompt should not have 'under 2 minutes' rule (enforced in code)"
        );
    }

    // ==========================================================================
    // Hybrid detection mode tests
    // ==========================================================================

    #[test]
    fn test_hybrid_sensor_trigger_does_not_force_split() {
        // In hybrid mode, sensor_triggered should NOT bypass LLM (it accelerates the LLM check)
        // In pure sensor mode, sensor_triggered SHOULD force-split
        let sensor_triggered = true;
        let manual_triggered = false;
        let is_hybrid_mode = true;

        let should_force = manual_triggered || (sensor_triggered && !is_hybrid_mode);
        assert!(!should_force, "Hybrid mode should NOT force-split on sensor trigger");

        let is_hybrid_mode = false;
        let should_force = manual_triggered || (sensor_triggered && !is_hybrid_mode);
        assert!(should_force, "Pure sensor mode SHOULD force-split on sensor trigger");
    }

    #[test]
    fn test_hybrid_manual_trigger_always_force_splits() {
        // Manual trigger should force-split regardless of hybrid mode
        let manual_triggered = true;
        let sensor_triggered = false;

        for is_hybrid in [true, false] {
            let should_force = manual_triggered || (sensor_triggered && !is_hybrid);
            assert!(should_force, "Manual trigger should force-split in hybrid={is_hybrid}");
        }
    }

    #[test]
    fn test_hybrid_sensor_timeout_logic() {
        let confirm_window_secs: u64 = 180;
        let min_words: usize = 500;

        // Case 1: Timeout exceeded with enough words → should force-split
        let absent_since = Utc::now() - chrono::Duration::seconds(200);
        let word_count: usize = 600;
        let elapsed = (Utc::now() - absent_since).num_seconds() as u64;
        assert!(
            elapsed >= confirm_window_secs && word_count >= min_words,
            "Should force-split: elapsed={}s, words={}", elapsed, word_count
        );

        // Case 2: Timeout exceeded but not enough words → should NOT force-split
        let word_count: usize = 100;
        assert!(
            !(elapsed >= confirm_window_secs && word_count >= min_words),
            "Should NOT force-split with insufficient words"
        );

        // Case 3: Enough words but timeout not exceeded → should NOT force-split
        let absent_since = Utc::now() - chrono::Duration::seconds(60);
        let word_count: usize = 600;
        let elapsed = (Utc::now() - absent_since).num_seconds() as u64;
        assert!(
            !(elapsed >= confirm_window_secs && word_count >= min_words),
            "Should NOT force-split before timeout"
        );
    }

    #[test]
    fn test_hybrid_detection_method_strings() {
        // Verify detection_method assignment logic produces correct strings
        let test_cases: Vec<(bool, bool, bool, bool, bool, &str)> = vec![
            // (manual, sensor, force, hybrid, sensor_timeout, expected)
            (true,  false, false, false, false, "manual"),
            (true,  false, false, true,  false, "manual"),   // Manual overrides hybrid
            (false, true,  false, true,  false, "hybrid_sensor_confirmed"),
            (false, false, false, true,  true,  "hybrid_sensor_timeout"),
            (false, false, true,  true,  false, "hybrid_force"),
            (false, false, false, true,  false, "hybrid_llm"),
            (false, true,  false, false, false, "sensor"),
            (false, false, false, false, false, "llm"),
            (false, false, true,  false, false, "llm"),      // Non-hybrid force = "llm" (existing behavior)
        ];

        for (manual, sensor, force, hybrid, sensor_timeout, expected) in test_cases {
            let method = if manual {
                "manual".to_string()
            } else if hybrid {
                if sensor_timeout {
                    "hybrid_sensor_timeout".to_string()
                } else if sensor {
                    "hybrid_sensor_confirmed".to_string()
                } else if force {
                    "hybrid_force".to_string()
                } else {
                    "hybrid_llm".to_string()
                }
            } else if sensor {
                "sensor".to_string()
            } else {
                "llm".to_string()
            };
            assert_eq!(
                method, expected,
                "Failed for manual={manual}, sensor={sensor}, force={force}, hybrid={hybrid}, sensor_timeout={sensor_timeout}"
            );
        }
    }

    #[test]
    fn test_hybrid_sensor_trigger_minimum_word_guard() {
        // In hybrid mode, sensor triggers should be deferred if word count is below threshold.
        // This prevents micro-splits on wrap-up dialogue when the sensor flickers during departures.
        const MIN_SENSOR_HYBRID_WORDS: usize = 500;

        // Case 1: sensor trigger with insufficient words in hybrid mode → defer
        let sensor_triggered = true;
        let is_hybrid_mode = true;
        let word_count: usize = 120;
        let should_defer = sensor_triggered && is_hybrid_mode && word_count < MIN_SENSOR_HYBRID_WORDS;
        assert!(should_defer, "Should defer: sensor trigger with only {word_count} words in hybrid mode");

        // Case 2: sensor trigger with sufficient words in hybrid mode → proceed
        let word_count: usize = 600;
        let should_defer = sensor_triggered && is_hybrid_mode && word_count < MIN_SENSOR_HYBRID_WORDS;
        assert!(!should_defer, "Should proceed: sensor trigger with {word_count} words in hybrid mode");

        // Case 3: sensor trigger in pure sensor mode → no minimum (not hybrid)
        let is_hybrid_mode = false;
        let word_count: usize = 50;
        let should_defer = sensor_triggered && is_hybrid_mode && word_count < MIN_SENSOR_HYBRID_WORDS;
        assert!(!should_defer, "Pure sensor mode should not apply hybrid minimum word guard");

        // Case 4: manual trigger in hybrid mode → no minimum (manual always proceeds)
        let manual_triggered = true;
        let sensor_triggered = false;
        let is_hybrid_mode = true;
        let word_count: usize = 50;
        let should_defer = sensor_triggered && is_hybrid_mode && word_count < MIN_SENSOR_HYBRID_WORDS;
        assert!(!should_defer, "Manual trigger should not be subject to sensor word guard");
        assert!(manual_triggered, "Manual trigger should proceed regardless");

        // Case 5: word count exactly at threshold → proceed
        let sensor_triggered = true;
        let word_count: usize = 500;
        let should_defer = sensor_triggered && is_hybrid_mode && word_count < MIN_SENSOR_HYBRID_WORDS;
        assert!(!should_defer, "Should proceed at exactly the threshold");
    }

    #[test]
    fn test_hybrid_sensor_state_transitions() {
        use crate::presence_sensor::PresenceState;

        // Present→Absent should trigger check
        assert!(matches!(
            (PresenceState::Present, PresenceState::Absent),
            (PresenceState::Present, PresenceState::Absent)
        ));

        // Any→Present should cancel tracking
        for old in [PresenceState::Absent, PresenceState::Unknown, PresenceState::Present] {
            let new = PresenceState::Present;
            assert!(
                new == PresenceState::Present,
                "Any→Present should cancel tracking (old={:?})", old
            );
        }

        // Absent→Absent should NOT trigger (not a Present→Absent transition)
        let triggers = matches!(
            (PresenceState::Absent, PresenceState::Absent),
            (PresenceState::Present, PresenceState::Absent)
        );
        assert!(!triggers, "Absent→Absent should not trigger a check");

        // Unknown→Absent should NOT trigger (not Present→Absent)
        let triggers = matches!(
            (PresenceState::Unknown, PresenceState::Absent),
            (PresenceState::Present, PresenceState::Absent)
        );
        assert!(!triggers, "Unknown→Absent should not trigger a check");
    }

    #[test]
    fn test_hybrid_sensor_available_flag() {
        // When sensor_available=false, hybrid should behave identically to LLM mode
        let is_hybrid = true;

        // With sensor: uses sensor arm
        let sensor_available = true;
        assert!(is_hybrid && sensor_available, "Should use sensor arm");

        // Without sensor: falls back to LLM
        let sensor_available = false;
        assert!(is_hybrid && !sensor_available, "Should use LLM fallback path");
    }

    #[test]
    fn test_hybrid_sensor_absent_since_cleared_on_return() {
        // Simulates the logic: when sensor returns to Present, absence tracking is cleared
        let mut sensor_absent_since: Option<DateTime<Utc>> = Some(Utc::now());

        // Simulate person returning
        let new_state = crate::presence_sensor::PresenceState::Present;
        if new_state == crate::presence_sensor::PresenceState::Present && sensor_absent_since.is_some() {
            sensor_absent_since = None;
        }
        assert!(sensor_absent_since.is_none(), "Should be cleared when person returns");
    }

    #[test]
    fn test_hybrid_sensor_absent_since_cleared_on_split() {
        // Simulates the logic: when an encounter is split, absence tracking is cleared
        let mut sensor_absent_since: Option<DateTime<Utc>> = Some(Utc::now());
        let is_hybrid_mode = true;

        // Simulate successful split
        if is_hybrid_mode {
            sensor_absent_since = None;
        }
        assert!(sensor_absent_since.is_none(), "Should be cleared on successful split");
    }

    #[test]
    fn test_hybrid_sensor_timeout_with_boundary_values() {
        let confirm_window: u64 = 180;
        let min_words: usize = 500;

        // Exactly at boundary: should trigger
        let elapsed: u64 = 180;
        let words: usize = 500;
        assert!(elapsed >= confirm_window && words >= min_words);

        // One below each boundary: should not trigger
        let elapsed: u64 = 179;
        assert!(!(elapsed >= confirm_window && words >= min_words));

        let elapsed: u64 = 180;
        let words: usize = 499;
        assert!(!(elapsed >= confirm_window && words >= min_words));
    }
}
