//! Medication list extraction from chart screenshots.
//!
//! Uses the same vision-LLM pipeline as `patient_name_tracker` to pull a
//! structured medication list out of an EMR screenshot. Output feeds both
//! the Clinical Assistant chat (as system-prompt context) and the
//! Medication Assessment tab (as the initial editable list).
//!
//! Mirrors `patient_name_tracker`'s prompt-builder + JSON parser, adapted
//! for JSON ARRAYS instead of single objects. Trust boundary is identical
//! to the existing Confirm Patient flow: vision-derived data is clinician
//! reviewed before any action.

use serde::{Deserialize, Serialize};

/// A single medication entry parsed from a vision LLM response.
///
/// Fields beyond `name` are optional because EMR med lists often display
/// dose/frequency in non-tabular ways the vision model can't reliably
/// segment — the LLM is allowed to return null/NOT_FOUND for those.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MedEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency: Option<String>,
}

/// Result of a one-shot medication-extraction screenshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MedExtractionResult {
    pub medications: Vec<MedEntry>,
    /// True if the underlying screenshot was mostly blank — almost always
    /// means Screen Recording permission isn't granted. Caller surfaces a
    /// "grant permission" message instead of "no meds found".
    pub likely_blank: bool,
}

/// Build the vision prompt for medication-list extraction.
/// Returns (system_prompt, user_prompt_text). When `templates` is provided
/// and the relevant field is non-empty, it overrides the hardcoded default.
///
/// Same templating contract as `patient_name_tracker::build_patient_name_prompt`.
pub(crate) fn build_medication_extraction_prompt(
    templates: Option<&crate::server_config::PromptTemplates>,
) -> (String, String) {
    let system = templates
        .and_then(|t| {
            (!t.medication_extraction_system.is_empty())
                .then(|| t.medication_extraction_system.clone())
        })
        .unwrap_or_else(|| {
            "You are analyzing a screenshot of a computer screen in a clinical setting. \
             If a patient's medication list is visible (e.g., the EMR's current-meds panel, \
             a printed med list, or a discharge summary section), extract each medication \
             with its dose and frequency when shown. Respond with ONLY a JSON array, no other text."
                .to_string()
        });

    let user = templates
        .and_then(|t| {
            (!t.medication_extraction_user.is_empty()).then(|| t.medication_extraction_user.clone())
        })
        .unwrap_or_else(|| {
            "Extract the patient's current medications from this screenshot. \
             Respond with ONLY a JSON array of objects shaped like \
             {\"name\": \"<drug name>\", \"dose\": \"<dose with unit or NOT_FOUND>\", \
             \"frequency\": \"<freq or NOT_FOUND>\"}. \
             If no medication list is visible, respond with an empty array []."
                .to_string()
        });

    (system, user)
}

/// Parse a vision response into a list of medications.
///
/// Strategy: locate the first balanced `[...]` block (handles markdown-fenced
/// JSON and leading garbage), parse as `Vec<serde_json::Value>`, project each
/// object into `MedEntry`. Entries with empty/NOT_FOUND `name` are dropped.
/// Empty `Vec` on any parse failure — caller treats this the same as "no
/// meds visible".
pub(crate) fn parse_medication_vision_response(response: &str) -> Vec<MedEntry> {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let array_slice = crate::patient_name_tracker::extract_first_balanced(trimmed, b'[', b']')
        .unwrap_or(trimmed);

    let parsed: Vec<serde_json::Value> = match serde_json::from_str(array_slice) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    parsed
        .into_iter()
        .filter_map(|entry| {
            let obj = entry.as_object()?;
            let name = obj
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.contains("NOT_FOUND"))?;
            let dose = obj
                .get("dose")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && !s.contains("NOT_FOUND"));
            let frequency = obj
                .get("frequency")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty() && !s.contains("NOT_FOUND"));
            Some(MedEntry {
                name: name.to_string(),
                dose,
                frequency,
            })
        })
        .collect()
}

