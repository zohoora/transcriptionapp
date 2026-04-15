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
    build_encounter_detection_prompt, parse_encounter_detection,
    EncounterDetectionResult, MergeCheckResult,
};
use crate::encounter_merge::{build_encounter_merge_prompt, parse_merge_check};
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
    /// Inject sensor-departed prompt context for the Baseline strategy.
    /// Production passes `Some(&ctx)` to `build_encounter_detection_prompt`
    /// when the sensor went absent; setting this to true makes the experiment
    /// match production's prompt for that scenario. Defaults false → matches
    /// legacy behavior (Baseline always called with `None` context).
    #[serde(default)]
    pub sensor_departed: bool,
    /// Inject sensor-confirmed-present prompt context for the Baseline strategy.
    /// Defaults false → matches legacy behavior.
    #[serde(default)]
    pub sensor_present: bool,
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
            sensor_departed: false,
            sensor_present: false,
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
    /// Multi-word phrase repetitions (phase 2)
    #[serde(default)]
    pub phrase_repetitions: Vec<PhraseHallucinationEntry>,
    pub original_word_count: usize,
    pub cleaned_word_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HallucinationEntry {
    pub word: String,
    pub original_count: usize,
    pub position: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhraseHallucinationEntry {
    pub phrase: String,
    pub ngram_size: usize,
    pub original_count: usize,
    pub position: usize,
}

// ============================================================================
// Hallucination Filter
// ============================================================================

/// Stateless filler phrases that the STT model (Qwen3-ASR) emits when given
/// zero-amplitude or noise-only input. These are short, semantically empty
/// strings that have no relationship to the actual audio content — they're
/// the model's fallback when it has nothing to transcribe.
///
/// The comparison is case-insensitive and whitespace-insensitive. A segment
/// whose entire `text.trim()` (after lowering and collapsing whitespace)
/// exactly matches one of these is dropped.
///
/// NEW ones observed in production should be added here — test with the
/// problematic audio via `stt-server` (`POST /v1/audio/transcribe/regular-batch`
/// with `language=English`) to reproduce before adding.
pub const QWEN_STATELESS_FILLERS: &[&str] = &[
    // English fillers
    "i'm not sure.",
    "i'm not sure",
    "i don't know.",
    "i don't know",
    "thank you.",
    "thank you",
    "thanks.",
    "thanks",
    // Hindi fillers (Devanagari "hmm", "yes", "yeah")
    "हम्म",
    "हूं।",
    "हूं",
    "हाँ।",
    "हाँ",
    // Punctuation-only or single-char outputs
    ".",
    ",",
    "?",
    "!",
];

/// Check if a segment's text is a stateless filler that should be dropped.
/// Returns true for case-insensitive matches against `QWEN_STATELESS_FILLERS`
/// on the trimmed, whitespace-collapsed input.
pub fn is_stateless_filler(text: &str) -> bool {
    let normalized = text
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        return false; // empty isn't filler, it's just silence
    }
    QWEN_STATELESS_FILLERS
        .iter()
        .any(|filler| normalized == *filler)
}

/// Two-phase hallucination detection and truncation:
/// Phase 0: Stateless filler phrases (Qwen silence artifacts like "I'm not sure.")
/// Phase 1: Single-word consecutive repetitions (e.g., "fractured" ×6000)
/// Phase 2: Multi-word phrase loops (e.g., 20-word phrase ×75)
///
/// Returns (cleaned_text, hallucination_report).
pub fn strip_hallucinations(text: &str, max_consecutive: usize) -> (String, HallucinationReport) {
    let original_word_count = text.split_whitespace().count();

    if original_word_count == 0 {
        return (
            String::new(),
            HallucinationReport {
                repetitions: Vec::new(),
                phrase_repetitions: Vec::new(),
                original_word_count: 0,
                cleaned_word_count: 0,
            },
        );
    }

    // Phase 0: If the ENTIRE text is a stateless filler, drop it to empty.
    // This handles the common case where an utterance is short and consists
    // entirely of a Qwen silence artifact (e.g., a VAD-gated "I'm not sure."
    // chunk with no other content). Longer utterances that merely contain
    // the filler as a subphrase are left alone; the LLM downstream can
    // decide how to handle those.
    if is_stateless_filler(text) {
        return (
            String::new(),
            HallucinationReport {
                repetitions: Vec::new(),
                phrase_repetitions: Vec::new(),
                original_word_count,
                cleaned_word_count: 0,
            },
        );
    }

    // Phase 1: Strip single-word repetitions
    let (phase1_text, single_reps) = strip_single_word_hallucinations(text, max_consecutive);

    // Phase 2: Strip multi-word phrase loops
    let (cleaned_text, phrase_reps) = strip_phrase_hallucinations(&phase1_text, max_consecutive);

    let cleaned_word_count = cleaned_text.split_whitespace().count();

    (
        cleaned_text,
        HallucinationReport {
            repetitions: single_reps,
            phrase_repetitions: phrase_reps,
            original_word_count,
            cleaned_word_count,
        },
    )
}

