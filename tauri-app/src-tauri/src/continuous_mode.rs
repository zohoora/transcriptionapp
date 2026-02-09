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

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::llm_client::LLMClient;
use crate::local_archive;
use crate::pipeline::{start_pipeline, PipelineConfig, PipelineMessage};

// ============================================================================
// Transcript Buffer
// ============================================================================

/// A timestamped transcript segment in the continuous buffer
#[derive(Debug, Clone)]
pub struct BufferedSegment {
    /// Monotonic sequence number
    pub index: u64,
    /// Wall-clock time of the segment (pipeline audio clock)
    pub timestamp_ms: u64,
    /// Absolute time when segment was received
    pub started_at: DateTime<Utc>,
    /// Transcribed text
    pub text: String,
    /// Speaker ID from diarization
    pub speaker_id: Option<String>,
    /// Pipeline generation that produced this segment (prevents stale data across restarts)
    pub generation: u64,
}

/// Safety cap: discard oldest segments when buffer exceeds this count.
/// ~5000 segments ≈ 8 hours at ~10 segments/minute. Prevents unbounded growth
/// if encounter detection fails or is misconfigured.
const MAX_BUFFER_SEGMENTS: usize = 5000;

/// Thread-safe transcript buffer for continuous mode.
/// Accumulates segments and allows the encounter detector to drain completed encounters.
pub struct TranscriptBuffer {
    segments: Vec<BufferedSegment>,
    next_index: u64,
    /// Current pipeline generation — segments from older generations are discarded on push
    current_generation: u64,
}

