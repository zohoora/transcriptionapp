use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Convert a serde-tagged enum to its `rename_all = "snake_case"` string key.
/// Used for round-tripping `ConditionType` / `ProcedureType` / `VisitType`
/// values into the snake_case strings the LLM emits and the rule engine /
/// experiment CLIs key off. Returns `None` only when the enum variant
/// somehow doesn't serialize to a JSON string (shouldn't happen for our
/// flat enums).
pub fn enum_to_snake_key<T: Serialize>(t: &T) -> Option<String> {
    serde_json::to_value(t).ok().and_then(|v| v.as_str().map(String::from))
}

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
    IddPrimaryCare,
    OpioidWithdrawalManagement,
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
    /// LLM-suggested OHIP 3-digit diagnostic code (validated against database post-extraction).
    /// Legacy field — kept for backward compat. Prefer `primary_diagnosis` text matching.
    #[serde(default)]
    pub suggested_diagnostic_code: Option<String>,
    /// Plain-text primary diagnosis from the SOAP Assessment section.
    /// The rule engine resolves this to a 3-digit OHIP diagnostic code via database search.
    #[serde(default)]
    pub primary_diagnosis: Option<String>,
    /// Evidence quotes from the SOAP note justifying each condition.
    /// Key = condition enum value (e.g. "diabetic_assessment"), value = brief quote.
    /// K-code conditions without evidence are suppressed by the rule engine.
    #[serde(default)]
    pub condition_evidence: HashMap<String, String>,
}

/// Billing-extraction prompt version tag (v0.10.62+). Stamped on
/// `ArchiveMetadata.billing_prompt_version` when billing is archived.
/// Bump this string whenever `build_billing_extraction_prompt` is materially
/// edited so audits can correlate billing drift to specific prompt revisions.
pub const BILLING_PROMPT_VERSION: &str = "v0.10.61";

// ── Prompt construction ────────────────────────────────────────────────────

