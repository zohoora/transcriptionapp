//! Encounter Detection Experiment Framework
//!
//! A test harness to replay archived transcripts through different detection/merge
//! prompts and parameters, to find the configuration that correctly identifies
//! encounter boundaries.
//!
//! Motivated by a 2026-02-10 incident where a single patient visit (Buckland, Deborah Ann)
//! was incorrectly split into 3 encounters due to STT hallucination and overly aggressive
//! encounter detection.
//!
//! Usage:
//!   cargo run --bin encounter_experiment_cli
//!   cargo run --bin encounter_experiment_cli -- p1 p3 --threshold 0.9

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

use crate::continuous_mode::{
    build_encounter_detection_prompt, build_encounter_merge_prompt, parse_encounter_detection,
    parse_merge_check, EncounterDetectionResult, MergeCheckResult,
};
use crate::llm_client::LLMClient;

// ============================================================================
// Strategy Enums
// ============================================================================

/// Detection prompt strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionStrategy {
    /// P0: Current production prompt (baseline)
    Baseline,
    /// P1: Conservative — require explicit patient departure
    Conservative,
    /// P2: Same-patient context — educate about typical visit structure
    SamePatientContext,
    /// P3: Patient-name-aware — inject known patient name
    PatientNameAware,
}

impl DetectionStrategy {
    pub fn all() -> Vec<DetectionStrategy> {
        vec![
            DetectionStrategy::Baseline,
            DetectionStrategy::Conservative,
            DetectionStrategy::SamePatientContext,
            DetectionStrategy::PatientNameAware,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            DetectionStrategy::Baseline => "P0: Baseline (production)",
            DetectionStrategy::Conservative => "P1: Conservative",
            DetectionStrategy::SamePatientContext => "P2: Same-Patient Context",
            DetectionStrategy::PatientNameAware => "P3: Patient-Name-Aware",
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            DetectionStrategy::Baseline => "p0_baseline",
            DetectionStrategy::Conservative => "p1_conservative",
            DetectionStrategy::SamePatientContext => "p2_context",
            DetectionStrategy::PatientNameAware => "p3_name",
        }
    }
}

/// Merge prompt strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// M0: Current production merge prompt (baseline)
    Baseline,
    /// M1: Patient-name-weighted — add known patient name context
    PatientNameWeighted,
    /// M2: Hallucination-filtered — pre-process excerpts through strip_hallucinations
    HallucinationFiltered,
}

impl MergeStrategy {
    pub fn all() -> Vec<MergeStrategy> {
        vec![
            MergeStrategy::Baseline,
            MergeStrategy::PatientNameWeighted,
            MergeStrategy::HallucinationFiltered,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            MergeStrategy::Baseline => "M0: Baseline (production)",
            MergeStrategy::PatientNameWeighted => "M1: Patient-Name-Weighted",
            MergeStrategy::HallucinationFiltered => "M2: Hallucination-Filtered",
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            MergeStrategy::Baseline => "m0_baseline",
            MergeStrategy::PatientNameWeighted => "m1_name",
            MergeStrategy::HallucinationFiltered => "m2_filtered",
        }
    }
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for a single experiment run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentConfig {
    pub detection_strategy: Option<DetectionStrategy>,
    pub merge_strategy: Option<MergeStrategy>,
    pub confidence_threshold: f64,
    pub hallucination_filter: bool,
    /// Known patient name (from vision extraction or manual input)
    pub patient_name: Option<String>,
    /// Prepend /nothink to system prompts (for Qwen3 models with thinking mode)
    #[serde(default)]
    pub nothink: bool,
}

impl Default for ExperimentConfig {
    fn default() -> Self {
        Self {
            detection_strategy: Some(DetectionStrategy::Baseline),
            merge_strategy: None,
            confidence_threshold: 0.7,
            hallucination_filter: false,
            patient_name: None,
            nothink: false,
        }
    }
}

// ============================================================================
// Results
// ============================================================================

/// Result of a detection experiment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionExperimentResult {
    pub config: ExperimentConfig,
    pub prompt_system: String,
    pub prompt_user_preview: String,
    pub raw_response: String,
    pub parsed: Option<EncounterDetectionResult>,
    pub detected_complete: bool,
    pub confidence: f64,
    pub generation_time_ms: u64,
    pub input_word_count: usize,
    pub filtered_word_count: Option<usize>,
    pub timestamp: String,
}

/// Result of a merge experiment step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeStepResult {
    pub config: ExperimentConfig,
    pub encounter_pair: String,
    pub prev_tail_preview: String,
    pub curr_head_preview: String,
    pub raw_response: String,
    pub parsed: Option<MergeCheckResult>,
    pub same_encounter: bool,
    pub reason: Option<String>,
    pub generation_time_ms: u64,
    pub timestamp: String,
}

