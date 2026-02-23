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
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::encounter_experiment::strip_hallucinations;
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

/// Word count forcing encounter check regardless of buffer age.
const FORCE_CHECK_WORD_THRESHOLD: usize = 5000;
/// Force-split when buffer exceeds this AND consecutive_no_split >= limit.
const FORCE_SPLIT_WORD_THRESHOLD: usize = 8000;
/// Consecutive non-split detection cycles before force-split (at FORCE_SPLIT_WORD_THRESHOLD).
const FORCE_SPLIT_CONSECUTIVE_LIMIT: u32 = 3;
/// Unconditional force-split — hard safety valve, no counter needed.
const ABSOLUTE_WORD_CAP: usize = 15_000;

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

    /// Get full text with speaker labels for display (e.g. "Speaker 1: text\n")
    pub fn full_text_with_speakers(&self) -> String {
        self.segments
            .iter()
            .map(|s| {
                if let Some(ref spk) = s.speaker_id {
                    format!("{}: {}", spk, s.text)
                } else {
                    s.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
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

    /// Format segments for LLM detection, truncated to ~3000 words.
    /// Keeps first ~1500 words (encounter start) + last ~1500 words (encounter end)
    /// with a separator, so the small detection model doesn't get overwhelmed.
    pub fn format_for_detection_truncated(&self) -> String {
        const MAX_WORDS: usize = 3000;
        const HALF: usize = MAX_WORDS / 2;

        let lines: Vec<String> = self.segments
            .iter()
            .map(|s| {
                let speaker = s.speaker_id.as_deref().unwrap_or("Unknown");
                format!("[{}] ({}): {}", s.index, speaker, s.text)
            })
            .collect();

        // Count words per line to find truncation points
        let word_counts: Vec<usize> = lines.iter()
            .map(|l| l.split_whitespace().count())
            .collect();
        let total_words: usize = word_counts.iter().sum();

        if total_words <= MAX_WORDS {
            return lines.join("\n");
        }

        // Find the line index where first HALF words end
        let mut head_words = 0;
        let mut head_end = 0;
        for (i, &wc) in word_counts.iter().enumerate() {
            head_words += wc;
            if head_words >= HALF {
                head_end = i + 1;
                break;
            }
        }

        // Find the line index where last HALF words start
        let mut tail_words = 0;
        let mut tail_start = lines.len();
        for (i, &wc) in word_counts.iter().enumerate().rev() {
            tail_words += wc;
            if tail_words >= HALF {
                tail_start = i;
                break;
            }
        }

        // Ensure no overlap
        if tail_start <= head_end {
            return lines.join("\n");
        }

        let skipped = tail_start - head_end;
        let head = lines[..head_end].join("\n");
        let tail = lines[tail_start..].join("\n");
        format!("{}\n\n[... {} segments omitted for brevity ...]\n\n{}", head, skipped, tail)
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
    /// Confidence score from the LLM (0.0-1.0). Used to gate low-confidence detections.
    #[serde(default)]
    pub confidence: Option<f64>,
}

/// Build the encounter detection prompt
pub fn build_encounter_detection_prompt(formatted_segments: &str) -> (String, String) {
    let system = r#"You MUST respond in English with ONLY a JSON object. No other text, no explanations, no markdown.

You are analyzing a continuous transcript from a medical office where the microphone records all day.

Your task: determine if there is a TRANSITION POINT where one patient encounter ends and another begins, or where a patient encounter has clearly concluded.

Signs of a transition or completed encounter:
- Farewell, wrap-up, or discharge instructions ("we'll see you in X weeks", "take care")
- A greeting or introduction of a DIFFERENT patient after clinical discussion
- A clear shift from one patient's clinical topics to another's
- Extended non-clinical gap (scheduling, staff chat) after substantive clinical content

This is NOT a transition:
- Brief pauses, phone calls, or sidebar conversations DURING an ongoing patient visit
- The very beginning of the first encounter (no prior encounter to split from)
- Short exchanges or greetings with no substantive clinical content yet

If you find a transition point or completed encounter, return:
{"complete": true, "end_segment_index": <last segment index of the CONCLUDED encounter>, "confidence": <0.0-1.0>}

If the current discussion is still one ongoing encounter with no transition, return:
{"complete": false, "confidence": <0.0-1.0>}

Respond with ONLY the JSON object."#;

    let user = format!(
        "Transcript (segments numbered with speaker labels):\n{}",
        formatted_segments
    );

    (system.to_string(), user)
}

/// Strip `<think>...</think>` tags from LLM output (model may emit these even with /nothink).
/// For unclosed `<think>` tags, keeps content after the tag (model may place JSON inside).
fn strip_think_tags(text: &str) -> String {
    let mut result = text.to_string();
    while let Some(start) = result.find("<think>") {
        if let Some(end) = result.find("</think>") {
            let end_pos = end + "</think>".len();
            result = format!("{}{}", &result[..start], &result[end_pos..]);
        } else {
            // Unclosed <think> — keep content after the tag (JSON may be inside)
            let after = result[start + "<think>".len()..].to_string();
            let before = result[..start].to_string();
            // Prefer whichever side contains JSON
            result = if after.contains('{') { after } else { before };
            break;
        }
    }
    // Strip markdown code fences (```json ... ``` or ``` ... ```)
    result = strip_markdown_code_fences(&result);
    result.trim().to_string()
}

/// Strip markdown code fences from text (e.g. ```json\n{...}\n``` → {...})
fn strip_markdown_code_fences(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("```") {
        // Find end of opening fence line
        let after_open = if let Some(newline_pos) = trimmed.find('\n') {
            &trimmed[newline_pos + 1..]
        } else {
            // Single line like ```json { ... }``` — strip opening backticks
            trimmed.trim_start_matches('`').trim_start_matches("json").trim_start()
        };
        // Strip closing fence
        let stripped = if let Some(close_pos) = after_open.rfind("```") {
            &after_open[..close_pos]
        } else {
            after_open
        };
        stripped.trim().to_string()
    } else {
        text.to_string()
    }
}

/// Extract the first balanced JSON object from text using brace counting.
/// Handles cases like `{return {"complete": ...}}` by finding the matched `{...}`.
fn extract_first_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in text[start..].char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start..=start + i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse the encounter detection response from the LLM
pub fn parse_encounter_detection(response: &str) -> Result<EncounterDetectionResult, String> {
    let cleaned = strip_think_tags(response);

    // Try outermost braces first
    if let Some(json_str) = extract_first_json_object(&cleaned) {
        if let Ok(result) = serde_json::from_str::<EncounterDetectionResult>(&json_str) {
            return Ok(result);
        }
    }

    // Fallback: model may wrap JSON in {return {...}} — find inner {"complete" object
    if let Some(inner_start) = cleaned.find("{\"complete\"") {
        if let Some(json_str) = extract_first_json_object(&cleaned[inner_start..]) {
            if let Ok(result) = serde_json::from_str::<EncounterDetectionResult>(&json_str) {
                return Ok(result);
            }
        }
    }

    Err(format!("Failed to parse encounter detection response: (raw: {})", response))
}

// ============================================================================
// Retrospective Encounter Merge
// ============================================================================

/// Result of encounter merge check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeCheckResult {
    pub same_encounter: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Build the encounter merge prompt — asks if two excerpts are from the same patient visit.
///
/// When `patient_name` is provided (e.g. from vision-based extraction), the prompt
/// includes it as context, significantly improving merge accuracy on topic-shift cases
/// (33% → 100% in experiments — see encounter-experiments/summary.md).
pub fn build_encounter_merge_prompt(prev_tail: &str, curr_head: &str, patient_name: Option<&str>) -> (String, String) {
    let patient_context = match patient_name {
        Some(name) if !name.is_empty() => format!(
            "\n\nCONTEXT: The patient being seen is {}. If both excerpts reference this patient or the same clinical context, they are almost certainly the same encounter.",
            name
        ),
        _ => String::new(),
    };

    let system = format!(
        r#"You are reviewing two consecutive transcript excerpts from a medical office where a microphone records all day.

The system split these into two separate encounters, but they may actually be the SAME patient visit that was incorrectly split (e.g., due to a pause, phone call, or silence during an examination).

Determine if both excerpts are from the SAME patient encounter or DIFFERENT encounters.

Signs they are the SAME encounter:
- Same patient name or context referenced
- Continuation of the same clinical discussion
- No farewell/greeting between them
- Natural pause (examination, looking at charts) rather than patient change
- Same medical condition being discussed from different angles

Signs they are DIFFERENT encounters:
- Different patient names or contexts
- A farewell followed by a new greeting
- Clearly different clinical topics with no continuity{}

Return JSON:
{{"same_encounter": true, "reason": "brief explanation"}}
or
{{"same_encounter": false, "reason": "brief explanation"}}

Return ONLY the JSON object, nothing else."#,
        patient_context
    );

    let user = format!(
        "EXCERPT FROM END OF PREVIOUS ENCOUNTER:\n{}\n\n---\n\nEXCERPT FROM START OF NEXT ENCOUNTER:\n{}",
        prev_tail, curr_head
    );

    (system, user)
}

/// Parse the merge check response from the LLM
pub fn parse_merge_check(response: &str) -> Result<MergeCheckResult, String> {
    let cleaned = strip_think_tags(response);

    // Try outermost braces first
    if let Some(json_str) = extract_first_json_object(&cleaned) {
        if let Ok(result) = serde_json::from_str::<MergeCheckResult>(&json_str) {
            return Ok(result);
        }
    }

    // Fallback: look for {"same_encounter" inside wrapper
    if let Some(inner_start) = cleaned.find("{\"same_encounter\"") {
        if let Some(json_str) = extract_first_json_object(&cleaned[inner_start..]) {
            if let Ok(result) = serde_json::from_str::<MergeCheckResult>(&json_str) {
                return Ok(result);
            }
        }
    }

    Err(format!("Failed to parse merge check response: (raw: {})", response))
}

// ============================================================================
// Patient Name Extraction (Vision-Based)
// ============================================================================

/// Tracks patient name votes from periodic screenshot analysis.
/// Multiple screenshots are analyzed per encounter; majority vote determines
/// the most likely patient name for labeling.
pub struct PatientNameTracker {
    /// Name → count of screenshots where this name was extracted
    votes: HashMap<String, u32>,
}

impl PatientNameTracker {
    pub fn new() -> Self {
        Self {
            votes: HashMap::new(),
        }
    }

    /// Record a vote for a patient name (normalized: trimmed, title-cased)
    pub fn record(&mut self, name: &str) {
        let normalized = normalize_patient_name(name);
        if !normalized.is_empty() {
            *self.votes.entry(normalized).or_insert(0) += 1;
        }
    }

    /// Returns the name with the most votes, or None if no votes recorded
    pub fn majority_name(&self) -> Option<String> {
        self.votes
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(name, _)| name.clone())
    }

    /// Clear all votes for a new encounter period
    pub fn reset(&mut self) {
        self.votes.clear();
    }
}

/// Normalize a patient name: trim whitespace, collapse multiple spaces, title-case
fn normalize_patient_name(name: &str) -> String {
    let trimmed: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    trimmed
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build the vision prompt for patient name extraction.
/// Returns (system_prompt, user_prompt_text).
pub(crate) fn build_patient_name_prompt() -> (String, String) {
    let system = "You are analyzing a screenshot of a computer screen in a clinical setting. \
        If a patient's chart or medical record is clearly visible, extract the patient's full name. \
        If no patient name is clearly visible, respond with NOT_FOUND.";

    let user = "Extract the patient name if one is clearly visible on screen. \
        Respond with ONLY the patient name or NOT_FOUND. No explanation.";

    (system.to_string(), user.to_string())
}

/// Parse the vision model's response for a patient name.
/// Returns Some(name) if a name was extracted, None if NOT_FOUND or empty.
pub(crate) fn parse_patient_name(response: &str) -> Option<String> {
    let trimmed = response.trim();
    if trimmed.is_empty() || trimmed.contains("NOT_FOUND") {
        return None;
    }
    let normalized = normalize_patient_name(trimmed);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
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
    /// Shadow mode: accumulated shadow decisions for the current encounter
    pub shadow_decisions: Arc<Mutex<Vec<crate::shadow_log::ShadowDecisionSummary>>>,
    /// Shadow mode: most recent shadow decision (for dashboard display)
    pub last_shadow_decision: Arc<Mutex<Option<crate::shadow_log::ShadowDecision>>>,
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
            shadow_decisions: Arc::new(Mutex::new(Vec::new())),
            last_shadow_decision: Arc::new(Mutex::new(None)),
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
            }
        }
    });

    // Start presence sensor if in sensor or shadow detection mode
    let is_shadow_mode = config.encounter_detection_mode == "shadow";
    let shadow_active_method = config.shadow_active_method.clone();
    let use_sensor_mode = (config.encounter_detection_mode == "sensor"
        || (is_shadow_mode && !config.presence_sensor_port.is_empty()))
        && !config.presence_sensor_port.is_empty();
    let mut sensor_handle: Option<crate::presence_sensor::PresenceSensor> = None;
    let sensor_absence_trigger: Arc<tokio::sync::Notify>;

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
    let effective_sensor_mode = if is_shadow_mode {
        shadow_active_method == "sensor" && sensor_handle.is_some()
    } else {
        sensor_handle.is_some()
    };

    // Spawn sensor status monitoring task (emits events on state/status changes)
    let sensor_monitor_task: Option<tokio::task::JoinHandle<()>> = if effective_sensor_mode {
        let mut state_rx = sensor_handle.as_ref().unwrap().subscribe_state();
        let mut status_rx = sensor_handle.as_ref().unwrap().subscribe_status();
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
        let shadow_method = if shadow_active_method == "sensor" { "llm" } else { "sensor" };
        let active_method = shadow_active_method.clone();
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
            // Active=LLM, Shadow=sensor — observe sensor absence triggers
            let sensor_trigger_for_shadow = sensor_absence_trigger.clone();

            Some(tokio::spawn(async move {
                info!("Shadow sensor observer started");
                loop {
                    if stop_for_shadow.load(Ordering::Relaxed) {
                        break;
                    }

                    // Wait for sensor absence trigger
                    sensor_trigger_for_shadow.notified().await;

                    if stop_for_shadow.load(Ordering::Relaxed) {
                        break;
                    }

                    // Read buffer state (non-destructive)
                    let (word_count, last_segment) = buffer_for_shadow
                        .lock()
                        .map(|b| (b.word_count(), b.last_index()))
                        .unwrap_or((0, None));

                    let decision = crate::shadow_log::ShadowDecision {
                        timestamp: Utc::now(),
                        shadow_method: "sensor".to_string(),
                        active_method: active_method.clone(),
                        outcome: crate::shadow_log::ShadowOutcome::WouldSplit,
                        confidence: Some(1.0),
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
                        "outcome": "would_split",
                        "buffer_words": word_count
                    }));

                    info!("Shadow sensor: would_split (buffer {} words)", word_count);
                }
                info!("Shadow sensor observer stopped");
            }))
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

                    // Read buffer state (non-destructive, truncated for small model)
                    let (formatted, word_count, last_segment) = buffer_for_shadow
                        .lock()
                        .map(|b| (b.format_for_detection_truncated(), b.word_count(), b.last_index()))
                        .unwrap_or_else(|_| (String::new(), 0, None));

                    if word_count < 100 {
                        continue; // Not enough text to analyze
                    }

                    // Call LLM for encounter detection
                    let outcome;
                    let confidence;
                    if let Some(ref client) = shadow_llm_client {
                        let (filtered, _) = crate::encounter_experiment::strip_hallucinations(&formatted, 5);
                        let (system_prompt, user_prompt) = build_encounter_detection_prompt(&filtered);
                        let system_prompt = if shadow_detection_nothink {
                            format!("/nothink\n{}", system_prompt)
                        } else {
                            system_prompt
                        };
                        let llm_future = client.generate(
                            &shadow_detection_model, &system_prompt, &user_prompt, "shadow_encounter_detection"
                        );
                        match tokio::time::timeout(tokio::time::Duration::from_secs(60), llm_future).await {
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
                                debug!("Shadow LLM: detection timed out after 60s");
                                continue;
                            }
                        }
                    } else {
                        continue; // No LLM client
                    }

                    let decision = crate::shadow_log::ShadowDecision {
                        timestamp: Utc::now(),
                        shadow_method: "llm".to_string(),
                        active_method: active_method.clone(),
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

    // Clone config values for flush-on-stop SOAP generation (outside detector task)
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

    // Clone biomarker reset flag for the detector task
    let reset_bio_flag = reset_bio_for_detector;

    // Clone sensor trigger for detector task
    let sensor_trigger_for_detector = sensor_absence_trigger.clone();

    // Clone shadow state for detector task
    let handle_shadow_decisions = handle.shadow_decisions.clone();
    let is_shadow_mode = is_shadow_mode;
    let shadow_active_method = shadow_active_method.clone();

    let detector_task = tokio::spawn(async move {
        let mut encounter_number: u32 = 0;
        let mut consecutive_no_split: u32 = 0;

        // Track previous encounter for retrospective merge checks
        let mut prev_encounter_session_id: Option<String> = None;
        let mut prev_encounter_text: Option<String> = None;
        let mut prev_encounter_date: Option<DateTime<Utc>> = None;

        loop {
            // Wait for trigger based on detection mode
            let (manual_triggered, sensor_triggered) = if effective_sensor_mode {
                // Sensor mode: wait for sensor absence threshold OR manual trigger
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
                // LLM mode: wait for timer, silence, or manual trigger (unchanged)
                let manual = tokio::select! {
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(check_interval as u64)) => false,
                    _ = silence_trigger_rx.notified() => {
                        info!("Silence gap detected — triggering encounter check");
                        false
                    }
                    _ = manual_trigger_rx.notified() => {
                        info!("Manual new patient trigger received");
                        true
                    }
                };
                (manual, false)
            };
            let manual_triggered = manual_triggered;

            if stop_for_detector.load(Ordering::Relaxed) {
                break;
            }

            // Check if buffer has enough content to analyze
            let (formatted, word_count, is_empty, first_ts) = {
                let buffer = match buffer_for_detector.lock() {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                (buffer.format_for_detection_truncated(), buffer.word_count(), buffer.is_empty(), buffer.first_timestamp())
            };

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

                // Also trigger if buffer is very large (safety valve)
                let force_check = word_count > FORCE_CHECK_WORD_THRESHOLD;

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
                    info!("Buffer exceeds {} words ({}) — forcing encounter check", FORCE_CHECK_WORD_THRESHOLD, word_count);
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
            // Manual/sensor trigger: skip LLM — directly split the encounter
            let detection_result = if manual_triggered || sensor_triggered {
                let last_idx = buffer_for_detector.lock().ok().and_then(|b| b.last_index());
                let source = if sensor_triggered { "Sensor" } else { "Manual" };
                info!("{} trigger: forcing encounter split (last_index={:?})", source, last_idx);
                Some(EncounterDetectionResult {
                    complete: true,
                    end_segment_index: last_idx,
                    confidence: Some(1.0),
                })
            } else if let Some(ref client) = llm_client {
                // Strip hallucinated repetitions (e.g. STT "fractured" loop) before LLM call
                let (filtered_formatted, _) = strip_hallucinations(&formatted, 5);
                let (system_prompt, user_prompt) = build_encounter_detection_prompt(&filtered_formatted);
                // Prepend /nothink for Qwen3 models to disable thinking mode (improves detection accuracy)
                let system_prompt = if detection_nothink {
                    format!("/nothink\n{}", system_prompt)
                } else {
                    system_prompt
                };
                let llm_future = client.generate(&detection_model, &system_prompt, &user_prompt, "encounter_detection");
                match tokio::time::timeout(tokio::time::Duration::from_secs(60), llm_future).await {
                    Ok(Ok(response)) => {
                        match parse_encounter_detection(&response) {
                            Ok(result) => {
                                info!(
                                    "Detection result: complete={}, confidence={:?}, end_segment_index={:?}, word_count={}",
                                    result.complete, result.confidence, result.end_segment_index, word_count
                                );
                                Some(result)
                            }
                            Err(e) => {
                                warn!("Failed to parse encounter detection: {}", e);
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
                        warn!("Encounter detection LLM call failed: {}", e);
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
                        warn!("Encounter detection LLM call timed out after 60s");
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

            // Absolute word cap: unconditional force-split at 15K words
            if word_count > ABSOLUTE_WORD_CAP && !manual_triggered && !sensor_triggered {
                warn!("ABSOLUTE WORD CAP: force-splitting at {} words", word_count);
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
                        "Detection non-split: result={}, consecutive_no_split={}, word_count={}",
                        if detection_result.is_none() { "error/timeout" } else { "complete=false" },
                        consecutive_no_split, word_count
                    );
                    // Graduated force-split
                    if word_count > FORCE_SPLIT_WORD_THRESHOLD
                        && consecutive_no_split >= FORCE_SPLIT_CONSECUTIVE_LIMIT
                    {
                        warn!(
                            "Force-splitting: {} consecutive non-splits with {} words",
                            consecutive_no_split, word_count
                        );
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

            // Process detection result
            if let Some(result) = detection_result {
                if result.complete {
                    // Confidence gate: require >= 0.7 to proceed (skip for forced splits)
                    let confidence = result.confidence.unwrap_or(0.0);
                    if confidence < 0.7 && !force_split {
                        consecutive_no_split += 1;
                        info!(
                            "Confidence gate rejected: confidence={:.2}, word_count={}, consecutive_no_split={}",
                            confidence, word_count, consecutive_no_split
                        );
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
                                        // Record how this encounter was detected
                                        metadata.detection_method = Some(
                                            if manual_triggered { "manual" }
                                            else if sensor_triggered { "sensor" }
                                            else { "llm" }.to_string()
                                        );
                                        // Add patient name from vision extraction (majority vote)
                                        if let Ok(tracker) = name_tracker_for_detector.lock() {
                                            metadata.patient_name = tracker.majority_name();
                                        } else {
                                            warn!("Name tracker lock poisoned, patient name not written to metadata");
                                        }
                                        // Add shadow comparison data if in shadow mode
                                        if is_shadow_mode {
                                            let shadow_method = if shadow_active_method == "sensor" { "llm" } else { "sensor" };
                                            let decisions: Vec<crate::shadow_log::ShadowDecisionSummary> = handle_shadow_decisions
                                                .lock()
                                                .map(|d| d.clone())
                                                .unwrap_or_default();

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

                        // Read encounter notes BEFORE clearing (SOAP generation needs them)
                        let notes_text = encounter_notes_for_detector
                            .lock()
                            .map(|n| n.clone())
                            .unwrap_or_default();

                        // Clear encounter notes for next encounter
                        if let Ok(mut notes) = encounter_notes_for_detector.lock() {
                            notes.clear();
                        } else {
                            warn!("Encounter notes lock poisoned, notes not cleared for next encounter");
                        }

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

                        // Generate SOAP note (with 120s timeout — SOAP is heavier than detection)
                        if let Some(ref client) = llm_client {
                            // Strip hallucinated repetitions before SOAP generation
                            let (filtered_encounter_text, _) = strip_hallucinations(&encounter_text, 5);
                            // Build SOAP options with encounter notes from clinician (uses pre-cloned notes_text)
                            let soap_opts = crate::llm_client::SoapOptions {
                                detail_level: soap_detail_level,
                                format: if soap_format == "comprehensive" { crate::llm_client::SoapFormat::Comprehensive } else { crate::llm_client::SoapFormat::ProblemBased },
                                session_notes: notes_text,
                                ..Default::default()
                            };
                            info!("Generating SOAP for encounter #{}", encounter_number);
                            let soap_future = client.generate_multi_patient_soap_note(
                                &soap_model,
                                &filtered_encounter_text,
                                None, // No audio events in continuous mode
                                Some(&soap_opts),
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
                                    warn!("SOAP generation timed out after 120s for encounter #{}", encounter_number);
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
                        // After archiving + SOAP for encounter N, check if it should merge with N-1
                        if merge_enabled && encounter_number > 1 {
                            if let (Some(ref prev_id), Some(ref prev_text), Some(ref prev_date)) =
                                (&prev_encounter_session_id, &prev_encounter_text, &prev_encounter_date)
                            {
                                // Extract ~500 words from tail of prev + ~500 words from head of current
                                let prev_words: Vec<&str> = prev_text.split_whitespace().collect();
                                let prev_tail: String = if prev_words.len() > 500 {
                                    prev_words[prev_words.len() - 500..].join(" ")
                                } else {
                                    prev_text.clone()
                                };

                                let curr_words: Vec<&str> = encounter_text.split_whitespace().collect();
                                let curr_head: String = if curr_words.len() > 500 {
                                    curr_words[..500].join(" ")
                                } else {
                                    encounter_text.clone()
                                };

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
                                    let merge_future = client.generate(&fast_model, &merge_system, &merge_user, "encounter_merge");
                                    match tokio::time::timeout(tokio::time::Duration::from_secs(60), merge_future).await {
                                        Ok(Ok(merge_response)) => {
                                            match parse_merge_check(&merge_response) {
                                                Ok(merge_result) => {
                                                    if merge_result.same_encounter {
                                                        info!(
                                                            "Merge check: encounters are the same visit (reason: {:?}). Merging {} into {}",
                                                            merge_result.reason, session_id, prev_id
                                                        );

                                                        // Build merged transcript
                                                        let merged_text = format!("{}\n{}", prev_text, encounter_text);
                                                        let merged_wc = merged_text.split_whitespace().count();
                                                        let merged_duration = duration_ms; // Approximate

                                                        if let Err(e) = local_archive::merge_encounters(
                                                            prev_id,
                                                            &session_id,
                                                            prev_date,
                                                            &merged_text,
                                                            merged_wc,
                                                            merged_duration,
                                                        ) {
                                                            warn!("Failed to merge encounters: {}", e);
                                                        } else {
                                                            // Regenerate SOAP for the merged encounter
                                                            if let Some(ref client) = llm_client {
                                                                let (filtered_merged, _) = strip_hallucinations(&merged_text, 5);
                                                                let merge_notes = encounter_notes_for_detector
                                                                    .lock()
                                                                    .map(|n| n.clone())
                                                                    .unwrap_or_default();
                                                                let merge_soap_opts = crate::llm_client::SoapOptions {
                                                                    detail_level: soap_detail_level,
                                                                    format: if soap_format == "comprehensive" { crate::llm_client::SoapFormat::Comprehensive } else { crate::llm_client::SoapFormat::ProblemBased },
                                                                    session_notes: merge_notes,
                                                                    ..Default::default()
                                                                };
                                                                let soap_future = client.generate_multi_patient_soap_note(
                                                                    &soap_model,
                                                                    &filtered_merged,
                                                                    None,
                                                                    Some(&merge_soap_opts),
                                                                    None,
                                                                );
                                                                match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
                                                                    Ok(Ok(soap_result)) => {
                                                                        let soap_content = &soap_result.notes
                                                                            .iter()
                                                                            .map(|n| n.content.clone())
                                                                            .collect::<Vec<_>>()
                                                                            .join("\n\n---\n\n");
                                                                        let _ = local_archive::add_soap_note(
                                                                            prev_id,
                                                                            prev_date,
                                                                            soap_content,
                                                                            Some(soap_detail_level),
                                                                            Some(&soap_format),
                                                                        );
                                                                        info!("Regenerated SOAP for merged encounter {}", prev_id);
                                                                    }
                                                                    Ok(Err(e)) => warn!("Failed to regenerate SOAP after merge: {}", e),
                                                                    Err(_) => warn!("SOAP regeneration timed out after merge"),
                                                                }
                                                            }

                                                            encounter_number -= 1;

                                                            let _ = app_for_detector.emit("continuous_mode_event", serde_json::json!({
                                                                "type": "encounter_merged",
                                                                "merged_into_session_id": prev_id,
                                                                "removed_session_id": session_id
                                                            }));

                                                            // Update prev tracking to the merged encounter
                                                            prev_encounter_text = Some(merged_text);
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
                                                Err(e) => warn!("Failed to parse merge check: {}", e),
                                            }
                                        }
                                        Ok(Err(e)) => warn!("Merge check LLM call failed: {}", e),
                                        Err(_) => warn!("Merge check timed out after 60s"),
                                    }
                                }
                            }
                        }

                        // Update prev encounter tracking for next iteration
                        prev_encounter_session_id = Some(session_id.clone());
                        prev_encounter_text = Some(encounter_text);
                        prev_encounter_date = Some(Utc::now());
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

                let image_base64 = match capture_result {
                    Ok(Ok(b64)) => b64,
                    Ok(Err(e)) => {
                        debug!("Screenshot capture failed (may not have permission): {}", e);
                        continue;
                    }
                    Err(e) => {
                        debug!("Screenshot capture task panicked: {}", e);
                        continue;
                    }
                };

                // Save screenshot to disk for debugging
                {
                    use base64::Engine;
                    let debug_dir = dirs::home_dir()
                        .unwrap_or_default()
                        .join(".transcriptionapp")
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

                // Send to vision model for name extraction
                let client = match &llm_client_for_screenshot {
                    Some(c) => c,
                    None => {
                        debug!("No LLM client for screenshot name extraction");
                        continue;
                    }
                };

                let (system_prompt, user_text) = build_patient_name_prompt();
                let content_parts = vec![
                    crate::llm_client::ContentPart::Text { text: user_text },
                    crate::llm_client::ContentPart::ImageUrl {
                        image_url: crate::llm_client::ImageUrlContent {
                            url: format!("data:image/jpeg;base64,{}", image_base64),
                        },
                    },
                ];

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
                        if let Some(name) = parse_patient_name(&response) {
                            info!("Vision extracted patient name: {}", name);
                            if let Ok(mut tracker) = name_tracker_for_screenshot.lock() {
                                tracker.record(&name);
                            } else {
                                warn!("Name tracker lock poisoned, patient name vote dropped: {}", name);
                            }
                        } else {
                            debug!("Vision did not find a patient name on screen");
                        }
                    }
                    Ok(Err(e)) => {
                        debug!("Vision name extraction failed: {}", e);
                    }
                    Err(_) => {
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
            ) {
                warn!("Failed to archive final buffer: {}", e);
            } else {
                // Generate SOAP note for the flushed buffer (the orphaned encounter fix)
                if let Some(ref client) = flush_llm_client {
                    let flush_notes = handle.encounter_notes
                        .lock()
                        .map(|n| n.clone())
                        .unwrap_or_default();
                    let flush_soap_opts = crate::llm_client::SoapOptions {
                        detail_level: flush_soap_detail_level,
                        format: if flush_soap_format == "comprehensive" { crate::llm_client::SoapFormat::Comprehensive } else { crate::llm_client::SoapFormat::ProblemBased },
                        session_notes: flush_notes,
                        ..Default::default()
                    };
                    info!("Generating SOAP for flushed buffer ({} words)", word_count);
                    let soap_future = client.generate_multi_patient_soap_note(
                        &flush_soap_model,
                        &filtered_text,
                        None,
                        Some(&flush_soap_opts),
                        None,
                    );
                    match tokio::time::timeout(tokio::time::Duration::from_secs(120), soap_future).await {
                        Ok(Ok(soap_result)) => {
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
                            warn!("Failed to generate SOAP for flushed buffer: {}", e);
                        }
                        Err(_) => {
                            warn!("SOAP generation timed out for flushed buffer");
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
    fn test_transcript_buffer_full_text_with_speakers() {
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello doctor".to_string(), 1000, Some("Speaker 1".to_string()), 0);
        buffer.push("How are you?".to_string(), 2000, Some("Speaker 2".to_string()), 0);
        buffer.push("ambient noise".to_string(), 3000, None, 0);

        let text = buffer.full_text_with_speakers();
        assert_eq!(text, "Speaker 1: Hello doctor\nSpeaker 2: How are you?\nambient noise");
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
        let response = r#"{"complete": true, "end_segment_index": 15, "confidence": 0.95}"#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(result.complete);
        assert_eq!(result.end_segment_index, Some(15));
        assert!((result.confidence.unwrap() - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_parse_encounter_detection_complete_without_confidence() {
        // Backwards-compatible: confidence is optional
        let response = r#"{"complete": true, "end_segment_index": 15}"#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(result.complete);
        assert_eq!(result.end_segment_index, Some(15));
        assert!(result.confidence.is_none());
    }

    #[test]
    fn test_parse_encounter_detection_incomplete() {
        let response = r#"{"complete": false, "confidence": 0.1}"#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(!result.complete);
        assert_eq!(result.end_segment_index, None);
        assert!((result.confidence.unwrap() - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_parse_encounter_detection_with_surrounding_text() {
        let response = r#"Based on my analysis, here is the result: {"complete": true, "end_segment_index": 42, "confidence": 0.85} That's my assessment."#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(result.complete);
        assert_eq!(result.end_segment_index, Some(42));
        assert!((result.confidence.unwrap() - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_parse_encounter_detection_with_think_tags() {
        let response = r#"<think> </think> {"complete": false, "confidence": 0.0}"#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(!result.complete);
    }

    #[test]
    fn test_parse_encounter_detection_with_unclosed_think_and_code_fence() {
        // Exact production failure: unclosed <think> with JSON inside markdown code fence
        let response = "<think> ```json { \"complete\": false, \"confidence\": 0.0 } ```";
        let result = parse_encounter_detection(response).unwrap();
        assert!(!result.complete);
    }

    #[test]
    fn test_parse_encounter_detection_with_unclosed_think_complete() {
        let response = "<think> ```json\n{ \"complete\": true, \"end_segment_index\": 42, \"confidence\": 0.95 }\n```";
        let result = parse_encounter_detection(response).unwrap();
        assert!(result.complete);
        assert_eq!(result.end_segment_index, Some(42));
    }

    #[test]
    fn test_parse_encounter_detection_with_code_fence_no_think() {
        let response = "```json\n{\"complete\": false, \"confidence\": 0.0}\n```";
        let result = parse_encounter_detection(response).unwrap();
        assert!(!result.complete);
    }

    #[test]
    fn test_parse_encounter_detection_with_return_wrapper() {
        // Model wraps JSON in {return {...}} — the actual error from production
        let response = r#"<think> </think> {return {"complete": false, "confidence": 0.0}}"#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(!result.complete);
    }

    #[test]
    fn test_parse_merge_check_with_think_tags() {
        let response = r#"<think>reasoning here</think> {"same_encounter": true, "reason": "same patient"}"#;
        let result = parse_merge_check(response).unwrap();
        assert!(result.same_encounter);
    }

    #[test]
    fn test_parse_encounter_detection_low_confidence() {
        let response = r#"{"complete": true, "end_segment_index": 10, "confidence": 0.3}"#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(result.complete);
        // Confidence is below 0.7 threshold — caller should skip this detection
        assert!(result.confidence.unwrap() < 0.7);
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

    // ---- Merge prompt/parse tests ----

    #[test]
    fn test_build_encounter_merge_prompt() {
        let (system, user) = build_encounter_merge_prompt(
            "...we'll see you in two weeks",
            "Good morning Mr. Smith...",
            None,
        );
        assert!(system.contains("SAME patient visit"));
        assert!(user.contains("we'll see you in two weeks"));
        assert!(user.contains("Good morning Mr. Smith"));
    }

    #[test]
    fn test_build_encounter_merge_prompt_with_patient_name() {
        let (system, _user) = build_encounter_merge_prompt(
            "tail text",
            "head text",
            Some("Buckland, Deborah Ann"),
        );
        assert!(system.contains("Buckland, Deborah Ann"));
        assert!(system.contains("almost certainly the same encounter"));
    }

    #[test]
    fn test_parse_merge_check_same() {
        let response = r#"{"same_encounter": true, "reason": "Same patient, brief pause for examination"}"#;
        let result = parse_merge_check(response).unwrap();
        assert!(result.same_encounter);
        assert!(result.reason.unwrap().contains("pause"));
    }

    #[test]
    fn test_parse_merge_check_different() {
        let response = r#"{"same_encounter": false, "reason": "Different patients — farewell followed by new greeting"}"#;
        let result = parse_merge_check(response).unwrap();
        assert!(!result.same_encounter);
        assert!(result.reason.is_some());
    }

    #[test]
    fn test_parse_merge_check_with_surrounding_text() {
        let response = r#"Here is my analysis: {"same_encounter": true, "reason": "continuation"} Done."#;
        let result = parse_merge_check(response).unwrap();
        assert!(result.same_encounter);
    }

    // ---- Patient Name Tracker tests ----

    #[test]
    fn test_patient_name_tracker_majority() {
        let mut tracker = PatientNameTracker::new();
        tracker.record("John Smith");
        tracker.record("John Smith");
        tracker.record("John Smith");
        tracker.record("Jane Doe");
        assert_eq!(tracker.majority_name(), Some("John Smith".to_string()));
    }

    #[test]
    fn test_patient_name_tracker_empty() {
        let tracker = PatientNameTracker::new();
        assert_eq!(tracker.majority_name(), None);
    }

    #[test]
    fn test_patient_name_tracker_reset() {
        let mut tracker = PatientNameTracker::new();
        tracker.record("John Smith");
        tracker.record("John Smith");
        assert!(tracker.majority_name().is_some());
        tracker.reset();
        assert_eq!(tracker.majority_name(), None);
    }

    #[test]
    fn test_patient_name_tracker_normalization() {
        let mut tracker = PatientNameTracker::new();
        tracker.record("  john   SMITH  ");
        assert_eq!(tracker.majority_name(), Some("John Smith".to_string()));
    }

    // ---- Vision prompt/parse tests ----

    #[test]
    fn test_parse_patient_name_found() {
        assert_eq!(
            parse_patient_name("John Smith"),
            Some("John Smith".to_string())
        );
    }

    #[test]
    fn test_parse_patient_name_not_found() {
        assert_eq!(parse_patient_name("NOT_FOUND"), None);
    }

    #[test]
    fn test_parse_patient_name_empty() {
        assert_eq!(parse_patient_name(""), None);
        assert_eq!(parse_patient_name("   "), None);
    }

    #[test]
    fn test_parse_patient_name_whitespace() {
        assert_eq!(
            parse_patient_name("  John Smith  "),
            Some("John Smith".to_string())
        );
    }

    #[test]
    fn test_parse_patient_name_not_found_in_sentence() {
        // If the response contains NOT_FOUND anywhere, treat as not found
        assert_eq!(parse_patient_name("The result is NOT_FOUND here"), None);
    }

    #[test]
    fn test_build_patient_name_prompt() {
        let (system, user) = build_patient_name_prompt();
        assert!(!system.is_empty());
        assert!(!user.is_empty());
        assert!(system.contains("patient"));
        assert!(user.contains("NOT_FOUND"));
    }

    #[test]
    fn test_format_for_detection_truncated_small_buffer() {
        // Under 3000 words — should return everything, same as format_for_detection
        let mut buffer = TranscriptBuffer::new();
        buffer.push("Hello doctor".to_string(), 1000, Some("Speaker 1".to_string()), 0);
        buffer.push("How are you today".to_string(), 2000, Some("Speaker 2".to_string()), 0);

        let full = buffer.format_for_detection();
        let truncated = buffer.format_for_detection_truncated();
        assert_eq!(full, truncated);
    }

    #[test]
    fn test_format_for_detection_truncated_large_buffer() {
        // Create a buffer with >3000 words to trigger truncation
        let mut buffer = TranscriptBuffer::new();
        // Each segment: ~50 words, so 80 segments = ~4000 words
        for i in 0..80 {
            let text = (0..50).map(|w| format!("word{}seg{}", w, i)).collect::<Vec<_>>().join(" ");
            buffer.push(text, i * 1000, Some(format!("Speaker {}", i % 3)), 0);
        }

        let total_words = buffer.word_count();
        assert!(total_words > 3000, "Buffer should have >3000 words, got {}", total_words);

        let truncated = buffer.format_for_detection_truncated();
        assert!(truncated.contains("segments omitted for brevity"));

        // Should contain first segment and last segment
        assert!(truncated.contains("[0]"));
        assert!(truncated.contains(&format!("[{}]", 79)));

        // Truncated should be shorter than full
        let full = buffer.format_for_detection();
        assert!(truncated.len() < full.len(), "Truncated ({}) should be shorter than full ({})", truncated.len(), full.len());
    }

    #[test]
    fn test_detection_prompt_requires_english() {
        let (system, _) = build_encounter_detection_prompt("test transcript");
        assert!(system.contains("MUST respond in English"), "Prompt should require English response");
        assert!(system.contains("ONLY a JSON object"), "Prompt should require JSON only");
    }

    #[test]
    fn test_force_split_constants() {
        assert!(
            FORCE_CHECK_WORD_THRESHOLD < FORCE_SPLIT_WORD_THRESHOLD,
            "FORCE_CHECK ({}) must be less than FORCE_SPLIT ({})",
            FORCE_CHECK_WORD_THRESHOLD, FORCE_SPLIT_WORD_THRESHOLD
        );
        assert!(
            FORCE_SPLIT_WORD_THRESHOLD < ABSOLUTE_WORD_CAP,
            "FORCE_SPLIT ({}) must be less than ABSOLUTE_WORD_CAP ({})",
            FORCE_SPLIT_WORD_THRESHOLD, ABSOLUTE_WORD_CAP
        );
    }

    #[test]
    fn test_detection_prompt_transition_framing() {
        let (system, _) = build_encounter_detection_prompt("test transcript");
        assert!(
            system.to_lowercase().contains("transition"),
            "Prompt should use transition-based framing"
        );
        assert!(
            !system.contains("must have BOTH"),
            "Prompt should not require BOTH beginning and ending"
        );
        assert!(
            !system.contains("when in doubt"),
            "Prompt should not have 'when in doubt' bias"
        );
    }

    #[test]
    fn test_detection_prompt_no_premature_bias() {
        let (system, _) = build_encounter_detection_prompt("test transcript");
        assert!(
            !system.contains("better to wait"),
            "Prompt should not have 'better to wait' bias"
        );
        assert!(
            !system.contains("under 2 minutes"),
            "Prompt should not have 'under 2 minutes' rule (enforced in code)"
        );
    }
}