impl TranscriptBuffer {
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
            next_index: 0,
            current_generation: 0,
        }
    }

    /// Set the expected pipeline generation. Segments from older generations
    /// that arrive after this call will be discarded.
    pub fn set_generation(&mut self, generation: u64) {
        self.current_generation = generation;
    }

    /// Add a new segment to the buffer, tagged with the given generation.
    /// Segments from stale generations are silently dropped.
    pub fn push(&mut self, text: String, timestamp_ms: u64, speaker_id: Option<String>, generation: u64) {
        if generation < self.current_generation {
            return; // Stale segment from a previous pipeline instance
        }
        let segment = BufferedSegment {
            index: self.next_index,
            timestamp_ms,
            started_at: Utc::now(),
            text,
            speaker_id,
            generation,
        };
        self.next_index += 1;
        self.segments.push(segment);

        // Safety cap: trim oldest segments to prevent unbounded growth
        if self.segments.len() > MAX_BUFFER_SEGMENTS {
            let excess = self.segments.len() - MAX_BUFFER_SEGMENTS;
            warn!(
                "Transcript buffer exceeded {} segments, discarding {} oldest",
                MAX_BUFFER_SEGMENTS, excess
            );
            self.segments.drain(..excess);
        }
    }

    /// Get all text from segments with index > the given index
    pub fn get_text_since(&self, index: u64) -> String {
        self.segments
            .iter()
            .filter(|s| s.index > index)
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Remove and return all segments with index <= through_index
    pub fn drain_through(&mut self, through_index: u64) -> Vec<BufferedSegment> {
        let (drained, remaining): (Vec<_>, Vec<_>) = self
            .segments
            .drain(..)
            .partition(|s| s.index <= through_index);
        self.segments = remaining;
        drained
    }

    /// Get full text of all buffered segments
    pub fn full_text(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Format segments for the encounter detector prompt (numbered)
    pub fn format_for_detection(&self) -> String {
        self.segments
            .iter()
            .map(|s| {
                let speaker = s
                    .speaker_id
                    .as_deref()
                    .unwrap_or("Unknown");
                format!("[{}] ({}): {}", s.index, speaker, s.text)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Total word count in the buffer
    pub fn word_count(&self) -> usize {
        self.segments
            .iter()
            .map(|s| s.text.split_whitespace().count())
            .sum()
    }

    /// First segment index, if any
    pub fn first_index(&self) -> Option<u64> {
        self.segments.first().map(|s| s.index)
    }

    /// Last segment index, if any
    pub fn last_index(&self) -> Option<u64> {
        self.segments.last().map(|s| s.index)
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Get the timestamp of the first segment
    pub fn first_timestamp(&self) -> Option<DateTime<Utc>> {
        self.segments.first().map(|s| s.started_at)
    }
}

// ============================================================================
// Encounter Detection
// ============================================================================

/// Result of encounter detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterDetectionResult {
    pub complete: bool,
    #[serde(default)]
    pub end_segment_index: Option<u64>,
}

/// Build the encounter detection prompt
pub(crate) fn build_encounter_detection_prompt(formatted_segments: &str) -> (String, String) {
    let system = r#"You are analyzing a continuous transcript from a medical office.
The microphone has been recording all day without stopping.

Determine if the text below contains one or more COMPLETE patient encounters.

A complete encounter:
- Begins with a greeting or start of clinical discussion with a patient
- Ends with a farewell, wrap-up ("we'll see you in X weeks"), or a clear shift to a different patient

If a COMPLETE encounter exists, return JSON:
{"complete": true, "end_segment_index": <last segment index number of the encounter>}

If the encounter is still in progress or the text is just ambient noise/hallway chatter, return:
{"complete": false}

IMPORTANT: Only mark an encounter complete if you are confident it has ended.
Do not split in the middle of a conversation.
Return ONLY the JSON object, nothing else."#;

    let user = format!(
        "Transcript (segments numbered with speaker labels):\n{}",
        formatted_segments
    );

    (system.to_string(), user)
}

/// Parse the encounter detection response from the LLM
pub(crate) fn parse_encounter_detection(response: &str) -> Result<EncounterDetectionResult, String> {
    // Try to extract JSON from the response (LLM may include surrounding text)
    let json_str = if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            &response[start..=end]
        } else {
            response
        }
    } else {
        response
    };

    serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse encounter detection response: {} (raw: {})", e, response))
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
    pub last_error: Option<String>,
    pub buffer_word_count: usize,
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
    pub last_error: Arc<Mutex<Option<String>>>,
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
            last_error: Arc::new(Mutex::new(None)),
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

        let buffer_wc = self
            .transcript_buffer
            .lock()
            .map(|b| b.word_count())
            .unwrap_or(0);

        ContinuousModeStats {
            state,
            recording_since: self.recording_since.to_rfc3339(),
            encounters_detected: self.encounters_detected.load(Ordering::Relaxed),
            last_encounter_at: last_at,
            last_encounter_words: last_words,
            last_error: last_err,
            buffer_word_count: buffer_wc,
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

    // Build pipeline config — same as session but with auto_end disabled
    let diarization_model_path = if config.diarization_enabled {
        config.get_diarization_model_path().ok()
    } else {
        None
    };
    let enhancement_model_path = if config.enhancement_enabled {
        config.get_enhancement_model_path().ok()
    } else {
        None
    };
    let yamnet_model_path = if config.biomarkers_enabled {
        config.get_yamnet_model_path().ok()
    } else {
        None
    };

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

    let model_path = config.get_model_path().unwrap_or_default();

    let pipeline_config = PipelineConfig {
        device_id: config.input_device_id.clone(),
        model_path,
        language: config.language.clone(),
        vad_threshold: config.vad_threshold,
        silence_to_flush_ms: config.silence_to_flush_ms,
        max_utterance_ms: config.max_utterance_ms,
        diarization_enabled: config.diarization_enabled,
        diarization_model_path,
        speaker_similarity_threshold: config.speaker_similarity_threshold,
        max_speakers: config.max_speakers,
        enhancement_enabled: config.enhancement_enabled,
        enhancement_model_path,
        biomarkers_enabled: config.biomarkers_enabled,
        yamnet_model_path,
        audio_output_path,
        preprocessing_enabled: config.preprocessing_enabled,
        preprocessing_highpass_hz: config.preprocessing_highpass_hz,
        preprocessing_agc_target_rms: config.preprocessing_agc_target_rms,
        whisper_server_url: config.whisper_server_url.clone(),
        whisper_server_model: config.whisper_server_model.clone(),
        stt_alias: config.stt_alias.clone(),
        stt_postprocess: config.stt_postprocess,
        initial_audio_buffer: None,
        auto_end_enabled: false, // Never auto-end in continuous mode
        auto_end_silence_ms: 0,
    };

    // Create message channel
    let (tx, mut rx) = mpsc::channel::<PipelineMessage>(32);

    // Start the pipeline
    let pipeline_handle = match start_pipeline(pipeline_config, tx) {
        Ok(h) => h,
        Err(e) => {
            error!("Failed to start continuous mode pipeline: {}", e);
            if let Ok(mut state) = handle.state.lock() {
                *state = ContinuousState::Error(e.to_string());
            }
            let _ = app.emit("continuous_mode_event", serde_json::json!({
                "type": "error",
                "error": e.to_string()
            }));
            return Err(e.to_string());
        }
    };

    info!("Continuous mode pipeline started");

    // Pipeline started successfully — now set state and emit event
    if let Ok(mut state) = handle.state.lock() {
        *state = ContinuousState::Recording;
    }
    let _ = app.emit("continuous_mode_event", serde_json::json!({
        "type": "started"
    }));

    // Tag the buffer with this pipeline's generation so stale segments are rejected
    let pipeline_generation: u64 = 1; // Single pipeline per continuous mode run
    if let Ok(mut buffer) = handle.transcript_buffer.lock() {
        buffer.set_generation(pipeline_generation);
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
                    }

                    if let Ok(mut buffer) = buffer_for_consumer.lock() {
                        buffer.push(
                            segment.text.clone(),
                            segment.end_ms,
                            segment.speaker_id.clone(),
                            pipeline_generation,
                        );
                    }

                    // Emit transcript preview for live monitoring view
                    if let Ok(buffer) = buffer_for_consumer.lock() {
                        let text = buffer.full_text();
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
                                silence_trigger_tx.notify_one();
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
            }
        }
    });

    // Spawn encounter detection loop
    let buffer_for_detector = handle.transcript_buffer.clone();
    let stop_for_detector = handle.stop_flag.clone();
    let state_for_detector = handle.state.clone();
    let encounters_for_detector = handle.encounters_detected.clone();
    let last_at_for_detector = handle.last_encounter_at.clone();
    let last_words_for_detector = handle.last_encounter_words.clone();
    let last_error_for_detector = handle.last_error.clone();
    let app_for_detector = app.clone();
    let check_interval = config.encounter_check_interval_secs;

    // Build LLM client for encounter detection
    let llm_client = if !config.llm_router_url.is_empty() {
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

    let soap_model = config.soap_model_fast.clone();
    let fast_model = config.fast_model.clone();
    let soap_detail_level = config.soap_detail_level;
    let soap_format = config.soap_format.clone();

    let detector_task = tokio::spawn(async move {
        let mut encounter_number: u32 = 0;

        loop {
            // Wait for either the check interval or a silence trigger
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval as u64)) => {}
                _ = silence_trigger_rx.notified() => {
                    info!("Silence gap detected — triggering encounter check");
                }
            }

            if stop_for_detector.load(Ordering::Relaxed) {
                break;
            }

            // Check if buffer has enough content to analyze
            let (formatted, word_count, is_empty) = {
                let buffer = match buffer_for_detector.lock() {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                (buffer.format_for_detection(), buffer.word_count(), buffer.is_empty())
            };

            if is_empty || word_count < 20 {
                continue; // Not enough text to analyze
            }

            // Also trigger if buffer is very large (safety valve)
            let force_check = word_count > 5000;
            if force_check {
                info!("Buffer exceeds 5000 words — forcing encounter check");
            }

            // Set state to checking
            if let Ok(mut state) = state_for_detector.lock() {
                *state = ContinuousState::Checking;
            }
            let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                "type": "checking"
            }));

            // Run encounter detection via LLM (with 60s timeout to prevent blocking)
            let detection_result = if let Some(ref client) = llm_client {
                let (system_prompt, user_prompt) = build_encounter_detection_prompt(&formatted);
                let llm_future = client.generate(&fast_model, &system_prompt, &user_prompt, "encounter_detection");
                match tokio::time::timeout(tokio::time::Duration::from_secs(60), llm_future).await {
                    Ok(Ok(response)) => {
                        match parse_encounter_detection(&response) {
                            Ok(result) => Some(result),
                            Err(e) => {
                                warn!("Failed to parse encounter detection: {}", e);
                                if let Ok(mut err) = last_error_for_detector.lock() {
                                    *err = Some(e);
                                }
                                None
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        warn!("Encounter detection LLM call failed: {}", e);
                        if let Ok(mut err) = last_error_for_detector.lock() {
                            *err = Some(e);
                        }
                        let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                            "type": "error",
                            "error": "Encounter detection failed"
                        }));
                        None
                    }
                    Err(_elapsed) => {
                        warn!("Encounter detection LLM call timed out after 60s");
                        if let Ok(mut err) = last_error_for_detector.lock() {
                            *err = Some("Encounter detection timed out".to_string());
                        }
                        None
                    }
                }
            } else {
                warn!("No LLM client configured for encounter detection");
                None
            };

            // Process detection result
            if let Some(result) = detection_result {
                if result.complete {
                    if let Some(end_index) = result.end_segment_index {
                        encounter_number += 1;
                        info!(
                            "Encounter #{} detected (end_segment_index={})",
                            encounter_number, end_index
                        );

                        // Extract encounter segments from buffer
                        let (encounter_text, encounter_word_count, encounter_start) = {
                            let mut buffer = match buffer_for_detector.lock() {
                                Ok(b) => b,
                                Err(_) => continue,
                            };
                            let drained = buffer.drain_through(end_index);
                            let text: String = drained
                                .iter()
                                .map(|s| {
                                    if let Some(ref spk) = s.speaker_id {
                                        format!("{}: {}", spk, s.text)
                                    } else {
                                        s.text.clone()
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            let wc = text.split_whitespace().count();
                            let start = drained.first().map(|s| s.started_at);
                            (text, wc, start)
                        };

                        // Generate session ID for this encounter
                        let session_id = uuid::Uuid::new_v4().to_string();

                        // Archive the encounter transcript
                        let duration_ms = encounter_start
                            .map(|s| (Utc::now() - s).num_milliseconds().max(0) as u64)
                            .unwrap_or(0);

                        if let Err(e) = local_archive::save_session(
                            &session_id,
                            &encounter_text,
                            duration_ms,
                            None, // No per-encounter audio in continuous mode
                            false,
                            None,
                        ) {
                            warn!("Failed to archive encounter: {}", e);
                        }

                        // Update archive metadata with continuous mode info
                        if let Ok(archive_dir) = local_archive::get_archive_dir() {
                            let now = Utc::now();
                            let date_path = archive_dir
                                .join(format!("{:04}", now.year()))
                                .join(format!("{:02}", now.month()))
                                .join(format!("{:02}", now.day()))
                                .join(&session_id)
                                .join("metadata.json");

                            if date_path.exists() {
                                if let Ok(content) = std::fs::read_to_string(&date_path) {
                                    if let Ok(mut metadata) = serde_json::from_str::<local_archive::ArchiveMetadata>(&content) {
                                        metadata.charting_mode = Some("continuous".to_string());
                                        metadata.encounter_number = Some(encounter_number);
                                        if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                                            let _ = std::fs::write(&date_path, json);
                                        }
                                    }
                                }
                            }
                        }

                        // Update stats
                        encounters_for_detector.fetch_add(1, Ordering::Relaxed);
                        if let Ok(mut at) = last_at_for_detector.lock() {
                            *at = Some(Utc::now());
                        }
                        if let Ok(mut words) = last_words_for_detector.lock() {
                            *words = Some(encounter_word_count as u32);
                        }

                        // Emit encounter detected event
                        let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                            "type": "encounter_detected",
                            "session_id": session_id,
                            "word_count": encounter_word_count
                        }));

                        // Generate SOAP note (with 120s timeout — SOAP is heavier than detection)
                        if let Some(ref client) = llm_client {
                            info!("Generating SOAP for encounter #{}", encounter_number);
                            let soap_future = client.generate_multi_patient_soap_note(
                                &soap_model,
                                &encounter_text,
                                None, // No audio events in continuous mode
                                None, // Use default SOAP options
                                None, // No speaker context
                            );
                            match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
                                Ok(Ok(soap_result)) => {
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

                                    let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                                        "type": "soap_generated",
                                        "session_id": session_id
                                    }));
                                    info!("SOAP generated for encounter #{}", encounter_number);
                                }
                                Ok(Err(e)) => {
                                    warn!("Failed to generate SOAP for encounter: {}", e);
                                    if let Ok(mut err) = last_error_for_detector.lock() {
                                        *err = Some(format!("SOAP generation failed: {}", e));
                                    }
                                    let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                                        "type": "soap_failed",
                                        "session_id": session_id,
                                        "error": e
                                    }));
                                }
                                Err(_elapsed) => {
                                    warn!("SOAP generation timed out after 120s for encounter #{}", encounter_number);
                                    if let Ok(mut err) = last_error_for_detector.lock() {
                                        *err = Some("SOAP generation timed out".to_string());
                                    }
                                    let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                                        "type": "soap_failed",
                                        "session_id": session_id,
                                        "error": "SOAP generation timed out"
                                    }));
                                }
                            }
                        }
                    }
                }
            }

            // Return to recording state
            if let Ok(mut state) = state_for_detector.lock() {
                if *state == ContinuousState::Checking {
                    *state = ContinuousState::Recording;
                }
            }
        }
    });

    // Wait for stop signal
    loop {
        if handle.is_stopped() {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Cleanup: stop pipeline
    info!("Stopping continuous mode pipeline");
    pipeline_handle.stop();

    // Wait for tasks to finish
    let _ = consumer_task.await;
    detector_task.abort(); // Force stop the detector loop
    let _ = detector_task.await;

    // Flush remaining buffer as final encounter check
    let remaining_text = {
        let buffer = handle.transcript_buffer.lock().unwrap_or_else(|e| e.into_inner());
        if !buffer.is_empty() {
            Some(buffer.full_text())
        } else {
            None
        }
    };

    if let Some(text) = remaining_text {
        let word_count = text.split_whitespace().count();
        if word_count > 20 {
            info!("Flushing remaining buffer ({} words) as final session", word_count);
            let session_id = uuid::Uuid::new_v4().to_string();
            if let Err(e) = local_archive::save_session(
                &session_id,
                &text,
                0, // Unknown duration for flush
                None,
                false,
                Some("continuous_mode_stopped"),
            ) {
                warn!("Failed to archive final buffer: {}", e);
            }
        }
    }

    // Set state to idle
    if let Ok(mut state) = handle.state.lock() {
        *state = ContinuousState::Idle;
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
    fn test_transcript_buffer_push_and_read() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello doctor".to_string(), 1000, Some("Speaker 1".to_string()), 0);
        buffer.push("How are you?".to_string(), 2000, Some("Speaker 2".to_string()), 0);

        assert_eq!(buffer.word_count(), 5);
        assert_eq!(buffer.first_index(), Some(0));
        assert_eq!(buffer.last_index(), Some(1));
        assert!(!buffer.is_empty());
    }

    #[test]
    fn test_transcript_buffer_full_text() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello".to_string(), 1000, None, 0);
        buffer.push("World".to_string(), 2000, None, 0);

        assert_eq!(buffer.full_text(), "Hello World");
    }

    #[test]
    fn test_transcript_buffer_drain_through() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("A".to_string(), 1000, None, 0);
        buffer.push("B".to_string(), 2000, None, 0);
        buffer.push("C".to_string(), 3000, None, 0);

        let drained = buffer.drain_through(1);
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].text, "A");
        assert_eq!(drained[1].text, "B");

        // Remaining should only have "C"
        assert_eq!(buffer.full_text(), "C");
        assert_eq!(buffer.first_index(), Some(2));
    }

    #[test]
    fn test_transcript_buffer_get_text_since() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("First".to_string(), 1000, None, 0);
        buffer.push("Second".to_string(), 2000, None, 0);
        buffer.push("Third".to_string(), 3000, None, 0);

        let text = buffer.get_text_since(0);
        assert_eq!(text, "Second Third");
    }

    #[test]
    fn test_transcript_buffer_format_for_detection() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello".to_string(), 1000, Some("Dr. Smith".to_string()), 0);
        buffer.push("Hi there".to_string(), 2000, None, 0);

        let formatted = buffer.format_for_detection();
        assert!(formatted.contains("[0] (Dr. Smith): Hello"));
        assert!(formatted.contains("[1] (Unknown): Hi there"));
    }

    #[test]
    fn test_transcript_buffer_stale_generation_rejected() {
        let mut buffer = TranscriptBuffer::new();
        buffer.set_generation(2);
        buffer.push("old".to_string(), 1000, None, 1); // stale
        buffer.push("current".to_string(), 2000, None, 2); // current
        assert_eq!(buffer.word_count(), 1);
        assert_eq!(buffer.full_text(), "current");
    }

    #[test]
    fn test_parse_encounter_detection_complete() {
        let response = r#"{"complete": true, "end_segment_index": 15}"#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(result.complete);
        assert_eq!(result.end_segment_index, Some(15));
    }

    #[test]
    fn test_parse_encounter_detection_incomplete() {
        let response = r#"{"complete": false}"#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(!result.complete);
        assert_eq!(result.end_segment_index, None);
    }

    #[test]
    fn test_parse_encounter_detection_with_surrounding_text() {
        let response = r#"Based on my analysis, here is the result: {"complete": true, "end_segment_index": 42} That's my assessment."#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(result.complete);
        assert_eq!(result.end_segment_index, Some(42));
    }

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
}