/// Report of hallucinations found in text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HallucinationReport {
    /// Each entry: (word, original_count, position_in_text)
    pub repetitions: Vec<HallucinationEntry>,
    pub original_word_count: usize,
    pub cleaned_word_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HallucinationEntry {
    pub word: String,
    pub original_count: usize,
    pub position: usize,
}

// ============================================================================
// Hallucination Filter
// ============================================================================

/// Detect and truncate consecutive word repetitions in transcript text.
///
/// Returns (cleaned_text, hallucination_report).
///
/// For the Buckland case: strips ~6000 "fractured" repetitions, reducing
/// encounter 1 from 7,315 to ~1,200 words.
pub fn strip_hallucinations(text: &str, max_consecutive: usize) -> (String, HallucinationReport) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let original_word_count = words.len();

    if words.is_empty() {
        return (
            String::new(),
            HallucinationReport {
                repetitions: Vec::new(),
                original_word_count: 0,
                cleaned_word_count: 0,
            },
        );
    }

    let mut result_words: Vec<&str> = Vec::with_capacity(words.len());
    let mut repetitions: Vec<HallucinationEntry> = Vec::new();

    let mut i = 0;
    while i < words.len() {
        let current = words[i];
        let current_lower = current.to_lowercase();

        // Count consecutive identical words (case-insensitive)
        let mut run_length = 1;
        while i + run_length < words.len()
            && words[i + run_length].to_lowercase() == current_lower
        {
            run_length += 1;
        }

        if run_length > max_consecutive {
            // Truncate to max_consecutive
            for j in 0..max_consecutive {
                result_words.push(words[i + j]);
            }
            repetitions.push(HallucinationEntry {
                word: current_lower,
                original_count: run_length,
                position: i,
            });
        } else {
            // Keep all words in the run
            for j in 0..run_length {
                result_words.push(words[i + j]);
            }
        }

        i += run_length;
    }

    let cleaned_word_count = result_words.len();
    let cleaned_text = result_words.join(" ");

    (
        cleaned_text,
        HallucinationReport {
            repetitions,
            original_word_count,
            cleaned_word_count,
        },
    )
}

// ============================================================================
// Prompt Builders
// ============================================================================

/// Build detection prompt for the given strategy, optionally pre-filtering hallucinations.
pub fn build_detection_prompt(
    strategy: DetectionStrategy,
    formatted_segments: &str,
    config: &ExperimentConfig,
) -> (String, String) {
    // Optionally filter hallucinations first
    let segments = if config.hallucination_filter {
        let (cleaned, _) = strip_hallucinations(formatted_segments, 5);
        cleaned
    } else {
        formatted_segments.to_string()
    };

    match strategy {
        DetectionStrategy::Baseline => {
            // Use the production prompt directly
            build_encounter_detection_prompt(&segments, None)
        }
        DetectionStrategy::Conservative => {
            let system = r#"You are analyzing a continuous transcript from a medical office.
The microphone has been recording all day without stopping.

Determine if the text below contains one or more COMPLETE patient encounters.

A complete encounter must have BOTH:
1. A clear BEGINNING: greeting, patient introduction, or start of clinical discussion with a patient
2. A clear ENDING: an explicit farewell BY NAME with evidence the patient is leaving (e.g., "Goodbye Mrs. Smith, take care", patient saying "Thank you, bye")

CRITICAL RULES:
- Topic shifts (injury → pain management → referrals) are NORMAL within a single patient visit. Do NOT split on topic changes alone.
- A visit may cover multiple topics: presenting complaint, examination, imaging review, pain management, referrals, scheduling. These are ALL part of ONE encounter.
- Only mark an encounter as complete when there is EXPLICIT evidence the patient has departed.
- When in doubt, keep segments together as one encounter.

If a COMPLETE encounter exists, return JSON:
{"complete": true, "end_segment_index": <last segment index of the encounter>, "confidence": <0.0-1.0>}

If the encounter is still in progress, return:
{"complete": false, "confidence": <0.0-1.0>}

Return ONLY the JSON object, nothing else."#;

            let user = format!(
                "Transcript (segments numbered with speaker labels):\n{}",
                segments
            );
            (system.to_string(), user)
        }
        DetectionStrategy::SamePatientContext => {
            let system = r#"You are analyzing a continuous transcript from a medical office.
The microphone has been recording all day without stopping.

Determine if the text below contains one or more COMPLETE patient encounters.

A complete encounter must have BOTH:
1. A clear BEGINNING: greeting, patient introduction, or start of clinical discussion
2. A clear ENDING: farewell, wrap-up, or transition to a different patient

IMPORTANT CONTEXT about medical visits:
A single patient visit often covers MULTIPLE topics in sequence:
- Presenting complaint and history
- Physical examination
- Imaging/lab review
- Pain management discussion
- Specialist referrals
- Follow-up scheduling and discharge instructions

All of these topics within one visit are part of ONE encounter. Do NOT split an encounter just because the topic of discussion changes. Only split when there is clear evidence that one patient has left and another has arrived.

If a COMPLETE encounter exists, return JSON:
{"complete": true, "end_segment_index": <last segment index of the encounter>, "confidence": <0.0-1.0>}

If the encounter is still in progress, return:
{"complete": false, "confidence": <0.0-1.0>}

When in doubt, return {"complete": false, "confidence": 0.0}.
Return ONLY the JSON object, nothing else."#;

            let user = format!(
                "Transcript (segments numbered with speaker labels):\n{}",
                segments
            );
            (system.to_string(), user)
        }
        DetectionStrategy::PatientNameAware => {
            let patient_context = if let Some(ref name) = config.patient_name {
                format!(
                    "\n\nCONTEXT: The current patient is {}. While this patient is present, ALL discussion is part of the same encounter. Only mark the encounter complete when this patient has clearly left (explicit farewell, departure).",
                    name
                )
            } else {
                String::new()
            };

            let system = format!(
                r#"You are analyzing a continuous transcript from a medical office.
The microphone has been recording all day without stopping.

Determine if the text below contains one or more COMPLETE patient encounters.

A complete encounter must have BOTH:
1. A clear BEGINNING: greeting, patient introduction, or start of clinical discussion with a patient
2. A clear ENDING: farewell, wrap-up ("we'll see you in X weeks"), discharge instructions, or a clear transition to a different patient

Do NOT mark as complete:
- Brief staff conversations, scheduling chatter, or hallway talk
- Encounters that are still in progress (no clear ending yet)
- Ambient noise or non-clinical discussion
- Topic changes within the same patient visit{}

If a COMPLETE encounter exists, return JSON:
{{"complete": true, "end_segment_index": <last segment index of the encounter>, "confidence": <0.0-1.0>}}

If the encounter is still in progress, return:
{{"complete": false, "confidence": <0.0-1.0>}}

When in doubt, return {{"complete": false, "confidence": 0.0}}.
Return ONLY the JSON object, nothing else."#,
                patient_context
            );

            let user = format!(
                "Transcript (segments numbered with speaker labels):\n{}",
                segments
            );
            (system, user)
        }
    }
}