/// Build the system + user prompts for clinical feature extraction.
/// `context_hints` contains physician-provided billing context (visit setting, patient age, etc.).
/// Returns `(system_prompt, user_prompt)`.
/// When `templates` is provided and the relevant field is non-empty, it overrides the hardcoded default.
pub fn build_billing_extraction_prompt(
    soap_content: &str,
    transcript: &str,
    context_hints: &str,
    templates: Option<&crate::server_config::PromptTemplates>,
) -> (String, String) {
    let system_prompt = templates
        .and_then(|t| (!t.billing_extraction.is_empty()).then(|| t.billing_extraction.clone()))
        .unwrap_or_else(|| r#"You are a medical billing analyst for Ontario FHO+ family medicine. Your task is to extract clinical features from a SOAP note and transcript to determine appropriate OHIP billing codes.

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
  "confidence": <float 0.0-1.0>,
  "primaryDiagnosis": "<plain-text primary diagnosis from the Assessment section>",
  "suggestedDiagnosticCode": "<3-digit OHIP diagnostic code or null>",
  "conditionEvidence": {"<condition_name>": "<brief quote from SOAP supporting this condition>", ...}
}
```

### primaryDiagnosis (REQUIRED)
Extract the single most important diagnosis from the Assessment section as a short plain-text phrase (e.g. "COPD with progressive dyspnea", "Type 2 diabetes suboptimal control", "Anxiety disorder"). This will be matched to an OHIP diagnostic code automatically. Be specific — use the clinical language from the Assessment.

### suggestedDiagnosticCode (optional — 3-digit OHIP diagnostic code)
If you are confident of the exact code, provide it. Common codes: 250 Diabetes | 401 Hypertension | 428 CHF | 496 COPD | 493 Asthma | 311 Depression | 300 Anxiety | 715 Osteoarthritis | 724 Back pain | 917 Annual health exam | 707 Chronic skin ulcer / wound | 451 Phlebitis / DVT / thrombophlebitis | 216 Benign skin neoplasm (skin tags / acrochordons / nevi) | 574 Cholelithiasis / gallstones / pre-cholecystectomy | 692 Eczema / dermatitis | 477 Allergic rhinitis | 460 Common cold / nasopharyngitis | 599 UTI | 785 Cardiovascular symptoms (edema, chest pain) | 786 Respiratory symptoms (cough, dyspnea) | 787 GI symptoms | 788 Urinary symptoms | 780 General symptoms (fatigue, dizziness). Use 799 (ill-defined) ONLY when no other 3-digit category fits — prefer a body-system code over 799 wherever possible. Leave null if uncertain — the system will resolve from primaryDiagnosis.

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
- "benign_excision_small" — Excision of small benign skin lesion (NOT lipoma/cyst — use cyst_excision for those) (Z125A)
- "benign_excision_medium" — Excision of larger benign skin lesion (NOT lipoma/cyst) (Z125A)
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
- "nail_debridement" — Extensive debridement of thickened (onychogryphotic) nail WITHOUT removal. Trimming/grinding only (Z110A)
- "nail_excision_single" — Partial or complete nail plate REMOVAL/AVULSION of one nail (e.g., ingrown toenail removal, nail avulsion with phenol). Requires local anaesthetic/nerve block (Z128A)
- "nail_excision_multiple" — Nail plate removal of multiple nails in same visit (Z129A)
- "foreign_body_removal" — Foreign body removal from skin/subcutaneous tissue under local anaesthetic (Z114A)
- "biopsy_with_sutures" — Skin biopsy using any method where sutures ARE used (Z116A)
- "catheterization" — Urinary catheterization in hospital (Z611A)
- "cyst_excision_face" — Excision of cyst, haemangioma, lipoma, or sebaceous cyst — FACE or NECK location (Z122A)
- "cyst_excision_other" — Excision of cyst, haemangioma, lipoma, or sebaceous cyst — BODY location (back, arm, leg, trunk). Use this for lipoma excision on the body (Z125A)
- "keratosis_excision_single" — Group 1 lesion (keratosis, pyogenic granuloma) removal by excision and suture — single (Z156A)
- "keratosis_excision_two" — Group 1 lesion removal by excision and suture — two lesions (Z157A)
- "keratosis_excision_three" — Group 1 lesion removal by excision and suture — three or more (Z158A)
- "nevus_excision" — Group 2 lesion (nevus/mole) removal by excision and suture — single (Z162A)
- "pap_smear_repeat" — Additional or repeat Pap smear collection (not the periodic one) (G394A)
- "electrocoagulation_three_plus" — Electrocoagulation/curetting of three or more Group 1 lesions (Z161A)

### conditions (array — only include if SPECIFIC MANAGEMENT was done, not just mentioned in history)
- "diabetes_management" — Diabetes management INCENTIVE: billable when at least 3 diabetic assessment visits (K030A) have occurred in the past year. Include this whenever diabetic_assessment is also included (Q040A)
- "smoking_cessation" — Active TOBACCO smoking cessation COUNSELLING. REQUIRES: patient is a CURRENT TOBACCO smoker (cigarettes / cigars / chewing tobacco / vaping nicotine) AND clinician discussed quit date, NRT/varenicline/bupropion options, withdrawal triggers, or started cessation medication. DO NOT use for: cannabis/marijuana counselling, decongestant nasal spray cessation, alcohol cessation, opioid tapering, or any non-tobacco substance. (Q042A)
- "sti_management" — STI testing ORDERED/PERFORMED, STI treatment prescribed, or contact tracing discussed. Active STI workup, not just sexual history taking (K028A)
- "chf_management" — Active CONGESTIVE HEART FAILURE management. REQUIRES: explicit diagnosis of heart failure / CHF / cardiomyopathy / reduced ejection fraction in the Assessment AND active HF-specific management (diuretic dose adjustment, fluid/salt restriction counselling, daily-weight monitoring instructions, ACE-I/ARB/SGLT2 titration FOR HEART FAILURE, BNP/echo review). DO NOT use for: routine hypertension management, beta-blocker for HTN, BP-only monitoring, isolated bradycardia/arrhythmia management. Bisoprolol/metoprolol alone for HTN is NOT chf_management. (Q050A)
- "neurocognitive" — FORMAL cognitive assessment performed: MMSE, MoCA, clock drawing test, or structured dementia screening tool administered and scored. Takes 20+ min of dedicated testing. NOT general mental status observations, NOT memory complaints discussed without formal testing, NOT neurological exam for non-cognitive concerns (K032A)
- "home_care" — Home care services APPLICATION submitted (CCAC/home care referral form) or active home care supervision visit (K070A/K071A)
- "smoking_cessation_follow_up" — FOLLOW-UP visit specifically for smoking cessation progress. Patient previously started cessation program, this is a check-in on quit attempt (K039A)
- "primary_mental_health" — Primary mental health care session (K005A). Individual care, minimum 20 min per unit. NOT psychotherapy
- "psychotherapy" — Individual psychotherapy session (K007A). Minimum 30 min per unit
- "hiv_primary_care" — HIV primary care management session (K022A). Minimum 20 min per unit
- "insulin_therapy_support" — Insulin therapy support: training patients on insulin use, device education (K029A). Max 6 units/year
- "diabetic_assessment" — Dedicated DIABETIC management assessment. REQUIRES: explicit diabetes / type 1 DM / type 2 DM / gestational diabetes diagnosis in the Assessment AND active diabetic-specific management (A1C / HbA1c review, fasting glucose / fingerstick review, foot exam for neuropathy, retinopathy referral, insulin / metformin / SGLT2 / GLP-1 dose adjustment FOR DIABETES, diabetic dietary counselling). DO NOT use for: Ozempic / semaglutide / GLP-1 prescribed for WEIGHT LOSS or cardiovascular benefit (without diabetes diagnosis); routine BMI/weight discussion; ordering thyroid or iron labs; metabolic syndrome without diabetes; family history of diabetes only. (K030A). Max 4/year. Include BOTH diabetic_assessment AND diabetes_management when diabetes is actively managed
- "counselling_additional" — Counselling after the first 3 K013 units are exhausted for the year (K033A). Out-of-basket at full FFS
- "fibromyalgia_care" — Fibromyalgia or myalgic encephalomyelitis care session (K037A). Per unit
- "idd_primary_care" — Primary care for patient with intellectual/developmental disability (autism, Down syndrome, cerebral palsy, FAS, spina bifida). Minimum 20 min. Requires IDD diagnosis (K125A)
- "opioid_withdrawal_management" — Active opioid use disorder management: methadone/suboxone/buprenorphine prescribing, dose adjustment, tapering plan, or withdrawal assessment. Not just noting opioid use in history (K005A)

### conditionEvidence (REQUIRED for every condition listed)
For EACH condition in the conditions array, provide a brief quote or paraphrase from the SOAP note that supports it. The key must match the condition name exactly. If you cannot find specific evidence in the SOAP note for a condition, do NOT include that condition.

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
{"visitType":"minor_assessment","procedures":[],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":8,"confidence":0.95,"primaryDiagnosis":"Viral pharyngitis","suggestedDiagnosticCode":"463"}
```

### Example 2: General assessment with procedure
SOAP: "S: New patient, 45F, referred for suspicious mole on back. Full history obtained. O: Full exam. 8mm pigmented lesion on upper back, irregular borders. Excision performed. A: Suspicious melanocytic lesion. P: Pathology pending, follow-up 2 weeks."
```json
{"visitType":"general_assessment","procedures":["lesion_excision_small"],"conditions":[],"setting":"in_office","isNewPatient":true,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":30,"confidence":0.92,"primaryDiagnosis":"Suspicious melanocytic lesion upper back","suggestedDiagnosticCode":"216"}
```

### Example 3: Prenatal visit
SOAP: "S: 28 weeks gestation, routine follow-up. O: BP 120/80, fundal height 28cm, FHR 145. A: Normal pregnancy progression. P: Routine labs, next visit 2 weeks."
```json
{"visitType":"prenatal_minor","procedures":[],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":15,"confidence":0.95,"primaryDiagnosis":"Normal pregnancy progression 28 weeks","suggestedDiagnosticCode":"650"}
```

### Example 4: Chronic disease management
SOAP: "S: Type 2 DM follow-up, A1C review. O: A1C 7.8%, up from 7.2%. BMI 31. A: Suboptimal diabetes control. P: Increase metformin, dietary counselling, recheck A1C in 3 months."
```json
{"visitType":"general_reassessment","procedures":[],"conditions":["diabetic_assessment","diabetes_management"],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":20,"confidence":0.90,"primaryDiagnosis":"Suboptimal type 2 diabetes control","suggestedDiagnosticCode":"250","conditionEvidence":{"diabetic_assessment":"A1C 7.8%, up from 7.2%, increase metformin, recheck A1C in 3 months","diabetes_management":"A1C review, medication adjustment, dietary counselling"}}
```

### Example 5: After-hours counselling
SOAP: "S: Patient called after hours, anxious about chest tightness. O: History taken by phone. A: Anxiety-related symptoms, no red flags. P: Reassurance, follow-up tomorrow if persists."
```json
{"visitType":"counselling","procedures":[],"conditions":[],"setting":"telephone_remote","isNewPatient":false,"isAfterHours":true,"patientCount":null,"estimatedDurationMinutes":15,"confidence":0.85,"primaryDiagnosis":"Anxiety-related chest tightness","suggestedDiagnosticCode":"300"}
```

### Example 6: Follow-up with knee injection
SOAP: "S: Follow-up for right knee OA. Pain worse with stairs. O: Moderate effusion R knee. A: Right knee OA, moderate. P: Cortisone injection into right knee performed. Follow-up 6 weeks."
```json
{"visitType":"intermediate_assessment","procedures":["joint_injection"],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":15,"confidence":0.95,"primaryDiagnosis":"Right knee osteoarthritis moderate","suggestedDiagnosticCode":"715"}
```

### Example 7: Multiple trigger point injections
SOAP: "S: Chronic neck and upper back pain. Multiple tender trigger points. O: Trigger points identified at bilateral trapezius and right levator scapulae. A: Myofascial pain syndrome. P: Trigger point injections performed at 3 sites with lidocaine."
```json
{"visitType":"intermediate_assessment","procedures":["trigger_point_injection","trigger_point_additional"],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":20,"confidence":0.95,"primaryDiagnosis":"Myofascial pain syndrome neck and upper back","suggestedDiagnosticCode":"726"}
```

### Example 8: Ingrown toenail — nerve block + nail removal (MULTIPLE procedures)
SOAP: "S: Painful ingrown toenail right great toe. O: Medial nail border embedded, erythema. A: Ingrown toenail with paronychia. P: Digital nerve block performed. Partial nail avulsion with phenol matrixectomy."
```json
{"visitType":"intermediate_assessment","procedures":["nerve_block_peripheral","nail_excision_single"],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":20,"confidence":0.95,"primaryDiagnosis":"Ingrown toenail with paronychia","suggestedDiagnosticCode":"703"}
```

### Example 9: Lipoma excision on back
SOAP: "S: Lump on upper back for 1 year. O: 3cm soft mobile subcutaneous mass. A: Lipoma right upper back. P: Excision performed under local anaesthesia. Wound closed with sutures."
```json
{"visitType":"intermediate_assessment","procedures":["cyst_excision_other"],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":25,"confidence":0.95,"primaryDiagnosis":"Lipoma right upper back","suggestedDiagnosticCode":"214"}
```

CRITICAL RULES:
1. Include ALL procedures physically performed during the visit — not just the main one. If a nerve block was done AND a nail excision, include BOTH. If an injection was given AND a biopsy taken, include BOTH.
2. Only include procedures that were PHYSICALLY PERFORMED, not just discussed or planned. A procedure is billable ONLY when described in the past tense ("applied", "injected", "froze", "drained", "sutured", "removed"). Modal verbs ("could", "would", "might", "will", "let me grab") without follow-through, or "discussed but used over-the-counter instead", do NOT count as performance.
3. Only include conditions where ACTIVE MANAGEMENT occurred during this visit, not conditions merely listed in the patient's medical history.
4. For visits involving procedures (injection, excision, biopsy, etc.), the visit type should typically be "intermediate_assessment" unless it was truly brief (<10 min) or comprehensive (multi-system).
5. When uncertain about a procedure, leave it out rather than guessing. But when a procedure is clearly described as performed, always include it.
6. If no condition from the list matches what happened in the visit, leave the conditions array EMPTY. Do NOT force-fit unrelated clinical activities into a condition. Ordering labs, making referrals, or general investigations are NOT conditions — they are part of the assessment and plan.
7. diabetic_assessment requires EXPLICIT diabetes management (A1C, glucose, diabetic medication adjustment, diabetic foot exam). Ordering thyroid or iron labs is NOT diabetic_assessment. Ozempic / GLP-1 agonists prescribed for WEIGHT LOSS without a diabetes diagnosis is NOT diabetic_assessment.
8. smoking_cessation requires TOBACCO use. Counselling on cannabis, marijuana, decongestant nasal sprays, or any non-tobacco substance is NOT smoking_cessation.
9. chf_management requires an EXPLICIT heart failure diagnosis. Routine hypertension management with a beta-blocker is NOT chf_management.

VISIT-TYPE CALIBRATION:
Calibrate visitType to the actual scope of the visit, not just count of issues:
- "minor_assessment" (A001A) — SINGLE focused complaint, brief history + targeted exam, <10 min. UTI symptom check, single Rx renewal with brief exam, single rash review.
- "intermediate_assessment" (A007A) — Moderate complexity, 1-2 issues addressed, 10-20 min. Standard follow-up, well-baby check, routine chronic disease follow-up.
- "general_reassessment" (A004A) — Comprehensive ESTABLISHED patient follow-up addressing 3+ active problems. Multi-system review, typically 20-30 min. If the SOAP A section lists three or more distinct diagnoses or the Plan addresses several different organ systems / chronic conditions, prefer "general_reassessment" over "intermediate_assessment".
- "general_assessment" (A003A) — Comprehensive NEW patient workup OR annual complete exam.
- "mini_assessment" (A008A) — <5 min, no exam (Rx renewal, form signature only).
- "virtual_phone" (A102A) — Telephone visit. Pick this when the SOAP explicitly contains telephone-call language ("calling", "we'll call you back", "phone visit", "over the phone").
- "virtual_video" (A101A) — Video telemedicine visit.

DIAGNOSTIC CODE — chief-complaint reasoning:
Before emitting suggestedDiagnosticCode, walk through:
  1. Enumerate every distinct diagnosis listed in the SOAP A section.
  2. Identify the CHIEF COMPLAINT — the visit's actual reason today (often the dominant entry in S, or the diagnosis driving the most active management in P).
  3. Pick the 3-digit OHIP code matching the chief complaint, NOT the most prominent chronic condition.

Examples:
  • "S: nasal congestion. A: rhinitis medicamentosa. P: stop nasal spray." → 477 (allergic rhinitis), NOT 401 (HTN).
  • "S: wart on foot. A: HTN management on apixaban; plantar wart." → 707 (skin), NOT 401. Wart is the visit reason; HTN is incidental.
  • "S: foot swelling. A: foot edema, pending DVT rule-out." → 451 (phlebitis/DVT), NOT 799.
  • "S: skin tags. A: irritated acrochordons. P: liquid nitrogen offered." → 216 (benign skin neoplasm), NOT 799.
  • "A: rheumatoid arthritis with peripheral nerve symptoms." → 714 (RA), NOT 715 (osteoarthritis) — pick the most specific autoimmune dx when stated.
  • "S: low back pain x 1 week. A: chronic LBP with sciatica." → 724 (back pain), NOT 729 (fibromyalgia).

If multiple codes are equally plausible, pick the most specific one tied to the chief complaint. Use 799 ONLY when no body-system code fits.

OUTPUT FORMAT — STRICT:
- Output ONE JSON object and NOTHING else. No explanations, no Markdown fences, no prose before or after, no "correction" notes, no summary.
- If you need to revise during reasoning, internally do so but emit ONLY the final corrected JSON object.
- The conditionEvidence value for each condition must be a brief AFFIRMATIVE quote that supports inclusion. If the evidence text would say "not present", "not mentioned", "should not be included", "no evidence", or otherwise admit the condition does not apply, REMOVE that condition from the conditions array entirely — do not include it with a self-defeating evidence string."#.to_string());

    let context_section = if context_hints.is_empty() {
        String::new()
    } else {
        format!("\n\n## Billing Context (provided by physician)\n\n{}", context_hints)
    };

    let user_prompt = format!(
        "## SOAP Note\n\n{}\n\n## Full Transcript\n\n{}{}",
        soap_content, transcript, context_section
    );

    (system_prompt.to_string(), user_prompt)
}

