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
    Consultation,
    RepeatConsultation,
    LimitedConsultation,
    VirtualVideo,
    VirtualPhone,
    HouseCall,
    EmergencyDeptEquiv,
    PeriodicHealthChild,
    PeriodicHealthAdolescent,
    PeriodicHealthAdult,
    PeriodicHealthSenior,
    PeriodicHealthIdd,
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
    ImmunizationFlu,
    ImmunizationTdap,
    ImmunizationHepB,
    ImmunizationHpv,
    ImmunizationMmr,
    ImmunizationPneumococcal,
    ImmunizationVaricella,
    ImmunizationPediatric,
    InjectionSoleReason,
    // Injections
    JointInjection,
    JointInjectionAdditional,
    TriggerPointInjection,
    TriggerPointAdditional,
    ImInjectionWithVisit,
    IntralesionalSmall,
    IntralesionalLarge,
    IntravenousAdmin,
    // Nerve blocks
    NerveBlockPeripheral,
    NerveBlockParavertebral,
    NerveBlockAdditional,
    // Other common procedures
    EarSyringing,
    Tonometry,
    NailDebridement,
    NailExcisionSingle,
    NailExcisionMultiple,
    ForeignBodyRemoval,
    BiopsyWithSutures,
    WoundCatheterization,
    GroupThreeExcisionFace,
    GroupThreeExcisionOther,
    GroupOneExcisionSingle,
    GroupOneExcisionTwo,
    GroupOneExcisionThree,
    NevusExcision,
    PapSmearRepeat,
    ElectrocoagThreeOrMore,
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
    PrimaryMentalHealth,
    Psychotherapy,
    HivPrimaryCare,
    InsulinTherapySupport,
    DiabeticAssessment,
    CounsellingAdditional,
    FibromyalgiaCare,
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
    transcript: &str,
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

### visitType (pick ONE — the primary assessment type)
- "minor_assessment" — Single focused complaint, brief history + targeted exam, <10 min. Examples: UTI symptom check, single Rx renewal with brief exam, wart assessment, single rash review (A001A)
- "intermediate_assessment" — Moderate complexity, 1-2 issues addressed, 10-20 min. Standard follow-up visits, well-baby checks, routine chronic disease follow-up (A007A)
- "general_assessment" — Comprehensive NEW patient workup OR annual complete exam. Multi-system history + full physical exam, typically 20-45 min. Must include multiple body systems examined (A003A)
- "general_reassessment" — Comprehensive ESTABLISHED patient follow-up addressing multiple active problems. Multi-system review, typically 20-30 min. NOT a simple single-issue follow-up (A004A)
- "mini_assessment" — Very brief encounter, <5 min. Single Rx renewal without exam, form signature, phone message relay (A008A)
- "prenatal_major" — FIRST prenatal visit: complete obstetric history, baseline labs ordered, dating, risk assessment. Only use for initial pregnancy visit (P003A)
- "prenatal_minor" — FOLLOW-UP prenatal visit: fundal height, FHR, BP check, routine monitoring. NOT the first visit (P004A)
- "palliative_care" — Patient with terminal diagnosis receiving palliative/end-of-life care. Must involve symptom management for life-limiting illness (K023A)
- "counselling" — Visit is PRIMARILY counselling/psychotherapy, not a physical assessment. Extended discussion of mental health, lifestyle, substance use. The main activity is talking, not examining (K005A/K013A)
- "shared_appointment" — Multiple patients seen TOGETHER in a group visit for chronic disease education (diabetes, COPD, etc.). NOT a family visit where patients are seen separately (K140-K144A)
- "well_baby_visit" — Well-baby or well-child preventive health check at standard intervals (2w, 1m, 2m, 4m, 6m, 9m, 12m, 18m, 24m). Developmental milestones + immunizations (A007A)
- "consultation" — Formal consultation following a written referral from another physician. Requires referral letter (A005A)
- "repeat_consultation" — Follow-up consultation for previously consulted patient, same problem (A006A)
- "limited_consultation" — Less demanding consultation, requires less physician time than full consultation (A905A)
- "virtual_video" — Video visit using telemedicine platform (A101A)
- "virtual_phone" — Telephone visit (A102A)
- "house_call" — Complex house call assessment for frail/housebound patient at their home (A900A)
- "emergency_dept_equiv" — Emergency department equivalent assessment on weekend/holiday (A888A)
- "periodic_health_child" — Periodic health visit for child after second birthday (K017A)
- "periodic_health_adolescent" — Periodic health visit for adolescent 16-17 (K130A)
- "periodic_health_adult" — Periodic health visit for adult 18-64 (K131A)
- "periodic_health_senior" — Periodic health visit for adult 65+ (K132A)
- "periodic_health_idd" — Periodic health visit for adult with intellectual/developmental disability (K133A)