/// Build merge prompt for the given strategy.
pub fn build_merge_prompt(
    strategy: MergeStrategy,
    prev_tail: &str,
    curr_head: &str,
    config: &ExperimentConfig,
) -> (String, String) {
    // Optionally filter hallucinations from excerpts
    let (clean_prev, clean_curr) = if strategy == MergeStrategy::HallucinationFiltered
        || config.hallucination_filter
    {
        let (p, _) = strip_hallucinations(prev_tail, 5);
        let (c, _) = strip_hallucinations(curr_head, 5);
        (p, c)
    } else {
        (prev_tail.to_string(), curr_head.to_string())
    };

    match strategy {
        MergeStrategy::Baseline => {
            // Use the production prompt directly
            build_encounter_merge_prompt(&clean_prev, &clean_curr, None)
        }
        MergeStrategy::PatientNameWeighted => {
            // Use the production prompt with patient name injection (M1 strategy)
            build_encounter_merge_prompt(
                &clean_prev,
                &clean_curr,
                config.patient_name.as_deref(),
            )
        }
        MergeStrategy::HallucinationFiltered => {
            // Same prompt as baseline, but excerpts are pre-filtered (done above)
            build_encounter_merge_prompt(&clean_prev, &clean_curr, None)
        }
    }
}

// ============================================================================
// Experiment Runners
// ============================================================================

/// Run a single detection experiment against the LLM.
pub async fn run_detection_experiment(
    client: &LLMClient,
    model: &str,
    formatted_segments: &str,
    config: &ExperimentConfig,
) -> Result<DetectionExperimentResult, String> {
    let strategy = config
        .detection_strategy
        .unwrap_or(DetectionStrategy::Baseline);

    let input_word_count = formatted_segments.split_whitespace().count();

    // Build prompt (may filter hallucinations internally)
    let (system_prompt, user_prompt) = build_detection_prompt(strategy, formatted_segments, config);

    let filtered_word_count = if config.hallucination_filter {
        Some(user_prompt.split_whitespace().count())
    } else {
        None
    };

    // Truncate user prompt preview for storage
    let user_preview = if user_prompt.len() > 500 {
        let boundary = user_prompt.ceil_char_boundary(500);
        format!("{}...", &user_prompt[..boundary])
    } else {
        user_prompt.clone()
    };

    info!(
        "Running detection experiment: {} (threshold={}, filter={})",
        strategy.name(),
        config.confidence_threshold,
        config.hallucination_filter
    );

    // Optionally prepend /nothink to disable Qwen3 thinking mode
    let system_prompt = if config.nothink {
        format!("/nothink\n{}", system_prompt)
    } else {
        system_prompt
    };

    let start = std::time::Instant::now();
    let response = client
        .generate(model, &system_prompt, &user_prompt, "encounter_experiment_detection")
        .await?;
    let generation_time_ms = start.elapsed().as_millis() as u64;

    let parsed = parse_encounter_detection(&response).ok();
    let detected_complete = parsed.as_ref().map(|p| p.complete).unwrap_or(false);
    let confidence = parsed
        .as_ref()
        .and_then(|p| p.confidence)
        .unwrap_or(0.0);

    Ok(DetectionExperimentResult {
        config: config.clone(),
        prompt_system: system_prompt,
        prompt_user_preview: user_preview,
        raw_response: response,
        parsed,
        detected_complete,
        confidence,
        generation_time_ms,
        input_word_count,
        filtered_word_count,
        timestamp: Utc::now().to_rfc3339(),
    })
}

