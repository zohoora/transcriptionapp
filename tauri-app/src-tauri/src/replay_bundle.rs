//! Replay bundle builder for continuous mode.
//!
//! Accumulates structured data throughout an encounter lifecycle and writes
//! a self-contained `replay_bundle.json` at archive time. This file contains
//! everything needed to replay the encounter detection pipeline offline:
//! config snapshot, all segments, sensor transitions, vision results,
//! every LLM call (prompts + responses), and the final outcome.
//!
//! Contains PHI — stored alongside existing PHI (transcript, SOAP) in the archive.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;

const BUNDLE_FILENAME: &str = "replay_bundle.json";
/// Filename prefix for merged-away encounter bundles. Full name format:
/// `replay_bundle.merged_{short_id}.json` where `short_id` is the first 8
/// characters of the merged-away session's UUID. Lives as a sibling to the
/// surviving session's canonical `replay_bundle.json`.
///
/// See `ReplayBundleBuilder::build_merged_and_reset()`.
const MERGED_BUNDLE_PREFIX: &str = "replay_bundle.merged_";
/// v2 (2026-04): added `sensor_continuous_present`, `sensor_triggered`,
/// `manual_triggered` to `LoopState` so `detection_replay_cli` can reconstruct
/// the full production `DetectionEvalContext` without hardcoded defaults.
/// v3 (2026-04): added `MultiPatientDetection.split_decision` to capture the
/// multi-patient SPLIT prompt's parsed line_index for replay regression testing.
/// v4 (2026-04): added `MergeCheck.prev_source` + `prev_soap_excerpt` so the
/// SOAP-aware merge-check is fully replayable.
/// v5 (2026-04): added `system_prompt`, `user_prompt`, `response_raw` to
/// `SoapResult` and `BillingResult` so SOAP and billing prompt-engineering
/// experiments can replay archived sessions through new prompts without
/// re-issuing the original LLM calls. Older bundles still load via
/// `#[serde(default)]` — older replay tools see None.
const SCHEMA_VERSION: u32 = 5;

/// UTF-8-safe cap for v5 prompt/response captures. Production prompts run
/// 2-8 KB and responses 1-3 KB; this leaves ample headroom while bounding the
/// damage from a pathological 100 KB+ LLM response (which would otherwise
/// bloat the on-disk bundle and slow downstream tooling).
pub const CAPTURE_MAX_BYTES: usize = 32 * 1024;

/// Truncate a captured prompt/response to `CAPTURE_MAX_BYTES` on a UTF-8
/// boundary. Returns the input untouched when already within the cap.
fn cap_capture(s: String) -> String {
    if s.len() <= CAPTURE_MAX_BYTES {
        s
    } else {
        let end = s.ceil_char_boundary(CAPTURE_MAX_BYTES);
        s[..end].to_string()
    }
}

