//! Encounter detection logic for continuous mode.
//!
//! Provides the LLM prompt construction and response parsing for detecting
//! transition points between patient encounters in a continuous transcript.

use serde::{Deserialize, Serialize};

/// Word count forcing encounter check regardless of buffer age.
pub const FORCE_CHECK_WORD_THRESHOLD: usize = 3000;
/// Force-split when buffer exceeds this AND consecutive_no_split >= limit.
pub const FORCE_SPLIT_WORD_THRESHOLD: usize = 5000;
/// Consecutive non-split detection cycles before force-split (at FORCE_SPLIT_WORD_THRESHOLD).
pub const FORCE_SPLIT_CONSECUTIVE_LIMIT: u32 = 3;
/// Unconditional force-split -- hard safety valve, no counter needed.
pub const ABSOLUTE_WORD_CAP: usize = 10_000;

/// Optional context signals for encounter detection.
/// Provides real-time signals from vision (chart switch) and sensor (departure)
/// to augment the LLM prompt with high-confidence evidence.
#[derive(Debug, Clone, Default)]
pub struct EncounterDetectionContext {
    /// Current patient name from vision tracker (majority vote so far)
    pub current_patient_name: Option<String>,
    /// New patient name detected by vision (different chart on screen)
    pub new_patient_name: Option<String>,
    /// Whether the presence sensor detected someone left the room
    pub sensor_departed: bool,
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

Signs of a transition or completed encounter:
- Farewell, wrap-up, or discharge instructions ("we'll see you in X weeks", "take care")
- A greeting or introduction of a DIFFERENT patient after clinical discussion
- A clear shift from one patient's clinical topics to another's
- Extended non-clinical gap (scheduling, staff chat) after substantive clinical content
- IN-ROOM PIVOT: the doctor transitions from one family member or companion to another without anyone leaving (e.g., "Okay, now let's talk about your husband's knee" or addressing a different person by name)
- CHART SWITCH: the clinical discussion shifts to a different patient — different medications, conditions, or medical history than earlier in the transcript
- The doctor begins taking a new history, asking "what brings you in today?" or similar intake questions after already having a substantive clinical discussion with someone else

Examples of in-room transitions:
- After discussing Mrs. Smith's diabetes, the doctor says "Now, Mr. Smith, how has your blood pressure been?" — this is a transition between two encounters
- The doctor finishes discussing a child's ear infection with the mother, then asks the mother about her own back pain — this is a transition
- The doctor says "Let me pull up your chart" after already having a full discussion about a different patient's condition — likely a transition

This is NOT a transition:
- Brief pauses, phone calls, or sidebar conversations DURING an ongoing patient visit
- The very beginning of the first encounter (no prior encounter to split from)
- Short exchanges or greetings with no substantive clinical content yet
- Discussion of multiple body parts or conditions for the SAME patient (one visit can cover many topics)

If you find a transition point or completed encounter, return:
{"complete": true, "end_segment_index": <last segment index of the CONCLUDED encounter>, "confidence": <0.0-1.0>}

If the current discussion is still one ongoing encounter with no transition, return:
{"complete": false, "confidence": <0.0-1.0>}

Respond with ONLY the JSON object."#;

    // Build context section if signals are available
    let context_section = if let Some(ctx) = context {
        let mut parts = Vec::new();
        // Vision-detected chart switch — strong signal
        if let (Some(current), Some(new)) = (&ctx.current_patient_name, &ctx.new_patient_name) {
            if current != new {
                parts.push(format!(
                    "IMPORTANT: The EMR screen now shows patient '{}' instead of '{}'. This strongly suggests a patient transition has occurred.",
                    new, current
                ));
            }
        }
        // Sensor departure — moderate signal
        if ctx.sensor_departed {
            parts.push("CONTEXT: The presence sensor detected someone left the room.".to_string());
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

/// Parse the encounter detection response from the LLM
pub fn parse_encounter_detection(response: &str) -> Result<EncounterDetectionResult, String> {
    let cleaned = strip_think_tags(response);

    // Try outermost braces first
    if let Some(json_str) = extract_first_json_object(&cleaned) {
        if let Ok(result) = serde_json::from_str::<EncounterDetectionResult>(&json_str) {
            return Ok(result);
        }
    }

    // Fallback: model may wrap JSON in {return {...}} -- find inner {"complete" object
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
    let cleaned = strip_think_tags(response);

    if let Some(json_str) = extract_first_json_object(&cleaned) {
        if let Ok(result) = serde_json::from_str::<ClinicalContentCheckResult>(&json_str) {
            return Ok(result);
        }
    }

    // Fallback: try to find {"clinical" in the text
    if let Some(inner_start) = cleaned.find("{\"clinical\"") {
        if let Some(json_str) = extract_first_json_object(&cleaned[inner_start..]) {
            if let Ok(result) = serde_json::from_str::<ClinicalContentCheckResult>(&json_str) {
                return Ok(result);
            }
        }
    }

    Err(format!("Failed to parse clinical content check response: (raw: {})", response))
}

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
    fn test_detection_prompt_in_room_transitions() {
        let (system, _) = build_encounter_detection_prompt("test transcript", None);
        assert!(
            system.contains("IN-ROOM PIVOT"),
            "Prompt should mention in-room pivot transitions"
        );
        assert!(
            system.contains("CHART SWITCH"),
            "Prompt should mention chart switch transitions"
        );
    }

    #[test]
    fn test_detection_prompt_with_context_chart_switch() {
        let ctx = EncounterDetectionContext {
            current_patient_name: Some("John Smith".to_string()),
            new_patient_name: Some("Jane Smith".to_string()),
            sensor_departed: false,
        };
        let (_, user) = build_encounter_detection_prompt("test transcript", Some(&ctx));
        assert!(user.contains("Jane Smith"), "User prompt should mention new patient name");
        assert!(user.contains("John Smith"), "User prompt should mention current patient name");
        assert!(user.contains("IMPORTANT"), "Chart switch should be marked IMPORTANT");
    }

    #[test]
    fn test_detection_prompt_with_context_sensor_departed() {
        let ctx = EncounterDetectionContext {
            current_patient_name: None,
            new_patient_name: None,
            sensor_departed: true,
        };
        let (_, user) = build_encounter_detection_prompt("test transcript", Some(&ctx));
        assert!(user.contains("presence sensor"), "User prompt should mention sensor departure");
    }

    #[test]
    fn test_detection_prompt_with_no_context_signals() {
        let ctx = EncounterDetectionContext {
            current_patient_name: Some("John Smith".to_string()),
            new_patient_name: None,
            sensor_departed: false,
        };
        let (_, user) = build_encounter_detection_prompt("test transcript", Some(&ctx));
        // No chart switch (new_patient_name is None), no sensor — no context section
        assert!(!user.contains("IMPORTANT"), "No chart switch signal should be present");
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
}
