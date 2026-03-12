//! Encounter detection logic for continuous mode.
//!
//! Provides the LLM prompt construction and response parsing for detecting
//! transition points between patient encounters in a continuous transcript.

use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Word count forcing encounter check regardless of buffer age.
pub const FORCE_CHECK_WORD_THRESHOLD: usize = 3000;
/// Force-split when buffer exceeds this AND consecutive LLM failures >= limit.
/// Only counts LLM errors/timeouts, not confident "no split" responses.
pub const FORCE_SPLIT_WORD_THRESHOLD: usize = 5000;
/// Consecutive LLM failure cycles before force-split (at FORCE_SPLIT_WORD_THRESHOLD).
pub const FORCE_SPLIT_CONSECUTIVE_LIMIT: u32 = 3;
/// Unconditional force-split -- hard safety valve, no counter needed.
/// Set high enough for ~5 hour single-patient sessions (~70 words/min).
pub const ABSOLUTE_WORD_CAP: usize = 25_000;
/// Minimum word count for clinical content check + SOAP generation.
/// Encounters below this threshold are treated as non-clinical (still archived with transcript).
pub const MIN_WORDS_FOR_CLINICAL_CHECK: usize = 100;
/// Grace period (seconds) after encounter split during which screenshot votes matching the
/// previous encounter's patient name are suppressed (stale screenshot detection).
pub const SCREENSHOT_STALE_GRACE_SECS: i64 = 90;
/// Minimum merged word count to trigger retrospective multi-patient check after merge-back.
pub const MULTI_PATIENT_CHECK_WORD_THRESHOLD: usize = 2500;
/// Minimum words per half for a retrospective split to be accepted (size gate).
pub const MULTI_PATIENT_SPLIT_MIN_WORDS: usize = 500;

/// Optional context signals for encounter detection.
/// Provides real-time signals from sensor (departure/presence) to augment
/// the LLM prompt. Vision-extracted patient names are used only for metadata
/// labeling, NOT for split decisions (EMR chart name is unreliable — doctor
/// may open family members, not open chart, or vision may parse same name
/// differently).
#[derive(Debug, Clone, Default)]
pub struct EncounterDetectionContext {
    /// Whether the presence sensor detected someone left the room
    pub sensor_departed: bool,
    /// Whether the presence sensor confirms someone is still in the room
    pub sensor_present: bool,
}

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

/// Build the encounter detection prompt.
/// Accepts optional context signals from vision and sensor to improve accuracy.
pub fn build_encounter_detection_prompt(
    formatted_segments: &str,
    context: Option<&EncounterDetectionContext>,
) -> (String, String) {
    let system = r#"You MUST respond in English with ONLY a JSON object. No other text, no explanations, no markdown.

You are analyzing a continuous transcript from a medical office where the microphone records all day.

Your task: determine if there is a TRANSITION POINT where one patient encounter ends and another begins, or where a patient encounter has clearly concluded.

A completed encounter typically includes a clinical discussion and a concluding plan (e.g., medication changes, follow-up timing, referrals, or instructions). A short transcript that contains only vitals, greetings, or brief exchanges — with no clinical discussion or plan — is likely a pre-visit assessment still in progress, not a completed encounter. A nurse or medical assistant may see the patient before the doctor arrives to take vitals or ask initial questions — this is part of the same encounter, not a separate visit.

Each segment includes elapsed time (MM:SS) from the start of the recording. Large gaps between timestamps may indicate silence, examination, or the room being empty between patients.

If you find a transition point or completed encounter, return:
{"complete": true, "end_segment_index": <last segment index of the CONCLUDED encounter>, "confidence": <0.0-1.0>}

If the current discussion is still one ongoing encounter with no transition, return:
{"complete": false, "confidence": <0.0-1.0>}

Respond with ONLY the JSON object."#;

    // Build context section if signals are available
    let context_section = if let Some(ctx) = context {
        let mut parts = Vec::new();
        // Sensor departure — soft signal, not a split trigger on its own
        if ctx.sensor_departed {
            parts.push(
                "CONTEXT: The presence sensor detected possible movement away from the room. \
                Note: brief departures during medical visits are common (hand washing, supplies, \
                injection preparation, bathroom). Evaluate the TRANSCRIPT CONTENT to determine \
                if the encounter has actually concluded — a sensor departure alone is not sufficient. \
                IMPORTANT: Transcript timestamps are more reliable than the sensor. If segments \
                show continuous or very recent speech (no large time gap), the encounter is likely \
                still active regardless of the sensor signal.".to_string()
            );
        }
        // Sensor still present — use original production prompt (proven reliable)
        if ctx.sensor_present && !ctx.sensor_departed {
            parts.push(
                "CONTEXT: The presence sensor confirms someone is still in the room. \
                Topic changes or pauses within the same visit are NOT transitions. \
                Only split if there is strong evidence of a different patient \
                (new name, new history intake, greeting a new person).".to_string()
            );
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("\n\nReal-time context signals:\n{}", parts.join("\n"))
        }
    } else {
        String::new()
    };

    let user = format!(
        "Transcript (segments numbered with speaker labels):\n{}{}",
        formatted_segments, context_section
    );

    (system.to_string(), user)
}

