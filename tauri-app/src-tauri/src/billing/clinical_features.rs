use serde::{Deserialize, Serialize};

// ── Constrained enums for LLM extraction ───────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VisitType {
    MinorAssessment,
    IntermediateAssessment,
    GeneralAssessment,
    GeneralReassessment,
    MiniAssessment,
    PrenatalMajor,
    PrenatalMinor,
    PalliativeCare,
    Counselling,
    SharedAppointment,
    WellBabyVisit,
    PeriodicHealthVisit,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureType {
    PapSmear,
    IudInsertion,
    IudRemoval,
    LesionExcisionSmall,
    LesionExcisionMedium,
    LesionExcisionLarge,
    AbscessDrainage,
    SkinBiopsy,
    CryotherapySingle,
    CryotherapyMultiple,
    ElectrocoagulationSingle,
    ElectrocoagulationMultiple,
    BenignExcisionSmall,
    BenignExcisionMedium,
    LacerationRepairSimpleSmall,
    LacerationRepairSimpleLarge,
    LacerationRepairComplex,
    EpistaxisCautery,
    EpistaxisPacking,
    Sigmoidoscopy,
    Anoscopy,
    HemorrhoidIncision,
    CornealForeignBody,
    Immunization,
    InjectionSoleReason,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConditionType {
    DiabetesManagement,
    SmokingCessation,
    StiManagement,
    ChfManagement,
    Neurocognitive,
    HomeCare,
    SmokingCessationFollowUp,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EncounterSetting {
    InOffice,
    HomeVisit,
    TelephoneInOffice,
    TelephoneRemote,
    Video,
}

// ── Extracted clinical features ────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ClinicalFeatures {
    pub visit_type: VisitType,
    pub procedures: Vec<ProcedureType>,
    pub conditions: Vec<ConditionType>,
    pub setting: EncounterSetting,
    pub is_new_patient: bool,
    pub is_after_hours: bool,
    pub patient_count: Option<u8>,
    pub estimated_duration_minutes: Option<u16>,
    pub confidence: f32,
}

// ── Prompt construction ────────────────────────────────────────────────────

/// Build the system + user prompts for clinical feature extraction.
/// Returns `(system_prompt, user_prompt)`.
pub fn build_billing_extraction_prompt(
    soap_content: &str,
    transcript_excerpt: &str,
) -> (String, String) {
    let system_prompt = r#"You are a medical billing analyst for Ontario FHO+ family medicine. Your task is to extract clinical features from a SOAP note and transcript to determine appropriate OHIP billing codes.

Analyze the provided SOAP note and transcript excerpt, then output a JSON object matching the schema below. Only extract features explicitly supported by the SOAP note content. Do not guess or infer procedures not mentioned.

## Output Schema

```json
{
  "visitType": "<visit_type>",
  "procedures": ["<procedure_type>", ...],
  "conditions": ["<condition_type>", ...],
  "setting": "<encounter_setting>",
  "isNewPatient": <bool>,
  "isAfterHours": <bool>,
  "patientCount": <number or null>,
  "estimatedDurationMinutes": <number or null>,
  "confidence": <float 0.0-1.0>
}
```

## Valid Values

### visitType
- "minor_assessment" — Brief focused visit, single issue, <10 min (A001A)
- "intermediate_assessment" — Moderate complexity, 10-20 min (A007A)
- "general_assessment" — Comprehensive new patient workup, multi-system (A003A)
- "general_reassessment" — Comprehensive follow-up, multi-system (A004A)
- "mini_assessment" — Very brief, e.g. single Rx renewal, <5 min (A008A)
- "prenatal_major" — First prenatal visit, comprehensive (P003A)
- "prenatal_minor" — Follow-up prenatal visit (P004A)
- "palliative_care" — Palliative care encounter (K023A)
- "counselling" — Primarily counselling encounter (K005A/K013A)
- "shared_appointment" — Group medical visit with multiple patients (K140-K144A)
- "well_baby_visit" — Well-baby/child check (A007A)
- "periodic_health_visit" — Annual physical/preventive health visit (K130-K132A)

### procedures (array, may be empty)
- "pap_smear" — Pap smear collection (G365A)
- "iud_insertion" — IUD insertion (G378A)
- "iud_removal" — IUD removal (G552A)
- "lesion_excision_small" — Malignant lesion excision <1cm (R048A)
- "lesion_excision_medium" — Malignant lesion excision 1-2cm (R051A)
- "lesion_excision_large" — Malignant lesion excision >2cm (R094A)
- "abscess_drainage" — Incision and drainage of abscess (Z101A)
- "skin_biopsy" — Skin punch/shave biopsy (Z104A)
- "cryotherapy_single" — Cryotherapy for single lesion (Z108A)
- "cryotherapy_multiple" — Cryotherapy for 2-5 lesions (Z110A)
- "electrocoagulation_single" — Electrocoagulation single lesion (Z112A)
- "electrocoagulation_multiple" — Electrocoagulation 2-5 lesions (Z113A)
- "benign_excision_small" — Benign lesion excision <1cm (Z114A)
- "benign_excision_medium" — Benign lesion excision 1-2cm (Z119A)
- "laceration_repair_simple_small" — Simple laceration <5cm (Z154A)
- "laceration_repair_simple_large" — Simple laceration 5-10cm (Z160A)
- "laceration_repair_complex" — Complex laceration repair (Z176A)
- "epistaxis_cautery" — Nasal cautery for nosebleed (Z314A)
- "epistaxis_packing" — Anterior nasal packing (Z315A)
- "sigmoidoscopy" — Flexible sigmoidoscopy (Z535A)
- "anoscopy" — Anoscopy (Z543A)
- "hemorrhoid_incision" — Thrombosed hemorrhoid incision (Z545A)
- "corneal_foreign_body" — Corneal foreign body removal (Z847A)
- "immunization" — Any vaccine administration (G538A/G840-G848A)
- "injection_sole_reason" — Injection as the sole reason for visit

### conditions (array, may be empty)
- "diabetes_management" — Active diabetes management discussion (Q040A)
- "smoking_cessation" — Smoking cessation counselling (Q042A add-on)
- "sti_management" — STI testing/treatment/counselling (K028A)
- "chf_management" — CHF management and monitoring (Q050A)
- "neurocognitive" — Neurocognitive/dementia assessment (K032A)
- "home_care" — Home care application or supervision (K070A/K071A)
- "smoking_cessation_follow_up" — Smoking cessation follow-up visit (K039A)

### setting
- "in_office" — Standard in-person office visit
- "home_visit" — Home visit
- "telephone_in_office" — Phone call, patient known to practice, in-office
- "telephone_remote" — Phone call, remote/after-hours
- "video" — Video/virtual visit

## Examples

### Example 1: Minor assessment
SOAP: "S: Patient presents with sore throat x 2 days. O: Pharynx erythematous, no exudate. A: Viral pharyngitis. P: Supportive care, return if worsening."
```json
{"visitType":"minor_assessment","procedures":[],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":8,"confidence":0.95}
```

### Example 2: General assessment with procedure
SOAP: "S: New patient, 45F, referred for suspicious mole on back. Full history obtained. O: Full exam. 8mm pigmented lesion on upper back, irregular borders. Excision performed. A: Suspicious melanocytic lesion. P: Pathology pending, follow-up 2 weeks."
```json
{"visitType":"general_assessment","procedures":["lesion_excision_small"],"conditions":[],"setting":"in_office","isNewPatient":true,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":30,"confidence":0.92}
```

### Example 3: Prenatal visit
SOAP: "S: 28 weeks gestation, routine follow-up. O: BP 120/80, fundal height 28cm, FHR 145. A: Normal pregnancy progression. P: Routine labs, next visit 2 weeks."
```json
{"visitType":"prenatal_minor","procedures":[],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":15,"confidence":0.95}
```

### Example 4: Chronic disease management
SOAP: "S: Type 2 DM follow-up, A1C review. O: A1C 7.8%, up from 7.2%. BMI 31. A: Suboptimal diabetes control. P: Increase metformin, dietary counselling, recheck A1C in 3 months."
```json
{"visitType":"general_reassessment","procedures":[],"conditions":["diabetes_management"],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":20,"confidence":0.90}
```

### Example 5: After-hours counselling
SOAP: "S: Patient called after hours, anxious about chest tightness. O: History taken by phone. A: Anxiety-related symptoms, no red flags. P: Reassurance, follow-up tomorrow if persists."
```json
{"visitType":"counselling","procedures":[],"conditions":[],"setting":"telephone_remote","isNewPatient":false,"isAfterHours":true,"patientCount":null,"estimatedDurationMinutes":15,"confidence":0.85}
```

Respond ONLY with the JSON object. No explanations or commentary."#;

    let user_prompt = format!(
        "## SOAP Note\n\n{}\n\n## Transcript Excerpt\n\n{}",
        soap_content, transcript_excerpt
    );

    (system_prompt.to_string(), user_prompt)
}

// ── JSON parsing ───────────────────────────────────────────────────────────

/// Parse the LLM response into a `ClinicalFeatures` struct.
///
/// Handles common LLM output quirks: markdown code fences, leading prose
/// before the JSON, trailing text after the JSON, etc.
pub fn parse_billing_extraction(response: &str) -> Result<ClinicalFeatures, String> {
    let mut text = response.to_string();

    // Strip markdown code fences
    text = text.replace("```json", "").replace("```", "");

    // Find JSON object boundaries
    let start = text
        .find('{')
        .ok_or_else(|| "No JSON object found in response".to_string())?;
    let end = text
        .rfind('}')
        .ok_or_else(|| "No closing brace found in response".to_string())?;

    if end < start {
        return Err("Malformed JSON: closing brace before opening brace".to_string());
    }

    let json_str = &text[start..=end];

    serde_json::from_str::<ClinicalFeatures>(json_str).map_err(|e| {
        format!(
            "Failed to parse clinical features JSON: {}. Input: {}",
            e,
            &json_str[..json_str.len().min(200)]
        )
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_billing_extraction_prompt() {
        let (system, user) = build_billing_extraction_prompt("SOAP content here", "Transcript here");
        assert!(system.contains("FHO+"));
        assert!(system.contains("minor_assessment"));
        assert!(system.contains("pap_smear"));
        assert!(system.contains("diabetes_management"));
        assert!(system.contains("in_office"));
        assert!(user.contains("SOAP content here"));
        assert!(user.contains("Transcript here"));
    }

    #[test]
    fn test_parse_clean_json() {
        let json = r#"{"visitType":"minor_assessment","procedures":[],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":10,"confidence":0.95}"#;
        let features = parse_billing_extraction(json).unwrap();
        assert_eq!(features.visit_type, VisitType::MinorAssessment);
        assert!(features.procedures.is_empty());
        assert_eq!(features.setting, EncounterSetting::InOffice);
        assert!(!features.is_after_hours);
        assert_eq!(features.estimated_duration_minutes, Some(10));
    }

    #[test]
    fn test_parse_with_code_fence() {
        let response = "```json\n{\"visitType\":\"general_assessment\",\"procedures\":[\"pap_smear\"],\"conditions\":[],\"setting\":\"in_office\",\"isNewPatient\":true,\"isAfterHours\":false,\"patientCount\":null,\"estimatedDurationMinutes\":25,\"confidence\":0.90}\n```";
        let features = parse_billing_extraction(response).unwrap();
        assert_eq!(features.visit_type, VisitType::GeneralAssessment);
        assert_eq!(features.procedures, vec![ProcedureType::PapSmear]);
        assert!(features.is_new_patient);
    }

    #[test]
    fn test_parse_with_leading_text() {
        let response = "Here is the extracted billing information:\n\n{\"visitType\":\"counselling\",\"procedures\":[],\"conditions\":[\"smoking_cessation\"],\"setting\":\"telephone_remote\",\"isNewPatient\":false,\"isAfterHours\":true,\"patientCount\":null,\"estimatedDurationMinutes\":15,\"confidence\":0.85}";
        let features = parse_billing_extraction(response).unwrap();
        assert_eq!(features.visit_type, VisitType::Counselling);
        assert_eq!(features.conditions, vec![ConditionType::SmokingCessation]);
        assert!(features.is_after_hours);
        assert_eq!(features.setting, EncounterSetting::TelephoneRemote);
    }

    #[test]
    fn test_parse_with_procedures_and_conditions() {
        let json = r#"{"visitType":"general_reassessment","procedures":["skin_biopsy","cryotherapy_multiple"],"conditions":["diabetes_management","chf_management"],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":30,"confidence":0.88}"#;
        let features = parse_billing_extraction(json).unwrap();
        assert_eq!(features.procedures.len(), 2);
        assert_eq!(features.procedures[0], ProcedureType::SkinBiopsy);
        assert_eq!(features.procedures[1], ProcedureType::CryotherapyMultiple);
        assert_eq!(features.conditions.len(), 2);
        assert_eq!(features.conditions[0], ConditionType::DiabetesManagement);
        assert_eq!(features.conditions[1], ConditionType::ChfManagement);
    }

    #[test]
    fn test_parse_prenatal() {
        let json = r#"{"visitType":"prenatal_major","procedures":[],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":30,"confidence":0.92}"#;
        let features = parse_billing_extraction(json).unwrap();
        assert_eq!(features.visit_type, VisitType::PrenatalMajor);
    }

    #[test]
    fn test_parse_shared_appointment() {
        let json = r#"{"visitType":"shared_appointment","procedures":[],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":3,"estimatedDurationMinutes":60,"confidence":0.80}"#;
        let features = parse_billing_extraction(json).unwrap();
        assert_eq!(features.visit_type, VisitType::SharedAppointment);
        assert_eq!(features.patient_count, Some(3));
    }

    #[test]
    fn test_parse_error_no_json() {
        let result = parse_billing_extraction("No JSON here at all");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No JSON object found"));
    }

    #[test]
    fn test_parse_error_invalid_json() {
        let result = parse_billing_extraction("{invalid json}");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse"));
    }

    #[test]
    fn test_parse_error_wrong_enum_value() {
        let json = r#"{"visitType":"unknown_type","procedures":[],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":10,"confidence":0.5}"#;
        let result = parse_billing_extraction(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_serde_visit_type_roundtrip() {
        for (variant, expected) in [
            (VisitType::MinorAssessment, "\"minor_assessment\""),
            (VisitType::GeneralAssessment, "\"general_assessment\""),
            (VisitType::PrenatalMajor, "\"prenatal_major\""),
            (VisitType::PalliativeCare, "\"palliative_care\""),
            (VisitType::SharedAppointment, "\"shared_appointment\""),
            (VisitType::WellBabyVisit, "\"well_baby_visit\""),
            (VisitType::PeriodicHealthVisit, "\"periodic_health_visit\""),
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected, "VisitType serialization mismatch for {:?}", variant);
            let deser: VisitType = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, variant);
        }
    }

    #[test]
    fn test_serde_procedure_type_roundtrip() {
        let all_procedures = vec![
            ProcedureType::PapSmear,
            ProcedureType::IudInsertion,
            ProcedureType::IudRemoval,
            ProcedureType::LesionExcisionSmall,
            ProcedureType::AbscessDrainage,
            ProcedureType::SkinBiopsy,
            ProcedureType::CryotherapySingle,
            ProcedureType::CryotherapyMultiple,
            ProcedureType::ElectrocoagulationSingle,
            ProcedureType::ElectrocoagulationMultiple,
            ProcedureType::BenignExcisionSmall,
            ProcedureType::BenignExcisionMedium,
            ProcedureType::LacerationRepairComplex,
            ProcedureType::EpistaxisCautery,
            ProcedureType::Sigmoidoscopy,
            ProcedureType::Anoscopy,
            ProcedureType::HemorrhoidIncision,
            ProcedureType::CornealForeignBody,
            ProcedureType::Immunization,
            ProcedureType::InjectionSoleReason,
        ];
        for p in all_procedures {
            let json = serde_json::to_string(&p).unwrap();
            let deser: ProcedureType = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, p, "ProcedureType roundtrip failed for {:?}", p);
        }
    }

    #[test]
    fn test_serde_condition_type_roundtrip() {
        for c in [
            ConditionType::DiabetesManagement,
            ConditionType::SmokingCessation,
            ConditionType::StiManagement,
            ConditionType::ChfManagement,
            ConditionType::Neurocognitive,
            ConditionType::HomeCare,
            ConditionType::SmokingCessationFollowUp,
        ] {
            let json = serde_json::to_string(&c).unwrap();
            let deser: ConditionType = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, c, "ConditionType roundtrip failed for {:?}", c);
        }
    }

    #[test]
    fn test_serde_encounter_setting_roundtrip() {
        for s in [
            EncounterSetting::InOffice,
            EncounterSetting::HomeVisit,
            EncounterSetting::TelephoneInOffice,
            EncounterSetting::TelephoneRemote,
            EncounterSetting::Video,
        ] {
            let json = serde_json::to_string(&s).unwrap();
            let deser: EncounterSetting = serde_json::from_str(&json).unwrap();
            assert_eq!(deser, s, "EncounterSetting roundtrip failed for {:?}", s);
        }
    }

    #[test]
    fn test_clinical_features_camel_case() {
        let features = ClinicalFeatures {
            visit_type: VisitType::MinorAssessment,
            procedures: vec![],
            conditions: vec![],
            setting: EncounterSetting::InOffice,
            is_new_patient: false,
            is_after_hours: false,
            patient_count: None,
            estimated_duration_minutes: Some(10),
            confidence: 0.95,
        };
        let json = serde_json::to_string(&features).unwrap();
        assert!(json.contains("\"visitType\""));
        assert!(json.contains("\"isNewPatient\""));
        assert!(json.contains("\"isAfterHours\""));
        assert!(json.contains("\"patientCount\""));
        assert!(json.contains("\"estimatedDurationMinutes\""));
    }
}