/// Self-contained replay test case for an encounter.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReplayBundle {
    pub schema_version: u32,
    pub config: serde_json::Value,
    pub segments: Vec<ReplaySegment>,
    pub sensor_transitions: Vec<SensorTransition>,
    pub vision_results: Vec<VisionResult>,
    pub detection_checks: Vec<DetectionCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_decision: Option<SplitDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clinical_check: Option<ClinicalCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_check: Option<MergeCheck>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soap_result: Option<SoapResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_result: Option<BillingResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_tracker: Option<NameTrackerState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
    /// Multi-patient detection LLM calls (pre-SOAP inline, retrospective
    /// post-merge, standalone safety-net). 0, 1, or 2+ per encounter.
    /// Schema v2+; defaults to empty Vec for older bundles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub multi_patient_detections: Vec<MultiPatientDetection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySegment {
    pub ts: String,
    pub index: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorTransition {
    pub ts: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionResult {
    pub ts: String,
    pub parsed_name: Option<String>,
    pub is_stale: bool,
    pub is_blank: bool,
    pub latency_ms: u64,
}

impl VisionResult {
    /// Create a result for a failed or timed-out vision call.
    pub fn failed(latency_ms: u64) -> Self {
        Self { ts: chrono::Utc::now().to_rfc3339(), parsed_name: None, is_stale: false, is_blank: false, latency_ms }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionCheck {
    pub ts: String,
    pub segment_range: (u64, u64),
    pub word_count: usize,
    pub cleaned_word_count: usize,
    pub sensor_context: SensorContext,
    pub prompt_system: String,
    pub prompt_user: String,
    pub response_raw: Option<String>,
    pub parsed_complete: Option<bool>,
    pub parsed_confidence: Option<f64>,
    pub parsed_end_index: Option<u64>,
    pub latency_ms: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub loop_state: LoopState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorContext {
    pub departed: bool,
    pub present: bool,
    pub unknown: bool,
}

impl SensorContext {
    pub fn new(departed: bool, present: bool) -> Self {
        Self { departed, present, unknown: !departed && !present }
    }
}

impl DetectionCheck {
    /// Build a detection check with common fields. Result-specific fields (`response_raw`,
    /// `parsed_*`, `success`, `error`) are set by the caller after construction.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        segment_range: (u64, u64),
        word_count: usize,
        cleaned_word_count: usize,
        sensor_context: SensorContext,
        prompt_system: String,
        prompt_user: String,
        latency_ms: u64,
        consecutive_failures: u32,
        merge_back_count: u32,
        buffer_age_secs: f64,
        sensor_absent_since: Option<String>,
        sensor_continuous_present: bool,
        sensor_triggered: bool,
        manual_triggered: bool,
    ) -> Self {
        Self {
            ts: chrono::Utc::now().to_rfc3339(),
            segment_range,
            word_count,
            cleaned_word_count,
            sensor_context,
            prompt_system,
            prompt_user,
            response_raw: None,
            parsed_complete: None,
            parsed_confidence: None,
            parsed_end_index: None,
            latency_ms,
            success: false,
            error: None,
            loop_state: LoopState {
                consecutive_failures,
                merge_back_count,
                buffer_age_secs,
                sensor_absent_since,
                sensor_continuous_present,
                sensor_triggered,
                manual_triggered,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopState {
    pub consecutive_failures: u32,
    pub merge_back_count: u32,
    pub buffer_age_secs: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_absent_since: Option<String>,
    /// True when the sensor has remained continuously Present since the last
    /// encounter split. Production uses this to raise the LLM-only split
    /// threshold to 0.99 (block false splits during couples/family visits).
    /// Schema v2+; defaults to false for older bundles.
    #[serde(default)]
    pub sensor_continuous_present: bool,
    /// True when this detection check was triggered by a sensor Present→Absent
    /// transition (hybrid mode). In pure sensor mode, sensor triggers short-
    /// circuit the LLM and no bundle check is produced, so this is only
    /// meaningful in hybrid mode. Schema v2+; defaults to false.
    #[serde(default)]
    pub sensor_triggered: bool,
    /// True when this check was triggered by a manual "new patient" button
    /// press. Manual triggers also short-circuit the LLM so bundle checks
    /// rarely record this as true — it exists mainly so `--override
    /// manual_triggered=true` works in the replay CLI. Schema v2+.
    #[serde(default)]
    pub manual_triggered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitDecision {
    pub ts: String,
    pub trigger: String,
    pub word_count: usize,
    pub cleaned_word_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_segment_index: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClinicalCheck {
    pub ts: String,
    pub is_clinical: bool,
    pub latency_ms: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeCheck {
    pub ts: String,
    pub prev_session_id: String,
    /// Last portion of the prev transcript. Always populated for auditability
    /// even when the LLM actually saw the SOAP note — so replay tools can
    /// compare what the SOAP-based check decided vs. what the tail-based check
    /// would have decided on the same excerpt.
    pub prev_tail_excerpt: String,
    pub curr_head_excerpt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patient_name: Option<String>,
    pub prompt_system: String,
    pub prompt_user: String,
    pub response_raw: Option<String>,
    pub parsed_same_encounter: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed_reason: Option<String>,
    pub latency_ms: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_merge_gate: Option<String>,
    /// Which prev-side input the LLM actually consumed: "soap_note" or
    /// "transcript_tail" (or "auto_merge_small_orphan" when the auto-merge gate
    /// short-circuited the LLM call). `None` for bundles written before the
    /// SOAP-aware merge-check shipped (schema v3 and earlier).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_source: Option<String>,
    /// Verbatim prev SOAP fed to the merge LLM when `prev_source == "soap_note"`.
    /// `None` when the tail path was used. Kept separate from `prev_tail_excerpt`
    /// so replay tools can inspect either view without ambiguity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_soap_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoapResult {
    pub ts: String,
    pub latency_ms: u64,
    pub success: bool,
    pub word_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Number of patients detected (>1 for per-patient SOAP generation)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patient_count: Option<usize>,
    /// SOAP system prompt fed to the LLM. Schema v5+. Captured so
    /// `soap_experiment_cli` can replay the same transcript through alternate
    /// prompts and diff the outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// SOAP user prompt (transcript + clinician notes). Schema v5+.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_prompt: Option<String>,
    /// Raw SOAP-LLM response text (JSON pre-parsing). Schema v5+.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_raw: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MultiPatientStage {
    /// Inline pre-SOAP detection at >=500 words. Runs immediately after a
    /// split decision, before SOAP generation. Can fire on every clinical
    /// encounter.
    PreSoap,
    /// Retrospective check on the merged text after a merge-back, when the
    /// merged encounter exceeds 500 words. Captured in the SURVIVING bundle
    /// (not the merged-away sibling) — production attributes it to the
    /// surviving session via `logger.set_session(prev_dir)`.
    Retrospective,
    /// Standalone safety net for very large encounters (>=2500 words),
    /// runs after the merge check, only on clinical encounters.
    Standalone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPatientDetection {
    pub ts: String,
    pub stage: MultiPatientStage,
    pub word_count: usize,
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_raw: Option<String>,
    /// Patient count from parsed result. None when LLM call failed or parse failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_patient_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_reasoning: Option<String>,
    /// Labels for each detected patient (empty when none parsed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patient_labels: Vec<String>,
    pub latency_ms: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// If a multi-patient split was performed after this detection, the captured
    /// LLM call that found the line_index boundary. Schema v3+ field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_decision: Option<MultiPatientSplitDecision>,
}

/// Captured LLM call for the multi-patient SPLIT prompt — finds the line_index
/// boundary between the first and second patient's encounters.
/// Only populated when retrospective multi-patient detection found 2+ patients
/// and the system invoked the split prompt to find the boundary line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPatientSplitDecision {
    pub ts: String,
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_raw: Option<String>,
    /// Last line index of the FIRST patient's encounter. None when LLM returned
    /// empty `{}` (no clear boundary) or parse failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_line_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_confidence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_reason: Option<String>,
    pub latency_ms: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingResult {
    pub ts: String,
    pub latency_ms: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codes_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_amount_cents: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_codes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Billing-extraction system prompt fed to the LLM. Schema v5+.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// Billing-extraction user prompt (SOAP + transcript + context hints). Schema v5+.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_prompt: Option<String>,
    /// Raw billing-LLM response text (clinical_features JSON pre-parsing). Schema v5+.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_raw: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NameTrackerState {
    pub majority_name: Option<String>,
    pub vote_count: usize,
    pub unique_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub session_id: String,
    pub encounter_number: u32,
    pub word_count: usize,
    pub is_clinical: bool,
    pub was_merged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_into: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patient_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection_method: Option<String>,
}

/// Accumulates replay data for a single encounter, then writes `replay_bundle.json`.
pub struct ReplayBundleBuilder {
    config: serde_json::Value,
    segments: Vec<ReplaySegment>,
    sensor_transitions: Vec<SensorTransition>,
    vision_results: Vec<VisionResult>,
    detection_checks: Vec<DetectionCheck>,
    split_decision: Option<SplitDecision>,
    clinical_check: Option<ClinicalCheck>,
    merge_check: Option<MergeCheck>,
    soap_result: Option<SoapResult>,
    billing_result: Option<BillingResult>,
    name_tracker: Option<NameTrackerState>,
    outcome: Option<Outcome>,
    multi_patient_detections: Vec<MultiPatientDetection>,
}

impl ReplayBundleBuilder {
    pub fn new(config: serde_json::Value) -> Self {
        Self {
            config,
            segments: Vec::new(),
            sensor_transitions: Vec::new(),
            vision_results: Vec::new(),
            detection_checks: Vec::new(),
            split_decision: None,
            clinical_check: None,
            merge_check: None,
            soap_result: None,
            billing_result: None,
            name_tracker: None,
            outcome: None,
            multi_patient_detections: Vec::new(),
        }
    }

    pub fn add_segment(&mut self, segment: ReplaySegment) {
        self.segments.push(segment);
    }

    pub fn add_sensor_transition(&mut self, transition: SensorTransition) {
        self.sensor_transitions.push(transition);
    }

    pub fn add_vision_result(&mut self, result: VisionResult) {
        self.vision_results.push(result);
    }

    pub fn add_detection_check(&mut self, check: DetectionCheck) {
        self.detection_checks.push(check);
    }

    pub fn set_split_decision(&mut self, decision: SplitDecision) {
        self.split_decision = Some(decision);
    }

    pub fn set_clinical_check(&mut self, check: ClinicalCheck) {
        self.clinical_check = Some(check);
    }

    pub fn set_merge_check(&mut self, check: MergeCheck) {
        self.merge_check = Some(check);
    }

    pub fn set_soap_result(&mut self, mut result: SoapResult) {
        // v5+ capture fields are UTF-8-truncated at the bundle boundary so
        // a pathological LLM response can't bloat the on-disk bundle.
        result.system_prompt = result.system_prompt.map(cap_capture);
        result.user_prompt = result.user_prompt.map(cap_capture);
        result.response_raw = result.response_raw.map(cap_capture);
        self.soap_result = Some(result);
    }

    pub fn set_billing_result(&mut self, mut result: BillingResult) {
        result.system_prompt = result.system_prompt.map(cap_capture);
        result.user_prompt = result.user_prompt.map(cap_capture);
        result.response_raw = result.response_raw.map(cap_capture);
        self.billing_result = Some(result);
    }

    pub fn set_name_tracker(&mut self, state: NameTrackerState) {
        self.name_tracker = Some(state);
    }

    pub fn set_outcome(&mut self, outcome: Outcome) {
        self.outcome = Some(outcome);
    }

    pub fn add_multi_patient_detection(&mut self, detection: MultiPatientDetection) {
        self.multi_patient_detections.push(detection);
    }

    /// Attach a split-decision LLM call to the most recently added multi-
    /// patient detection. No-op when there's no prior detection. The split
    /// prompt only fires after detection succeeds, so the call site naturally
    /// satisfies that ordering.
    pub fn set_split_decision_on_last_mp_detection(
        &mut self,
        decision: MultiPatientSplitDecision,
    ) {
        if let Some(last) = self.multi_patient_detections.last_mut() {
            last.split_decision = Some(decision);
        }
    }

    /// Returns the trigger string from the split decision, if set.
    pub fn split_decision_trigger(&self) -> Option<String> {
        self.split_decision.as_ref().map(|d| d.trigger.clone())
    }

    /// Drain per-encounter state into a `ReplayBundle`. Config is cloned, all
    /// other fields are moved out via `mem::take` / `Option::take`.
    fn take_bundle(&mut self) -> ReplayBundle {
        ReplayBundle {
            schema_version: SCHEMA_VERSION,
            config: self.config.clone(),
            segments: std::mem::take(&mut self.segments),
            sensor_transitions: std::mem::take(&mut self.sensor_transitions),
            vision_results: std::mem::take(&mut self.vision_results),
            detection_checks: std::mem::take(&mut self.detection_checks),
            split_decision: self.split_decision.take(),
            clinical_check: self.clinical_check.take(),
            merge_check: self.merge_check.take(),
            soap_result: self.soap_result.take(),
            billing_result: self.billing_result.take(),
            name_tracker: self.name_tracker.take(),
            outcome: self.outcome.take(),
            multi_patient_detections: std::mem::take(&mut self.multi_patient_detections),
        }
    }

    /// Serialize a bundle to pretty JSON and write it to `path`. Logs a
    /// warning on failure; never panics.
    fn write_bundle(path: &Path, bundle: &ReplayBundle) {
        match serde_json::to_string_pretty(bundle) {
            Ok(json) => {
                if let Err(e) = fs::write(path, json) {
                    warn!("Failed to write replay bundle to {}: {}", path.display(), e);
                }
            }
            Err(e) => warn!("Failed to serialize replay bundle: {}", e),
        }
    }

    /// Write the replay bundle to `session_dir/replay_bundle.json` and reset
    /// the builder for the next encounter (config is preserved).
    pub fn build_and_reset(&mut self, session_dir: &Path) {
        let bundle = self.take_bundle();
        Self::write_bundle(&session_dir.join(BUNDLE_FILENAME), &bundle);
    }

    /// Reset per-encounter state without writing to disk. Config is preserved
    /// (same as `build_and_reset`). Used when builder state must be discarded
    /// without producing an artifact — fallback for the merge-back path when
    /// the surviving session's directory cannot be resolved.
    pub fn clear(&mut self) {
        // Drop the bundle immediately so allocations are released.
        let _ = self.take_bundle();
    }

    /// Write this builder's state as a merged-away sibling under the SURVIVING
    /// session's directory, then reset for the next encounter.
    ///
    /// Filename format: `replay_bundle.merged_{short_id}.json` where short_id
    /// is the first 8 chars of the outcome's session_id (or the full ID if
    /// shorter). Forces `outcome.was_merged = true` and
    /// `outcome.merged_into = Some(surviving_session_id)` immediately before
    /// serialization. Caller MUST set the outcome via `set_outcome()` before
    /// calling — the merged-away session_id is read from `outcome.session_id`.
    pub fn build_merged_and_reset(&mut self, surviving_dir: &Path, surviving_session_id: &str) {
        let merged_away_short = self
            .outcome
            .as_ref()
            .and_then(|o| o.session_id.get(..8).map(str::to_string).or_else(|| Some(o.session_id.clone())))
            .unwrap_or_else(|| "unknown".to_string());

        if let Some(ref mut o) = self.outcome {
            o.was_merged = true;
            o.merged_into = Some(surviving_session_id.to_string());
        }

        let bundle = self.take_bundle();
        let filename = format!("{}{}.json", MERGED_BUNDLE_PREFIX, merged_away_short);
        Self::write_bundle(&surviving_dir.join(filename), &bundle);
    }
}

/// Recursively find every replay bundle file under `root`. Matches the
/// canonical `replay_bundle.json` and merged-away siblings
/// (`replay_bundle.merged_*.json`). Sorted alphabetically.
pub fn find_replay_bundles(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name == BUNDLE_FILENAME
                    || (name.starts_with(MERGED_BUNDLE_PREFIX) && name.ends_with(".json"))
                {
                    out.push(path);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_config() -> serde_json::Value {
        serde_json::json!({
            "encounter_check_interval_secs": 120,
            "encounter_detection_mode": "hybrid"
        })
    }

    fn sample_segment(index: u64) -> ReplaySegment {
        ReplaySegment {
            ts: "2026-03-10T09:00:00Z".to_string(),
            index,
            start_ms: index * 1000,
            end_ms: (index + 1) * 1000,
            text: format!("Segment {} text", index),
            speaker_id: None,
            speaker_confidence: None,
        }
    }

    fn sample_detection_check() -> DetectionCheck {
        DetectionCheck {
            ts: "2026-03-10T09:05:00Z".to_string(),
            segment_range: (0, 10),
            word_count: 500,
            cleaned_word_count: 480,
            sensor_context: SensorContext {
                departed: false,
                present: true,
                unknown: false,
            },
            prompt_system: "You are a transition-point detector.".to_string(),
            prompt_user: "Evaluate the following transcript...".to_string(),
            response_raw: Some(r#"{"complete": false, "confidence": 0.3}"#.to_string()),
            parsed_complete: Some(false),
            parsed_confidence: Some(0.3),
            parsed_end_index: None,
            latency_ms: 1200,
            success: true,
            error: None,
            loop_state: LoopState {
                consecutive_failures: 0,
                merge_back_count: 0,
                buffer_age_secs: 300.0,
                sensor_absent_since: None,
                sensor_continuous_present: false,
                sensor_triggered: false,
                manual_triggered: false,
            },
        }
    }

    fn sample_split_decision() -> SplitDecision {
        SplitDecision {
            ts: "2026-03-10T09:15:00Z".to_string(),
            trigger: "llm".to_string(),
            word_count: 1200,
            cleaned_word_count: 1150,
            end_segment_index: Some(45),
        }
    }

    #[test]
    fn test_new_builder_empty() {
        let builder = ReplayBundleBuilder::new(sample_config());
        assert!(builder.segments.is_empty());
        assert!(builder.sensor_transitions.is_empty());
        assert!(builder.vision_results.is_empty());
        assert!(builder.detection_checks.is_empty());
        assert!(builder.split_decision.is_none());
        assert!(builder.clinical_check.is_none());
        assert!(builder.merge_check.is_none());
        assert!(builder.soap_result.is_none());
        assert!(builder.name_tracker.is_none());
        assert!(builder.outcome.is_none());
        assert_eq!(builder.config, sample_config());
    }

    #[test]
    fn test_add_segment() {
        let mut builder = ReplayBundleBuilder::new(sample_config());
        builder.add_segment(sample_segment(0));
        builder.add_segment(sample_segment(1));
        builder.add_segment(sample_segment(2));
        assert_eq!(builder.segments.len(), 3);
        assert_eq!(builder.segments[0].index, 0);
        assert_eq!(builder.segments[1].index, 1);
        assert_eq!(builder.segments[2].index, 2);
    }

    #[test]
    fn test_add_detection_check() {
        let mut builder = ReplayBundleBuilder::new(sample_config());
        builder.add_detection_check(sample_detection_check());
        assert_eq!(builder.detection_checks.len(), 1);
        assert_eq!(builder.detection_checks[0].word_count, 500);
        assert!(builder.detection_checks[0].success);
    }

    #[test]
    fn test_set_split_decision() {
        let mut builder = ReplayBundleBuilder::new(sample_config());
        assert!(builder.split_decision.is_none());
        builder.set_split_decision(sample_split_decision());
        assert!(builder.split_decision.is_some());
        let decision = builder.split_decision.as_ref().unwrap();
        assert_eq!(decision.trigger, "llm");
        assert_eq!(decision.end_segment_index, Some(45));
    }

    #[test]
    fn test_build_writes_json() {
        let dir = tempdir().expect("failed to create tempdir");
        let mut builder = ReplayBundleBuilder::new(sample_config());
        builder.add_segment(sample_segment(0));

        builder.build_and_reset(dir.path());

        let bundle_path = dir.path().join("replay_bundle.json");
        assert!(bundle_path.exists(), "replay_bundle.json should exist");

        let contents = fs::read_to_string(&bundle_path).expect("failed to read bundle");
        let parsed: serde_json::Value =
            serde_json::from_str(&contents).expect("bundle should be valid JSON");

        assert_eq!(parsed["schema_version"], SCHEMA_VERSION);
        assert_eq!(parsed["segments"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["config"]["encounter_detection_mode"], "hybrid");
    }

    #[test]
    fn test_build_and_reset_clears_data_keeps_config() {
        let dir = tempdir().expect("failed to create tempdir");
        let mut builder = ReplayBundleBuilder::new(sample_config());
        builder.add_segment(sample_segment(0));
        builder.add_segment(sample_segment(1));
        builder.add_detection_check(sample_detection_check());
        builder.set_split_decision(sample_split_decision());
        builder.set_clinical_check(ClinicalCheck {
            ts: "2026-03-10T09:16:00Z".to_string(),
            is_clinical: true,
            latency_ms: 800,
            success: true,
            error: None,
        });
        builder.set_outcome(Outcome {
            session_id: "test-session".to_string(),
            encounter_number: 1,
            word_count: 1200,
            is_clinical: true,
            was_merged: false,
            merged_into: None,
            patient_name: Some("Jane Doe".to_string()),
            detection_method: Some("hybrid_llm".to_string()),
        });

        // Verify data is present before build_and_reset
        assert_eq!(builder.segments.len(), 2);
        assert!(builder.split_decision.is_some());
        assert!(builder.clinical_check.is_some());
        assert!(builder.outcome.is_some());

        builder.build_and_reset(dir.path());

        // Everything cleared except config
        assert!(builder.segments.is_empty());
        assert!(builder.sensor_transitions.is_empty());
        assert!(builder.vision_results.is_empty());
        assert!(builder.detection_checks.is_empty());
        assert!(builder.split_decision.is_none());
        assert!(builder.clinical_check.is_none());
        assert!(builder.merge_check.is_none());
        assert!(builder.soap_result.is_none());
        assert!(builder.name_tracker.is_none());
        assert!(builder.outcome.is_none());

        // Config preserved
        assert_eq!(builder.config, sample_config());

        // File was written before reset
        let bundle_path = dir.path().join("replay_bundle.json");
        assert!(bundle_path.exists());
    }

    #[test]
    fn test_build_with_all_fields() {
        let dir = tempdir().expect("failed to create tempdir");
        let mut builder = ReplayBundleBuilder::new(sample_config());

        // Add segments
        builder.add_segment(sample_segment(0));
        builder.add_segment(sample_segment(1));

        // Add sensor transition
        builder.add_sensor_transition(SensorTransition {
            ts: "2026-03-10T09:02:00Z".to_string(),
            from: "Present".to_string(),
            to: "Absent".to_string(),
        });

        // Add vision result
        builder.add_vision_result(VisionResult {
            ts: "2026-03-10T09:03:00Z".to_string(),
            parsed_name: Some("John Smith".to_string()),
            is_stale: false,
            is_blank: false,
            latency_ms: 2500,
        });

        // Add detection check
        builder.add_detection_check(sample_detection_check());

        // Set split decision
        builder.set_split_decision(sample_split_decision());

        // Set clinical check
        builder.set_clinical_check(ClinicalCheck {
            ts: "2026-03-10T09:16:00Z".to_string(),
            is_clinical: true,
            latency_ms: 800,
            success: true,
            error: None,
        });

        // Set merge check
        builder.set_merge_check(MergeCheck {
            ts: "2026-03-10T09:17:00Z".to_string(),
            prev_session_id: "prev-session-123".to_string(),
            prev_tail_excerpt: "...and follow up in two weeks.".to_string(),
            curr_head_excerpt: "Hi, how are you doing today...".to_string(),
            patient_name: Some("John Smith".to_string()),
            prompt_system: "You are an encounter merge evaluator.".to_string(),
            prompt_user: "Compare these two transcript excerpts...".to_string(),
            response_raw: Some(r#"{"same_encounter": false, "reason": "Different patients"}"#.to_string()),
            parsed_same_encounter: Some(false),
            parsed_reason: Some("Different patients".to_string()),
            latency_ms: 1500,
            success: true,
            auto_merge_gate: Some("name_mismatch".to_string()),
            prev_source: Some("transcript_tail".to_string()),
            prev_soap_excerpt: None,
        });

        // Set SOAP result
        builder.set_soap_result(SoapResult {
            ts: "2026-03-10T09:18:00Z".to_string(),
            latency_ms: 5000,
            success: true,
            word_count: 1200,
            error: None,
            patient_count: None,
            system_prompt: None,
            user_prompt: None,
            response_raw: None,
        });

        // Set name tracker state
        builder.set_name_tracker(NameTrackerState {
            majority_name: Some("John Smith".to_string()),
            vote_count: 3,
            unique_names: vec!["John Smith".to_string(), "J. Smith".to_string()],
        });

        // Set outcome
        builder.set_outcome(Outcome {
            session_id: "session-456".to_string(),
            encounter_number: 2,
            word_count: 1200,
            is_clinical: true,
            was_merged: false,
            merged_into: None,
            patient_name: Some("John Smith".to_string()),
            detection_method: Some("hybrid_llm".to_string()),
        });

        builder.build_and_reset(dir.path());

        let bundle_path = dir.path().join("replay_bundle.json");
        assert!(bundle_path.exists());

        let contents = fs::read_to_string(&bundle_path).expect("failed to read bundle");
        let parsed: serde_json::Value =
            serde_json::from_str(&contents).expect("bundle should be valid JSON");

        // Verify all top-level fields present
        assert_eq!(parsed["schema_version"], SCHEMA_VERSION);
        assert!(parsed["config"].is_object());
        assert_eq!(parsed["segments"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["sensor_transitions"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["vision_results"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["detection_checks"].as_array().unwrap().len(), 1);
        assert!(parsed["split_decision"].is_object());
        assert!(parsed["clinical_check"].is_object());
        assert!(parsed["merge_check"].is_object());
        assert!(parsed["soap_result"].is_object());
        assert!(parsed["name_tracker"].is_object());
        assert!(parsed["outcome"].is_object());

        // Spot-check nested values
        assert_eq!(parsed["split_decision"]["trigger"], "llm");
        assert_eq!(parsed["clinical_check"]["is_clinical"], true);
        assert_eq!(parsed["merge_check"]["prev_session_id"], "prev-session-123");
        assert_eq!(parsed["soap_result"]["latency_ms"], 5000);
        assert_eq!(parsed["name_tracker"]["majority_name"], "John Smith");
        assert_eq!(parsed["outcome"]["session_id"], "session-456");
        assert_eq!(parsed["outcome"]["encounter_number"], 2);
        assert_eq!(parsed["vision_results"][0]["parsed_name"], "John Smith");
        assert_eq!(parsed["sensor_transitions"][0]["from"], "Present");
        assert_eq!(parsed["detection_checks"][0]["sensor_context"]["present"], true);
        assert_eq!(parsed["detection_checks"][0]["loop_state"]["consecutive_failures"], 0);
    }

    // ---- clear() and build_merged_and_reset tests ----

    fn sample_outcome(session_id: &str) -> Outcome {
        Outcome {
            session_id: session_id.to_string(),
            encounter_number: 3,
            word_count: 250,
            is_clinical: true,
            was_merged: false,
            merged_into: None,
            patient_name: Some("Test Patient".to_string()),
            detection_method: Some("llm".to_string()),
        }
    }

    #[test]
    fn test_clear_resets_without_writing() {
        let dir = tempdir().expect("tempdir");
        let mut builder = ReplayBundleBuilder::new(sample_config());
        builder.add_segment(sample_segment(0));
        builder.add_segment(sample_segment(1));
        builder.add_detection_check(sample_detection_check());
        builder.set_clinical_check(ClinicalCheck {
            ts: "2026-04-15T10:00:00Z".into(),
            is_clinical: true,
            latency_ms: 800,
            success: true,
            error: None,
        });
        builder.set_outcome(sample_outcome("test-session"));

        builder.clear();

        // All per-encounter state cleared
        assert!(builder.segments.is_empty());
        assert!(builder.sensor_transitions.is_empty());
        assert!(builder.vision_results.is_empty());
        assert!(builder.detection_checks.is_empty());
        assert!(builder.split_decision.is_none());
        assert!(builder.clinical_check.is_none());
        assert!(builder.merge_check.is_none());
        assert!(builder.soap_result.is_none());
        assert!(builder.billing_result.is_none());
        assert!(builder.name_tracker.is_none());
        assert!(builder.outcome.is_none());

        // Config preserved
        assert_eq!(builder.config, sample_config());

        // No file written
        assert!(!dir.path().join("replay_bundle.json").exists());
    }

    #[test]
    fn test_build_merged_and_reset_writes_sibling_file() {
        let dir = tempdir().expect("tempdir");
        let mut builder = ReplayBundleBuilder::new(sample_config());
        builder.add_segment(sample_segment(0));
        builder.add_detection_check(sample_detection_check());
        builder.set_outcome(sample_outcome("merged-abc12345-fake"));

        builder.build_merged_and_reset(dir.path(), "surviving-xyz");

        // Sibling file created with the expected name (first 8 chars of outcome.session_id)
        let expected = dir.path().join("replay_bundle.merged_merged-a.json");
        assert!(
            expected.exists(),
            "merged sibling file should exist at {}",
            expected.display()
        );

        let content = fs::read_to_string(&expected).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["schema_version"], SCHEMA_VERSION);
        assert_eq!(parsed["outcome"]["was_merged"], true);
        assert_eq!(parsed["outcome"]["merged_into"], "surviving-xyz");
        assert_eq!(parsed["segments"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["detection_checks"].as_array().unwrap().len(), 1);

        // Builder is reset
        assert!(builder.segments.is_empty());
        assert!(builder.detection_checks.is_empty());
        assert!(builder.outcome.is_none());
        assert_eq!(builder.config, sample_config()); // config preserved
    }

    #[test]
    fn test_build_merged_and_reset_short_session_id_fallback() {
        let dir = tempdir().expect("tempdir");
        let mut builder = ReplayBundleBuilder::new(sample_config());
        builder.set_outcome(sample_outcome("abc"));

        // Session ID shorter than 8 chars — uses full ID as fallback
        builder.build_merged_and_reset(dir.path(), "surviving");
        assert!(dir.path().join("replay_bundle.merged_abc.json").exists());
    }

    #[test]
    fn test_build_merged_and_reset_overrides_was_merged_flags() {
        let dir = tempdir().expect("tempdir");
        let mut builder = ReplayBundleBuilder::new(sample_config());
        // Set outcome with was_merged=false (simulating a normal split outcome)
        let mut outcome = sample_outcome("merged-session");
        outcome.was_merged = false;
        outcome.merged_into = None;
        builder.set_outcome(outcome);

        builder.build_merged_and_reset(dir.path(), "surviving");

        let content =
            fs::read_to_string(dir.path().join("replay_bundle.merged_merged-s.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        // build_merged_and_reset must override these regardless of caller intent
        assert_eq!(parsed["outcome"]["was_merged"], true);
        assert_eq!(parsed["outcome"]["merged_into"], "surviving");
    }

    // ---- multi-patient detection capture tests ----

    fn sample_multi_patient_detection(stage: MultiPatientStage) -> MultiPatientDetection {
        MultiPatientDetection {
            ts: "2026-04-15T10:00:00Z".into(),
            stage,
            word_count: 800,
            model: "fast-model".into(),
            system_prompt: "You are a multi-patient detector".into(),
            user_prompt: "Transcript:".into(),
            response_raw: Some(r#"{"patient_count": 2}"#.into()),
            parsed_patient_count: Some(2),
            parsed_confidence: Some(0.92),
            parsed_reasoning: Some("Two distinct patients discussed".into()),
            patient_labels: vec!["Mrs. Smith".into(), "Mr. Jones".into()],
            latency_ms: 1200,
            success: true,
            error: None,
            split_decision: None,
        }
    }

    #[test]
    fn test_add_multi_patient_detection() {
        let mut builder = ReplayBundleBuilder::new(sample_config());
        builder.add_multi_patient_detection(sample_multi_patient_detection(
            MultiPatientStage::PreSoap,
        ));
        builder.add_multi_patient_detection(sample_multi_patient_detection(
            MultiPatientStage::Standalone,
        ));
        assert_eq!(builder.multi_patient_detections.len(), 2);
    }

    #[test]
    fn test_multi_patient_detection_persists_in_bundle() {
        let dir = tempdir().expect("tempdir");
        let mut builder = ReplayBundleBuilder::new(sample_config());
        builder.add_multi_patient_detection(sample_multi_patient_detection(
            MultiPatientStage::PreSoap,
        ));
        builder.build_and_reset(dir.path());

        let content = fs::read_to_string(dir.path().join("replay_bundle.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["multi_patient_detections"][0]["stage"], "pre_soap");
        assert_eq!(parsed["multi_patient_detections"][0]["parsed_patient_count"], 2);
        assert_eq!(parsed["multi_patient_detections"][0]["patient_labels"][0], "Mrs. Smith");
        // Cleared after reset
        assert!(builder.multi_patient_detections.is_empty());
    }

    #[test]
    fn test_multi_patient_detection_cleared_by_clear() {
        let mut builder = ReplayBundleBuilder::new(sample_config());
        builder.add_multi_patient_detection(sample_multi_patient_detection(
            MultiPatientStage::Retrospective,
        ));
        builder.clear();
        assert!(builder.multi_patient_detections.is_empty());
    }

    #[test]
    fn test_multi_patient_detection_cleared_by_build_merged_and_reset() {
        let dir = tempdir().expect("tempdir");
        let mut builder = ReplayBundleBuilder::new(sample_config());
        builder.add_multi_patient_detection(sample_multi_patient_detection(
            MultiPatientStage::PreSoap,
        ));
        builder.set_outcome(sample_outcome("merged-id"));
        builder.build_merged_and_reset(dir.path(), "surviving-id");

        // Builder is cleared
        assert!(builder.multi_patient_detections.is_empty());

        // The sibling file contains the detection
        let path = dir.path().join("replay_bundle.merged_merged-i.json");
        let content = fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["multi_patient_detections"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["multi_patient_detections"][0]["stage"], "pre_soap");
    }

    #[test]
    fn test_v1_bundle_deserializes_with_empty_multi_patient_detections() {
        // Backward compat: a bundle without the new field deserializes
        // with an empty Vec via #[serde(default)].
        let json = r#"{
            "schema_version": 2,
            "config": {},
            "segments": [],
            "sensor_transitions": [],
            "vision_results": [],
            "detection_checks": []
        }"#;
        let bundle: ReplayBundle = serde_json::from_str(json).unwrap();
        assert!(bundle.multi_patient_detections.is_empty());
    }

    #[test]
    fn test_multi_patient_stage_serializes_snake_case() {
        // The serde rename_all = "snake_case" must produce stage values that
        // match expectations: pre_soap, retrospective, standalone.
        let pre = serde_json::to_string(&MultiPatientStage::PreSoap).unwrap();
        let retro = serde_json::to_string(&MultiPatientStage::Retrospective).unwrap();
        let stand = serde_json::to_string(&MultiPatientStage::Standalone).unwrap();
        assert_eq!(pre, r#""pre_soap""#);
        assert_eq!(retro, r#""retrospective""#);
        assert_eq!(stand, r#""standalone""#);
    }

    #[test]
    fn test_v5_soap_result_roundtrip_with_prompts() {
        // Schema v5: SoapResult carries system_prompt + user_prompt + response_raw.
        let r = SoapResult {
            ts: "2026-04-25T12:00:00Z".to_string(),
            latency_ms: 1234,
            success: true,
            word_count: 42,
            error: None,
            patient_count: Some(2),
            system_prompt: Some("You are a medical scribe...".to_string()),
            user_prompt: Some("Transcript here".to_string()),
            response_raw: Some(r#"{"subjective":["foo"]}"#.to_string()),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("system_prompt"));
        assert!(json.contains("response_raw"));
        let r2: SoapResult = serde_json::from_str(&json).unwrap();
        assert_eq!(r2.system_prompt.as_deref(), Some("You are a medical scribe..."));
        assert_eq!(r2.response_raw.as_deref(), Some(r#"{"subjective":["foo"]}"#));
    }

    #[test]
    fn test_v5_billing_result_roundtrip_with_prompts() {
        let r = BillingResult {
            ts: "2026-04-25T12:00:00Z".to_string(),
            latency_ms: 5678,
            success: true,
            codes_count: Some(2),
            total_amount_cents: Some(3935),
            selected_codes: Some(vec!["A007A".to_string(), "Q310A".to_string()]),
            error: None,
            system_prompt: Some("Billing prompt".to_string()),
            user_prompt: Some("SOAP note + transcript".to_string()),
            response_raw: Some(r#"{"visitType":"intermediate_assessment"}"#.to_string()),
        };
        let json = serde_json::to_string(&r).unwrap();
        let r2: BillingResult = serde_json::from_str(&json).unwrap();
        assert_eq!(r2.system_prompt.as_deref(), Some("Billing prompt"));
    }

    #[test]
    fn test_capture_setters_truncate_oversized_strings() {
        let dir = tempdir().expect("tempdir");
        let mut builder = ReplayBundleBuilder::new(sample_config());
        let huge = "x".repeat(CAPTURE_MAX_BYTES * 4);
        builder.set_soap_result(SoapResult {
            ts: "2026-04-25T00:00:00Z".to_string(),
            latency_ms: 1,
            success: true,
            word_count: 0,
            error: None,
            patient_count: None,
            system_prompt: Some(huge.clone()),
            user_prompt: None,
            response_raw: Some(huge.clone()),
        });
        builder.build_and_reset(dir.path());
        let json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("replay_bundle.json")).unwrap()
        ).unwrap();
        let sys = json["soap_result"]["system_prompt"].as_str().unwrap();
        let raw = json["soap_result"]["response_raw"].as_str().unwrap();
        assert!(sys.len() <= CAPTURE_MAX_BYTES, "system_prompt should be capped");
        assert!(raw.len() <= CAPTURE_MAX_BYTES, "response_raw should be capped");
    }

    #[test]
    fn test_v4_bundle_loads_with_v5_fields_default_none() {
        // Backward compat: a bundle written under v4 (no system_prompt etc.)
        // must deserialize cleanly with new fields = None.
        let v4_soap = r#"{
            "ts": "2026-04-23T10:00:00Z",
            "latency_ms": 1000,
            "success": true,
            "word_count": 100,
            "patient_count": 1
        }"#;
        let r: SoapResult = serde_json::from_str(v4_soap).expect("v4 SoapResult should load");
        assert_eq!(r.system_prompt, None);
        assert_eq!(r.user_prompt, None);
        assert_eq!(r.response_raw, None);

        let v4_billing = r#"{
            "ts": "2026-04-23T10:01:00Z",
            "latency_ms": 2000,
            "success": true,
            "codes_count": 1,
            "total_amount_cents": 4455
        }"#;
        let b: BillingResult = serde_json::from_str(v4_billing).expect("v4 BillingResult should load");
        assert_eq!(b.system_prompt, None);
        assert_eq!(b.response_raw, None);
    }

    #[test]
    fn test_no_state_leaks_after_clear_then_build() {
        // Critical regression: simulate the bug where merge-back left state
        // behind. Build encounter A as merged → clear via build_merged_and_reset
        // → build encounter B normally → assert B's bundle has only B's data.
        let dir = tempdir().expect("tempdir");
        let mut builder = ReplayBundleBuilder::new(sample_config());

        // Encounter A: merged-away
        builder.add_segment(sample_segment(0));
        builder.add_detection_check(sample_detection_check());
        builder.set_outcome(sample_outcome("session-a-merged"));
        builder.build_merged_and_reset(dir.path(), "surviving");

        // Confirm builder is empty
        assert!(builder.segments.is_empty());

        // Encounter B: normal split
        builder.add_segment(sample_segment(99));
        builder.set_outcome(sample_outcome("session-b"));
        builder.build_and_reset(dir.path());

        let b_content = fs::read_to_string(dir.path().join("replay_bundle.json")).unwrap();
        let b_parsed: serde_json::Value = serde_json::from_str(&b_content).unwrap();
        let segs = b_parsed["segments"].as_array().unwrap();
        assert_eq!(segs.len(), 1, "B's bundle must have only B's segments, no leak from A");
        assert_eq!(segs[0]["index"], 99, "B's segment index, not A's (0)");
        assert_eq!(b_parsed["outcome"]["was_merged"], false, "B was not merged");
    }

    /// Guard against SCHEMA_VERSION drift between the code and the docs that
    /// promise a specific version. When SCHEMA_VERSION is bumped, update the
    /// callouts in these files or this test fails with a clear diff.
    ///
    /// Why these files? `tauri-app/CLAUDE.md` is the canonical codebase guide
    /// and tables the schema version explicitly; `docs/benchmarks/multi-patient-split.md`
    /// promises that the replay CLI accepts "schema v3+" and would mislead
    /// benchmark consumers if SCHEMA_VERSION advanced without the doc updating.
    #[test]
    fn test_schema_version_docs_in_sync() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest_dir.parent().expect("src-tauri parent").parent().expect("tauri-app parent");

        let checks: &[(&str, String)] = &[
            (
                "tauri-app/CLAUDE.md",
                format!("SCHEMA_VERSION={}", SCHEMA_VERSION),
            ),
            (
                "docs/benchmarks/multi-patient-split.md",
                format!("schema v{}", SCHEMA_VERSION),
            ),
        ];

        for (rel, needle) in checks {
            let path = repo_root.join(rel);
            if !path.exists() {
                // Doc moved or deleted — not this test's job to guard against that.
                continue;
            }
            let body = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
            assert!(
                body.contains(needle.as_str()),
                "SCHEMA_VERSION drift: {rel} is missing the literal `{needle}`. \
                 replay_bundle.rs declares SCHEMA_VERSION={} — update {rel} to match.",
                SCHEMA_VERSION,
            );
        }
    }
}