// ── JSON parsing ───────────────────────────────────────────────────────────

/// Parse the LLM response into a `ClinicalFeatures` struct.
///
/// Handles common LLM output quirks: markdown code fences, leading prose
/// before the JSON, trailing text after the JSON, etc. When the response
/// contains MULTIPLE JSON blocks (LLM emits draft + corrected revision), the
/// LAST parseable block wins — Apr 24 2026 saw the LLM self-correct in prose
/// after its first JSON, so taking the first block locked in hallucinated
/// conditions the LLM had already retracted.
///
/// After successful parse, conditions whose `conditionEvidence` text contains
/// negation phrases ("not present", "should not be included", "no evidence",
/// etc.) are stripped — the LLM uses the evidence field itself to confess
/// when it shouldn't have included a condition (Alexander Gulas + Jerry
/// Zandbergen, Apr 24 2026).
pub fn parse_billing_extraction(response: &str) -> Result<ClinicalFeatures, String> {
    let blocks = candidate_json_blocks(response);
    if blocks.is_empty() {
        return Err("No JSON object found in response".to_string());
    }

    // Walk newest → oldest so the LAST parseable JSON in the response wins.
    let mut last_err: Option<String> = None;
    for block in blocks.iter().rev() {
        match try_parse_features(block) {
            Ok(features) => return Ok(strip_self_negated_conditions(features)),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| "Failed to parse any JSON block".to_string()))
}