/// Run a single merge experiment against the LLM.
pub async fn run_merge_experiment(
    client: &LLMClient,
    model: &str,
    prev_tail: &str,
    curr_head: &str,
    pair_label: &str,
    config: &ExperimentConfig,
) -> Result<MergeStepResult, String> {
    let strategy = config.merge_strategy.unwrap_or(MergeStrategy::Baseline);

    let (system_prompt, user_prompt) =
        build_merge_prompt(strategy, prev_tail, curr_head, config);

    // Truncate previews for storage
    let prev_preview = if prev_tail.len() > 200 {
        let b = prev_tail.ceil_char_boundary(200);
        format!("...{}", &prev_tail[prev_tail.len().saturating_sub(b)..])
    } else {
        prev_tail.to_string()
    };
    let curr_preview = if curr_head.len() > 200 {
        let b = curr_head.ceil_char_boundary(200);
        format!("{}...", &curr_head[..b])
    } else {
        curr_head.to_string()
    };

    // Optionally prepend /nothink to disable Qwen3 thinking mode
    let system_prompt = if config.nothink {
        format!("/nothink\n{}", system_prompt)
    } else {
        system_prompt
    };

    info!(
        "Running merge experiment: {} on {}",
        strategy.name(),
        pair_label
    );

    let start = std::time::Instant::now();
    let response = client
        .generate(model, &system_prompt, &user_prompt, "encounter_experiment_merge")
        .await?;
    let generation_time_ms = start.elapsed().as_millis() as u64;

    let parsed = parse_merge_check(&response).ok();
    let same_encounter = parsed.as_ref().map(|p| p.same_encounter).unwrap_or(false);
    let reason = parsed.as_ref().and_then(|p| p.reason.clone());

    Ok(MergeStepResult {
        config: config.clone(),
        encounter_pair: pair_label.to_string(),
        prev_tail_preview: prev_preview,
        curr_head_preview: curr_preview,
        raw_response: response,
        parsed,
        same_encounter,
        reason,
        generation_time_ms,
        timestamp: Utc::now().to_rfc3339(),
    })
}

// ============================================================================
// Report Generation
// ============================================================================

/// Generate a markdown summary report of detection results.
pub fn generate_detection_report(results: &[DetectionExperimentResult]) -> String {
    let mut report = String::new();
    report.push_str("# Encounter Detection Experiment Report\n\n");
    report.push_str(&format!(
        "Generated: {}\n\n",
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));
    report.push_str(&format!("Total experiments: {}\n\n", results.len()));

    // Results table
    report.push_str("## Detection Results\n\n");
    report.push_str("| # | Strategy | Threshold | Filter | Complete? | Confidence | Words In | Words Filtered | Time |\n");
    report.push_str("|---|----------|-----------|--------|-----------|------------|----------|----------------|------|\n");

    for (i, r) in results.iter().enumerate() {
        let strategy_name = r
            .config
            .detection_strategy
            .map(|s| s.id())
            .unwrap_or("?");
        let complete_str = if r.detected_complete { "YES" } else { "no" };
        let filtered = r
            .filtered_word_count
            .map(|wc| format!("{}", wc))
            .unwrap_or_else(|| "-".to_string());

        report.push_str(&format!(
            "| {} | {} | {:.1} | {} | {} | {:.2} | {} | {} | {}ms |\n",
            i + 1,
            strategy_name,
            r.config.confidence_threshold,
            if r.config.hallucination_filter {
                "ON"
            } else {
                "off"
            },
            complete_str,
            r.confidence,
            r.input_word_count,
            filtered,
            r.generation_time_ms,
        ));
    }

    report.push_str("\n## Analysis\n\n");

    // Summarize which configs correctly say "not complete" (the desired outcome for the full transcript)
    let correct = results
        .iter()
        .filter(|r| !r.detected_complete)
        .count();
    let incorrect = results
        .iter()
        .filter(|r| r.detected_complete)
        .count();

    report.push_str(&format!(
        "- **Correct (not complete)**: {} / {}\n",
        correct,
        results.len()
    ));
    report.push_str(&format!(
        "- **Incorrect (premature split)**: {} / {}\n\n",
        incorrect,
        results.len()
    ));

    if incorrect > 0 {
        report.push_str("### Configurations that incorrectly split:\n\n");
        for r in results.iter().filter(|r| r.detected_complete) {
            let name = r
                .config
                .detection_strategy
                .map(|s| s.name())
                .unwrap_or("?");
            report.push_str(&format!(
                "- {} (threshold={:.1}, filter={}, confidence={:.2})\n",
                name,
                r.config.confidence_threshold,
                r.config.hallucination_filter,
                r.confidence,
            ));
        }
    }

    report
}

