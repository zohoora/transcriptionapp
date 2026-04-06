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
use std::path::Path;
use tracing::warn;

const BUNDLE_FILENAME: &str = "replay_bundle.json";
const SCHEMA_VERSION: u32 = 1;

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

    pub fn set_soap_result(&mut self, result: SoapResult) {
        self.soap_result = Some(result);
    }

    pub fn set_billing_result(&mut self, result: BillingResult) {
        self.billing_result = Some(result);
    }

    pub fn set_name_tracker(&mut self, state: NameTrackerState) {
        self.name_tracker = Some(state);
    }

    pub fn set_outcome(&mut self, outcome: Outcome) {
        self.outcome = Some(outcome);
    }

    /// Returns the trigger string from the split decision, if set.
    pub fn split_decision_trigger(&self) -> Option<String> {
        self.split_decision.as_ref().map(|d| d.trigger.clone())
    }

    /// Write the replay bundle to `session_dir/replay_bundle.json` and reset
    /// the builder for the next encounter (config is preserved).
    /// Never panics — logs warnings on I/O errors.
    pub fn build_and_reset(&mut self, session_dir: &Path) {
        let bundle = ReplayBundle {
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
        };

        let json = match serde_json::to_string_pretty(&bundle) {
            Ok(j) => j,
            Err(e) => {
                warn!("Failed to serialize replay bundle: {}", e);
                return;
            }
        };

        let path = session_dir.join(BUNDLE_FILENAME);
        if let Err(e) = fs::write(&path, json) {
            warn!("Failed to write replay bundle to {}: {}", path.display(), e);
        }
    }
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
        });

        // Set SOAP result
        builder.set_soap_result(SoapResult {
            ts: "2026-03-10T09:18:00Z".to_string(),
            latency_ms: 5000,
            success: true,
            word_count: 1200,
            error: None,
            patient_count: None,
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
}