/// Return every `{...}` substring in the response, in document order. Markdown
/// code fences are stripped before scanning. Brace counting respects strings
/// (so `"foo: {"` doesn't open a new block).
fn candidate_json_blocks(response: &str) -> Vec<String> {
    let stripped = response.replace("```json", "").replace("```", "");
    let bytes = stripped.as_bytes();
    let mut blocks = Vec::new();
    let mut depth: i32 = 0;
    let mut start: Option<usize> = None;
    let mut in_str = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        blocks.push(stripped[s..=i].to_string());
                    }
                }
                if depth < 0 {
                    depth = 0;
                }
            }
            _ => {}
        }
    }
    blocks
}

fn try_parse_features(json_str: &str) -> Result<ClinicalFeatures, String> {
    if let Ok(features) = serde_json::from_str::<ClinicalFeatures>(json_str) {
        return Ok(features);
    }
    let mut deserializer = serde_json::Deserializer::from_str(json_str);
    ClinicalFeatures::deserialize(&mut deserializer).map_err(|e| {
        format!(
            "Failed to parse clinical features JSON: {}. Input: {}",
            e,
            &json_str[..json_str.len().min(200)]
        )
    })
}

/// Phrases the LLM uses to confess in the evidence field that the condition
/// shouldn't have been included. Lowercased; substring match.
const SELF_NEGATING_EVIDENCE_PHRASES: &[&str] = &[
    "not present",
    "not mentioned",
    "not documented",
    "no evidence",
    "should not be included",
    "should not include",
    "not supported",
    "not applicable",
    "n/a",
    "is not",
    "no specific evidence",
];