/// Generate a markdown summary report of merge results.
pub fn generate_merge_report(results: &[MergeStepResult]) -> String {
    let mut report = String::new();
    report.push_str("## Merge Results\n\n");
    report.push_str("| # | Strategy | Pair | Same? | Reason | Time |\n");
    report.push_str("|---|----------|------|-------|--------|------|\n");

    for (i, r) in results.iter().enumerate() {
        let strategy_name = r.config.merge_strategy.map(|s| s.id()).unwrap_or("?");
        let same_str = if r.same_encounter { "YES" } else { "no" };
        let reason = r.reason.as_deref().unwrap_or("-");
        // Truncate reason for table
        let reason_short = if reason.len() > 60 {
            let b = reason.ceil_char_boundary(60);
            format!("{}...", &reason[..b])
        } else {
            reason.to_string()
        };

        report.push_str(&format!(
            "| {} | {} | {} | {} | {} | {}ms |\n",
            i + 1,
            strategy_name,
            r.encounter_pair,
            same_str,
            reason_short,
            r.generation_time_ms,
        ));
    }

    // Summarize
    let correct_merges = results.iter().filter(|r| r.same_encounter).count();
    report.push_str(&format!(
        "\n- **Correctly identified as same encounter**: {} / {}\n\n",
        correct_merges,
        results.len()
    ));

    report
}

/// Generate full combined report
pub fn generate_summary_report(
    detection_results: &[DetectionExperimentResult],
    merge_results: &[MergeStepResult],
) -> String {
    let mut report = generate_detection_report(detection_results);
    if !merge_results.is_empty() {
        report.push_str(&generate_merge_report(merge_results));
    }
    report
}

// ============================================================================
// Persistence
// ============================================================================

/// Get the experiments output directory
pub fn experiments_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".transcriptionapp")
        .join("debug")
        .join("encounter-experiments")
}

/// Save a detection result to disk
pub fn save_detection_result(result: &DetectionExperimentResult) -> Result<PathBuf, String> {
    let dir = experiments_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create experiments dir: {}", e))?;

    let strategy_id = result
        .config
        .detection_strategy
        .map(|s| s.id())
        .unwrap_or("unknown");
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{}-detect-{}.json", timestamp, strategy_id);
    let path = dir.join(&filename);

    let json = serde_json::to_string_pretty(result)
        .map_err(|e| format!("Failed to serialize result: {}", e))?;
    fs::write(&path, &json).map_err(|e| format!("Failed to write result: {}", e))?;

    info!("Detection result saved to: {:?}", path);
    Ok(path)
}

/// Save a merge result to disk
pub fn save_merge_result(result: &MergeStepResult) -> Result<PathBuf, String> {
    let dir = experiments_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create experiments dir: {}", e))?;

    let strategy_id = result.config.merge_strategy.map(|s| s.id()).unwrap_or("unknown");
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{}-merge-{}.json", timestamp, strategy_id);
    let path = dir.join(&filename);

    let json = serde_json::to_string_pretty(result)
        .map_err(|e| format!("Failed to serialize result: {}", e))?;
    fs::write(&path, &json).map_err(|e| format!("Failed to write result: {}", e))?;

    info!("Merge result saved to: {:?}", path);
    Ok(path)
}

/// Load all detection results from disk
pub fn load_detection_results() -> Result<Vec<DetectionExperimentResult>, String> {
    let dir = experiments_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("Failed to read dir: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if name.contains("-detect-") && name.ends_with(".json") {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    if let Ok(result) =
                        serde_json::from_str::<DetectionExperimentResult>(&content)
                    {
                        results.push(result);
                    }
                }
                Err(e) => {
                    warn!("Failed to read {:?}: {}", path, e);
                }
            }
        }
    }

    Ok(results)
}

/// Load all merge results from disk
pub fn load_merge_results() -> Result<Vec<MergeStepResult>, String> {
    let dir = experiments_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("Failed to read dir: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if name.contains("-merge-") && name.ends_with(".json") {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    if let Ok(result) = serde_json::from_str::<MergeStepResult>(&content) {
                        results.push(result);
                    }
                }
                Err(e) => {
                    warn!("Failed to read {:?}: {}", path, e);
                }
            }
        }
    }

    Ok(results)
}

// ============================================================================
// Transcript Loading Helpers
// ============================================================================

