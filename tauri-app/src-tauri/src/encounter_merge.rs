//! Retrospective encounter merge logic for continuous mode.
//!
//! After an encounter is split, this module checks whether the new encounter
//! and the previous one are actually from the same patient visit (e.g., split
//! due to a pause or examination). If so, they can be merged.

use serde::{Deserialize, Serialize};

use crate::encounter_detection::{strip_think_tags, extract_first_json_object};

/// Result of encounter merge check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeCheckResult {
    pub same_encounter: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Build the encounter merge prompt -- asks if two excerpts are from the same patient visit.
///
/// When `patient_name` is provided (e.g. from vision-based extraction), the prompt
/// includes it as context, significantly improving merge accuracy on topic-shift cases
/// (33% -> 100% in experiments -- see encounter-experiments/summary.md).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_merge_check_with_think_tags() {
        let response = r#"<think>reasoning here</think> {"same_encounter": true, "reason": "same patient"}"#;
        let result = parse_merge_check(response).unwrap();
        assert!(result.same_encounter);
    }

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
        let response = r#"{"same_encounter": false, "reason": "Different patients -- farewell followed by new greeting"}"#;
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
}