### procedures (array — only include procedures ACTUALLY PERFORMED during this visit, not just discussed)
- "pap_smear" — Pap smear/cervical cytology ACTUALLY COLLECTED during visit. Must involve speculum exam + sample collection (G365A)
- "iud_insertion" — IUD physically INSERTED during this visit. Not just counselling about IUD (G378A)
- "iud_removal" — IUD physically REMOVED during this visit (G552A)
- "lesion_excision_small" — Excision of MALIGNANT/suspicious skin lesion <1cm with scalpel. Pathology sent. Not cryotherapy (R048A)
- "lesion_excision_medium" — Excision of malignant/suspicious lesion 1-2cm (R094A)
- "lesion_excision_large" — Excision of malignant/suspicious lesion >2cm (R094A)
- "abscess_drainage" — Incision and drainage of abscess performed. Requires cutting, draining, packing (Z101A)
- "skin_biopsy" — Skin biopsy using any method where sutures are NOT used (punch biopsy, shave biopsy). If sutures ARE used, use biopsy_with_sutures instead (Z113A)
- "cryotherapy_single" — Liquid nitrogen or chemical treatment applied to skin lesion(s) (wart, actinic keratosis). Actual treatment performed (Z117A)
- "cryotherapy_multiple" — Same as cryotherapy_single — Z117A covers one or more lesions (Z117A)
- "electrocoagulation_single" — Electrocautery/electrocoagulation/curetting of single lesion (Z159A)
- "electrocoagulation_multiple" — Electrocautery/curetting of two lesions (Z160A)
- "benign_excision_small" — Excision of BENIGN lesion (cyst, lipoma) with closure (Z125A)
- "benign_excision_medium" — Excision of benign lesion, larger (Z125A)
- "laceration_repair_simple_small" — Suturing of simple wound/laceration <5cm. Must involve actual sutures/staples (Z154A)
- "laceration_repair_simple_large" — Suturing of simple laceration 5.1-10cm (Z175A)
- "laceration_repair_complex" — Complex laceration repair: deep tissue, layered closure, debridement (Z176A)
- "epistaxis_cautery" — Silver nitrate or electrocautery applied to nasal blood vessel for active nosebleed (Z314A)
- "epistaxis_packing" — Anterior nasal packing inserted for uncontrolled nosebleed (Z315A)
- "sigmoidoscopy" — Flexible sigmoidoscopy PERFORMED in office (Z535A)
- "anoscopy" — Anoscopy PERFORMED in office (Z543A)
- "hemorrhoid_incision" — Thrombosed external hemorrhoid incised and drained (Z545A)
- "corneal_foreign_body" — Foreign body REMOVED from cornea with slit lamp/needle (Z847A)
- "immunization" — Generic vaccine administration not specifically listed below (G538A)
- "immunization_flu" — Influenza vaccine administered (G590A)
- "immunization_tdap" — Adult Tdap (tetanus/diphtheria/pertussis) vaccine (G847A)
- "immunization_hep_b" — Hepatitis B vaccine (G842A)
- "immunization_hpv" — HPV vaccine (G843A)
- "immunization_mmr" — MMR (measles/mumps/rubella) vaccine (G845A)
- "immunization_pneumococcal" — Pneumococcal conjugate vaccine (G846A)
- "immunization_varicella" — Varicella (chickenpox) vaccine (G848A)
- "immunization_pediatric" — Paediatric DTaP/IPV or DTaP-IPV-Hib vaccine (G840A/G841A)
- "injection_sole_reason" — IM/SC/intradermal injection as the SOLE reason for visit with NO assessment performed (e.g., flu shot walk-in, B12 injection only). For joint/trigger point/nerve block injections, use the specific injection code instead (G373A)
- "joint_injection" — Injection INTO a joint, bursa, ganglion, or tendon sheath (e.g., knee injection, shoulder injection, cortisone injection into joint). NOT an IM injection (G370A)
- "joint_injection_additional" — Each additional joint/bursa/ganglion/tendon sheath injected in the same visit (max 5). Use WITH joint_injection (G371A)
- "trigger_point_injection" — Infiltration of tissue at a trigger point for pain relief. NOT a joint injection (G384A)
- "trigger_point_additional" — Each additional trigger point site (max 2). Use WITH trigger_point_injection (G385A)
- "im_injection_with_visit" — Additional IM/SC/intradermal injection given during a visit that also included an assessment. NOT the sole reason for visit (G372A)
- "intralesional_small" — Intralesional infiltration of 1-2 lesions (e.g., steroid injection into keloid or cyst) (G375A)
- "intralesional_large" — Intralesional infiltration of 3+ lesions (G377A)
- "intravenous_admin" — Intravenous administration to child/adolescent/adult (G379A)
- "nerve_block_peripheral" — Somatic or peripheral nerve block at one nerve or site (e.g., digital block, intercostal) (G231A)
- "nerve_block_paravertebral" — Paravertebral nerve block at any spinal level: cervical, thoracic, lumbar, sacral, or coccygeal (G228A)
- "nerve_block_additional" — Additional nerve block site(s) in the same visit (G223A)
- "ear_syringing" — Ear syringing, curetting, or debridement — unilateral or bilateral (G420A)
- "tonometry" — Tonometry eye pressure measurement (G435A)
- "nail_debridement" — Extensive debridement of onychogryphotic (thickened) nail (Z110A)
- "nail_excision_single" — Nail plate excision requiring anaesthesia — one nail (Z128A)
- "nail_excision_multiple" — Nail plate excision requiring anaesthesia — multiple nails (Z129A)
- "foreign_body_removal" — Foreign body removal from skin/subcutaneous tissue under local anaesthetic (Z114A)
- "biopsy_with_sutures" — Skin biopsy using any method where sutures ARE used (Z116A)
- "catheterization" — Urinary catheterization in hospital (Z611A)
- "cyst_excision_face" — Excision of cyst, haemangioma, or lipoma — face or neck (Z122A)
- "cyst_excision_other" — Excision of cyst, haemangioma, or lipoma — other body areas (Z125A)
- "keratosis_excision_single" — Group 1 lesion (keratosis, pyogenic granuloma) removal by excision and suture — single (Z156A)
- "keratosis_excision_two" — Group 1 lesion removal by excision and suture — two lesions (Z157A)
- "keratosis_excision_three" — Group 1 lesion removal by excision and suture — three or more (Z158A)
- "nevus_excision" — Group 2 lesion (nevus/mole) removal by excision and suture — single (Z162A)
- "pap_smear_repeat" — Additional or repeat Pap smear collection (not the periodic one) (G394A)
- "electrocoagulation_three_plus" — Electrocoagulation/curetting of three or more Group 1 lesions (Z161A)