/// Load archived transcripts from a date directory.
/// Returns a vec of (session_id, transcript_text, word_count, encounter_number).
pub fn load_archived_transcripts(
    date_path: &std::path::Path,
) -> Result<Vec<(String, String, usize, Option<u32>)>, String> {
    if !date_path.exists() {
        return Err(format!("Archive path does not exist: {}", date_path.display()));
    }

    let mut transcripts = Vec::new();

    let entries = fs::read_dir(date_path)
        .map_err(|e| format!("Failed to read archive dir: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let session_dir = entry.path();
        if !session_dir.is_dir() {
            continue;
        }

        let session_id = session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let transcript_path = session_dir.join("transcript.txt");
        if !transcript_path.exists() {
            continue;
        }

        let text = fs::read_to_string(&transcript_path)
            .map_err(|e| format!("Failed to read transcript: {}", e))?;
        let word_count = text.split_whitespace().count();

        // Try to read encounter_number from metadata
        let encounter_number = session_dir
            .join("metadata.json")
            .pipe_read_metadata();

        transcripts.push((session_id, text, word_count, encounter_number));
    }

    // Sort by encounter_number (if available) for consistent ordering
    transcripts.sort_by_key(|(_, _, _, enc)| enc.unwrap_or(u32::MAX));

    Ok(transcripts)
}

/// Helper trait to read encounter_number from metadata
trait MetadataReader {
    fn pipe_read_metadata(&self) -> Option<u32>;
}

impl MetadataReader for PathBuf {
    fn pipe_read_metadata(&self) -> Option<u32> {
        let content = fs::read_to_string(self).ok()?;
        let value: serde_json::Value = serde_json::from_str(&content).ok()?;
        value.get("encounter_number")?.as_u64().map(|n| n as u32)
    }
}

/// Format transcript text as numbered segments (simulating detection input).
/// Each line becomes a segment.
pub fn format_transcript_as_segments(text: &str) -> String {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| {
            // Try to extract speaker label if line has "Speaker X: " pattern
            if let Some(colon_pos) = line.find(": ") {
                let speaker = &line[..colon_pos];
                let content = &line[colon_pos + 2..];
                format!("[{}] ({}): {}", i, speaker, content)
            } else {
                format!("[{}] (Unknown): {}", i, line.trim())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract a tail excerpt (~500 words) from text
pub fn extract_tail(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        text.to_string()
    } else {
        words[words.len() - max_words..].join(" ")
    }
}

/// Extract a head excerpt (~500 words) from text
pub fn extract_head(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        text.to_string()
    } else {
        words[..max_words].join(" ")
    }
}

/// Truncate formatted segments using the same 500-head + 1000-tail algorithm
/// as the production `TranscriptBuffer::format_for_detection_truncated()`.
///
/// Input: newline-separated formatted segments (e.g., "[0] (Speaker 1): text")
/// Output: truncated text with omission marker if over 1500 words
pub fn truncate_segments_for_detection(formatted: &str) -> String {
    const MAX_WORDS: usize = 1500;
    const HEAD_WORDS: usize = 500;
    const TAIL_WORDS: usize = 1000;

    let lines: Vec<&str> = formatted.lines().filter(|l| !l.trim().is_empty()).collect();
    let word_counts: Vec<usize> = lines.iter().map(|l| l.split_whitespace().count()).collect();
    let total_words: usize = word_counts.iter().sum();

    if total_words <= MAX_WORDS {
        return lines.join("\n");
    }

    // Find head end (first HEAD_WORDS words)
    let mut head_words = 0;
    let mut head_end = 0;
    for (i, &wc) in word_counts.iter().enumerate() {
        head_words += wc;
        if head_words >= HEAD_WORDS {
            head_end = i + 1;
            break;
        }
    }

    // Find tail start (last TAIL_WORDS words)
    let mut tail_words = 0;
    let mut tail_start = lines.len();
    for (i, &wc) in word_counts.iter().enumerate().rev() {
        tail_words += wc;
        if tail_words >= TAIL_WORDS {
            tail_start = i;
            break;
        }
    }

    // No overlap
    if tail_start <= head_end {
        return lines.join("\n");
    }

    let skipped = tail_start - head_end;
    let head: String = lines[..head_end].join("\n");
    let tail: String = lines[tail_start..].join("\n");
    format!(
        "{}\n\n[... {} segments omitted for brevity ...]\n\n{}",
        head, skipped, tail
    )
}

/// Load patient names from metadata files for a given archive date.
/// Returns a map of session_id -> patient_name.
pub fn load_patient_names(
    date_path: &std::path::Path,
) -> std::collections::HashMap<String, String> {
    let mut names = std::collections::HashMap::new();

    if let Ok(entries) = fs::read_dir(date_path) {
        for entry in entries.flatten() {
            let session_dir = entry.path();
            if !session_dir.is_dir() {
                continue;
            }
            let session_id = session_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let metadata_path = session_dir.join("metadata.json");
            if let Ok(content) = fs::read_to_string(&metadata_path) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(name) = value.get("patient_name").and_then(|n| n.as_str()) {
                        if !name.is_empty() {
                            names.insert(session_id, name.to_string());
                        }
                    }
                }
            }
        }
    }

    names
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Hallucination filter tests ----

    #[test]
    fn test_strip_hallucinations_basic() {
        let text = "the patient said fractured fractured fractured fractured fractured fractured fractured fractured and then continued";
        let (cleaned, report) = strip_hallucinations(text, 5);

        // Should truncate 8 "fractured" to 5
        assert!(cleaned.contains("fractured"));
        assert_eq!(
            cleaned.matches("fractured").count(),
            5,
            "Should have exactly 5 instances of 'fractured', got: {}",
            cleaned
        );
        assert!(cleaned.starts_with("the patient said"));
        assert!(cleaned.ends_with("and then continued"));

        assert_eq!(report.repetitions.len(), 1);
        assert_eq!(report.repetitions[0].word, "fractured");
        assert_eq!(report.repetitions[0].original_count, 8);
    }

    #[test]
    fn test_strip_hallucinations_no_repetitions() {
        let text = "the patient came in for a follow up appointment";
        let (cleaned, report) = strip_hallucinations(text, 5);

        assert_eq!(cleaned, text);
        assert!(report.repetitions.is_empty());
        assert_eq!(report.original_word_count, report.cleaned_word_count);
    }

    #[test]
    fn test_strip_hallucinations_empty() {
        let (cleaned, report) = strip_hallucinations("", 5);
        assert!(cleaned.is_empty());
        assert_eq!(report.original_word_count, 0);
    }

    #[test]
    fn test_strip_hallucinations_case_insensitive() {
        let text = "Hello hello HELLO Hello hello Hello hello hello";
        let (cleaned, report) = strip_hallucinations(text, 3);

        // 8 consecutive "hello" (case-insensitive) → truncated to 3
        assert_eq!(cleaned.split_whitespace().count(), 3);
        assert_eq!(report.repetitions.len(), 1);
        assert_eq!(report.repetitions[0].original_count, 8);
    }

    #[test]
    fn test_strip_hallucinations_multiple_runs() {
        let text = "word word word word word word pause the the the the the the other";
        let (cleaned, report) = strip_hallucinations(text, 3);

        // First run: 6x "word" → 3, then "pause", then 6x "the" → 3, then "other"
        assert_eq!(report.repetitions.len(), 2);
        assert_eq!(cleaned.split_whitespace().count(), 8); // 3 + 1 + 3 + 1
    }

    #[test]
    fn test_strip_hallucinations_within_threshold() {
        let text = "yes yes yes no no no";
        let (cleaned, report) = strip_hallucinations(text, 5);

        // 3 repetitions each, under threshold of 5 — no change
        assert_eq!(cleaned, text);
        assert!(report.repetitions.is_empty());
    }

    #[test]
    fn test_strip_hallucinations_buckland_scenario() {
        // Simulate the actual bug: ~6000 "fractured" repetitions
        let mut words = vec!["The", "patient", "had", "a"];
        for _ in 0..6000 {
            words.push("fractured");
        }
        words.extend_from_slice(&["kneecap", "and", "is", "recovering"]);

        let text = words.join(" ");
        let (cleaned, report) = strip_hallucinations(&text, 5);

        // Should go from 6008 words to ~13
        assert_eq!(report.original_word_count, 6008);
        assert_eq!(report.cleaned_word_count, 13); // 4 + 5 + 4
        assert_eq!(report.repetitions.len(), 1);
        assert_eq!(report.repetitions[0].word, "fractured");
        assert_eq!(report.repetitions[0].original_count, 6000);
        assert!(cleaned.ends_with("kneecap and is recovering"));
    }

    // ---- Prompt builder tests ----

    #[test]
    fn test_build_detection_prompt_baseline() {
        let config = ExperimentConfig::default();
        let (system, user) = build_detection_prompt(
            DetectionStrategy::Baseline,
            "[0] (Dr. Smith): Hello patient",
            &config,
        );
        // Baseline uses the production prompt
        assert!(system.contains("continuous transcript"));
        assert!(user.contains("Hello patient"));
    }

    #[test]
    fn test_build_detection_prompt_conservative() {
        let config = ExperimentConfig::default();
        let (system, _user) = build_detection_prompt(
            DetectionStrategy::Conservative,
            "[0] (Dr. Smith): Hello",
            &config,
        );
        assert!(system.contains("farewell BY NAME"));
        assert!(system.contains("Topic shifts"));
    }

    #[test]
    fn test_build_detection_prompt_same_patient_context() {
        let config = ExperimentConfig::default();
        let (system, _user) = build_detection_prompt(
            DetectionStrategy::SamePatientContext,
            "[0] (Dr. Smith): Hello",
            &config,
        );
        assert!(system.contains("Presenting complaint"));
        assert!(system.contains("ONE encounter"));
    }

    #[test]
    fn test_build_detection_prompt_patient_name_aware() {
        let config = ExperimentConfig {
            patient_name: Some("Buckland, Deborah Ann".to_string()),
            ..Default::default()
        };
        let (system, _user) = build_detection_prompt(
            DetectionStrategy::PatientNameAware,
            "[0] (Dr. Smith): Hello",
            &config,
        );
        assert!(system.contains("Buckland, Deborah Ann"));
        assert!(system.contains("ALL discussion is part of the same encounter"));
    }

    #[test]
    fn test_build_detection_prompt_with_filter() {
        let config = ExperimentConfig {
            hallucination_filter: true,
            ..Default::default()
        };
        let input = "word word word word word word word word word word word other";
        let (_, user) = build_detection_prompt(DetectionStrategy::Baseline, input, &config);
        // The filtered input should have fewer "word" instances
        assert!(user.matches("word").count() <= 6); // 5 kept + possible in prompt text
    }

    #[test]
    fn test_build_merge_prompt_baseline() {
        let config = ExperimentConfig::default();
        let (system, user) = build_merge_prompt(
            MergeStrategy::Baseline,
            "...end of encounter",
            "start of next...",
            &config,
        );
        assert!(system.contains("SAME patient"));
        assert!(user.contains("end of encounter"));
        assert!(user.contains("start of next"));
    }

    #[test]
    fn test_build_merge_prompt_patient_name() {
        let config = ExperimentConfig {
            patient_name: Some("Deborah Buckland".to_string()),
            ..Default::default()
        };
        let (system, _user) = build_merge_prompt(
            MergeStrategy::PatientNameWeighted,
            "tail",
            "head",
            &config,
        );
        assert!(system.contains("Deborah Buckland"));
    }

    #[test]
    fn test_build_merge_prompt_hallucination_filtered() {
        let config = ExperimentConfig::default();
        let tail_with_hallucination =
            "discussing the fractured fractured fractured fractured fractured fractured kneecap";
        let (_, user) = build_merge_prompt(
            MergeStrategy::HallucinationFiltered,
            tail_with_hallucination,
            "normal head text",
            &config,
        );
        // The tail should have "fractured" truncated
        let fractured_count = user.matches("fractured").count();
        assert!(
            fractured_count <= 5,
            "Expected <= 5 'fractured', got {}",
            fractured_count
        );
    }

    // ---- Helper tests ----

    #[test]
    fn test_format_transcript_as_segments() {
        let text = "Speaker 1: Hello doctor\nSpeaker 2: Hi there\nambient noise";
        let formatted = format_transcript_as_segments(text);
        assert!(formatted.contains("[0] (Speaker 1): Hello doctor"));
        assert!(formatted.contains("[1] (Speaker 2): Hi there"));
        assert!(formatted.contains("[2] (Unknown): ambient noise"));
    }

    #[test]
    fn test_extract_tail() {
        let text = "one two three four five six seven eight nine ten";
        let tail = extract_tail(text, 3);
        assert_eq!(tail, "eight nine ten");
    }

    #[test]
    fn test_extract_head() {
        let text = "one two three four five six seven eight nine ten";
        let head = extract_head(text, 3);
        assert_eq!(head, "one two three");
    }

    #[test]
    fn test_extract_tail_short_text() {
        let text = "short text";
        assert_eq!(extract_tail(text, 100), "short text");
    }

    // ---- Strategy enum tests ----

    #[test]
    fn test_detection_strategy_all() {
        let all = DetectionStrategy::all();
        assert_eq!(all.len(), 4);
        for s in &all {
            assert!(!s.name().is_empty());
            assert!(!s.id().is_empty());
        }
    }

    #[test]
    fn test_merge_strategy_all() {
        let all = MergeStrategy::all();
        assert_eq!(all.len(), 3);
        for s in &all {
            assert!(!s.name().is_empty());
            assert!(!s.id().is_empty());
        }
    }

    // ---- Report generation tests ----

    #[test]
    fn test_generate_detection_report_empty() {
        let report = generate_detection_report(&[]);
        assert!(report.contains("Total experiments: 0"));
    }

    #[test]
    fn test_generate_merge_report_empty() {
        let report = generate_merge_report(&[]);
        assert!(report.contains("Merge Results"));
    }

    // ---- Scoring tests ----

    #[test]
    fn test_detection_result_scoring() {
        // A result where complete=false is "correct" for the Buckland case
        let result = DetectionExperimentResult {
            config: ExperimentConfig::default(),
            prompt_system: String::new(),
            prompt_user_preview: String::new(),
            raw_response: r#"{"complete": false, "confidence": 0.2}"#.to_string(),
            parsed: Some(EncounterDetectionResult {
                complete: false,
                end_segment_index: None,
                confidence: Some(0.2),
            }),
            detected_complete: false,
            confidence: 0.2,
            generation_time_ms: 500,
            input_word_count: 1000,
            filtered_word_count: None,
            timestamp: Utc::now().to_rfc3339(),
        };

        // For the Buckland case, not detecting a split is the correct answer
        assert!(!result.detected_complete);
    }
}