/// Strip `<think>...</think>` tags from LLM output (model may emit these even with /nothink).
/// For unclosed `<think>` tags, keeps content after the tag (model may place JSON inside).
pub(crate) fn strip_think_tags(text: &str) -> String {
    let mut result = text.to_string();
    while let Some(start) = result.find("<think>") {
        if let Some(end) = result[start..].find("</think>") {
            let end_pos = start + end + "</think>".len();
            result = format!("{}{}", &result[..start], &result[end_pos..]);
        } else {
            // Unclosed <think> -- keep content after the tag (JSON may be inside)
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

/// Strip markdown code fences from text (e.g. ```json\n{...}\n``` -> {...})
fn strip_markdown_code_fences(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("```") {
        // Find end of opening fence line
        let after_open = if let Some(newline_pos) = trimmed.find('\n') {
            &trimmed[newline_pos + 1..]
        } else {
            // Single line like ```json { ... }``` -- strip opening backticks
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
pub(crate) fn extract_first_json_object(text: &str) -> Option<String> {
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

/// Parse a JSON response from the LLM, handling think tags, code fences, and wrapper objects.
///
/// Two-pass strategy:
/// 1. Try extracting the outermost JSON object from the cleaned text
/// 2. If that fails, look for a fallback key prefix (e.g. `{"complete"`) and try the inner object
///
/// Used by encounter detection, clinical content check, and merge check parsers.
pub(crate) fn parse_llm_json_response<T: DeserializeOwned>(
    response: &str,
    fallback_key_prefix: &str,
    error_context: &str,
) -> Result<T, String> {
    let cleaned = strip_think_tags(response);

    if let Some(json_str) = extract_first_json_object(&cleaned) {
        if let Ok(result) = serde_json::from_str::<T>(&json_str) {
            return Ok(result);
        }
    }

    if let Some(inner_start) = cleaned.find(fallback_key_prefix) {
        if let Some(json_str) = extract_first_json_object(&cleaned[inner_start..]) {
            if let Ok(result) = serde_json::from_str::<T>(&json_str) {
                return Ok(result);
            }
        }
    }

    Err(format!("Failed to parse {} response: (raw: {})", error_context, response))
}

/// Parse the encounter detection response from the LLM
pub fn parse_encounter_detection(response: &str) -> Result<EncounterDetectionResult, String> {
    parse_llm_json_response(response, "{\"complete\"", "encounter detection")
}

// ============================================================================
// Clinical Content Check (post-split two-pass)
// ============================================================================

/// Result of clinical content check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClinicalContentCheckResult {
    pub clinical: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Build the clinical content check prompt.
/// Called after encounter text is extracted, before SOAP generation.
pub fn build_clinical_content_check_prompt(encounter_text: &str) -> (String, String) {
    let system = r#"You MUST respond in English with ONLY a JSON object. No other text, no explanations, no markdown.

You are reviewing a segment of transcript from a medical office where the microphone records all day.

Your task: determine if this transcript contains a clinical patient encounter (examination, consultation, treatment discussion) OR if it is non-clinical content (personal conversation, staff chat, phone calls unrelated to patient care, silence/noise).

If it contains ANY substantive clinical content (history-taking, physical exam, diagnosis discussion, treatment planning), return:
{"clinical": true, "reason": "brief description of clinical content found"}

If it is entirely non-clinical (personal chat, administrative only, no patient care), return:
{"clinical": false, "reason": "brief description of why this is not clinical"}

Respond with ONLY the JSON object."#;

    // Truncate to ~2000 words for fast-model efficiency
    let words: Vec<&str> = encounter_text.split_whitespace().collect();
    let truncated = if words.len() > 2000 {
        format!(
            "{}\n[... {} words omitted ...]\n{}",
            words[..1000].join(" "),
            words.len() - 2000,
            words[words.len() - 1000..].join(" ")
        )
    } else {
        encounter_text.to_string()
    };

    let user = format!("Transcript to evaluate:\n{}", truncated);

    (system.to_string(), user)
}

/// Parse the clinical content check response from the LLM
pub fn parse_clinical_content_check(response: &str) -> Result<ClinicalContentCheckResult, String> {
    parse_llm_json_response(response, "{\"clinical\"", "clinical content check")
}

// ============================================================================
// Retrospective Multi-Patient Check (post-merge)
// ============================================================================

/// Result of multi-patient check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPatientCheckResult {
    pub multiple_patients: bool,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// System prompt for multi-patient detection (gate check after merge-back).
pub const MULTI_PATIENT_CHECK_PROMPT: &str = r#"You MUST respond in English with ONLY a JSON object. No other text.

You are reviewing a clinical transcript to determine if the DOCTOR conducted separate clinical visits with DIFFERENT patients in this recording.

IMPORTANT DISTINCTION:
- A companion/partner/family member who ACCOMPANIES a patient and provides context about that patient's health is NOT a separate patient visit, even if they speak extensively. They are part of the same visit.
- A separate patient visit means the doctor conducts a distinct clinical assessment: separate history-taking, separate physical findings, separate treatment plan for a DIFFERENT individual.

Multiple patients = the doctor addresses different individuals as patients at different points, with separate clinical assessments (e.g., "Lynn, your blood work shows..." then later "Jim, your thyroid levels...").

Single patient = one person receives clinical assessment, even if others speak, provide history, ask questions, or discuss their own concerns in passing.

Return: {"multiple_patients": true/false, "confidence": <0.0-1.0>, "reason": "<brief explanation>"}
Respond with ONLY the JSON."#;

/// Parse the multi-patient check response from the LLM
pub fn parse_multi_patient_check(response: &str) -> Result<MultiPatientCheckResult, String> {
    parse_llm_json_response(response, "{\"multiple_patients\"", "multi-patient check")
}

// ============================================================================
// Per-Patient Multi-Patient Detection (transcript-only, handles interleaved care)
// ============================================================================

/// Minimum word count for multi-patient detection.
/// Lower than MULTI_PATIENT_CHECK_WORD_THRESHOLD (2500) because short couples visits
/// (~10 min = ~1500 words) should still be detected.
pub const MULTI_PATIENT_DETECT_WORD_THRESHOLD: usize = 500;

/// Minimum confidence for multi-patient detection to be acted upon.
/// Matches existing detection gates in the codebase.
pub const MULTI_PATIENT_DETECT_MIN_CONFIDENCE: f64 = 0.7;

/// Maximum words to send in the detection prompt (head + tail sampling).
/// Mirrors the truncation strategy in `build_clinical_content_check_prompt`.
pub const MULTI_PATIENT_DETECT_MAX_WORDS: usize = 3000;

/// System prompt for per-patient multi-patient detection.
/// Unlike MULTI_PATIENT_CHECK_PROMPT (binary yes/no), this returns structured patient
/// data (count, labels, summaries) needed to generate per-patient SOAP notes.
/// Handles interleaved/interwoven care where the doctor goes back and forth.
pub const MULTI_PATIENT_DETECT_PROMPT: &str = r#"You MUST respond in English with ONLY a JSON object. No other text.

You are reviewing a clinical transcript to determine if the DOCTOR conducted separate clinical assessments for DIFFERENT patients in this recording.

IMPORTANT DISTINCTION:
- A companion/partner/family member who ACCOMPANIES a patient and provides context is NOT a separate patient, UNLESS the doctor also conducts a clinical assessment (history, exam, treatment plan) for them.
- Couples and family members are often seen together — the doctor may go back and forth between patients in the same visit. The conversation may be interwoven.
- A separate patient means the doctor conducts a distinct clinical assessment for a DIFFERENT individual: their own symptoms, their own examination, their own treatment plan.

Return:
{"patient_count": <number>, "patients": [{"label": "<name or identifier>", "summary": "<1 sentence: what they were seen for>"}], "confidence": <0.0-1.0>, "reasoning": "<brief explanation>"}

If only one patient was clinically assessed, return patient_count: 1 with that patient's info.

Respond with ONLY the JSON."#;

/// A single detected patient from multi-patient detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPatient {
    /// Label for this patient (e.g., name or "Patient 1")
    pub label: String,
    /// One-sentence summary of what they were seen for
    #[serde(default)]
    pub summary: String,
}

/// Result of per-patient multi-patient detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiPatientDetectionResult {
    /// Number of patients detected
    pub patient_count: u32,
    /// Detected patients with labels and summaries
    #[serde(default)]
    pub patients: Vec<DetectedPatient>,
    /// Detection confidence (0.0-1.0)
    #[serde(default)]
    pub confidence: Option<f64>,
    /// Brief reasoning from the LLM
    #[serde(default)]
    pub reasoning: Option<String>,
}

/// Parse the multi-patient detection response from the LLM
pub fn parse_multi_patient_detection(response: &str) -> Result<MultiPatientDetectionResult, String> {
    parse_llm_json_response(response, "{\"patient_count\"", "multi-patient detection")
}

/// System prompt for enhanced split-point detection (used after multi-patient check confirms
/// multiple patients). Unlike the standard split prompt which looks for farewell markers,
/// this prompt focuses on NAME TRANSITIONS — critical for family visits where the doctor
/// switches patients without a formal goodbye.
pub const MULTI_PATIENT_SPLIT_PROMPT: &str = r#"You MUST respond in English with ONLY a JSON object. No other text.

You are analyzing a clinical transcript that was recorded continuously in a medical office.
This transcript has been confirmed to contain MULTIPLE DISTINCT patient encounters.

Your task: find the line where the FIRST patient's encounter ends and the SECOND patient's encounter begins.

Look for:
- A different patient name being introduced or addressed
- The doctor beginning a new clinical assessment for a different person
- Someone saying "next patient" or introducing another person by name
- A shift from one person's medical issues to another person's medical issues

IMPORTANT: In family visits, the transition may be subtle — no formal farewell, just a name switch ("Mercedes is next", "how about Jim's labs"). Focus on WHICH PATIENT is being clinically assessed, not conversational flow.

Return the LINE NUMBER of the LAST line of the FIRST patient's encounter.

Return a JSON object (or empty object {} if no clear boundary):
{"line_index": <line number>, "confidence": <0.0-1.0>, "reason": "<brief explanation>"}

Respond with ONLY the JSON."#;

#[cfg(test)]
mod tests {
    use super::*;

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
        // Model wraps JSON in {return {...}} -- the actual error from production
        let response = r#"<think> </think> {return {"complete": false, "confidence": 0.0}}"#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(!result.complete);
    }

    #[test]
    fn test_parse_encounter_detection_low_confidence() {
        let response = r#"{"complete": true, "end_segment_index": 10, "confidence": 0.3}"#;
        let result = parse_encounter_detection(response).unwrap();
        assert!(result.complete);
        // Confidence is below 0.7 threshold -- caller should skip this detection
        assert!(result.confidence.unwrap() < 0.7);
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
    fn test_min_words_below_force_check() {
        assert!(
            MIN_WORDS_FOR_CLINICAL_CHECK < FORCE_CHECK_WORD_THRESHOLD,
            "MIN_WORDS_FOR_CLINICAL_CHECK ({}) must be less than FORCE_CHECK_WORD_THRESHOLD ({})",
            MIN_WORDS_FOR_CLINICAL_CHECK, FORCE_CHECK_WORD_THRESHOLD
        );
    }

    #[test]
    fn test_detection_prompt_requires_english() {
        let (system, _) = build_encounter_detection_prompt("test transcript", None);
        assert!(system.contains("MUST respond in English"), "Prompt should require English response");
        assert!(system.contains("ONLY a JSON object"), "Prompt should require JSON only");
    }

    #[test]
    fn test_detection_prompt_transition_framing() {
        let (system, _) = build_encounter_detection_prompt("test transcript", None);
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
    fn test_detection_prompt_core_framing() {
        let (system, _) = build_encounter_detection_prompt("test transcript", None);
        assert!(
            system.contains("pre-visit assessment"),
            "Prompt should mention pre-visit assessment"
        );
        assert!(
            system.contains("concluding plan"),
            "Prompt should mention concluding plan"
        );
    }

    #[test]
    fn test_detection_prompt_with_context_sensor_departed() {
        let ctx = EncounterDetectionContext {
            sensor_departed: true,
            sensor_present: false,
        };
        let (_, user) = build_encounter_detection_prompt("test transcript", Some(&ctx));
        assert!(user.contains("presence sensor"), "User prompt should mention sensor departure");
    }

    #[test]
    fn test_detection_prompt_with_sensor_present() {
        let ctx = EncounterDetectionContext {
            sensor_departed: false,
            sensor_present: true,
        };
        let (_, user) = build_encounter_detection_prompt("test transcript", Some(&ctx));
        assert!(user.contains("still in the room"), "User prompt should mention sensor presence");
        assert!(user.contains("NOT transitions"), "Should mention topic changes are not transitions");
    }

    #[test]
    fn test_detection_prompt_with_no_context_signals() {
        let ctx = EncounterDetectionContext {
            sensor_departed: false,
            sensor_present: false,
        };
        let (_, user) = build_encounter_detection_prompt("test transcript", Some(&ctx));
        // No sensor signals — no context section
        assert!(!user.contains("presence sensor"), "No sensor signal should be present");
    }

    // ── Clinical content check tests ─────────────────────────────

    #[test]
    fn test_parse_clinical_content_check_clinical() {
        let response = r#"{"clinical": true, "reason": "Patient history-taking and exam discussion"}"#;
        let result = parse_clinical_content_check(response).unwrap();
        assert!(result.clinical);
        assert!(result.reason.unwrap().contains("history"));
    }

    #[test]
    fn test_parse_clinical_content_check_non_clinical() {
        let response = r#"{"clinical": false, "reason": "Personal conversation about weekend plans"}"#;
        let result = parse_clinical_content_check(response).unwrap();
        assert!(!result.clinical);
        assert!(result.reason.unwrap().contains("weekend"));
    }

    #[test]
    fn test_parse_clinical_content_check_with_think_tags() {
        let response = r#"<think>analyzing</think>{"clinical": true, "reason": "exam"}"#;
        let result = parse_clinical_content_check(response).unwrap();
        assert!(result.clinical);
    }

    #[test]
    fn test_parse_clinical_content_check_no_reason() {
        let response = r#"{"clinical": false}"#;
        let result = parse_clinical_content_check(response).unwrap();
        assert!(!result.clinical);
        assert!(result.reason.is_none());
    }

    #[test]
    fn test_build_clinical_content_check_prompt_truncation() {
        let long_text = "word ".repeat(3000);
        let (_, user) = build_clinical_content_check_prompt(&long_text);
        assert!(user.contains("words omitted"));
    }

    #[test]
    fn test_build_clinical_content_check_prompt_short() {
        let short_text = "Patient reports headache for two weeks.";
        let (system, user) = build_clinical_content_check_prompt(short_text);
        assert!(system.contains("clinical patient encounter"));
        assert!(user.contains("headache"));
        assert!(!user.contains("words omitted"));
    }

    // ── Multi-patient check tests ─────────────────────────────

    #[test]
    fn test_parse_multi_patient_check_true() {
        let response = r#"{"multiple_patients": true, "confidence": 0.95, "reason": "Two separate assessments"}"#;
        let result = parse_multi_patient_check(response).unwrap();
        assert!(result.multiple_patients);
        assert!((result.confidence.unwrap() - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_parse_multi_patient_check_false() {
        let response = r#"{"multiple_patients": false, "confidence": 0.9, "reason": "Single patient"}"#;
        let result = parse_multi_patient_check(response).unwrap();
        assert!(!result.multiple_patients);
    }

    #[test]
    fn test_parse_multi_patient_check_with_think_tags() {
        let response = r#"<think>reviewing</think>{"multiple_patients": true, "confidence": 0.8, "reason": "Lynn and Jim"}"#;
        let result = parse_multi_patient_check(response).unwrap();
        assert!(result.multiple_patients);
    }

    #[test]
    fn test_multi_patient_constants() {
        assert!(
            MULTI_PATIENT_SPLIT_MIN_WORDS < MULTI_PATIENT_CHECK_WORD_THRESHOLD,
            "Min split words ({}) must be less than check threshold ({})",
            MULTI_PATIENT_SPLIT_MIN_WORDS, MULTI_PATIENT_CHECK_WORD_THRESHOLD
        );
    }

    #[test]
    fn test_detection_prompt_mentions_elapsed_time() {
        let (system, _user) = build_encounter_detection_prompt("test transcript", None);
        assert!(system.contains("elapsed time"));
        assert!(system.contains("Large gaps between timestamps"));
    }

    // ── Per-patient multi-patient detection tests ────────────────

    #[test]
    fn test_parse_multi_patient_detection_single() {
        let response = r#"{"patient_count": 1, "patients": [{"label": "Patient 1", "summary": "Follow-up for hypertension"}], "confidence": 0.95, "reasoning": "Only one patient assessed"}"#;
        let result = parse_multi_patient_detection(response).unwrap();
        assert_eq!(result.patient_count, 1);
        assert_eq!(result.patients.len(), 1);
        assert_eq!(result.patients[0].label, "Patient 1");
        assert!((result.confidence.unwrap() - 0.95).abs() < 0.001);
    }

    #[test]
    fn test_parse_multi_patient_detection_multiple() {
        let response = r#"{"patient_count": 2, "patients": [{"label": "Lynn", "summary": "Diabetes follow-up"}, {"label": "Jim", "summary": "Thyroid management"}], "confidence": 0.9, "reasoning": "Two separate clinical assessments"}"#;
        let result = parse_multi_patient_detection(response).unwrap();
        assert_eq!(result.patient_count, 2);
        assert_eq!(result.patients.len(), 2);
        assert_eq!(result.patients[0].label, "Lynn");
        assert_eq!(result.patients[1].label, "Jim");
        assert!((result.confidence.unwrap() - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_parse_multi_patient_detection_with_think_tags() {
        let response = r#"<think>analyzing the transcript</think>{"patient_count": 2, "patients": [{"label": "A", "summary": "X"}, {"label": "B", "summary": "Y"}], "confidence": 0.85, "reasoning": "two patients"}"#;
        let result = parse_multi_patient_detection(response).unwrap();
        assert_eq!(result.patient_count, 2);
        assert_eq!(result.patients.len(), 2);
    }

    #[test]
    fn test_parse_multi_patient_detection_low_confidence() {
        let response = r#"{"patient_count": 2, "patients": [{"label": "A", "summary": "X"}, {"label": "B", "summary": "Y"}], "confidence": 0.4, "reasoning": "uncertain"}"#;
        let result = parse_multi_patient_detection(response).unwrap();
        assert_eq!(result.patient_count, 2);
        // Confidence below 0.7 threshold — caller should treat as single-patient
        assert!(result.confidence.unwrap() < 0.7);
    }

    #[test]
    fn test_parse_multi_patient_detection_no_patients_array() {
        // Graceful handling when patients array is missing
        let response = r#"{"patient_count": 1, "confidence": 0.9, "reasoning": "single patient"}"#;
        let result = parse_multi_patient_detection(response).unwrap();
        assert_eq!(result.patient_count, 1);
        assert!(result.patients.is_empty());
    }

    #[test]
    fn test_multi_patient_detect_threshold() {
        assert!(
            MULTI_PATIENT_DETECT_WORD_THRESHOLD < MULTI_PATIENT_CHECK_WORD_THRESHOLD,
            "New detect threshold ({}) should be lower than old check threshold ({})",
            MULTI_PATIENT_DETECT_WORD_THRESHOLD, MULTI_PATIENT_CHECK_WORD_THRESHOLD
        );
    }
}