fn strip_self_negated_conditions(mut features: ClinicalFeatures) -> ClinicalFeatures {
    let mut dropped_keys: Vec<String> = Vec::new();
    features.conditions.retain(|c| {
        let Some(key) = enum_to_snake_key(c) else { return true };
        let Some(evidence) = features.condition_evidence.get(&key) else { return true };
        let lc = evidence.to_lowercase();
        let self_negated = SELF_NEGATING_EVIDENCE_PHRASES
            .iter()
            .any(|p| lc.contains(p));
        if self_negated {
            dropped_keys.push(key);
            false
        } else {
            true
        }
    });
    if !dropped_keys.is_empty() {
        tracing::warn!(
            "Dropped conditions with self-negating evidence: {:?}",
            dropped_keys
        );
        for k in &dropped_keys {
            features.condition_evidence.remove(k);
        }
    }
    features
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_billing_extraction_prompt() {
        let (system, user) = build_billing_extraction_prompt("SOAP content here", "Transcript here", "", None);
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
        // v0.10.61 calibration sections must be present
        assert!(system.contains("VISIT-TYPE CALIBRATION"));
        assert!(system.contains("3+ active problems"));
        assert!(system.contains("DIAGNOSTIC CODE — chief-complaint reasoning"));
        assert!(system.contains("CHIEF COMPLAINT"));
        assert!(user.contains("SOAP content here"));
        assert!(user.contains("Transcript here"));
        // No context section when hints are empty
        assert!(!user.contains("Billing Context"));
    }

    #[test]
    fn test_build_billing_extraction_prompt_with_context() {
        let hints = "Visit was conducted by VIDEO telemedicine. Use A101 (limited virtual care video).";
        let (_, user) = build_billing_extraction_prompt("SOAP", "Transcript", hints, None);
        assert!(user.contains("## Billing Context (provided by physician)"));
        assert!(user.contains("VIDEO telemedicine"));
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
    fn test_parse_takes_last_json_block_when_llm_revises() {
        // The LLM emits a draft JSON, then a corrected JSON after prose.
        // The parser must take the LAST block so the correction wins.
        let response = r#"```json
{"visitType":"general_reassessment","procedures":[],"conditions":["diabetic_assessment"],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":20,"confidence":0.85,"primaryDiagnosis":"foo","conditionEvidence":{"diabetic_assessment":"some evidence"}}
```

On reflection, no diabetes management actually occurred. Here is the corrected output:

```json
{"visitType":"general_reassessment","procedures":[],"conditions":[],"setting":"in_office","isNewPatient":false,"isAfterHours":false,"patientCount":null,"estimatedDurationMinutes":20,"confidence":0.9,"primaryDiagnosis":"foo"}
```"#;
        let features = parse_billing_extraction(response).unwrap();
        assert!(features.conditions.is_empty(), "corrected JSON's empty conditions should win, got {:?}", features.conditions);
    }

    #[test]
    fn test_parse_strips_self_negated_conditions() {
        // 2026-04-24 Alexander Gulas + Jerry Zandbergen: LLM left
        // "diabetic_assessment" in conditions[] but its evidence string
        // explicitly admitted it shouldn't be included.
        let response = r#"{
  "visitType": "general_reassessment",
  "procedures": [],
  "conditions": ["diabetic_assessment"],
  "setting": "in_office",
  "isNewPatient": false,
  "isAfterHours": false,
  "patientCount": null,
  "estimatedDurationMinutes": 20,
  "confidence": 0.85,
  "primaryDiagnosis": "Rheumatoid arthritis",
  "conditionEvidence": {
    "diabetic_assessment": "The transcript mentions diabetic_assessment but the SOAP does NOT contain evidence of diabetic management. Therefore, this should NOT be included as it is not supported by the text."
  }
}"#;
        let features = parse_billing_extraction(response).unwrap();
        assert!(
            features.conditions.is_empty(),
            "self-negating evidence should drop the condition; got {:?}",
            features.conditions
        );
        assert!(
            features.condition_evidence.is_empty(),
            "evidence map should also be cleared for the dropped condition"
        );
    }

    #[test]
    fn test_parse_keeps_genuine_evidence() {
        let response = r#"{
  "visitType": "general_reassessment",
  "procedures": [],
  "conditions": ["diabetic_assessment"],
  "setting": "in_office",
  "isNewPatient": false,
  "isAfterHours": false,
  "patientCount": null,
  "estimatedDurationMinutes": 20,
  "confidence": 0.9,
  "primaryDiagnosis": "Suboptimal type 2 diabetes",
  "conditionEvidence": {
    "diabetic_assessment": "A1C 7.8%, increased metformin, recheck in 3 months"
  }
}"#;
        let features = parse_billing_extraction(response).unwrap();
        assert_eq!(features.conditions, vec![ConditionType::DiabeticAssessment]);
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
            suggested_diagnostic_code: None,
            primary_diagnosis: None,
            condition_evidence: HashMap::new(),
        };
        let json = serde_json::to_string(&features).unwrap();
        assert!(json.contains("\"visitType\""));
        assert!(json.contains("\"isNewPatient\""));
        assert!(json.contains("\"isAfterHours\""));
        assert!(json.contains("\"patientCount\""));
        assert!(json.contains("\"estimatedDurationMinutes\""));
    }
}