/// Phase 1: Detect and truncate consecutive single-word repetitions.
fn strip_single_word_hallucinations(text: &str, max_consecutive: usize) -> (String, Vec<HallucinationEntry>) {
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.is_empty() {
        return (String::new(), Vec::new());
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

    (result_words.join(" "), repetitions)
}

/// Phase 2: Detect and truncate consecutive multi-word phrase (n-gram) loops.
///
/// Scans for n-grams of sizes 3-15 words. For each size (largest first),
/// finds runs of consecutive identical n-grams. If a phrase repeats more than
/// `max_consecutive` times in a row, truncates to `max_consecutive` occurrences.
fn strip_phrase_hallucinations(text: &str, max_consecutive: usize) -> (String, Vec<PhraseHallucinationEntry>) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut phrase_reps: Vec<PhraseHallucinationEntry> = Vec::new();

    if words.len() < 6 {
        // Need at least 2× the minimum n-gram (3) to detect a repeat
        return (text.to_string(), phrase_reps);
    }

    // Work with owned words so we can mutate across passes
    let mut current_words: Vec<String> = words.iter().map(|w| w.to_string()).collect();

    // Scan from largest n-gram to smallest to catch broad patterns first.
    // Range 3-25 covers real-world STT loops (e.g., 20-word phrase ×75).
    for ngram_size in (3..=25).rev() {
        if current_words.len() < ngram_size * 2 {
            continue;
        }

        let mut result: Vec<String> = Vec::with_capacity(current_words.len());
        let mut i = 0;

        while i < current_words.len() {
            if i + ngram_size > current_words.len() {
                // Not enough words left for an n-gram — copy remainder
                result.extend_from_slice(&current_words[i..]);
                break;
            }

            // Build the n-gram at position i (lowercased for comparison)
            let ngram: Vec<String> = current_words[i..i + ngram_size]
                .iter()
                .map(|w| w.to_lowercase())
                .collect();

            // Count consecutive identical n-grams
            let mut run_count = 1;
            let mut pos = i + ngram_size;
            while pos + ngram_size <= current_words.len() {
                let next_ngram: Vec<String> = current_words[pos..pos + ngram_size]
                    .iter()
                    .map(|w| w.to_lowercase())
                    .collect();
                if next_ngram == ngram {
                    run_count += 1;
                    pos += ngram_size;
                } else {
                    break;
                }
            }

            if run_count > max_consecutive {
                // Truncate: keep only max_consecutive occurrences
                for _ in 0..max_consecutive {
                    result.extend_from_slice(&current_words[i..i + ngram_size]);
                }
                phrase_reps.push(PhraseHallucinationEntry {
                    phrase: ngram.join(" "),
                    ngram_size,
                    original_count: run_count,
                    position: i,
                });
                i += run_count * ngram_size;
            } else {
                // Not a hallucination — emit one word and advance by one
                // (sliding window, not jumping by n-gram size)
                result.push(current_words[i].clone());
                i += 1;
            }
        }

        current_words = result;
    }

    (current_words.join(" "), phrase_reps)
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
            // Use the production prompt directly. When sensor flags are set,
            // build a context matching what production would inject so the
            // Baseline experiment matches production's actual prompt for
            // sensor-active scenarios. When neither flag is set, pass None
            // for byte-identical legacy behavior.
            let ctx = if config.sensor_departed || config.sensor_present {
                Some(crate::encounter_detection::EncounterDetectionContext {
                    sensor_departed: config.sensor_departed,
                    sensor_present: config.sensor_present,
                })
            } else {
                None
            };
            build_encounter_detection_prompt(&segments, ctx.as_ref(), None)
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
            build_encounter_merge_prompt(&clean_prev, &clean_curr, None, None)
        }
        MergeStrategy::PatientNameWeighted => {
            // Use the production prompt with patient name injection (M1 strategy)
            build_encounter_merge_prompt(
                &clean_prev,
                &clean_curr,
                config.patient_name.as_deref(),
                None,
            )
        }
        MergeStrategy::HallucinationFiltered => {
            // Same prompt as baseline, but excerpts are pre-filtered (done above)
            build_encounter_merge_prompt(&clean_prev, &clean_curr, None, None)
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
// Bundle-based Encounter Loading (Tier 5a)
// ============================================================================
//
// The encounter_experiment tool reads continuous-mode archives via the
// `replay_bundle.json` files. Session-mode archives that never had encounter
// detection (and therefore no bundle) are silently skipped.
//
// Why bundle-only? Production's encounter detection prompt explicitly says
// "Each segment includes elapsed time (MM:SS)" — flat `transcript.txt` files
// don't carry per-segment timestamps, so they can't be reformatted into the
// production format. The bundle has `ReplaySegment { index, start_ms, ... }`
// which IS the same data shape production uses, so we can produce
// byte-identical prompts to what the live pipeline would have built.

/// One archived encounter, loaded from `replay_bundle.json`.
#[derive(Debug, Clone)]
pub struct ArchivedEncounter {
    pub session_id: String,
    /// Plain transcript text (concatenated segment texts) for tail/head excerpts.
    pub plain_text: String,
    /// All segments from the bundle. Callers reformat as needed via
    /// `format_replay_segments_for_detection` — typically after concatenating
    /// multiple encounters' segments and re-indexing.
    pub segments: Vec<crate::replay_bundle::ReplaySegment>,
    pub word_count: usize,
    pub encounter_number: Option<u32>,
    pub patient_name: Option<String>,
}

/// Format a slice of `ReplaySegment` into the same detection-prompt format
/// as production's `format_segments_for_detection` in
/// `transcript_buffer.rs:50`. Output: `[index] (MM:SS) (Speaker Label): text`.
///
/// Drift protection: a unit test (below) builds matched `ReplaySegment` and
/// `BufferedSegment` slices and asserts both formatters produce byte-identical
/// output. If `transcript_buffer::format_segments_for_detection` ever changes,
/// the test breaks and we update both.
pub fn format_replay_segments_for_detection(
    segments: &[crate::replay_bundle::ReplaySegment],
) -> String {
    let first_start_ms = segments.first().map(|s| s.start_ms).unwrap_or(0);
    segments
        .iter()
        .map(|s| {
            let elapsed_ms = s.start_ms.saturating_sub(first_start_ms);
            let total_secs = elapsed_ms / 1000;
            let hours = total_secs / 3600;
            let minutes = (total_secs % 3600) / 60;
            let seconds = total_secs % 60;
            let elapsed = if hours > 0 {
                format!("{}:{:02}:{:02}", hours, minutes, seconds)
            } else {
                format!("{:02}:{:02}", minutes, seconds)
            };
            let speaker_label = match (s.speaker_id.as_deref(), s.speaker_confidence) {
                (Some(spk), Some(conf)) => format!("{} ({:.0}%)", spk, conf * 100.0),
                (Some(spk), None) => spk.to_string(),
                _ => "Unknown".to_string(),
            };
            format!("[{}] ({}) ({}): {}", s.index, elapsed, speaker_label, s.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Load archived encounters from a date directory.
///
/// Reads `replay_bundle.json` from each session directory (continuous-mode
/// only). Session-mode archives without a bundle are silently skipped.
///
/// Returns encounters sorted by `encounter_number` then by `session_id` for
/// stable ordering.
pub fn load_archived_encounters(
    date_path: &std::path::Path,
) -> Result<Vec<ArchivedEncounter>, String> {
    if !date_path.exists() {
        return Err(format!("Archive path does not exist: {}", date_path.display()));
    }

    let mut encounters = Vec::new();
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

        // Bundle-only: skip session-mode archives.
        let bundle_path = session_dir.join("replay_bundle.json");
        if !bundle_path.exists() {
            continue;
        }
        let bundle_json = match fs::read_to_string(&bundle_path) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to read bundle {}: {}", bundle_path.display(), e);
                continue;
            }
        };
        let bundle: crate::replay_bundle::ReplayBundle = match serde_json::from_str(&bundle_json) {
            Ok(b) => b,
            Err(e) => {
                warn!("Failed to parse bundle {}: {}", bundle_path.display(), e);
                continue;
            }
        };

        let plain_text: String = bundle
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let word_count = plain_text.split_whitespace().count();
        let encounter_number = bundle.outcome.as_ref().map(|o| o.encounter_number);
        let patient_name = bundle.outcome.as_ref().and_then(|o| o.patient_name.clone());

        encounters.push(ArchivedEncounter {
            session_id,
            plain_text,
            segments: bundle.segments,
            word_count,
            encounter_number,
            patient_name,
        });
    }

    // Sort by (encounter_number, session_id) for stable ordering
    encounters.sort_by_key(|e| (e.encounter_number.unwrap_or(u32::MAX), e.session_id.clone()));
    Ok(encounters)
}

/// Extract a tail excerpt (~500 words) from text. Used for merge experiments.
pub fn extract_tail(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        text.to_string()
    } else {
        words[words.len() - max_words..].join(" ")
    }
}

/// Extract a head excerpt (~500 words) from text. Used for merge experiments.
pub fn extract_head(text: &str, max_words: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= max_words {
        text.to_string()
    } else {
        words[..max_words].join(" ")
    }
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
    fn test_stateless_filler_is_filler() {
        // Exact matches (case / whitespace insensitive)
        assert!(is_stateless_filler("I'm not sure."));
        assert!(is_stateless_filler("I'M NOT SURE."));
        assert!(is_stateless_filler("  i'm not sure  "));
        assert!(is_stateless_filler("I'm not sure"));
        assert!(is_stateless_filler("Thank you."));
        assert!(is_stateless_filler("thanks"));
        assert!(is_stateless_filler("हम्म"));
        assert!(is_stateless_filler("."));

        // NOT fillers — real content, or filler as a subphrase of a longer utterance
        assert!(!is_stateless_filler("I'm not sure about the diagnosis"));
        assert!(!is_stateless_filler("Thank you for coming in today"));
        assert!(!is_stateless_filler("the patient is stable"));
        assert!(!is_stateless_filler("")); // empty is not filler, just silence
    }

    #[test]
    fn test_strip_hallucinations_drops_stateless_filler() {
        // A segment consisting entirely of a Qwen silence artifact should be
        // stripped to empty, not passed downstream.
        let (cleaned, report) = strip_hallucinations("I'm not sure.", 5);
        assert_eq!(cleaned, "");
        assert_eq!(report.cleaned_word_count, 0);
        assert_eq!(report.original_word_count, 3);

        let (cleaned2, _) = strip_hallucinations("हम्म", 5);
        assert_eq!(cleaned2, "");

        let (cleaned3, _) = strip_hallucinations("  Thank you.  ", 5);
        assert_eq!(cleaned3, "");
    }

    #[test]
    fn test_strip_hallucinations_preserves_filler_in_context() {
        // When the filler appears inside a longer utterance, we leave it
        // alone — the downstream LLM decides how to handle it.
        let text = "Thank you for coming in today Mrs Johnson";
        let (cleaned, _) = strip_hallucinations(text, 5);
        assert_eq!(cleaned, text);
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

    // ---- Phrase hallucination filter tests ----

    #[test]
    fn test_strip_phrase_hallucinations_basic() {
        // 3-word phrase repeated 8 times (exceeds max_consecutive=5)
        let phrase = "the patient said";
        let mut words: Vec<&str> = vec!["hello"];
        for _ in 0..8 {
            words.extend_from_slice(&["the", "patient", "said"]);
        }
        words.push("goodbye");
        let text = words.join(" ");

        let (cleaned, report) = strip_hallucinations(&text, 5);

        // Should truncate 8 occurrences to 5
        assert_eq!(
            cleaned.matches(phrase).count(),
            5,
            "Should have exactly 5 instances of phrase, got: {}",
            cleaned
        );
        assert!(cleaned.starts_with("hello"));
        assert!(cleaned.ends_with("goodbye"));
        assert_eq!(report.phrase_repetitions.len(), 1);
        assert_eq!(report.phrase_repetitions[0].ngram_size, 3);
        assert_eq!(report.phrase_repetitions[0].original_count, 8);
    }

    #[test]
    fn test_strip_phrase_hallucinations_no_repetitions() {
        let text = "the patient came in for a follow up appointment and was discharged home";
        let (cleaned, report) = strip_hallucinations(text, 5);

        assert_eq!(cleaned, text);
        assert!(report.phrase_repetitions.is_empty());
    }

    #[test]
    fn test_strip_phrase_hallucinations_within_threshold() {
        // 3-word phrase repeated 4 times (under max_consecutive=5)
        let mut words: Vec<&str> = Vec::new();
        for _ in 0..4 {
            words.extend_from_slice(&["the", "patient", "said"]);
        }
        let text = words.join(" ");
        let (cleaned, report) = strip_hallucinations(&text, 5);

        assert_eq!(cleaned, text);
        assert!(report.phrase_repetitions.is_empty());
    }

    #[test]
    fn test_strip_phrase_hallucinations_large_ngram() {
        // Simulate the Feb 27 bug: 20-word phrase repeated 75 times
        let phrase_words = vec![
            "so", "we", "will", "continue", "to", "monitor", "your", "blood",
            "pressure", "and", "adjust", "the", "medication", "as", "needed",
            "please", "follow", "up", "in", "two",
        ];
        let mut words: Vec<&str> = vec!["Doctor", "says"];
        for _ in 0..75 {
            words.extend_from_slice(&phrase_words);
        }
        words.extend_from_slice(&["weeks", "from", "now"]);

        let text = words.join(" ");
        let original_wc = text.split_whitespace().count();
        assert_eq!(original_wc, 2 + 75 * 20 + 3); // 1505 words

        let (cleaned, report) = strip_hallucinations(&text, 5);

        // Should truncate 75 occurrences to 5
        let cleaned_wc = cleaned.split_whitespace().count();
        assert_eq!(cleaned_wc, 2 + 5 * 20 + 3); // 105 words
        assert!(cleaned.starts_with("Doctor says"));
        assert!(cleaned.ends_with("weeks from now"));
        assert_eq!(report.phrase_repetitions.len(), 1);
        assert_eq!(report.phrase_repetitions[0].original_count, 75);
    }

    #[test]
    fn test_strip_phrase_hallucinations_combined_with_single_word() {
        // Both single-word AND phrase hallucinations in the same text
        let mut words: Vec<&str> = vec!["start"];
        // Single-word loop: "okay" x 10
        for _ in 0..10 {
            words.push("okay");
        }
        words.push("middle");
        // Phrase loop: "see you soon" x 8
        for _ in 0..8 {
            words.extend_from_slice(&["see", "you", "soon"]);
        }
        words.push("end");
        let text = words.join(" ");

        let (cleaned, report) = strip_hallucinations(&text, 5);

        // Single-word: 10 → 5
        assert_eq!(report.repetitions.len(), 1);
        assert_eq!(report.repetitions[0].word, "okay");
        // Phrase: 8 → 5
        assert_eq!(report.phrase_repetitions.len(), 1);
        assert_eq!(report.phrase_repetitions[0].phrase, "see you soon");
        assert!(cleaned.starts_with("start"));
        assert!(cleaned.ends_with("end"));
    }

    #[test]
    fn test_strip_phrase_hallucinations_short_text_no_crash() {
        // Text too short for any phrase detection
        let text = "hi there";
        let (cleaned, report) = strip_hallucinations(text, 5);
        assert_eq!(cleaned, text);
        assert!(report.phrase_repetitions.is_empty());
    }

    #[test]
    fn test_strip_phrase_hallucinations_3word_loop_with_speaker_label() {
        // Exact pattern from March 2 clinic: "Speaker 3: At what time?" repeated 2048 times
        // The speaker label at the start shouldn't prevent n-gram detection of the loop
        let mut text = String::from("Speaker 3:");
        for _ in 0..100 {
            text.push_str(" At what time?");
        }
        let (cleaned, report) = strip_hallucinations(&text, 5);
        // Should truncate to ~5 repetitions of the 3-word phrase + speaker label
        assert!(report.phrase_repetitions.len() > 0, "Should detect phrase repetitions");
        let cleaned_words: Vec<&str> = cleaned.split_whitespace().collect();
        // 2 words (Speaker 3:) + 5 * 3 (five repetitions of "At what time?") = 17
        assert!(cleaned_words.len() <= 20, "Should truncate to roughly 17 words, got {}", cleaned_words.len());
    }

    #[test]
    fn test_strip_phrase_hallucinations_5word_loop_with_prefix() {
        // Pattern from March 2: "...It was just a little bit of congestion, a little bit of congestion, ..."
        let mut text = String::from("It was just");
        for _ in 0..100 {
            text.push_str(" a little bit of congestion,");
        }
        let (cleaned, report) = strip_hallucinations(&text, 5);
        assert!(report.phrase_repetitions.len() > 0, "Should detect phrase repetitions");
        let cleaned_words: Vec<&str> = cleaned.split_whitespace().collect();
        // "It was just" (3 words) + 5 * 5 words = 28
        assert!(cleaned_words.len() <= 30, "Should truncate loop, got {}", cleaned_words.len());
    }

    #[test]
    fn test_strip_phrase_hallucinations_multi_segment_with_speaker_labels() {
        // If multiple segments each have speaker labels interspersed in the loop,
        // the filter should still catch intra-segment loops
        let text = "Speaker 3: At what time? At what time? At what time? At what time? At what time? At what time? At what time? At what time? At what time? At what time? \
                    Speaker 3: At what time? At what time? At what time? At what time? At what time? At what time? At what time? At what time? At what time? At what time?";
        let (cleaned, report) = strip_hallucinations(text, 5);
        assert!(report.phrase_repetitions.len() > 0, "Should detect phrase repetitions even with speaker labels");
        let cleaned_words: Vec<&str> = cleaned.split_whitespace().collect();
        // Each segment: 2 (Speaker 3:) + 5 kept * 3 = 17 per segment, × 2 = 34
        assert!(cleaned_words.len() <= 40, "Should truncate each segment's loop, got {}", cleaned_words.len());
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

    // ---- Bundle loader / format helper tests (Tier 5a) ----

    fn make_replay_segment(
        index: u64,
        start_ms: u64,
        text: &str,
        speaker: Option<&str>,
        conf: Option<f32>,
    ) -> crate::replay_bundle::ReplaySegment {
        crate::replay_bundle::ReplaySegment {
            ts: "2026-04-15T10:00:00Z".to_string(),
            index,
            start_ms,
            end_ms: start_ms + 1000,
            text: text.to_string(),
            speaker_id: speaker.map(|s| s.to_string()),
            speaker_confidence: conf,
        }
    }

    /// Drift protection: format_replay_segments_for_detection must produce
    /// byte-identical output to production's format_segments_for_detection.
    /// If `transcript_buffer.rs::format_segments_for_detection` ever changes,
    /// this test breaks and we must update both implementations in lockstep.
    #[test]
    fn test_format_replay_segments_matches_production_format() {
        use crate::transcript_buffer::{format_segments_for_detection, BufferedSegment};
        use chrono::Utc;

        let now = Utc::now();
        let buffered = vec![
            BufferedSegment {
                index: 0,
                start_ms: 0,
                timestamp_ms: 1000,
                started_at: now,
                text: "Hello doctor".into(),
                speaker_id: Some("Speaker 1".into()),
                speaker_confidence: Some(0.92),
                generation: 0,
            },
            BufferedSegment {
                index: 1,
                start_ms: 8000,
                timestamp_ms: 9000,
                started_at: now,
                text: "Vitals look fine.".into(),
                speaker_id: Some("Speaker 2".into()),
                speaker_confidence: Some(0.65),
                generation: 0,
            },
            BufferedSegment {
                index: 2,
                start_ms: 35_000,
                timestamp_ms: 36_000,
                started_at: now,
                text: "Any other concerns?".into(),
                speaker_id: None,
                speaker_confidence: None,
                generation: 0,
            },
        ];
        let replay = vec![
            make_replay_segment(0, 0, "Hello doctor", Some("Speaker 1"), Some(0.92)),
            make_replay_segment(1, 8000, "Vitals look fine.", Some("Speaker 2"), Some(0.65)),
            make_replay_segment(2, 35_000, "Any other concerns?", None, None),
        ];
        let production_output = format_segments_for_detection(&buffered);
        let experiment_output = format_replay_segments_for_detection(&replay);
        assert_eq!(
            production_output, experiment_output,
            "format_replay_segments_for_detection must produce byte-identical output to \
             transcript_buffer::format_segments_for_detection. If this test fails, both \
             functions need updating in lockstep."
        );
    }

    #[test]
    fn test_format_replay_segments_emits_mmss() {
        let segments = vec![make_replay_segment(0, 0, "Hello", Some("Speaker 1"), Some(0.92))];
        let formatted = format_replay_segments_for_detection(&segments);
        assert!(formatted.contains("[0] (00:00) (Speaker 1 (92%)): Hello"));
    }

    #[test]
    fn test_format_replay_segments_emits_hours_for_long_recordings() {
        // start_ms = 1 hour, 5 min, 30 sec = 3_930_000 ms
        // first_start_ms is the same, so elapsed = 0 → "00:00"
        let segments = vec![make_replay_segment(0, 3_930_000, "Still here", None, None)];
        let formatted = format_replay_segments_for_detection(&segments);
        assert!(formatted.contains("[0] (00:00) (Unknown): Still here"));
    }

    #[test]
    fn test_format_replay_segments_relative_elapsed_time() {
        // Two segments: first at 0ms, second at 65000ms (1:05). The second
        // should display as (01:05) because elapsed is computed from the first.
        let segments = vec![
            make_replay_segment(0, 0, "First", Some("Speaker 1"), None),
            make_replay_segment(1, 65_000, "Second", Some("Speaker 1"), None),
        ];
        let formatted = format_replay_segments_for_detection(&segments);
        assert!(formatted.contains("[0] (00:00) (Speaker 1): First"));
        assert!(formatted.contains("[1] (01:05) (Speaker 1): Second"));
    }

    #[test]
    fn test_format_replay_segments_unknown_speaker() {
        let segments = vec![make_replay_segment(0, 0, "ambient noise", None, None)];
        let formatted = format_replay_segments_for_detection(&segments);
        assert!(formatted.contains("[0] (00:00) (Unknown): ambient noise"));
    }

    #[test]
    fn test_format_replay_segments_empty() {
        let formatted = format_replay_segments_for_detection(&[]);
        assert_eq!(formatted, "");
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

    // ---- Baseline sensor context tests (Tier 5b) ----

    #[test]
    fn test_baseline_no_sensor_matches_legacy() {
        // Backward compat: when neither sensor flag is set, the Baseline strategy
        // produces byte-identical output to a direct call with `None` context.
        let cfg = ExperimentConfig::default();
        let (sys_new, user_new) = build_detection_prompt(
            DetectionStrategy::Baseline,
            "[0] (Speaker 1): hello",
            &cfg,
        );
        let (sys_legacy, user_legacy) = build_encounter_detection_prompt("[0] (Speaker 1): hello", None, None);
        assert_eq!(sys_new, sys_legacy);
        assert_eq!(user_new, user_legacy);
    }

    #[test]
    fn test_baseline_sensor_departed_includes_context() {
        let cfg = ExperimentConfig {
            sensor_departed: true,
            ..ExperimentConfig::default()
        };
        let (system, user) = build_detection_prompt(
            DetectionStrategy::Baseline,
            "[0] (Speaker 1): hello",
            &cfg,
        );
        // Production injects "Real-time context signals" framing into the user prompt
        // when sensor_departed is true. Cross-check by building the production prompt
        // directly and asserting equivalence.
        let ctx = crate::encounter_detection::EncounterDetectionContext {
            sensor_departed: true,
            sensor_present: false,
        };
        let (sys_prod, user_prod) =
            build_encounter_detection_prompt("[0] (Speaker 1): hello", Some(&ctx), None);
        assert_eq!(system, sys_prod);
        assert_eq!(user, user_prod);
        // And confirm the context section is actually present
        assert!(user.contains("Real-time context signals") || user.contains("CONTEXT:"));
    }

    #[test]
    fn test_baseline_sensor_present_includes_context() {
        let cfg = ExperimentConfig {
            sensor_present: true,
            ..ExperimentConfig::default()
        };
        let (system, user) = build_detection_prompt(
            DetectionStrategy::Baseline,
            "[0] (Speaker 1): hello",
            &cfg,
        );
        let ctx = crate::encounter_detection::EncounterDetectionContext {
            sensor_departed: false,
            sensor_present: true,
        };
        let (sys_prod, user_prod) =
            build_encounter_detection_prompt("[0] (Speaker 1): hello", Some(&ctx), None);
        assert_eq!(system, sys_prod);
        assert_eq!(user, user_prod);
        assert!(user.contains("still in the room"));
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