/// Build the LLM prompt for parsing a clinician's free-text medication
/// list into structured `MedEntry` JSON. The prompt accepts both fresh
/// lists ("metformin 500 bid, lipitor 40, aspirin") and modifications
/// ("stop the lipitor", "increase metformin to 1000") on top of the
/// current list. Output shape matches the vision-extraction prompt so
/// `parse_medication_vision_response` parses both.
pub(crate) fn build_medication_text_parse_prompt(
    current_meds: &[MedEntry],
    user_text: &str,
    templates: Option<&crate::server_config::PromptTemplates>,
) -> (String, String) {
    let system = templates
        .and_then(|t| {
            (!t.medication_text_parse_system.is_empty())
                .then(|| t.medication_text_parse_system.clone())
        })
        .unwrap_or_else(|| {
            "You normalize a clinician's free-text medication entries into structured JSON. \
             The clinician is in clinic and types quickly — the input may have typos, abbreviations, \
             or be a mix of a fresh list and modifications to the existing list. \
             You will receive the CURRENT list and the clinician's input; apply the input to the \
             current list and output the FINAL list as JSON. Respond with ONLY a JSON array, no other text."
                .to_string()
        });

    let current_json = serde_json::to_string(current_meds).unwrap_or_else(|_| "[]".to_string());

    let user_template = templates
        .and_then(|t| {
            (!t.medication_text_parse_user.is_empty())
                .then(|| t.medication_text_parse_user.clone())
        })
        .unwrap_or_else(|| {
            "Current medication list:\n{CURRENT_LIST}\n\n\
             Clinician input:\n{USER_TEXT}\n\n\
             Output the FINAL medication list as a JSON array. Each object has:\n\
             - \"name\": drug name, lowercase, correct spelling (e.g. fix \"asprin\" → \"aspirin\"). \
             Preserve brand vs. generic as the clinician wrote it.\n\
             - \"dose\": dose with units (e.g. \"500 mg\", \"40 mg\"), or null if not stated. \
             A bare number after a drug name (\"metformin 500\") usually means mg.\n\
             - \"frequency\": standardized abbreviation (OD, BID, TID, QID, QHS, PRN), \
             or a short phrase, or null.\n\n\
             Rules:\n\
             - If the clinician's input is a fresh complete list (no modification language), \
             REPLACE the current list with it.\n\
             - If it's modifications (\"stop X\", \"add Y\", \"increase Z to W\"), apply them \
             to the current list — keep meds the clinician didn't mention.\n\
             - Drop empty/garbage entries.\n\
             - Respond with ONLY the JSON array."
                .to_string()
        });

    let user = user_template
        .replace("{CURRENT_LIST}", &current_json)
        .replace("{USER_TEXT}", user_text);

    (system, user)
}

