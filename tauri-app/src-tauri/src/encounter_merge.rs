//! Retrospective encounter merge logic for continuous mode.
//!
//! After an encounter is split, this module checks whether the new encounter
//! and the previous one are actually from the same patient visit (e.g., split
//! due to a pause or examination). If so, they can be merged.

use serde::{Deserialize, Serialize};

use crate::encounter_detection::parse_llm_json_response;

/// Result of encounter merge check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeCheckResult {
    pub same_encounter: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

/// What the merge-check sees on the PREVIOUS-encounter side of the comparison.
///
/// The prev SOAP is a stronger signal than a 500-word transcript tail: it carries
/// explicit patient labels, assessment, and plan bullets, so the LLM can tell whether
/// the incoming encounter continues the prev visit or starts a new one. The tail
/// fallback exists for cases where prev has no SOAP (non-clinical visits, or
/// SOAP generation that produced a malformed-output placeholder).
#[derive(Debug, Clone, Copy)]
pub enum PrevMergeInput<'a> {
    SoapNote(&'a str),
    TranscriptTail(&'a str),
}

impl<'a> PrevMergeInput<'a> {
    pub fn content(&self) -> &'a str {
        match self {
            PrevMergeInput::SoapNote(s) | PrevMergeInput::TranscriptTail(s) => s,
        }
    }

    /// Tag used in pipeline_log / replay_bundle so we can tell which branch fired.
    pub fn source_tag(&self) -> &'static str {
        match self {
            PrevMergeInput::SoapNote(_) => "soap_note",
            PrevMergeInput::TranscriptTail(_) => "transcript_tail",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            PrevMergeInput::SoapNote(_) => "PREVIOUS ENCOUNTER SOAP NOTE",
            PrevMergeInput::TranscriptTail(_) => "EXCERPT FROM END OF PREVIOUS ENCOUNTER",
        }
    }
}

/// Build the encounter merge prompt -- asks if two excerpts are from the same patient visit.
///
/// When `patient_name` is provided (e.g. from vision-based extraction), the prompt
/// includes it as context, significantly improving merge accuracy on topic-shift cases
/// (33% -> 100% in experiments -- see encounter-experiments/summary.md).
pub fn build_encounter_merge_prompt(
    prev: PrevMergeInput<'_>,
    curr_head: &str,
    patient_name: Option<&str>,
    templates: Option<&crate::server_config::PromptTemplates>,
) -> (String, String) {
    let patient_context = match patient_name {
        Some(name) if !name.is_empty() => format!(
            "\n\nCONTEXT: The patient being seen is {}. If both excerpts reference this patient or the same clinical context, they are almost certainly the same encounter.",
            name
        ),
        _ => String::new(),
    };

    let prev_form_note = match prev {
        PrevMergeInput::SoapNote(_) => "The PREVIOUS-encounter side is presented as its generated SOAP note (S/O/A/P sections). Use the patient label, assessment, and plan to judge continuity against the transcript head of the NEXT encounter.",
        PrevMergeInput::TranscriptTail(_) => "The PREVIOUS-encounter side is the last portion of its raw transcript.",
    };

    let system = templates
        .and_then(|t| (!t.encounter_merge_system.is_empty()).then(|| {
            format!("{}{}", t.encounter_merge_system, patient_context)
        }))
        .unwrap_or_else(|| format!(
        r#"You are reviewing two consecutive excerpts from a medical office where a microphone records all day.

The system split these into two separate encounters, but they may actually be the SAME patient visit that was incorrectly split (e.g., due to a pause, phone call, or silence during an examination). {}

Determine if both excerpts are from the SAME patient encounter or DIFFERENT encounters.

Signs they are the SAME encounter:
- Same patient name or context referenced
- Continuation of the same clinical discussion
- No farewell/greeting between them
- Natural pause (examination, looking at charts) rather than patient change
- Same medical condition being discussed from different angles
- When the previous side is a SOAP note, the next excerpt's content clearly continues one of its S/O/A/P threads (same medications, same plan items, same specific findings)

Signs they are DIFFERENT encounters:
- Different patient names or contexts
- A farewell followed by a new greeting
- Clearly different clinical topics with no continuity
- When the previous side is a SOAP note that already lists multiple distinct patients, a new greeting in the next excerpt indicates yet another distinct encounter rather than a continuation{}

Return JSON:
{{"same_encounter": true, "reason": "brief explanation"}}
or
{{"same_encounter": false, "reason": "brief explanation"}}

Return ONLY the JSON object, nothing else."#,
        prev_form_note,
        patient_context
    ));

    let user = format!(
        "{}:\n{}\n\n---\n\nEXCERPT FROM START OF NEXT ENCOUNTER:\n{}",
        prev.label(),
        prev.content(),
        curr_head
    );

    (system, user)
}

/// Parse the merge check response from the LLM
pub fn parse_merge_check(response: &str) -> Result<MergeCheckResult, String> {
    parse_llm_json_response(response, "{\"same_encounter\"", "merge check")
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
    fn test_build_encounter_merge_prompt_tail() {
        let (system, user) = build_encounter_merge_prompt(
            PrevMergeInput::TranscriptTail("...we'll see you in two weeks"),
            "Good morning Mr. Smith...",
            None,
            None,
        );
        assert!(system.contains("SAME patient visit"));
        assert!(system.contains("last portion of its raw transcript"));
        assert!(user.contains("EXCERPT FROM END OF PREVIOUS ENCOUNTER"));
        assert!(user.contains("we'll see you in two weeks"));
        assert!(user.contains("Good morning Mr. Smith"));
    }

    #[test]
    fn test_build_encounter_merge_prompt_soap() {
        let soap = "S: knee pain\nO: swelling\nA: osteoarthritis\nP: follow up 2 weeks";
        let (system, user) = build_encounter_merge_prompt(
            PrevMergeInput::SoapNote(soap),
            "Good morning Mr. Smith...",
            None,
            None,
        );
        assert!(system.contains("generated SOAP note"));
        assert!(system.contains("patient label, assessment, and plan"));
        assert!(user.contains("PREVIOUS ENCOUNTER SOAP NOTE"));
        assert!(user.contains("osteoarthritis"));
        assert!(user.contains("Good morning Mr. Smith"));
    }

    #[test]
    fn test_build_encounter_merge_prompt_with_patient_name() {
        let (system, _user) = build_encounter_merge_prompt(
            PrevMergeInput::TranscriptTail("tail text"),
            "head text",
            Some("Buckland, Deborah Ann"),
            None,
        );
        assert!(system.contains("Buckland, Deborah Ann"));
        assert!(system.contains("almost certainly the same encounter"));
    }

    #[test]
    fn test_prev_merge_input_source_tag() {
        assert_eq!(PrevMergeInput::SoapNote("x").source_tag(), "soap_note");
        assert_eq!(PrevMergeInput::TranscriptTail("x").source_tag(), "transcript_tail");
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