### conditions (array — only include if SPECIFIC MANAGEMENT was done, not just mentioned in history)
- "diabetes_management" — Active diabetes care: A1C review, medication adjustment, glucose log review, diabetic foot exam, or diet counselling for diabetes. Must involve treatment decisions, not just "has diabetes" in history (Q040A)
- "smoking_cessation" — Active smoking cessation COUNSELLING provided: discussed quit date, NRT options, triggers, or started cessation medication. Not just "smoker" noted in history (Q042A)
- "sti_management" — STI testing ORDERED/PERFORMED, STI treatment prescribed, or contact tracing discussed. Active STI workup, not just sexual history taking (K028A)
- "chf_management" — Active CHF management: fluid status assessment, diuretic adjustment, weight monitoring, exercise counselling specifically for heart failure. Not just "has CHF" in history (Q050A)
- "neurocognitive" — FORMAL cognitive assessment performed: MMSE, MoCA, clock drawing test, or structured dementia screening tool administered and scored. Takes 20+ min of dedicated testing. NOT general mental status observations, NOT memory complaints discussed without formal testing, NOT neurological exam for non-cognitive concerns (K032A)
- "home_care" — Home care services APPLICATION submitted (CCAC/home care referral form) or active home care supervision visit (K070A/K071A)
- "smoking_cessation_follow_up" — FOLLOW-UP visit specifically for smoking cessation progress. Patient previously started cessation program, this is a check-in on quit attempt (K039A)
- "primary_mental_health" — Primary mental health care session (K005A). Individual care, minimum 20 min per unit. NOT psychotherapy
- "psychotherapy" — Individual psychotherapy session (K007A). Minimum 30 min per unit
- "hiv_primary_care" — HIV primary care management session (K022A). Minimum 20 min per unit
- "insulin_therapy_support" — Insulin therapy support: training patients on insulin use, device education (K029A). Max 6 units/year
- "diabetic_assessment" — Dedicated diabetic management assessment: A1C review, foot exam, medication adjustment (K030A). Max 4/year. Different from diabetes_management incentive (Q040A)
- "counselling_additional" — Counselling after the first 3 K013 units are exhausted for the year (K033A). Out-of-basket at full FFS
- "fibromyalgia_care" — Fibromyalgia or myalgic encephalomyelitis care session (K037A). Per unit

