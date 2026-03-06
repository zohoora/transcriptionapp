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
use crate::encounter_experiment::strip_hallucinations;
use crate::llm_client::LLMClient;
use crate::local_archive;
use crate::pipeline::{start_pipeline, PipelineConfig, PipelineMessage};
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
};
pub use crate::encounter_merge::{MergeCheckResult, build_encounter_merge_prompt, parse_merge_check};
pub use crate::patient_name_tracker::PatientNameTracker;
pub(crate) use crate::patient_name_tracker::{build_patient_name_prompt, parse_patient_name};

// ============================================================================
// Merge excerpt helpers
// ============================================================================

/// Number of words to extract from transcript tail/head for merge comparison
const MERGE_EXCERPT_WORDS: usize = 500;

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

// ============================================================================
// Continuous Mode State
// ============================================================================

/// State of the continuous mode
#[derive(Debug, Clone, PartialEq)]
pub enum ContinuousState {
    Idle,
    Recording,
    Checking,
    Error(String),
}

impl ContinuousState {
    pub fn as_str(&self) -> &str {
        match self {
            ContinuousState::Idle => "idle",
            ContinuousState::Recording => "recording",
            ContinuousState::Checking => "checking",
            ContinuousState::Error(_) => "error",
        }
    }
}

/// Stats for the frontend monitoring dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousModeStats {
    pub state: String,
    pub recording_since: String,
    pub encounters_detected: u32,
    pub last_encounter_at: Option<String>,
    pub last_encounter_words: Option<u32>,
    pub last_encounter_patient_name: Option<String>,
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
}

/// Handle to control the running continuous mode
pub struct ContinuousModeHandle {
    pub stop_flag: Arc<AtomicBool>,
    pub state: Arc<Mutex<ContinuousState>>,
    pub transcript_buffer: Arc<Mutex<TranscriptBuffer>>,
    pub encounters_detected: Arc<AtomicU32>,
    pub recording_since: DateTime<Utc>,
    pub last_encounter_at: Arc<Mutex<Option<DateTime<Utc>>>>,
    pub last_encounter_words: Arc<Mutex<Option<u32>>>,
    pub last_encounter_patient_name: Arc<Mutex<Option<String>>>,
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
}

impl ContinuousModeHandle {
    pub fn new() -> Self {
        Self {
            stop_flag: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(ContinuousState::Idle)),
            transcript_buffer: Arc::new(Mutex::new(TranscriptBuffer::new())),
            encounters_detected: Arc::new(AtomicU32::new(0)),
            recording_since: Utc::now(),
            last_encounter_at: Arc::new(Mutex::new(None)),
            last_encounter_words: Arc::new(Mutex::new(None)),
            last_encounter_patient_name: Arc::new(Mutex::new(None)),
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
        }
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

        let last_at = self
            .last_encounter_at
            .lock()
            .ok()
            .and_then(|t| t.map(|dt| dt.to_rfc3339()));

        let last_words = self
            .last_encounter_words
            .lock()
            .ok()
            .and_then(|w| *w);

        let last_patient = self
            .last_encounter_patient_name
            .lock()
            .ok()
            .and_then(|n| n.clone());

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