/// Flatten a med list into the newline-delimited text the pharm-refactor
/// `/analyze` endpoint expects (`engine/parser.py::parse_medication_list`).
pub fn medications_to_text(meds: &[MedEntry]) -> String {
    meds.iter()
        .map(|m| {
            let mut line = m.name.clone();
            if let Some(dose) = &m.dose {
                line.push(' ');
                line.push_str(dose);
            }
            if let Some(freq) = &m.frequency {
                line.push(' ');
                line.push_str(freq);
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json_array() {
        let response = r#"[{"name":"metformin","dose":"500 mg","frequency":"BID"},{"name":"lisinopril","dose":"10 mg","frequency":"daily"}]"#;
        let meds = parse_medication_vision_response(response);
        assert_eq!(meds.len(), 2);
        assert_eq!(meds[0].name, "metformin");
        assert_eq!(meds[0].dose.as_deref(), Some("500 mg"));
        assert_eq!(meds[0].frequency.as_deref(), Some("BID"));
        assert_eq!(meds[1].name, "lisinopril");
    }

    #[test]
    fn parses_markdown_fenced_json() {
        let response = "```json\n[{\"name\":\"warfarin\",\"dose\":\"5 mg\",\"frequency\":\"daily\"}]\n```";
        let meds = parse_medication_vision_response(response);
        assert_eq!(meds.len(), 1);
        assert_eq!(meds[0].name, "warfarin");
    }

    #[test]
    fn parses_empty_array() {
        let meds = parse_medication_vision_response("[]");
        assert!(meds.is_empty());
    }

    #[test]
    fn empty_response_returns_empty_vec() {
        assert!(parse_medication_vision_response("").is_empty());
        assert!(parse_medication_vision_response("   ").is_empty());
    }

    #[test]
    fn entries_with_not_found_name_are_dropped() {
        let response = r#"[{"name":"NOT_FOUND","dose":"10 mg"},{"name":"metformin","dose":"NOT_FOUND","frequency":"NOT_FOUND"}]"#;
        let meds = parse_medication_vision_response(response);
        assert_eq!(meds.len(), 1);
        assert_eq!(meds[0].name, "metformin");
        assert!(meds[0].dose.is_none());
        assert!(meds[0].frequency.is_none());
    }

    #[test]
    fn missing_optional_fields_become_none() {
        let meds = parse_medication_vision_response(r#"[{"name":"aspirin"}]"#);
        assert_eq!(meds.len(), 1);
        assert_eq!(meds[0].name, "aspirin");
        assert!(meds[0].dose.is_none());
        assert!(meds[0].frequency.is_none());
    }

    #[test]
    fn truncated_json_returns_empty_vec() {
        let response = r#"[{"name":"metformin","dose":"500"#;
        assert!(parse_medication_vision_response(response).is_empty());
    }

    #[test]
    fn ocr_noise_returns_empty_vec() {
        let response = "I cannot see a medication list in this screenshot.";
        assert!(parse_medication_vision_response(response).is_empty());
    }

    #[test]
    fn leading_garbage_is_skipped() {
        let response = "Here is the list:\n[{\"name\":\"metoprolol\",\"dose\":\"25 mg\"}]";
        let meds = parse_medication_vision_response(response);
        assert_eq!(meds.len(), 1);
        assert_eq!(meds[0].name, "metoprolol");
    }

    #[test]
    fn name_whitespace_is_trimmed() {
        let response = r#"[{"name":"  amlodipine  ","dose":"5 mg"}]"#;
        let meds = parse_medication_vision_response(response);
        assert_eq!(meds.len(), 1);
        assert_eq!(meds[0].name, "amlodipine");
    }

    #[test]
    fn build_prompt_uses_defaults_when_templates_absent() {
        let (system, user) = build_medication_extraction_prompt(None);
        assert!(system.contains("medication list"));
        assert!(user.contains("JSON array"));
    }

    #[test]
    fn build_prompt_honors_template_override() {
        let templates = crate::server_config::PromptTemplates {
            medication_extraction_system: "CUSTOM SYSTEM".to_string(),
            medication_extraction_user: "CUSTOM USER".to_string(),
            ..Default::default()
        };
        let (system, user) = build_medication_extraction_prompt(Some(&templates));
        assert_eq!(system, "CUSTOM SYSTEM");
        assert_eq!(user, "CUSTOM USER");
    }

    #[test]
    fn text_parse_prompt_embeds_current_list_and_user_input() {
        let current = vec![
            MedEntry {
                name: "metformin".into(),
                dose: Some("500 mg".into()),
                frequency: Some("BID".into()),
            },
            MedEntry {
                name: "lipitor".into(),
                dose: Some("40 mg".into()),
                frequency: Some("OD".into()),
            },
        ];
        let (system, user) =
            build_medication_text_parse_prompt(&current, "stop the lipitor, add asprin 81", None);
        // System prompt mentions the normalization task and JSON output rule.
        assert!(system.contains("clinician"));
        assert!(system.contains("JSON"));
        // User prompt contains the serialized current list (as JSON) and the
        // clinician's input verbatim — placeholders must be replaced.
        assert!(!user.contains("{CURRENT_LIST}"));
        assert!(!user.contains("{USER_TEXT}"));
        assert!(user.contains("metformin"));
        assert!(user.contains("lipitor"));
        assert!(user.contains("stop the lipitor"));
        assert!(user.contains("asprin"));
        // Output schema description so the LLM knows the shape.
        assert!(user.contains("name"));
        assert!(user.contains("dose"));
        assert!(user.contains("frequency"));
    }

    #[test]
    fn text_parse_prompt_with_empty_current_list_serializes_empty_array() {
        let (_, user) =
            build_medication_text_parse_prompt(&[], "metformin 500 bid, lipitor 40", None);
        // No current meds means the placeholder gets replaced with "[]".
        assert!(user.contains("[]"));
        assert!(user.contains("metformin 500 bid"));
    }

    #[test]
    fn text_parse_prompt_honors_template_override() {
        let templates = crate::server_config::PromptTemplates {
            medication_text_parse_system: "CUSTOM TEXT-PARSE SYS".to_string(),
            medication_text_parse_user: "CURR={CURRENT_LIST} TEXT={USER_TEXT}".to_string(),
            ..Default::default()
        };
        let (system, user) =
            build_medication_text_parse_prompt(&[], "type whatever", Some(&templates));
        assert_eq!(system, "CUSTOM TEXT-PARSE SYS");
        // Placeholders still get substituted in custom templates.
        assert_eq!(user, "CURR=[] TEXT=type whatever");
    }

    #[test]
    fn medications_to_text_round_trips_fields() {
        let meds = vec![
            MedEntry {
                name: "metformin".into(),
                dose: Some("500 mg".into()),
                frequency: Some("BID".into()),
            },
            MedEntry {
                name: "aspirin".into(),
                dose: None,
                frequency: None,
            },
        ];
        assert_eq!(
            medications_to_text(&meds),
            "metformin 500 mg BID\naspirin"
        );
    }
}
