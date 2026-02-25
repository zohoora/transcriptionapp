//! Encounter detection logic for continuous mode.
//!
//! Provides the LLM prompt construction and response parsing for detecting
//! transition points between patient encounters in a continuous transcript.

use serde::{Deserialize, Serialize};

/// Word count forcing encounter check regardless of buffer age.
pub const FORCE_CHECK_WORD_THRESHOLD: usize = 5000;
/// Force-split when buffer exceeds this AND consecutive_no_split >= limit.
pub const FORCE_SPLIT_WORD_THRESHOLD: usize = 8000;
/// Consecutive non-split detection cycles before force-split (at FORCE_SPLIT_WORD_THRESHOLD).
pub const FORCE_SPLIT_CONSECUTIVE_LIMIT: u32 = 3;
/// Unconditional force-split -- hard safety valve, no counter needed.
pub const ABSOLUTE_WORD_CAP: usize = 15_000;

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
        let (system, _) = build_encounter_detection_prompt("test transcript");
        assert!(system.contains("MUST respond in English"), "Prompt should require English response");
        assert!(system.contains("ONLY a JSON object"), "Prompt should require JSON only");
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
}