### setting
- "in_office" — Standard in-person office visit (default if not specified)
- "home_visit" — Physician traveled to patient's home for the visit
- "telephone_in_office" — Phone call from the clinic/office to patient
- "telephone_remote" — Phone call when physician is NOT physically in the clinic (e.g., from home, after hours)
- "video" — Video/virtual visit via telemedicine platform

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

### Example 6: Follow-up with knee injection
SOAP: "S: Follow-up for right knee OA. Pain worse with stairs. O: Moderate effusion R knee. A: Right knee OA, moderate. P: Cortisone injection into right knee performed. Follow-up 6 weeks."
```json
{"visitType":"intermediate_assessment","procedures":["joint_injection"],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":15,"confidence":0.95}
```

### Example 7: Multiple trigger point injections
SOAP: "S: Chronic neck and upper back pain. Multiple tender trigger points. O: Trigger points identified at bilateral trapezius and right levator scapulae. A: Myofascial pain syndrome. P: Trigger point injections performed at 3 sites with lidocaine."
```json
{"visitType":"intermediate_assessment","procedures":["trigger_point_injection","trigger_point_additional"],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":20,"confidence":0.95}
```

IMPORTANT: Only include procedures that were PHYSICALLY PERFORMED, not just discussed or planned. Only include conditions where ACTIVE MANAGEMENT occurred during this visit, not conditions merely listed in the patient's medical history. When uncertain, leave the array empty rather than guessing.