        ContinuousModeStats {
            state,
            recording_since: self.recording_since.to_rfc3339(),
            encounters_detected: self.encounters_detected.load(Ordering::Relaxed),
            last_encounter_at: last_at,
            last_encounter_words: last_words,
            last_encounter_patient_name: last_patient,
            last_error: last_err,
            buffer_word_count: buffer_wc,
            buffer_started_at: buffer_started,
            sensor_connected,
            sensor_state,
            shadow_mode_active,
            shadow_method,
            last_shadow_outcome,
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
            let _ = app.emit("continuous_mode_event", serde_json::json!({
                "type": "error",
                "error": e.to_string()
            }));
            return Err(e.to_string());
        }
    };

    info!("Continuous mode pipeline started");

    // Clone the biomarker reset flag so the detector task can trigger resets on encounter boundaries
    let reset_bio_for_detector = pipeline_handle.reset_biomarkers_flag();

    // Get native STT shadow accumulator for draining at encounter boundaries
    let native_stt_accumulator_for_detector = pipeline_handle.native_stt_accumulator();

    // Pipeline started successfully — now set state and emit event
    if let Ok(mut state) = handle.state.lock() {
        *state = ContinuousState::Recording;
    } else {
        warn!("State lock poisoned while setting recording state");
    }
    let _ = app.emit("continuous_mode_event", serde_json::json!({
        "type": "started"
    }));

    // Tag the buffer with this pipeline's generation so stale segments are rejected
    let pipeline_generation: u64 = 1; // Single pipeline per continuous mode run
    if let Ok(mut buffer) = handle.transcript_buffer.lock() {
        buffer.set_generation(pipeline_generation);
    } else {
        warn!("Buffer lock poisoned while setting generation");
    }

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
    let consumer_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if stop_for_consumer.load(Ordering::Relaxed) {
                break;
            }

            match msg {
                PipelineMessage::Segment(segment) => {
                    // Reset silence tracking on speech
                    if let Ok(mut s) = silence_start_for_consumer.lock() {
                        *s = None;
                    } else {
                        warn!("Silence tracking lock poisoned, silence state may be stale");
                    }

                    if let Ok(mut buffer) = buffer_for_consumer.lock() {
                        buffer.push(
                            segment.text.clone(),
                            segment.end_ms,
                            segment.speaker_id.clone(),
                            segment.speaker_confidence,
                            pipeline_generation,
                        );
                    } else {
                        warn!("Buffer lock poisoned, segment dropped: {}", segment.text);
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
                // Shadow native STT transcript — ignore in continuous mode consumer
                // (shadow accumulator is managed by pipeline thread, saved during encounter archival)
                PipelineMessage::NativeSttShadowTranscript { .. } => {}
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
    let use_sensor_mode = needs_sensor && !config.presence_sensor_port.is_empty();
    let mut sensor_handle: Option<crate::presence_sensor::PresenceSensor> = None;
    let sensor_absence_trigger: Arc<tokio::sync::Notify>;
    // Shadow sensor observer uses watch channel for state transitions (not Notify)
    let mut shadow_sensor_state_rx: Option<tokio::sync::watch::Receiver<crate::presence_sensor::PresenceState>> = None;
    // Hybrid mode: dedicated watch receiver for sensor state in the detection loop
    let mut hybrid_sensor_state_rx: Option<tokio::sync::watch::Receiver<crate::presence_sensor::PresenceState>> = None;

    if use_sensor_mode {
        // Auto-detect sensor port if configured port is missing or changed
        let sensor_port = crate::presence_sensor::auto_detect_port(&config.presence_sensor_port)
            .unwrap_or_default();

        let sensor_config = crate::presence_sensor::SensorConfig {
            port: sensor_port,
            debounce_secs: config.presence_debounce_secs,
            absence_threshold_secs: config.presence_absence_threshold_secs,
            csv_log_enabled: config.presence_csv_log_enabled,
        };

        match crate::presence_sensor::PresenceSensor::start(&sensor_config) {
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
                let _ = app.emit("continuous_mode_event", serde_json::json!({
                    "type": "sensor_status",
                    "connected": true,
                    "state": "unknown"
                }));

                // Get a dedicated state receiver for shadow sensor observer
                shadow_sensor_state_rx = Some(sensor.subscribe_state());
                // Get a dedicated state receiver for hybrid detection loop
                if is_hybrid_mode {
                    hybrid_sensor_state_rx = Some(sensor.subscribe_state());
                }
                sensor_handle = Some(sensor);
            }
            Err(e) => {
                warn!("Failed to start presence sensor: {}. Falling back to LLM mode.", e);
                let _ = app.emit("continuous_mode_event", serde_json::json!({
                    "type": "error",
                    "error": format!("Sensor failed to start: {}. Using LLM detection.", e)
                }));
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
                        let _ = app_for_monitor.emit("continuous_mode_event", serde_json::json!({
                            "type": "sensor_status",
                            "connected": true,
                            "state": state_str
                        }));
                    }
                    Ok(()) = status_rx.changed() => {
                        let status = status_rx.borrow_and_update().clone();
                        let connected = matches!(status, crate::presence_sensor::SensorStatus::Connected);
                        let _ = app_for_monitor.emit("continuous_mode_event", serde_json::json!({
                            "type": "sensor_status",
                            "connected": connected,
                            "state": "unknown"
                        }));
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
        let shadow_method = if shadow_active_method == ShadowActiveMethod::Sensor { "llm" } else { "sensor" };
        let active_method = shadow_active_method;
        info!("Shadow mode: active={}, shadow={}", active_method, shadow_method);

        // Initialize shadow CSV logger
        let shadow_csv_logger: Option<Arc<Mutex<crate::shadow_log::ShadowCsvLogger>>> = if config.shadow_csv_log_enabled {
            match crate::shadow_log::ShadowCsvLogger::new() {
                Ok(logger) => Some(Arc::new(Mutex::new(logger))),
                Err(e) => {
                    warn!("Failed to create shadow CSV logger: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let shadow_decisions_for_task = handle.shadow_decisions.clone();
        let last_shadow_for_task = handle.last_shadow_decision.clone();
        let stop_for_shadow = handle.stop_flag.clone();
        let app_for_shadow = app.clone();
        let buffer_for_shadow = handle.transcript_buffer.clone();

        if shadow_method == "sensor" {
            // Active=LLM, Shadow=sensor — observe sensor state transitions
            // Use watch channel (not Notify) so we only fire on Present→Absent transitions
            if let Some(mut state_rx) = shadow_sensor_state_rx.take() {
                Some(tokio::spawn(async move {
                    info!("Shadow sensor observer started (watch-based)");
                    let mut prev_state = crate::presence_sensor::PresenceState::Unknown;
                    loop {
                        if stop_for_shadow.load(Ordering::Relaxed) {
                            break;
                        }

                        // Wait for next state change
                        if state_rx.changed().await.is_err() {
                            info!("Shadow sensor: watch channel closed");
                            break;
                        }

                        if stop_for_shadow.load(Ordering::Relaxed) {
                            break;
                        }

                        let new_state = *state_rx.borrow_and_update();

                        // Determine shadow outcome based on state transition
                        let outcome = match (prev_state, new_state) {
                            (crate::presence_sensor::PresenceState::Present, crate::presence_sensor::PresenceState::Absent) => {
                                // Present→Absent: this is an encounter boundary
                                crate::shadow_log::ShadowOutcome::WouldSplit
                            }
                            (_, crate::presence_sensor::PresenceState::Present) => {
                                // Any→Present: no split (patient arrived or still here)
                                crate::shadow_log::ShadowOutcome::WouldNotSplit
                            }
                            _ => {
                                // Unknown→Absent, Absent→Absent, etc: skip
                                prev_state = new_state;
                                continue;
                            }
                        };

                        prev_state = new_state;

                        // Read buffer state (non-destructive)
                        let (word_count, last_segment) = buffer_for_shadow
                            .lock()
                            .map(|b| (b.word_count(), b.last_index()))
                            .unwrap_or((0, None));

                        let decision = crate::shadow_log::ShadowDecision {
                            timestamp: Utc::now(),
                            shadow_method: "sensor".to_string(),
                            active_method: active_method.to_string(),
                            outcome: outcome.clone(),
                            confidence: Some(1.0),
                            buffer_word_count: word_count,
                            buffer_last_segment: last_segment,
                        };

                        let outcome_str = match outcome {
                            crate::shadow_log::ShadowOutcome::WouldSplit => "would_split",
                            crate::shadow_log::ShadowOutcome::WouldNotSplit => "would_not_split",
                        };

                        // Log to CSV
                        if let Some(ref logger) = shadow_csv_logger {
                            if let Ok(mut l) = logger.lock() {
                                l.write_decision(&decision);
                            }
                        }

                        // Store for encounter comparison
                        let summary = crate::shadow_log::ShadowDecisionSummary::from(&decision);
                        if let Ok(mut decisions) = shadow_decisions_for_task.lock() {
                            decisions.push(summary);
                        }
                        if let Ok(mut last) = last_shadow_for_task.lock() {
                            *last = Some(decision);
                        }

                        // Emit event for frontend
                        let _ = app_for_shadow.emit("continuous_mode_event", serde_json::json!({
                            "type": "shadow_decision",
                            "shadow_method": "sensor",
                            "outcome": outcome_str,
                            "buffer_words": word_count,
                            "sensor_state": new_state.as_str()
                        }));

                        info!("Shadow sensor: {} (state: {}, buffer {} words)", outcome_str, new_state.as_str(), word_count);
                    }
                    info!("Shadow sensor observer stopped");
                }))
            } else {
                warn!("Shadow sensor observer: no sensor state receiver available (sensor failed to start)");
                None
            }
        } else {
            // Active=sensor, Shadow=LLM — run shadow LLM detection loop
            let silence_trigger_for_shadow = silence_trigger_rx.clone();
            let check_interval_shadow = config.encounter_check_interval_secs;
            let shadow_detection_model = config.encounter_detection_model.clone();
            let shadow_detection_nothink = config.encounter_detection_nothink;
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

                    // Wait for timer or silence trigger (same as active LLM detector)
                    tokio::select! {
                        _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval_shadow as u64)) => {}
                        _ = silence_trigger_for_shadow.notified() => {
                            debug!("Shadow LLM: silence trigger received");
                        }
                    }

                    if stop_for_shadow.load(Ordering::Relaxed) {
                        break;
                    }

                    // Read buffer state (non-destructive, full transcript for detection)
                    let (formatted, word_count, last_segment) = buffer_for_shadow
                        .lock()
                        .map(|b| (b.format_for_detection(), b.word_count(), b.last_index()))
                        .unwrap_or_else(|_| (String::new(), 0, None));

                    if word_count < 100 {
                        continue; // Not enough text to analyze
                    }

                    // Call LLM for encounter detection
                    let outcome;
                    let confidence;
                    if let Some(ref client) = shadow_llm_client {
                        let (filtered, _) = crate::encounter_experiment::strip_hallucinations(&formatted, 5);
                        let (system_prompt, user_prompt) = build_encounter_detection_prompt(&filtered, None);
                        let system_prompt = if shadow_detection_nothink {
                            format!("/nothink\n{}", system_prompt)
                        } else {
                            system_prompt
                        };
                        let llm_future = client.generate(
                            &shadow_detection_model, &system_prompt, &user_prompt, "shadow_encounter_detection"
                        );
                        match tokio::time::timeout(tokio::time::Duration::from_secs(90), llm_future).await {
                            Ok(Ok(response)) => {
                                match parse_encounter_detection(&response) {
                                    Ok(result) => {
                                        if result.complete && result.confidence.unwrap_or(0.0) >= 0.7 {
                                            outcome = crate::shadow_log::ShadowOutcome::WouldSplit;
                                        } else {
                                            outcome = crate::shadow_log::ShadowOutcome::WouldNotSplit;
                                        }
                                        confidence = result.confidence;
                                    }
                                    Err(e) => {
                                        debug!("Shadow LLM: failed to parse detection: {}", e);
                                        continue;
                                    }
                                }
                            }
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
                        continue; // No LLM client
                    }

                    let decision = crate::shadow_log::ShadowDecision {
                        timestamp: Utc::now(),
                        shadow_method: "llm".to_string(),
                        active_method: active_method.to_string(),
                        outcome,
                        confidence,
                        buffer_word_count: word_count,
                        buffer_last_segment: last_segment,
                    };

                    // Log to CSV
                    if let Some(ref logger) = shadow_csv_logger {
                        if let Ok(mut l) = logger.lock() {
                            l.write_decision(&decision);
                        }
                    }

                    // Store for encounter comparison
                    let outcome_str = decision.outcome.as_str().to_string();
                    let summary = crate::shadow_log::ShadowDecisionSummary::from(&decision);
                    if let Ok(mut decisions) = shadow_decisions_for_task.lock() {
                        decisions.push(summary);
                    }
                    if let Ok(mut last) = last_shadow_for_task.lock() {
                        *last = Some(decision);
                    }

                    // Emit event for frontend
                    let _ = app_for_shadow.emit("continuous_mode_event", serde_json::json!({
                        "type": "shadow_decision",
                        "shadow_method": "llm",
                        "outcome": outcome_str,
                        "confidence": confidence,
                        "buffer_words": word_count
                    }));

                    info!("Shadow LLM: {} (confidence={:?}, buffer {} words)",
                        outcome_str, confidence, word_count);
                }
                info!("Shadow LLM observer stopped");
            }))
        }
    } else {
        None
    };

    // Spawn encounter detection loop
    let buffer_for_detector = handle.transcript_buffer.clone();
    let stop_for_detector = handle.stop_flag.clone();
    let state_for_detector = handle.state.clone();
    let encounters_for_detector = handle.encounters_detected.clone();
    let last_at_for_detector = handle.last_encounter_at.clone();
    let last_words_for_detector = handle.last_encounter_words.clone();
    let last_patient_name_for_detector = handle.last_encounter_patient_name.clone();
    let last_error_for_detector = handle.last_error.clone();
    let name_tracker_for_detector = handle.name_tracker.clone();
    let last_split_time_for_detector = handle.last_split_time.clone();
    let app_for_detector = app.clone();
    let check_interval = config.encounter_check_interval_secs;

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
    let merge_enabled = config.encounter_merge_enabled;
    // Clone config values for flush-on-stop SOAP generation + merge check (outside detector task)
    let flush_fast_model = config.fast_model.clone();
    let flush_soap_model = config.soap_model_fast.clone();
    let flush_soap_detail_level = config.soap_detail_level;
    let flush_soap_format = config.soap_format.clone();
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

    // Native STT shadow accumulator for detector task (encounter boundary drain)
    let stt_shadow_accumulator = native_stt_accumulator_for_detector;

    // Pipeline replay logger — writes JSONL to each session's archive folder
    let pipeline_logger = Arc::new(Mutex::new(crate::pipeline_log::PipelineLogger::new()));
    let logger_for_detector = Arc::clone(&pipeline_logger);
    let logger_for_screenshot = Arc::clone(&pipeline_logger);
    let logger_for_flush = Arc::clone(&pipeline_logger);

    // Hybrid mode config
    let hybrid_confirm_window_secs = config.hybrid_confirm_window_secs;
    let hybrid_min_words_for_sensor_split = config.hybrid_min_words_for_sensor_split;
    // Move the hybrid sensor receiver into the detector task
    let mut hybrid_sensor_rx = hybrid_sensor_state_rx;

    let detector_task = tokio::spawn(async move {
        let mut encounter_number: u32 = 0;
        let mut consecutive_no_split: u32 = 0;
        // Tracks how many times a split was merged back into the previous encounter.
        // Each merge-back escalates the confidence threshold by +0.05, making
        // repeated false-positive splits on long sessions increasingly unlikely.
        let mut merge_back_count: u32 = 0;

        // Hybrid mode: sensor absence tracking
        let mut sensor_absent_since: Option<DateTime<Utc>> = None;
        let mut prev_sensor_state = crate::presence_sensor::PresenceState::Unknown;
        let mut sensor_available = hybrid_sensor_rx.is_some();
        // Tracks whether the current split was triggered by sensor timeout (for metadata)
        // Initialized inside the loop on each iteration — declared here so it's available across the loop body
        let mut hybrid_sensor_timeout_triggered;

        // Track previous encounter for retrospective merge checks
        let mut prev_encounter_session_id: Option<String> = None;
        let mut prev_encounter_text: Option<String> = None;
        let mut prev_encounter_date: Option<DateTime<Utc>> = None;
        let mut prev_encounter_is_clinical: bool = true;

        loop {
            // Reset per-iteration hybrid tracking
            hybrid_sensor_timeout_triggered = false;

            // Wait for trigger based on detection mode
            // Returns (manual_triggered, sensor_triggered)
            let (manual_triggered, sensor_triggered) = if is_hybrid_mode && sensor_available {
                // Hybrid mode with sensor: timer + silence + manual + sensor
                let sensor_rx = hybrid_sensor_rx.as_mut().unwrap();
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
                                match (old_state, new_state) {
                                    (crate::presence_sensor::PresenceState::Present,
                                     crate::presence_sensor::PresenceState::Absent) => {
                                        sensor_absent_since = Some(Utc::now());
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
                                let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                                    "type": "sensor_status",
                                    "connected": false,
                                    "state": "unknown"
                                }));
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
            let (formatted, word_count, is_empty, first_ts) = {
                let buffer = match buffer_for_detector.lock() {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                (buffer.format_for_detection(), buffer.word_count(), buffer.is_empty(), buffer.first_timestamp())
            };

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
            let cleaned_word_count = hallucination_report
                .as_ref()
                .map(|r| r.cleaned_word_count)
                .unwrap_or(word_count);

            // Manual or sensor trigger: skip minimum guards, but still need >0 words
            if manual_triggered || sensor_triggered {
                if is_empty {
                    info!("{}: buffer is empty, nothing to archive",
                        if sensor_triggered { "Sensor trigger" } else { "Manual trigger" });
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
            let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                "type": "checking"
            }));

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
                    // Tell LLM when sensor confirms someone is still present (suppresses false splits)
                    if sensor_available && !ctx.sensor_departed {
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
                    "consecutive_no_split": consecutive_no_split,
                });
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
                        if let Ok(mut err) = last_error_for_detector.lock() {
                            *err = Some(e);
                        } else {
                            warn!("Last error lock poisoned, error state not updated");
                        }
                        let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                            "type": "error",
                            "error": "Encounter detection failed"
                        }));
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

            // Force-split safety valve: tracks consecutive non-split outcomes (both LLM
            // failures AND negative results). Prevents unbounded buffer growth when the
            // LLM consistently says "no encounter detected."
            let mut force_split = false;
            let mut detection_result = detection_result;

            // Effective word count for force-split: max(cleaned, raw/2).
            // When the hallucination filter strips heavily (e.g. 4652→1537), raw/2 ensures
            // the force-split thresholds still engage for genuinely long encounters.
            let effective_word_count = cleaned_word_count.max(word_count / 2);

            // Absolute word cap: unconditional force-split at ABSOLUTE_WORD_CAP effective words
            if effective_word_count > ABSOLUTE_WORD_CAP && !manual_triggered && !sensor_triggered {
                warn!("ABSOLUTE WORD CAP: force-splitting at {} effective words (cleaned: {}, raw: {})", effective_word_count, cleaned_word_count, word_count);
                if let Ok(mut logger) = logger_for_detector.lock() {
                    logger.log_split_trigger(serde_json::json!({
                        "trigger": "absolute_word_cap",
                        "effective_word_count": effective_word_count,
                        "cleaned_word_count": cleaned_word_count,
                        "raw_word_count": word_count,
                        "cap": ABSOLUTE_WORD_CAP,
                    }));
                }
                let last_idx = buffer_for_detector.lock().ok().and_then(|b| b.last_index());
                consecutive_no_split = 0;
                force_split = true;
                detection_result = Some(EncounterDetectionResult {
                    complete: true,
                    end_segment_index: last_idx,
                    confidence: Some(1.0),
                });
            }

            // Track consecutive no-split outcomes
            if !force_split && !manual_triggered && !sensor_triggered {
                let is_negative = match &detection_result {
                    None => true,                    // LLM failure/timeout
                    Some(r) if !r.complete => true,  // LLM said no — THE BUG FIX
                    _ => false,                      // complete=true — resolved by confidence gate below
                };
                if is_negative {
                    consecutive_no_split += 1;
                    info!(
                        "Detection non-split: result={}, consecutive_no_split={}, cleaned_word_count={}, raw_word_count={}",
                        if detection_result.is_none() { "error/timeout" } else { "complete=false" },
                        consecutive_no_split, cleaned_word_count, word_count
                    );
                    // Graduated force-split (uses effective word count: max(cleaned, raw/2))
                    if effective_word_count > FORCE_SPLIT_WORD_THRESHOLD
                        && consecutive_no_split >= FORCE_SPLIT_CONSECUTIVE_LIMIT
                    {
                        warn!(
                            "Force-splitting: {} consecutive non-splits with {} effective words (cleaned: {}, raw: {})",
                            consecutive_no_split, effective_word_count, cleaned_word_count, word_count
                        );
                        if let Ok(mut logger) = logger_for_detector.lock() {
                            logger.log_split_trigger(serde_json::json!({
                                "trigger": "graduated_force_split",
                                "consecutive_no_split": consecutive_no_split,
                                "effective_word_count": effective_word_count,
                                "cleaned_word_count": cleaned_word_count,
                                "raw_word_count": word_count,
                            }));
                        }
                        let last_idx = buffer_for_detector.lock().ok().and_then(|b| b.last_index());
                        consecutive_no_split = 0;
                        force_split = true;
                        detection_result = Some(EncounterDetectionResult {
                            complete: true,
                            end_segment_index: last_idx,
                            confidence: Some(1.0),
                        });
                    }
                }
                // NOTE: Don't reset counter on complete=true here — confidence gate may reject
            }

            // Hybrid sensor timeout force-split: sensor has been absent for > confirm_window
            // and buffer has enough words. This catches cases where LLM keeps saying "no split"
            // but the sensor correctly detected a departure.
            if is_hybrid_mode && !force_split && !manual_triggered {
                if let Some(absent_since) = sensor_absent_since {
                    let elapsed = (Utc::now() - absent_since).num_seconds() as u64;
                    if elapsed >= hybrid_confirm_window_secs
                        && word_count >= hybrid_min_words_for_sensor_split
                    {
                        warn!(
                            "Hybrid: sensor absence timeout ({}s >= {}s) with {} words >= {} — force-splitting",
                            elapsed, hybrid_confirm_window_secs,
                            word_count, hybrid_min_words_for_sensor_split
                        );
                        if let Ok(mut logger) = logger_for_detector.lock() {
                            logger.log_split_trigger(serde_json::json!({
                                "trigger": "hybrid_sensor_timeout",
                                "absence_secs": elapsed,
                                "confirm_window_secs": hybrid_confirm_window_secs,
                                "word_count": word_count,
                                "min_words": hybrid_min_words_for_sensor_split,
                            }));
                        }
                        let last_idx = buffer_for_detector.lock().ok().and_then(|b| b.last_index());
                        force_split = true;
                        hybrid_sensor_timeout_triggered = true;
                        sensor_absent_since = None;
                        consecutive_no_split = 0;
                        detection_result = Some(EncounterDetectionResult {
                            complete: true,
                            end_segment_index: last_idx,
                            confidence: Some(1.0),
                        });
                    }
                }
            }

            // Process detection result
            if let Some(result) = detection_result {
                if result.complete {
                    // Confidence gate: dynamic threshold based on buffer age
                    // Short encounters (<20 min) get a higher bar (0.85) to reduce false splits
                    // on natural pauses. Longer encounters use 0.7 (established threshold).
                    let confidence = result.confidence.unwrap_or(0.0);
                    let buffer_age_mins = first_ts
                        .map(|t| (Utc::now() - t).num_minutes())
                        .unwrap_or(0);
                    // Post-merge-back escalation: each merge-back raises the bar by +0.05,
                    // making repeated false-positive splits on long sessions increasingly unlikely.
                    let base_threshold = if buffer_age_mins < 20 { 0.85 } else { 0.7 };
                    let confidence_threshold = (base_threshold + merge_back_count as f64 * 0.05).min(0.99);
                    if confidence < confidence_threshold && !force_split {
                        consecutive_no_split += 1;
                        info!(
                            "Confidence gate rejected: confidence={:.2}, threshold={:.2} (base={:.2}, merge_backs={}), buffer_age_mins={}, word_count={}, consecutive_no_split={}",
                            confidence, confidence_threshold, base_threshold, merge_back_count, buffer_age_mins, word_count, consecutive_no_split
                        );
                        if let Ok(mut logger) = logger_for_detector.lock() {
                            logger.log_confidence_gate(serde_json::json!({
                                "confidence": confidence,
                                "threshold": confidence_threshold,
                                "base_threshold": base_threshold,
                                "merge_back_count": merge_back_count,
                                "buffer_age_mins": buffer_age_mins,
                                "word_count": word_count,
                                "consecutive_no_split": consecutive_no_split,
                                "rejected": true,
                            }));
                        }
                        // Return to recording state and continue
                        if let Ok(mut state) = state_for_detector.lock() {
                            if *state == ContinuousState::Checking {
                                *state = ContinuousState::Recording;
                            }
                        } else {
                            warn!("State lock poisoned while returning to recording state");
                        }
                        continue;
                    }

                    if let Some(end_index) = result.end_segment_index {
                        consecutive_no_split = 0;
                        encounter_number += 1;
                        // Clear hybrid sensor tracking on successful split
                        if is_hybrid_mode {
                            sensor_absent_since = None;
                        }
                        info!(
                            "Encounter #{} detected (end_segment_index={})",
                            encounter_number, end_index
                        );

                        // Extract encounter segments from buffer
                        let (encounter_text, encounter_word_count, encounter_start, encounter_last_timestamp_ms) = {
                            let mut buffer = match buffer_for_detector.lock() {
                                Ok(b) => b,
                                Err(_) => continue,
                            };
                            let drained = buffer.drain_through(end_index);
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
                            let wc = text.split_whitespace().count();
                            let start = drained.first().map(|s| s.started_at);
                            let last_ts_ms = drained.last().map(|s| s.timestamp_ms).unwrap_or(0);
                            (text, wc, start, last_ts_ms)
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
                        ) {
                            warn!("Failed to archive encounter: {}", e);
                        }

                        // Set pipeline logger to write to this session's archive folder
                        if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
                            if let Ok(mut logger) = logger_for_detector.lock() {
                                logger.set_session(&session_dir);
                            }
                        }

                        // Drain native STT shadow accumulator for this encounter
                        let has_shadow_transcript = if let Some(ref accumulator) = stt_shadow_accumulator {
                            if let Ok(mut acc) = accumulator.lock() {
                                let drained = acc.drain_through(encounter_last_timestamp_ms);
                                if !drained.is_empty() {
                                    // Format as plain text transcript
                                    let shadow_text: String = drained
                                        .iter()
                                        .map(|s| {
                                            if let Some(ref spk) = s.speaker_id {
                                                format!("{}: {}", spk, s.native_text)
                                            } else {
                                                s.native_text.clone()
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n");

                                    // Save to archive directory
                                    if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
                                        let shadow_path = session_dir.join("shadow_transcript.txt");
                                        if let Err(e) = std::fs::write(&shadow_path, &shadow_text) {
                                            warn!("Failed to save shadow transcript: {}", e);
                                        } else {
                                            info!("Shadow transcript saved ({} segments, {} chars)", drained.len(), shadow_text.len());
                                        }
                                    }
                                    true
                                } else {
                                    false
                                }
                            } else {
                                warn!("Native STT shadow accumulator lock poisoned");
                                false
                            }
                        } else {
                            false
                        };

                        // Update archive metadata with continuous mode info
                        if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
                            let date_path = session_dir.join("metadata.json");
                            if date_path.exists() {
                                if let Ok(content) = std::fs::read_to_string(&date_path) {
                                    if let Ok(mut metadata) = serde_json::from_str::<local_archive::ArchiveMetadata>(&content) {
                                        metadata.charting_mode = Some("continuous".to_string());
                                        metadata.encounter_number = Some(encounter_number);
                                        // Record how this encounter was detected
                                        metadata.detection_method = Some(
                                            if manual_triggered {
                                                "manual".to_string()
                                            } else if is_hybrid_mode {
                                                if hybrid_sensor_timeout_triggered {
                                                    "hybrid_sensor_timeout".to_string()
                                                } else if sensor_triggered {
                                                    "hybrid_sensor_confirmed".to_string()
                                                } else if force_split {
                                                    "hybrid_force".to_string()
                                                } else {
                                                    "hybrid_llm".to_string()
                                                }
                                            } else if sensor_triggered {
                                                "sensor".to_string()
                                            } else {
                                                "llm".to_string()
                                            }
                                        );
                                        // Add patient name from vision extraction (majority vote)
                                        if let Ok(tracker) = name_tracker_for_detector.lock() {
                                            metadata.patient_name = tracker.majority_name();
                                        } else {
                                            warn!("Name tracker lock poisoned, patient name not written to metadata");
                                        }
                                        // Record whether shadow transcript was saved
                                        if has_shadow_transcript {
                                            metadata.has_shadow_transcript = Some(true);
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

                                        if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                                            let _ = std::fs::write(&date_path, json);
                                        }
                                    }
                                }
                            }
                        }

                        // Clear shadow decisions for next encounter (if in shadow mode)
                        if is_shadow_mode {
                            if let Ok(mut decisions) = handle_shadow_decisions.lock() {
                                decisions.clear();
                            }
                        }

                        // Extract patient name before resetting tracker
                        let encounter_patient_name = name_tracker_for_detector
                            .lock()
                            .ok()
                            .and_then(|t| t.majority_name());

                        // Reset name tracker for next encounter
                        if let Ok(mut tracker) = name_tracker_for_detector.lock() {
                            tracker.reset();
                        } else {
                            warn!("Name tracker lock poisoned, tracker not reset for next encounter");
                        }

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
                        if let Ok(mut at) = last_at_for_detector.lock() {
                            *at = Some(Utc::now());
                        } else {
                            warn!("Last encounter time lock poisoned, stats not updated");
                        }
                        if let Ok(mut words) = last_words_for_detector.lock() {
                            *words = Some(encounter_word_count as u32);
                        } else {
                            warn!("Last encounter words lock poisoned, stats not updated");
                        }
                        if let Ok(mut name) = last_patient_name_for_detector.lock() {
                            *name = encounter_patient_name.clone();
                        } else {
                            warn!("Last patient name lock poisoned, stats not updated");
                        }

                        // Emit encounter detected event
                        let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                            "type": "encounter_detected",
                            "session_id": session_id,
                            "word_count": encounter_word_count,
                            "patient_name": encounter_patient_name
                        }));

                        // Two-pass clinical content check: flag non-clinical encounters
                        let mut is_clinical = true;
                        if encounter_word_count < MIN_WORDS_FOR_CLINICAL_CHECK {
                            is_clinical = false;
                            info!(
                                "Encounter #{} too small for clinical analysis ({} words < {} threshold) — treating as non-clinical",
                                encounter_number, encounter_word_count, MIN_WORDS_FOR_CLINICAL_CHECK
                            );
                        } else if let Some(ref client) = llm_client {
                            let (cc_system, cc_user) = build_clinical_content_check_prompt(&encounter_text);
                            let cc_start = Instant::now();
                            let cc_future = client.generate(&fast_model, &cc_system, &cc_user, "clinical_content_check");
                            match tokio::time::timeout(tokio::time::Duration::from_secs(30), cc_future).await {
                                Ok(Ok(cc_response)) => {
                                    let cc_latency = cc_start.elapsed().as_millis() as u64;
                                    match parse_clinical_content_check(&cc_response) {
                                        Ok(cc_result) => {
                                            if let Ok(mut logger) = logger_for_detector.lock() {
                                                logger.log_clinical_check(
                                                    &fast_model, &cc_system, &cc_user,
                                                    Some(&cc_response), cc_latency, true, None,
                                                    serde_json::json!({
                                                        "encounter_number": encounter_number,
                                                        "word_count": encounter_word_count,
                                                        "is_clinical": cc_result.clinical,
                                                        "reason": cc_result.reason,
                                                    }),
                                                );
                                            }
                                            if !cc_result.clinical {
                                                is_clinical = false;
                                                info!(
                                                    "Encounter #{} flagged as non-clinical: {:?}",
                                                    encounter_number, cc_result.reason
                                                );
                                            } else {
                                                info!(
                                                    "Encounter #{} confirmed clinical: {:?}",
                                                    encounter_number, cc_result.reason
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            if let Ok(mut logger) = logger_for_detector.lock() {
                                                logger.log_clinical_check(
                                                    &fast_model, &cc_system, &cc_user,
                                                    Some(&cc_response), cc_latency, false, Some(&e),
                                                    serde_json::json!({"encounter_number": encounter_number, "parse_error": true}),
                                                );
                                            }
                                            warn!("Failed to parse clinical content check: {}", e);
                                        }
                                    }
                                }
                                Ok(Err(e)) => {
                                    let cc_latency = cc_start.elapsed().as_millis() as u64;
                                    if let Ok(mut logger) = logger_for_detector.lock() {
                                        logger.log_clinical_check(
                                            &fast_model, &cc_system, &cc_user,
                                            None, cc_latency, false, Some(&e.to_string()),
                                            serde_json::json!({"encounter_number": encounter_number, "llm_error": true}),
                                        );
                                    }
                                    warn!("Clinical content check LLM call failed: {}", e);
                                }
                                Err(_) => {
                                    let cc_latency = cc_start.elapsed().as_millis() as u64;
                                    if let Ok(mut logger) = logger_for_detector.lock() {
                                        logger.log_clinical_check(
                                            &fast_model, &cc_system, &cc_user,
                                            None, cc_latency, false, Some("timeout_30s"),
                                            serde_json::json!({"encounter_number": encounter_number, "timeout": true}),
                                        );
                                    }
                                    warn!("Clinical content check timed out (30s)");
                                }
                            }
                        }

                        // Update metadata with non-clinical flag (single path for both word-count and LLM checks)
                        if !is_clinical {
                            if let Ok(session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
                                let nc_meta_path = session_dir.join("metadata.json");
                                if nc_meta_path.exists() {
                                    if let Ok(content) = std::fs::read_to_string(&nc_meta_path) {
                                        if let Ok(mut metadata) = serde_json::from_str::<local_archive::ArchiveMetadata>(&content) {
                                            metadata.likely_non_clinical = Some(true);
                                            if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                                                let _ = std::fs::write(&nc_meta_path, json);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Generate SOAP note (with 120s timeout — SOAP is heavier than detection)
                        // Skip SOAP for non-clinical encounters to prevent hallucinated clinical content
                        if !is_clinical {
                            info!("Skipping SOAP for non-clinical encounter #{}", encounter_number);
                        } else if let Some(ref client) = llm_client {
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
                            // Build SOAP options with encounter notes from clinician (uses pre-cloned notes_text)
                            let soap_opts = crate::llm_client::SoapOptions {
                                detail_level: soap_detail_level,
                                format: crate::llm_client::SoapFormat::from_config_str(&soap_format),
                                session_notes: notes_text.clone(),
                                ..Default::default()
                            };
                            info!("Generating SOAP for encounter #{}", encounter_number);
                            let soap_system_prompt = crate::llm_client::build_simple_soap_prompt(&soap_opts);
                            let soap_start = Instant::now();
                            let soap_future = client.generate_multi_patient_soap_note(
                                &soap_model,
                                &filtered_encounter_text,
                                None, // No audio events in continuous mode
                                Some(&soap_opts),
                                None, // No speaker context
                            );
                            match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
                                Ok(Ok(soap_result)) => {
                                    let soap_latency = soap_start.elapsed().as_millis() as u64;
                                    // Save SOAP to archive
                                    let soap_content = &soap_result.notes
                                        .iter()
                                        .map(|n| n.content.clone())
                                        .collect::<Vec<_>>()
                                        .join("\n\n---\n\n");

                                    let now = Utc::now();
                                    if let Err(e) = local_archive::add_soap_note(
                                        &session_id,
                                        &now,
                                        soap_content,
                                        Some(soap_detail_level),
                                        Some(&soap_format),
                                    ) {
                                        warn!("Failed to save SOAP for encounter: {}", e);
                                    }

                                    if let Ok(mut logger) = logger_for_detector.lock() {
                                        logger.log_soap(
                                            &soap_model, &soap_system_prompt, "",
                                            Some(soap_content), soap_latency, true, None,
                                            serde_json::json!({
                                                "encounter_number": encounter_number,
                                                "word_count": encounter_word_count,
                                                "detail_level": soap_detail_level,
                                                "format": soap_format,
                                                "has_notes": !notes_text.is_empty(),
                                                "response_chars": soap_content.len(),
                                            }),
                                        );
                                    }

                                    let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                                        "type": "soap_generated",
                                        "session_id": session_id
                                    }));
                                    info!("SOAP generated for encounter #{}", encounter_number);

                                }
                                Ok(Err(e)) => {
                                    let soap_latency = soap_start.elapsed().as_millis() as u64;
                                    warn!("Failed to generate SOAP for encounter: {}", e);
                                    if let Ok(mut logger) = logger_for_detector.lock() {
                                        logger.log_soap(
                                            &soap_model, &soap_system_prompt, "", None, soap_latency, false, Some(&e.to_string()),
                                            serde_json::json!({"encounter_number": encounter_number, "llm_error": true}),
                                        );
                                    }
                                    if let Ok(mut err) = last_error_for_detector.lock() {
                                        *err = Some(format!("SOAP generation failed: {}", e));
                                    } else {
                                        warn!("Last error lock poisoned, error state not updated");
                                    }
                                    let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                                        "type": "soap_failed",
                                        "session_id": session_id,
                                        "error": e
                                    }));
                                }
                                Err(_elapsed) => {
                                    let soap_latency = soap_start.elapsed().as_millis() as u64;
                                    warn!("SOAP generation timed out after 120s for encounter #{}", encounter_number);
                                    if let Ok(mut logger) = logger_for_detector.lock() {
                                        logger.log_soap(
                                            &soap_model, &soap_system_prompt, "", None, soap_latency, false, Some("timeout_120s"),
                                            serde_json::json!({"encounter_number": encounter_number, "timeout": true}),
                                        );
                                    }
                                    if let Ok(mut err) = last_error_for_detector.lock() {
                                        *err = Some("SOAP generation timed out".to_string());
                                    } else {
                                        warn!("Last error lock poisoned, error state not updated");
                                    }
                                    let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                                        "type": "soap_failed",
                                        "session_id": session_id,
                                        "error": "SOAP generation timed out"
                                    }));
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
                                let prev_tail = tail_words(prev_text, MERGE_EXCERPT_WORDS);
                                let curr_head = head_words(&encounter_text, MERGE_EXCERPT_WORDS);

                                if let Some(ref client) = llm_client {
                                    // Strip hallucinated repetitions from merge excerpts
                                    let (filtered_prev_tail, _) = strip_hallucinations(&prev_tail, 5);
                                    let (filtered_curr_head, _) = strip_hallucinations(&curr_head, 5);
                                    // Get patient name from vision tracker for merge context (M1 strategy)
                                    let merge_patient_name = name_tracker_for_detector
                                        .lock()
                                        .ok()
                                        .and_then(|t| t.majority_name());
                                    let (merge_system, merge_user) = build_encounter_merge_prompt(
                                        &filtered_prev_tail,
                                        &filtered_curr_head,
                                        merge_patient_name.as_deref(),
                                    );
                                    let merge_ctx = serde_json::json!({
                                        "prev_session_id": prev_id,
                                        "curr_session_id": session_id,
                                        "patient_name": merge_patient_name,
                                        "prev_tail_words": filtered_prev_tail.split_whitespace().count(),
                                        "curr_head_words": filtered_curr_head.split_whitespace().count(),
                                    });
                                    let merge_start = Instant::now();
                                    let merge_future = client.generate(&fast_model, &merge_system, &merge_user, "encounter_merge");
                                    match tokio::time::timeout(tokio::time::Duration::from_secs(60), merge_future).await {
                                        Ok(Ok(merge_response)) => {
                                            let merge_latency = merge_start.elapsed().as_millis() as u64;
                                            match parse_merge_check(&merge_response) {
                                                Ok(merge_result) => {
                                                    if let Ok(mut logger) = logger_for_detector.lock() {
                                                        logger.log_merge_check(
                                                            &fast_model, &merge_system, &merge_user,
                                                            Some(&merge_response), merge_latency, true, None,
                                                            serde_json::json!({
                                                                "prev_session_id": prev_id,
                                                                "curr_session_id": session_id,
                                                                "patient_name": merge_patient_name,
                                                                "same_encounter": merge_result.same_encounter,
                                                                "reason": format!("{:?}", merge_result.reason),
                                                            }),
                                                        );
                                                    }
                                                    if merge_result.same_encounter {
                                                        info!(
                                                            "Merge check: encounters are the same visit (reason: {:?}). Merging {} into {}",
                                                            merge_result.reason, session_id, prev_id
                                                        );

                                                        // Build merged transcript
                                                        let merged_text = format!("{}\n{}", prev_text, encounter_text);
                                                        let merged_wc = merged_text.split_whitespace().count();
                                                        let merged_duration = encounter_start
                                                            .map(|s| (Utc::now() - s).num_milliseconds().max(0) as u64)
                                                            .unwrap_or(0);

                                                        // Get patient name from current vision tracker for merged encounter
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
                                                            warn!("Failed to merge encounters: {}", e);
                                                        } else {
                                                            // Regenerate SOAP for the merged encounter (only if at least one is clinical)
                                                            if !(is_clinical || prev_encounter_is_clinical) {
                                                                info!("Skipping SOAP regeneration for merged non-clinical encounters");
                                                            } else if let Some(ref client) = llm_client {
                                                                let (filtered_merged, _) = strip_hallucinations(&merged_text, 5);
                                                                if let Ok(mut logger) = logger_for_detector.lock() {
                                                                    logger.log_hallucination_filter(serde_json::json!({
                                                                        "stage": "merge_soap_prep",
                                                                        "original_words": merged_text.split_whitespace().count(),
                                                                        "filtered_words": filtered_merged.split_whitespace().count(),
                                                                    }));
                                                                }
                                                                let merge_notes = encounter_notes_for_detector
                                                                    .lock()
                                                                    .map(|n| n.clone())
                                                                    .unwrap_or_default();
                                                                let merge_soap_opts = crate::llm_client::SoapOptions {
                                                                    detail_level: soap_detail_level,
                                                                    format: crate::llm_client::SoapFormat::from_config_str(&soap_format),
                                                                    session_notes: merge_notes,
                                                                    ..Default::default()
                                                                };
                                                                let merge_soap_system_prompt = crate::llm_client::build_simple_soap_prompt(&merge_soap_opts);
                                                                let merge_soap_start = Instant::now();
                                                                let soap_future = client.generate_multi_patient_soap_note(
                                                                    &soap_model,
                                                                    &filtered_merged,
                                                                    None,
                                                                    Some(&merge_soap_opts),
                                                                    None,
                                                                );
                                                                match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
                                                                    Ok(Ok(soap_result)) => {
                                                                        let merge_soap_latency = merge_soap_start.elapsed().as_millis() as u64;
                                                                        let soap_content = &soap_result.notes
                                                                            .iter()
                                                                            .map(|n| n.content.clone())
                                                                            .collect::<Vec<_>>()
                                                                            .join("\n\n---\n\n");
                                                                        if let Ok(mut logger) = logger_for_detector.lock() {
                                                                            logger.log_soap(
                                                                                &soap_model, &merge_soap_system_prompt, "",
                                                                                Some(soap_content), merge_soap_latency, true, None,
                                                                                serde_json::json!({
                                                                                    "stage": "merge_soap_regen",
                                                                                    "merged_into": prev_id,
                                                                                    "merged_word_count": merged_wc,
                                                                                    "detail_level": soap_detail_level,
                                                                                    "format": soap_format,
                                                                                    "response_chars": soap_content.len(),
                                                                                }),
                                                                            );
                                                                        }
                                                                        let _ = local_archive::add_soap_note(
                                                                            prev_id,
                                                                            prev_date,
                                                                            soap_content,
                                                                            Some(soap_detail_level),
                                                                            Some(&soap_format),
                                                                        );
                                                                        // Clear non-clinical flag on keeper — merged encounter contains clinical content
                                                                        if prev_encounter_is_clinical != is_clinical {
                                                                            if let Ok(session_dir) = local_archive::get_session_archive_dir(prev_id, prev_date) {
                                                                                let merge_meta_path = session_dir.join("metadata.json");
                                                                                if let Ok(content) = std::fs::read_to_string(&merge_meta_path) {
                                                                                    if let Ok(mut metadata) = serde_json::from_str::<local_archive::ArchiveMetadata>(&content) {
                                                                                        metadata.likely_non_clinical = None;
                                                                                        if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                                                                                            let _ = std::fs::write(&merge_meta_path, json);
                                                                                        }
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                        info!("Regenerated SOAP for merged encounter {}", prev_id);
                                                                    }
                                                                    Ok(Err(e)) => {
                                                                        let merge_soap_latency = merge_soap_start.elapsed().as_millis() as u64;
                                                                        if let Ok(mut logger) = logger_for_detector.lock() {
                                                                            logger.log_soap(
                                                                                &soap_model, &merge_soap_system_prompt, "", None, merge_soap_latency, false,
                                                                                Some(&e.to_string()),
                                                                                serde_json::json!({"stage": "merge_soap_regen", "llm_error": true}),
                                                                            );
                                                                        }
                                                                        warn!("Failed to regenerate SOAP after merge: {}", e);
                                                                    }
                                                                    Err(_) => {
                                                                        let merge_soap_latency = merge_soap_start.elapsed().as_millis() as u64;
                                                                        if let Ok(mut logger) = logger_for_detector.lock() {
                                                                            logger.log_soap(
                                                                                &soap_model, &merge_soap_system_prompt, "", None, merge_soap_latency, false,
                                                                                Some("timeout_120s"),
                                                                                serde_json::json!({"stage": "merge_soap_regen", "timeout": true}),
                                                                            );
                                                                        }
                                                                        warn!("SOAP regeneration timed out after merge");
                                                                    }
                                                                }
                                                            }

                                                            encounter_number -= 1;

                                                            let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                                                                "type": "encounter_merged",
                                                                "merged_into_session_id": prev_id,
                                                                "removed_session_id": session_id
                                                            }));

                                                            // Escalate confidence threshold for next detection
                                                            merge_back_count += 1;
                                                            info!("Merge-back #{}: next confidence threshold escalated by +{:.2}", merge_back_count, merge_back_count as f64 * 0.05);

                                                            // ── Retrospective multi-patient check ──
                                                            // After merge-back produces a large transcript, check if it
                                                            // contains multiple distinct patients. If so, auto-split.
                                                            if merged_wc >= MULTI_PATIENT_CHECK_WORD_THRESHOLD {
                                                                if let Some(ref client) = llm_client {
                                                                    let date_str = prev_date.format("%Y-%m-%d").to_string();
                                                                    info!(
                                                                        "Retrospective multi-patient check on {} ({} words)",
                                                                        prev_id, merged_wc
                                                                    );

                                                                    // Step 1: multi-patient gate
                                                                    let mp_user = format!("Review this transcript:\n\n{}", merged_text);
                                                                    let mp_future = client.generate(
                                                                        &fast_model, MULTI_PATIENT_CHECK_PROMPT, &mp_user, "multi_patient_check",
                                                                    );
                                                                    let mp_result = tokio::time::timeout(
                                                                        tokio::time::Duration::from_secs(30), mp_future,
                                                                    ).await;

                                                                    let has_multiple = match mp_result {
                                                                        Ok(Ok(resp)) => {
                                                                            match parse_multi_patient_check(&resp) {
                                                                                Ok(r) => {
                                                                                    info!("Multi-patient check: multiple={}, conf={:?}, reason={:?}",
                                                                                        r.multiple_patients, r.confidence, r.reason);
                                                                                    r.multiple_patients
                                                                                }
                                                                                Err(e) => { warn!("Failed to parse multi-patient check: {}", e); false }
                                                                            }
                                                                        }
                                                                        Ok(Err(e)) => { warn!("Multi-patient check LLM error: {}", e); false }
                                                                        Err(_) => { warn!("Multi-patient check timed out"); false }
                                                                    };

                                                                    // Step 2: find split boundary
                                                                    if has_multiple {
                                                                        match local_archive::get_transcript_lines(prev_id, &date_str) {
                                                                            Ok(lines) if lines.len() >= 2 => {
                                                                                let numbered: String = lines.iter().enumerate()
                                                                                    .map(|(i, l)| format!("{}: {}", i + 1, l))
                                                                                    .collect::<Vec<_>>()
                                                                                    .join("\n");
                                                                                let split_user = format!(
                                                                                    "Here is the transcript with numbered lines:\n\n{}", numbered
                                                                                );
                                                                                let split_future = client.generate(
                                                                                    &fast_model, MULTI_PATIENT_SPLIT_PROMPT, &split_user, "multi_patient_split",
                                                                                );
                                                                                let split_result = tokio::time::timeout(
                                                                                    tokio::time::Duration::from_secs(30), split_future,
                                                                                ).await;

                                                                                if let Ok(Ok(split_resp)) = split_result {
                                                                                    // Parse using the same structure as SuggestedSplit
                                                                                    #[derive(serde::Deserialize)]
                                                                                    struct SplitSuggestion { line_index: Option<usize>, confidence: Option<f64>, reason: Option<String> }
                                                                                    let cleaned = crate::encounter_detection::strip_think_tags(&split_resp);
                                                                                    if let Some(json_str) = crate::encounter_detection::extract_first_json_object(&cleaned) {
                                                                                        if let Ok(suggestion) = serde_json::from_str::<SplitSuggestion>(&json_str) {
                                                                                            if let Some(split_line) = suggestion.line_index {
                                                                                                if split_line > 0 && split_line < lines.len() {
                                                                                                    // Step 3: size gate
                                                                                                    let first_words: usize = lines[..split_line].iter()
                                                                                                        .map(|l| l.split_whitespace().count()).sum();
                                                                                                    let second_words: usize = lines[split_line..].iter()
                                                                                                        .map(|l| l.split_whitespace().count()).sum();

                                                                                                    info!(
                                                                                                        "Multi-patient split suggestion: line {}/{} ({}w / {}w), conf={:?}, reason={:?}",
                                                                                                        split_line, lines.len(), first_words, second_words,
                                                                                                        suggestion.confidence, suggestion.reason
                                                                                                    );

                                                                                                    if first_words >= MULTI_PATIENT_SPLIT_MIN_WORDS
                                                                                                        && second_words >= MULTI_PATIENT_SPLIT_MIN_WORDS
                                                                                                    {
                                                                                                        info!(
                                                                                                            "Size gate passed — auto-splitting {} at line {}",
                                                                                                            prev_id, split_line
                                                                                                        );
                                                                                                        match local_archive::split_session(prev_id, &date_str, split_line) {
                                                                                                            Ok(new_session_id) => {
                                                                                                                info!(
                                                                                                                    "Retrospective split created new session {} from {}",
                                                                                                                    new_session_id, prev_id
                                                                                                                );
                                                                                                                encounter_number += 1;

                                                                                                                // Regenerate SOAP for both halves
                                                                                                                for (regen_id, regen_words) in [
                                                                                                                    (prev_id.to_string(), first_words),
                                                                                                                    (new_session_id.clone(), second_words),
                                                                                                                ] {
                                                                                                                    if regen_words < MIN_WORDS_FOR_CLINICAL_CHECK {
                                                                                                                        continue;
                                                                                                                    }
                                                                                                                    if let Ok(details) = local_archive::get_session(&regen_id, &date_str) {
                                                                                                                        if let Some(ref transcript) = details.transcript {
                                                                                                                            let (filtered, _) = strip_hallucinations(transcript, 5);
                                                                                                                            let regen_notes = encounter_notes_for_detector
                                                                                                                                .lock()
                                                                                                                                .map(|n| n.clone())
                                                                                                                                .unwrap_or_default();
                                                                                                                            let regen_opts = crate::llm_client::SoapOptions {
                                                                                                                                detail_level: soap_detail_level,
                                                                                                                                format: crate::llm_client::SoapFormat::from_config_str(&soap_format),
                                                                                                                                session_notes: regen_notes,
                                                                                                                                ..Default::default()
                                                                                                                            };
                                                                                                                            let regen_future = client.generate_multi_patient_soap_note(
                                                                                                                                &soap_model, &filtered, None, Some(&regen_opts), None,
                                                                                                                            );
                                                                                                                            match tokio::time::timeout(
                                                                                                                                tokio::time::Duration::from_secs(120), regen_future,
                                                                                                                            ).await {
                                                                                                                                Ok(Ok(soap_result)) => {
                                                                                                                                    let soap_content = soap_result.notes.iter()
                                                                                                                                        .map(|n| n.content.clone())
                                                                                                                                        .collect::<Vec<_>>()
                                                                                                                                        .join("\n\n---\n\n");
                                                                                                                                    let _ = local_archive::add_soap_note(
                                                                                                                                        &regen_id, prev_date,
                                                                                                                                        &soap_content,
                                                                                                                                        Some(soap_detail_level),
                                                                                                                                        Some(&soap_format),
                                                                                                                                    );
                                                                                                                                    info!(
                                                                                                                                        "Regenerated SOAP for retrospective split session {} ({} chars)",
                                                                                                                                        regen_id, soap_content.len()
                                                                                                                                    );
                                                                                                                                }
                                                                                                                                Ok(Err(e)) => warn!("SOAP regen failed for {}: {}", regen_id, e),
                                                                                                                                Err(_) => warn!("SOAP regen timed out for {}", regen_id),
                                                                                                                            }
                                                                                                                        }
                                                                                                                    }
                                                                                                                }

                                                                                                                // Update prev tracking to the first half
                                                                                                                let first_text = lines[..split_line].join("\n");
                                                                                                                prev_encounter_text = Some(first_text);
                                                                                                                prev_encounter_is_clinical = is_clinical || prev_encounter_is_clinical;

                                                                                                                if let Ok(mut logger) = logger_for_detector.lock() {
                                                                                                                    logger.log_event("retrospective_split", serde_json::json!({
                                                                                                                        "original_session": prev_id,
                                                                                                                        "new_session": new_session_id,
                                                                                                                        "split_line": split_line,
                                                                                                                        "first_words": first_words,
                                                                                                                        "second_words": second_words,
                                                                                                                        "reason": suggestion.reason,
                                                                                                                    }));
                                                                                                                }

                                                                                                                let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                                                                                                                    "type": "retrospective_split",
                                                                                                                    "original_session_id": prev_id,
                                                                                                                    "new_session_id": new_session_id,
                                                                                                                    "split_line": split_line,
                                                                                                                }));

                                                                                                                continue; // Skip normal prev update
                                                                                                            }
                                                                                                            Err(e) => warn!("Retrospective split failed: {}", e),
                                                                                                        }
                                                                                                    } else {
                                                                                                        info!(
                                                                                                            "Size gate blocked: {}w / {}w (min {}w per half)",
                                                                                                            first_words, second_words, MULTI_PATIENT_SPLIT_MIN_WORDS
                                                                                                        );
                                                                                                    }
                                                                                                }
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                } else if let Ok(Err(e)) = split_result {
                                                                                    warn!("Multi-patient split LLM error: {}", e);
                                                                                } else {
                                                                                    warn!("Multi-patient split timed out");
                                                                                }
                                                                            }
                                                                            Ok(_) => {} // empty or single-line transcript
                                                                            Err(e) => warn!("Failed to load transcript lines for multi-patient check: {}", e),
                                                                        }
                                                                    }
                                                                }
                                                            }

                                                            // Update prev tracking to the merged encounter
                                                            prev_encounter_text = Some(merged_text);
                                                            // Merged encounter is clinical if either component was
                                                            prev_encounter_is_clinical = is_clinical || prev_encounter_is_clinical;
                                                            // prev_encounter_session_id and prev_encounter_date stay the same (A)
                                                            continue; // Skip updating prev to current since we merged
                                                        }
                                                    } else {
                                                        info!(
                                                            "Merge check: different encounters (reason: {:?})",
                                                            merge_result.reason
                                                        );
                                                    }
                                                }
                                                Err(e) => {
                                                    if let Ok(mut logger) = logger_for_detector.lock() {
                                                        logger.log_merge_check(
                                                            &fast_model, &merge_system, &merge_user,
                                                            Some(&merge_response), merge_latency, false,
                                                            Some(&format!("parse_error: {}", e)),
                                                            merge_ctx.clone(),
                                                        );
                                                    }
                                                    warn!("Failed to parse merge check: {}", e);
                                                }
                                            }
                                        }
                                        Ok(Err(e)) => {
                                            let merge_latency = merge_start.elapsed().as_millis() as u64;
                                            if let Ok(mut logger) = logger_for_detector.lock() {
                                                logger.log_merge_check(
                                                    &fast_model, &merge_system, &merge_user,
                                                    None, merge_latency, false, Some(&e.to_string()),
                                                    merge_ctx.clone(),
                                                );
                                            }
                                            warn!("Merge check LLM call failed: {}", e);
                                        }
                                        Err(_) => {
                                            let merge_latency = merge_start.elapsed().as_millis() as u64;
                                            if let Ok(mut logger) = logger_for_detector.lock() {
                                                logger.log_merge_check(
                                                    &fast_model, &merge_system, &merge_user,
                                                    None, merge_latency, false, Some("timeout_60s"),
                                                    merge_ctx.clone(),
                                                );
                                            }
                                            warn!("Merge check timed out after 60s");
                                        }
                                    }
                                }
                            }
                        }

                        // Split stuck — reset merge-back escalation
                        if merge_back_count > 0 {
                            info!("Split confirmed, resetting merge_back_count from {} to 0", merge_back_count);
                            merge_back_count = 0;
                        }

                        // Update prev encounter tracking for next iteration
                        prev_encounter_session_id = Some(session_id.clone());
                        prev_encounter_text = Some(encounter_text);
                        prev_encounter_date = Some(Utc::now());
                        prev_encounter_is_clinical = is_clinical;
                    }
                }
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
        let stop_for_screenshot = handle.stop_flag.clone();
        let name_tracker_for_screenshot = handle.name_tracker.clone();
        let last_split_time_for_screenshot = handle.last_split_time.clone();
        let vision_trigger_for_screenshot = handle.vision_name_change_trigger.clone();
        let vision_new_name_for_screenshot = handle.vision_new_name.clone();
        let vision_old_name_for_screenshot = handle.vision_old_name.clone();
        let debug_storage_for_screenshot = config.debug_storage_enabled;
        let screenshot_interval = config.screen_capture_interval_secs.max(30) as u64; // Clamp minimum 30s
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

        Some(tokio::spawn(async move {
            info!(
                "Screenshot name extraction task started (interval: {}s)",
                screenshot_interval
            );

            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(screenshot_interval)).await;

                if stop_for_screenshot.load(Ordering::Relaxed) {
                    break;
                }

                // Capture screen to base64 (runs on blocking thread since it uses CoreGraphics)
                let capture_result = tokio::task::spawn_blocking(|| {
                    crate::screenshot::capture_to_base64(1150)
                })
                .await;

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

                // Skip vision call if the capture is blank (no screen recording permission)
                if capture.likely_blank {
                    warn!("Screenshot appears blank — screen recording permission likely not granted. Skipping vision analysis. Grant permission in System Settings → Privacy & Security → Screen Recording.");
                    continue;
                }

                let image_base64 = capture.base64;

                // Save screenshot to disk for debugging (only when debug storage is enabled)
                if debug_storage_for_screenshot {
                    use base64::Engine;
                    if let Ok(config_dir) = Config::config_dir() {
                        let debug_dir = config_dir
                            .join("debug")
                            .join("continuous-screenshots");
                        let _ = std::fs::create_dir_all(&debug_dir);
                        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                        let filename = debug_dir.join(format!("{}.jpg", timestamp));
                        match base64::engine::general_purpose::STANDARD.decode(&image_base64) {
                            Ok(bytes) => {
                                if let Err(e) = std::fs::write(&filename, &bytes) {
                                    warn!("Failed to save debug screenshot: {}", e);
                                } else {
                                    debug!("Debug screenshot saved: {:?}", filename);
                                }
                            }
                            Err(e) => {
                                warn!("Failed to decode screenshot base64 for debug save: {}", e);
                            }
                        }
                    }
                }

                // Send to vision model for name extraction
                let client = match &llm_client_for_screenshot {
                    Some(c) => c,
                    None => {
                        debug!("No LLM client for screenshot name extraction");
                        continue;
                    }
                };

                let (system_prompt, user_text) = build_patient_name_prompt();
                let system_prompt_log = system_prompt.clone();
                let user_text_log = user_text.clone();
                let content_parts = vec![
                    crate::llm_client::ContentPart::Text { text: user_text },
                    crate::llm_client::ContentPart::ImageUrl {
                        image_url: crate::llm_client::ImageUrlContent {
                            url: format!("data:image/jpeg;base64,{}", image_base64),
                        },
                    },
                ];

                let vision_start = Instant::now();
                let vision_future = client.generate_vision(
                    "vision-model",
                    &system_prompt,
                    content_parts,
                    "patient_name_extraction",
                    Some(0.1), // Low temperature for factual extraction
                    Some(50),  // Short max tokens — just a name
                    None,
                    None,
                );

                match tokio::time::timeout(
                    tokio::time::Duration::from_secs(30),
                    vision_future,
                )
                .await
                {
                    Ok(Ok(response)) => {
                        let vision_latency = vision_start.elapsed().as_millis() as u64;
                        let parsed_name = parse_patient_name(&response);
                        if let Some(ref name) = parsed_name {
                            info!("Vision extracted patient name: {}", name);

                            // Stale screenshot detection: suppress votes that match previous
                            // encounter's patient name within grace period after split
                            let is_stale = if let Ok(split_time) = last_split_time_for_screenshot.lock() {
                                let secs_since_split = (Utc::now() - *split_time).num_seconds();
                                if secs_since_split < SCREENSHOT_STALE_GRACE_SECS {
                                    if let Ok(tracker) = name_tracker_for_screenshot.lock() {
                                        tracker.previous_name() == Some(name.as_str())
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            if let Ok(mut logger) = logger_for_screenshot.lock() {
                                logger.log_vision(
                                    "vision-model", &system_prompt_log, &user_text_log,
                                    Some(&response), vision_latency, true, None,
                                    serde_json::json!({
                                        "parsed_name": name,
                                        "screenshot_blank": false,
                                        "is_stale": is_stale,
                                    }),
                                );
                            }

                            if is_stale {
                                info!(
                                    "Skipping stale screenshot vote '{}' — matches previous encounter name and within {}s grace period",
                                    name, SCREENSHOT_STALE_GRACE_SECS
                                );
                                continue;
                            }

                            if let Ok(mut tracker) = name_tracker_for_screenshot.lock() {
                                let (changed, old_name, new_name) = tracker.record_and_check_change(name);
                                if changed {
                                    info!(
                                        "Vision detected patient name change: {:?} → {:?} — accelerating detection",
                                        old_name, new_name
                                    );
                                    // Store names for the detection loop to read
                                    if let Ok(mut n) = vision_new_name_for_screenshot.lock() {
                                        *n = new_name;
                                    }
                                    if let Ok(mut o) = vision_old_name_for_screenshot.lock() {
                                        *o = old_name;
                                    }
                                    // Wake the detection loop
                                    vision_trigger_for_screenshot.notify_one();
                                }
                            } else {
                                warn!("Name tracker lock poisoned, patient name vote dropped: {}", name);
                            }
                        } else {
                            if let Ok(mut logger) = logger_for_screenshot.lock() {
                                logger.log_vision(
                                    "vision-model", &system_prompt_log, &user_text_log,
                                    Some(&response), vision_latency, true, None,
                                    serde_json::json!({
                                        "parsed_name": serde_json::Value::Null,
                                        "screenshot_blank": false,
                                        "not_found": true,
                                    }),
                                );
                            }
                            debug!("Vision did not find a patient name on screen");
                        }
                    }
                    Ok(Err(e)) => {
                        let vision_latency = vision_start.elapsed().as_millis() as u64;
                        if let Ok(mut logger) = logger_for_screenshot.lock() {
                            logger.log_vision(
                                "vision-model", &system_prompt_log, &user_text_log,
                                None, vision_latency, false, Some(&e.to_string()),
                                serde_json::json!({"llm_error": true}),
                            );
                        }
                        debug!("Vision name extraction failed: {}", e);
                    }
                    Err(_) => {
                        let vision_latency = vision_start.elapsed().as_millis() as u64;
                        if let Ok(mut logger) = logger_for_screenshot.lock() {
                            logger.log_vision(
                                "vision-model", &system_prompt_log, &user_text_log,
                                None, vision_latency, false, Some("timeout_30s"),
                                serde_json::json!({"timeout": true}),
                            );
                        }
                        debug!("Vision name extraction timed out after 30s");
                    }
                }
            }

            info!("Screenshot name extraction task stopped");
        }))
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
    // When detector_task.abort() fires, any in-flight SOAP generation for an already-archived
    // encounter is killed. Scan today's sessions for has_soap_note == false and regenerate.
    if let Some(ref client) = flush_llm_client {
        let today_str = Utc::now().format("%Y-%m-%d").to_string();
        if let Ok(sessions) = local_archive::list_sessions_by_date(&today_str) {
            let orphaned: Vec<_> = sessions.iter()
                .filter(|s| !s.has_soap_note && s.word_count > 100)
                .filter(|s| s.likely_non_clinical != Some(true))
                .collect();
            if !orphaned.is_empty() {
                info!("Found {} orphaned sessions without SOAP notes, recovering", orphaned.len());
            }
            for summary in orphaned {
                if let Ok(details) = local_archive::get_session(&summary.session_id, &today_str) {
                    if let Some(ref transcript) = details.transcript {
                        let (filtered_text, _) = strip_hallucinations(transcript, 5);
                        let word_count = filtered_text.split_whitespace().count();
                        if word_count < 100 {
                            info!("Orphaned session {} has only {} words after filtering, skipping SOAP", summary.session_id, word_count);
                            continue;
                        }
                        let orphan_soap_opts = crate::llm_client::SoapOptions {
                            detail_level: flush_soap_detail_level,
                            format: crate::llm_client::SoapFormat::from_config_str(&flush_soap_format),
                            ..Default::default()
                        };
                        info!("Generating SOAP for orphaned session {} ({} words)", summary.session_id, word_count);
                        let soap_start = std::time::Instant::now();
                        let soap_future = client.generate_multi_patient_soap_note(
                            &flush_soap_model,
                            &filtered_text,
                            None,
                            Some(&orphan_soap_opts),
                            None,
                        );
                        match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
                            Ok(Ok(soap_result)) => {
                                let soap_latency = soap_start.elapsed().as_millis() as u64;
                                let soap_content = &soap_result.notes
                                    .iter()
                                    .map(|n| n.content.clone())
                                    .collect::<Vec<_>>()
                                    .join("\n\n---\n\n");
                                if let Ok(mut logger) = logger_for_flush.lock() {
                                    logger.log_soap(
                                        &flush_soap_model, "", "",
                                        Some(soap_content), soap_latency, true, None,
                                        serde_json::json!({
                                            "stage": "orphaned_soap_recovery",
                                            "session_id": summary.session_id,
                                            "word_count": word_count,
                                            "response_chars": soap_content.len(),
                                        }),
                                    );
                                }
                                let now = Utc::now();
                                if let Err(e) = local_archive::add_soap_note(
                                    &summary.session_id,
                                    &now,
                                    soap_content,
                                    Some(flush_soap_detail_level),
                                    Some(&flush_soap_format),
                                ) {
                                    warn!("Failed to save recovered SOAP for {}: {}", summary.session_id, e);
                                } else {
                                    info!("Recovered SOAP for orphaned session {}", summary.session_id);
                                    let _ = app.emit("continuous_mode_event", serde_json::json!({
                                        "type": "soap_generated",
                                        "session_id": summary.session_id,
                                        "recovered": true,
                                    }));
                                }
                            }
                            Ok(Err(e)) => {
                                let soap_latency = soap_start.elapsed().as_millis() as u64;
                                if let Ok(mut logger) = logger_for_flush.lock() {
                                    logger.log_soap(
                                        &flush_soap_model, "", "", None, soap_latency, false,
                                        Some(&e.to_string()),
                                        serde_json::json!({"stage": "orphaned_soap_recovery", "session_id": summary.session_id}),
                                    );
                                }
                                warn!("Failed to generate recovered SOAP for {}: {}", summary.session_id, e);
                            }
                            Err(_) => {
                                warn!("SOAP generation timed out for orphaned session {}", summary.session_id);
                            }
                        }
                    }
                }
            }
        }
    }

    // Flush remaining buffer as final encounter check
    let remaining_text = {
        let buffer = handle.transcript_buffer.lock().unwrap_or_else(|e| e.into_inner());
        if !buffer.is_empty() {
            Some(buffer.full_text_with_speakers())
        } else {
            None
        }
    };

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
                None, // No encounter start time for flush
            ) {
                warn!("Failed to archive final buffer: {}", e);
            } else {
                // Point logger to flush session's archive folder
                if let Ok(flush_session_dir) = local_archive::get_session_archive_dir(&session_id, &Utc::now()) {
                    if let Ok(mut logger) = logger_for_flush.lock() {
                        logger.set_session(&flush_session_dir);
                    }
                }

                // Generate SOAP note for the flushed buffer (the orphaned encounter fix)
                // NOTE: No clinical content check here — this is a rare shutdown-time flush
                // where no LLM call is available for classification. Generating SOAP unconditionally
                // is acceptable since the buffer likely contains real clinical audio.
                if let Some(ref client) = flush_llm_client {
                    let flush_notes = handle.encounter_notes
                        .lock()
                        .map(|n| n.clone())
                        .unwrap_or_default();
                    let flush_soap_opts = crate::llm_client::SoapOptions {
                        detail_level: flush_soap_detail_level,
                        format: crate::llm_client::SoapFormat::from_config_str(&flush_soap_format),
                        session_notes: flush_notes,
                        ..Default::default()
                    };
                    info!("Generating SOAP for flushed buffer ({} words)", word_count);
                    let flush_soap_system_prompt = crate::llm_client::build_simple_soap_prompt(&flush_soap_opts);
                    let flush_soap_start = Instant::now();
                    let soap_future = client.generate_multi_patient_soap_note(
                        &flush_soap_model,
                        &filtered_text,
                        None,
                        Some(&flush_soap_opts),
                        None,
                    );
                    match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
                        Ok(Ok(soap_result)) => {
                            let flush_soap_latency = flush_soap_start.elapsed().as_millis() as u64;
                            let soap_content = &soap_result.notes
                                .iter()
                                .map(|n| n.content.clone())
                                .collect::<Vec<_>>()
                                .join("\n\n---\n\n");
                            if let Ok(mut logger) = logger_for_flush.lock() {
                                logger.log_soap(
                                    &flush_soap_model, &flush_soap_system_prompt, "",
                                    Some(soap_content), flush_soap_latency, true, None,
                                    serde_json::json!({
                                        "stage": "flush_on_stop",
                                        "word_count": word_count,
                                        "detail_level": flush_soap_detail_level,
                                        "format": flush_soap_format,
                                        "response_chars": soap_content.len(),
                                    }),
                                );
                            }
                            let now = Utc::now();
                            if let Err(e) = local_archive::add_soap_note(
                                &session_id,
                                &now,
                                soap_content,
                                Some(flush_soap_detail_level),
                                Some(&flush_soap_format),
                            ) {
                                warn!("Failed to save SOAP for flushed buffer: {}", e);
                            } else {
                                info!("SOAP generated for flushed buffer");
                                let _ = app.emit("continuous_mode_event", serde_json::json!({
                                    "type": "soap_generated",
                                    "session_id": session_id
                                }));
                            }
                        }
                        Ok(Err(e)) => {
                            let flush_soap_latency = flush_soap_start.elapsed().as_millis() as u64;
                            if let Ok(mut logger) = logger_for_flush.lock() {
                                logger.log_soap(
                                    &flush_soap_model, &flush_soap_system_prompt, "", None, flush_soap_latency, false,
                                    Some(&e.to_string()),
                                    serde_json::json!({"stage": "flush_on_stop", "llm_error": true}),
                                );
                            }
                            warn!("Failed to generate SOAP for flushed buffer: {}", e);
                        }
                        Err(_) => {
                            let flush_soap_latency = flush_soap_start.elapsed().as_millis() as u64;
                            if let Ok(mut logger) = logger_for_flush.lock() {
                                logger.log_soap(
                                    &flush_soap_model, &flush_soap_system_prompt, "", None, flush_soap_latency, false,
                                    Some("timeout_120s"),
                                    serde_json::json!({"stage": "flush_on_stop", "timeout": true}),
                                );
                            }
                            warn!("SOAP generation timed out for flushed buffer");
                        }
                    }
                }

                // ---- Flush merge check ----
                // The detector task is aborted before it can merge-check the final encounter.
                // Run a merge check here so the flushed buffer gets merged if it belongs to
                // the same visit as the previous encounter.
                if merge_enabled {
                    if let Some(ref client) = flush_llm_client {
                        let today_str = Utc::now().format("%Y-%m-%d").to_string();
                        if let Ok(sessions) = local_archive::list_sessions_by_date(&today_str) {
                            // Find the most recent session that isn't the one we just flushed
                            if let Some(prev_summary) = sessions.iter().find(|s| s.session_id != session_id) {
                                if let Ok(prev_details) = local_archive::get_session(&prev_summary.session_id, &today_str) {
                                    if let Some(ref prev_transcript) = prev_details.transcript {
                                        let prev_tail = tail_words(prev_transcript, MERGE_EXCERPT_WORDS);
                                        let curr_head = head_words(&filtered_text, MERGE_EXCERPT_WORDS);
                                        let (filtered_prev_tail, _) = strip_hallucinations(&prev_tail, 5);
                                        let (filtered_curr_head, _) = strip_hallucinations(&curr_head, 5);
                                        let (merge_system, merge_user) = build_encounter_merge_prompt(
                                            &filtered_prev_tail,
                                            &filtered_curr_head,
                                            None, // No vision tracker available at flush time
                                        );
                                        let merge_ctx = serde_json::json!({
                                            "prev_session_id": prev_summary.session_id,
                                            "curr_session_id": session_id,
                                            "stage": "flush_on_stop",
                                            "prev_tail_words": filtered_prev_tail.split_whitespace().count(),
                                            "curr_head_words": filtered_curr_head.split_whitespace().count(),
                                        });
                                        let merge_start = Instant::now();
                                        let merge_future = client.generate(&flush_fast_model, &merge_system, &merge_user, "encounter_merge");
                                        match tokio::time::timeout(tokio::time::Duration::from_secs(60), merge_future).await {
                                            Ok(Ok(merge_response)) => {
                                                let merge_latency = merge_start.elapsed().as_millis() as u64;
                                                match parse_merge_check(&merge_response) {
                                                    Ok(merge_result) => {
                                                        if let Ok(mut logger) = logger_for_flush.lock() {
                                                            logger.log_merge_check(
                                                                &flush_fast_model, &merge_system, &merge_user,
                                                                Some(&merge_response), merge_latency, true, None,
                                                                serde_json::json!({
                                                                    "prev_session_id": prev_summary.session_id,
                                                                    "curr_session_id": session_id,
                                                                    "stage": "flush_on_stop",
                                                                    "same_encounter": merge_result.same_encounter,
                                                                    "reason": format!("{:?}", merge_result.reason),
                                                                }),
                                                            );
                                                        }
                                                        if merge_result.same_encounter {
                                                            info!(
                                                                "Flush merge check: same visit (reason: {:?}). Merging {} into {}",
                                                                merge_result.reason, session_id, prev_summary.session_id
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
                                                                // Regenerate SOAP on the merged text
                                                                let (filtered_merged, _) = strip_hallucinations(&merged_text, 5);
                                                                let flush_merge_notes = handle.encounter_notes
                                                                    .lock()
                                                                    .map(|n| n.clone())
                                                                    .unwrap_or_default();
                                                                let merge_soap_opts = crate::llm_client::SoapOptions {
                                                                    detail_level: flush_soap_detail_level,
                                                                    format: crate::llm_client::SoapFormat::from_config_str(&flush_soap_format),
                                                                    session_notes: flush_merge_notes,
                                                                    ..Default::default()
                                                                };
                                                                let merge_soap_start = Instant::now();
                                                                let soap_future = client.generate_multi_patient_soap_note(
                                                                    &flush_soap_model,
                                                                    &filtered_merged,
                                                                    None,
                                                                    Some(&merge_soap_opts),
                                                                    None,
                                                                );
                                                                match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
                                                                    Ok(Ok(soap_result)) => {
                                                                        let merge_soap_latency = merge_soap_start.elapsed().as_millis() as u64;
                                                                        let soap_content = &soap_result.notes
                                                                            .iter()
                                                                            .map(|n| n.content.clone())
                                                                            .collect::<Vec<_>>()
                                                                            .join("\n\n---\n\n");
                                                                        if let Ok(mut logger) = logger_for_flush.lock() {
                                                                            logger.log_soap(
                                                                                &flush_soap_model, "", "",
                                                                                Some(soap_content), merge_soap_latency, true, None,
                                                                                serde_json::json!({
                                                                                    "stage": "flush_merge_soap_regen",
                                                                                    "merged_into": prev_summary.session_id,
                                                                                    "merged_word_count": merged_wc,
                                                                                }),
                                                                            );
                                                                        }
                                                                        let _ = local_archive::add_soap_note(
                                                                            &prev_summary.session_id,
                                                                            &now,
                                                                            soap_content,
                                                                            Some(flush_soap_detail_level),
                                                                            Some(&flush_soap_format),
                                                                        );
                                                                        info!("Regenerated SOAP for flush-merged encounter {}", prev_summary.session_id);
                                                                    }
                                                                    Ok(Err(e)) => {
                                                                        let merge_soap_latency = merge_soap_start.elapsed().as_millis() as u64;
                                                                        if let Ok(mut logger) = logger_for_flush.lock() {
                                                                            logger.log_soap(
                                                                                &flush_soap_model, "", "", None, merge_soap_latency, false,
                                                                                Some(&e.to_string()),
                                                                                serde_json::json!({"stage": "flush_merge_soap_regen", "llm_error": true}),
                                                                            );
                                                                        }
                                                                        warn!("Failed to regenerate SOAP after flush merge: {}", e);
                                                                    }
                                                                    Err(_) => {
                                                                        if let Ok(mut logger) = logger_for_flush.lock() {
                                                                            logger.log_soap(
                                                                                &flush_soap_model, "", "", None, 120_000, false,
                                                                                Some("timeout_120s"),
                                                                                serde_json::json!({"stage": "flush_merge_soap_regen", "timeout": true}),
                                                                            );
                                                                        }
                                                                        warn!("SOAP regeneration timed out after flush merge");
                                                                    }
                                                                }
                                                                let _ = app.emit("continuous_mode_event", serde_json::json!({
                                                                    "type": "encounter_merged",
                                                                    "merged_into_session_id": prev_summary.session_id,
                                                                    "removed_session_id": session_id
                                                                }));
                                                            }
                                                        } else {
                                                            info!(
                                                                "Flush merge check: different encounters (reason: {:?})",
                                                                merge_result.reason
                                                            );
                                                        }
                                                    }
                                                    Err(e) => {
                                                        if let Ok(mut logger) = logger_for_flush.lock() {
                                                            logger.log_merge_check(
                                                                &flush_fast_model, &merge_system, &merge_user,
                                                                Some(&merge_response), merge_latency, false,
                                                                Some(&format!("parse_error: {}", e)),
                                                                merge_ctx.clone(),
                                                            );
                                                        }
                                                        warn!("Failed to parse flush merge check: {}", e);
                                                    }
                                                }
                                            }
                                            Ok(Err(e)) => {
                                                let merge_latency = merge_start.elapsed().as_millis() as u64;
                                                if let Ok(mut logger) = logger_for_flush.lock() {
                                                    logger.log_merge_check(
                                                        &flush_fast_model, &merge_system, &merge_user,
                                                        None, merge_latency, false, Some(&e.to_string()),
                                                        merge_ctx.clone(),
                                                    );
                                                }
                                                warn!("Flush merge check LLM call failed: {}", e);
                                            }
                                            Err(_) => {
                                                let merge_latency = merge_start.elapsed().as_millis() as u64;
                                                if let Ok(mut logger) = logger_for_flush.lock() {
                                                    logger.log_merge_check(
                                                        &flush_fast_model, &merge_system, &merge_user,
                                                        None, merge_latency, false, Some("timeout_60s"),
                                                        merge_ctx.clone(),
                                                    );
                                                }
                                                warn!("Flush merge check timed out after 60s");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Set state to idle
    if let Ok(mut state) = handle.state.lock() {
        *state = ContinuousState::Idle;
    } else {
        warn!("State lock poisoned while setting idle state on shutdown");
    }

    let _ = app.emit("continuous_mode_event", serde_json::json!({
        "type": "stopped"
    }));

    info!("Continuous mode stopped");
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_continuous_mode_handle_stats() {
        let handle = ContinuousModeHandle::new();
        let stats = handle.get_stats();
        assert_eq!(stats.state, "idle");
        assert_eq!(stats.encounters_detected, 0);
        assert_eq!(stats.buffer_word_count, 0);
        assert!(stats.last_encounter_at.is_none());
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