Respond ONLY with the JSON object. No explanations or commentary."#;

    let user_prompt = format!(
        "## SOAP Note\n\n{}\n\n## Full Transcript\n\n{}",
        soap_content, transcript
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

    // Try strict parse first
    match serde_json::from_str::<ClinicalFeatures>(json_str) {
        Ok(features) => return Ok(features),
        Err(_) => {}
    }

    // Fallback: use streaming deserializer to handle trailing content after valid JSON
    let mut deserializer = serde_json::Deserializer::from_str(json_str);
    match ClinicalFeatures::deserialize(&mut deserializer) {
        Ok(features) => Ok(features),
        Err(e) => Err(format!(
            "Failed to parse clinical features JSON: {}. Input: {}",
            e,
            &json_str[..json_str.len().min(200)]
        )),
    }
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
        assert!(system.contains("consultation"));
        assert!(system.contains("virtual_video"));
        assert!(system.contains("periodic_health_adult"));
        assert!(system.contains("immunization_flu"));
        assert!(system.contains("primary_mental_health"));
        assert!(system.contains("pap_smear_repeat"));
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
    fn test_parse_with_trailing_content() {
        // LLM sometimes adds commentary after the JSON object
        let response = r#"{"visitType":"intermediate_assessment","procedures":[],"conditions":["diabetes_management"],"setting":"telephone_remote","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":15,"confidence":0.9}

This patient had a diabetes follow-up via phone."#;
        let features = parse_billing_extraction(response).unwrap();
        assert_eq!(features.visit_type, VisitType::IntermediateAssessment);
        assert_eq!(features.conditions, vec![ConditionType::DiabetesManagement]);
        assert_eq!(features.setting, EncounterSetting::TelephoneRemote);
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
            (VisitType::Consultation, "\"consultation\""),
            (VisitType::RepeatConsultation, "\"repeat_consultation\""),
            (VisitType::LimitedConsultation, "\"limited_consultation\""),
            (VisitType::VirtualVideo, "\"virtual_video\""),
            (VisitType::VirtualPhone, "\"virtual_phone\""),
            (VisitType::HouseCall, "\"house_call\""),
            (VisitType::EmergencyDeptEquiv, "\"emergency_dept_equiv\""),
            (VisitType::PeriodicHealthChild, "\"periodic_health_child\""),
            (VisitType::PeriodicHealthAdolescent, "\"periodic_health_adolescent\""),
            (VisitType::PeriodicHealthAdult, "\"periodic_health_adult\""),
            (VisitType::PeriodicHealthSenior, "\"periodic_health_senior\""),
            (VisitType::PeriodicHealthIdd, "\"periodic_health_idd\""),
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
            ProcedureType::ImmunizationFlu,
            ProcedureType::ImmunizationTdap,
            ProcedureType::ImmunizationHepB,
            ProcedureType::ImmunizationHpv,
            ProcedureType::ImmunizationMmr,
            ProcedureType::ImmunizationPneumococcal,
            ProcedureType::ImmunizationVaricella,
            ProcedureType::ImmunizationPediatric,
            ProcedureType::InjectionSoleReason,
            ProcedureType::JointInjection,
            ProcedureType::JointInjectionAdditional,
            ProcedureType::TriggerPointInjection,
            ProcedureType::TriggerPointAdditional,
            ProcedureType::ImInjectionWithVisit,
            ProcedureType::IntralesionalSmall,
            ProcedureType::IntralesionalLarge,
            ProcedureType::IntravenousAdmin,
            ProcedureType::NerveBlockPeripheral,
            ProcedureType::NerveBlockParavertebral,
            ProcedureType::NerveBlockAdditional,
            ProcedureType::EarSyringing,
            ProcedureType::Tonometry,
            ProcedureType::NailDebridement,
            ProcedureType::NailExcisionSingle,
            ProcedureType::NailExcisionMultiple,
            ProcedureType::ForeignBodyRemoval,
            ProcedureType::BiopsyWithSutures,
            ProcedureType::WoundCatheterization,
            ProcedureType::GroupThreeExcisionFace,
            ProcedureType::GroupThreeExcisionOther,
            ProcedureType::GroupOneExcisionSingle,
            ProcedureType::GroupOneExcisionTwo,
            ProcedureType::GroupOneExcisionThree,
            ProcedureType::NevusExcision,
            ProcedureType::PapSmearRepeat,
            ProcedureType::ElectrocoagThreeOrMore,
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
            ConditionType::PrimaryMentalHealth,
            ConditionType::Psychotherapy,
            ConditionType::HivPrimaryCare,
            ConditionType::InsulinTherapySupport,
            ConditionType::DiabeticAssessment,
            ConditionType::CounsellingAdditional,
            ConditionType::FibromyalgiaCare,
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
