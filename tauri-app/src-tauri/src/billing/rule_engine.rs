use super::clinical_features::*;
use super::diagnostic_codes;
use super::ohip_codes::{self, Basket, OhipCode};
use super::types::*;

use super::BillingDataRef;

/// Context flags that influence companion code selection in the rule engine.
/// Populated from the `BillingContext` in commands/billing.rs.
#[derive(Debug, Default)]
pub struct RuleEngineContext {
    /// True = procedure done in hospital (no tray fees). False = out-of-hospital / office (default).
    pub is_hospital: bool,
    /// True = patient's 3 K013 units for the year are exhausted. Use K033A instead.
    pub counselling_exhausted: bool,
}

/// Map extracted clinical features to a draft billing record with OHIP codes.
pub fn map_features_to_billing(
    features: &ClinicalFeatures,
    session_id: &str,
    date: &str,
    duration_ms: u64,
    patient_name: Option<&str>,
    billing_data: BillingDataRef<'_>,
) -> BillingRecord {
    map_features_to_billing_with_context(features, session_id, date, duration_ms, patient_name, &RuleEngineContext::default(), billing_data)
}

/// Map extracted clinical features to a draft billing record, with additional context
/// for companion code decisions (tray fees, smoking cessation add-ons, etc.).
pub fn map_features_to_billing_with_context(
    features: &ClinicalFeatures,
    session_id: &str,
    date: &str,
    duration_ms: u64,
    patient_name: Option<&str>,
    ctx: &RuleEngineContext,
    billing_data: BillingDataRef<'_>,
) -> BillingRecord {
    let mut codes: Vec<BillingCode> = Vec::new();

    // 1. Visit type -> assessment code
    let mut assessment_code = visit_type_to_code(&features.visit_type, billing_data);

    // Counselling: switch K013A → K033A when yearly cap is exhausted
    if assessment_code == "K013A" && ctx.counselling_exhausted {
        assessment_code = "K033A".to_string();
    }

    if let Some(ohip) = ohip_codes::get_code(&assessment_code) {
        let confidence = if features.confidence >= 0.85 {
            BillingConfidence::High
        } else if features.confidence >= 0.65 {
            BillingConfidence::Medium
        } else {
            BillingConfidence::Low
        };
        let mut code = make_billing_code(ohip, confidence.clone(), features.is_after_hours);

        // Multi-patient encounters: each patient gets their own assessment code.
        // Set quantity = patient_count for non-per-unit assessment codes.
        let patient_count = features.patient_count.unwrap_or(1).max(1);

        // Per-unit codes (K013A, K033A, K005A, K007A): set quantity from session duration
        // using GP54 counselling unit table (½ hour or major part thereof).
        if matches!(assessment_code.as_str(), "K013A" | "K033A" | "K005A" | "K007A") {
            let units = counselling_units_from_duration(duration_ms, billing_data);

            // K013A is capped at 3 units/year — overflow goes to K033A (out-of-basket)
            if assessment_code == "K013A" && units > 3 {
                code.quantity = 3;
                codes.push(code);
                // Add K033A for the overflow units
                if let Some(k033) = ohip_codes::get_code("K033A") {
                    let mut overflow = make_billing_code(k033, confidence, false);
                    overflow.quantity = units - 3;
                    codes.push(overflow);
                }
            } else {
                code.quantity = units;
                codes.push(code);
            }
        } else {
            // Non-counselling assessment: multiply by patient count for multi-patient encounters
            if patient_count > 1 {
                code.quantity = patient_count;
            }
            codes.push(code);
        }
    }

    // 2. Procedures -> procedure codes
    let mut procedure_codes: Vec<String> = Vec::new();
    for proc in &features.procedures {
        let proc_code = procedure_type_to_code(proc, billing_data);
        if let Some(ohip) = ohip_codes::get_code(&proc_code) {
            codes.push(make_billing_code(ohip, BillingConfidence::High, false));
            procedure_codes.push(proc_code);
        }
    }

    // 3. Conditions -> K/Q codes
    //    K codes require evidence from the SOAP note — suppress if the LLM
    //    could not cite supporting text (guards against hallucinated conditions).
    //    K005A/K007A are suppressed when visitType is counselling (K013A) —
    //    they're mutually exclusive per-unit time codes for the same service.
    let mut condition_codes: Vec<String> = Vec::new();
    for cond in &features.conditions {
        let cond_codes = condition_type_to_codes(cond, billing_data);
        for code_str in cond_codes {
            // Suppress K005A/K007A when already billing K013A (or vice versa)
            if matches!(code_str.as_str(), "K005A" | "K007A") && assessment_code == "K013A" {
                continue;
            }
            if code_str.starts_with('K') {
                let key = condition_type_to_key(cond);
                let evidence = features
                    .condition_evidence
                    .get(key)
                    .map(|e| e.trim())
                    .unwrap_or("");
                if evidence.is_empty() {
                    continue; // suppress K code — no SOAP evidence provided
                }
                // Cross-validate: evidence text must contain condition-relevant keywords.
                // Prevents LLM from fabricating evidence for conditions not in the SOAP.
                if !validate_condition_evidence(cond, evidence) {
                    continue;
                }
            }
            if let Some(ohip) = ohip_codes::get_code(&code_str) {
                codes.push(make_billing_code(ohip, BillingConfidence::Medium, false));
                condition_codes.push(code_str);
            }
        }
    }

    // 4. Companion codes — auto-add related codes based on what was extracted

    // 4a. Tray fee (E542A) — for qualifying procedures performed outside hospital
    if !ctx.is_hospital {
        let tray_qualifying = procedure_codes.iter().any(|c| is_tray_fee_qualifying(c, billing_data));
        if tray_qualifying {
            if let Some(ohip) = ohip_codes::get_code("E542A") {
                codes.push(make_billing_code(ohip, BillingConfidence::High, false));
            }
        }
    }

    // 4b–4c. Companion codes from server config (if available) or hardcoded rules
    if let Some(data) = billing_data {
        if !data.companion_rules.is_empty() {
            apply_companion_rules_from_data(data, &procedure_codes, &condition_codes, ctx, &mut codes);
        } else {
            apply_hardcoded_companion_rules(&procedure_codes, &condition_codes, ctx, &mut codes);
        }
    } else {
        apply_hardcoded_companion_rules(&procedure_codes, &condition_codes, ctx, &mut codes);
    }

    // 5. After-hours premium: add Q012A for eligible codes
    //    Q012A is a percentage-based premium (50% of eligible FFS) — not in the
    //    static code database because it has no fixed SOB rate.
    if features.is_after_hours {
        // Compute the total premium from all after-hours-eligible codes
        let mut total_ah_premium_cents: u32 = 0;
        for c in &codes {
            if c.after_hours {
                total_ah_premium_cents =
                    total_ah_premium_cents.saturating_add(c.after_hours_premium_cents);
            }
        }
        if total_ah_premium_cents > 0 {
            // Q012A: 50% shadow billing on the computed premium
            let shadow_pct: u8 = 50;
            let billable = total_ah_premium_cents * shadow_pct as u32 / 100;
            codes.push(BillingCode {
                code: "Q012A".to_string(),
                description: "After-Hours Premium (50% of FFS)".to_string(),
                fee_cents: total_ah_premium_cents,
                category: "in_basket".to_string(),
                shadow_pct,
                billable_amount_cents: billable,
                confidence: BillingConfidence::High,
                auto_extracted: true,
                after_hours: false,
                after_hours_premium_cents: 0,
                quantity: 1,
            });
        }
    }

    // 6. Time entry
    let time_entry =
        super::time_tracking::calculate_direct_care_time(duration_ms, &features.setting, billing_data);
    let time_entries = if time_entry.billable_units > 0 {
        vec![time_entry]
    } else {
        vec![]
    };

    // 7. Collect billing code strings before moving `codes` into record
    let billing_code_strs: Vec<String> = codes.iter().map(|c| c.code.clone()).collect();

    // 8. Build record and calculate totals
    let now = chrono::Utc::now().to_rfc3339();
    let mut record = BillingRecord {
        session_id: session_id.to_string(),
        date: date.to_string(),
        patient_name: patient_name.map(|s| s.to_string()),
        status: BillingStatus::Draft,
        codes,
        time_entries,
        total_shadow_cents: 0,
        total_out_of_basket_cents: 0,
        total_time_based_cents: 0,
        total_amount_cents: 0,
        confirmed_at: None,
        notes: None,
        extraction_model: None,
        extracted_at: Some(now),
        diagnostic_code: None,
        diagnostic_description: None,
    };

    // Resolve diagnostic code via 4-stage pipeline:
    // 1. LLM's suggestedDiagnosticCode (if valid)
    // 2. Text-match primaryDiagnosis against 562-code database
    // 3. Billing-code-implied diagnosis (K030A→250, Q050A→428)
    // 4. SOB constraint (K133A/K125A require IDD codes)
    resolve_diagnostic_code(features, &billing_code_strs, &assessment_code, &mut record, billing_data);

    record.recalculate_totals();

    record
}

// ── Visit type mapping ─────────────────────────────────────────────────────

fn visit_type_to_code(vt: &VisitType, billing_data: BillingDataRef<'_>) -> String {
    // Try server config first
    if let Some(data) = billing_data {
        if !data.visit_type_mappings.is_empty() {
            let key = format!("{:?}", vt);
            if let Some(entry) = data.visit_type_mappings.get(&key) {
                return entry.code.clone();
            }
        }
    }
    // Fall back to hardcoded match
    match vt {
        VisitType::MinorAssessment => "A001A",
        VisitType::IntermediateAssessment => "A007A",
        VisitType::GeneralAssessment => "A003A",
        VisitType::GeneralReassessment => "A004A",
        VisitType::MiniAssessment => "A008A",
        VisitType::PrenatalMajor => "P003A",
        VisitType::PrenatalMinor => "P004A",
        VisitType::PalliativeCare => "K023A",
        VisitType::Counselling => "K013A",
        VisitType::SharedAppointment => "K140A", // default to 2-patient; frontend can adjust
        VisitType::WellBabyVisit => "A007A",
        VisitType::Consultation => "A005A",
        VisitType::RepeatConsultation => "A006A",
        VisitType::LimitedConsultation => "A905A",
        VisitType::VirtualVideo => "A101A",
        VisitType::VirtualPhone => "A102A",
        VisitType::HouseCall => "A900A",
        VisitType::EmergencyDeptEquiv => "A888A",
        VisitType::PeriodicHealthChild => "K017A",
        VisitType::PeriodicHealthAdolescent => "K130A",
        VisitType::PeriodicHealthAdult => "K131A",
        VisitType::PeriodicHealthSenior => "K132A",
        VisitType::PeriodicHealthIdd => "K133A",
    }.to_string()
}

// ── Procedure type mapping ─────────────────────────────────────────────────

fn procedure_type_to_code(proc: &ProcedureType, billing_data: BillingDataRef<'_>) -> String {
    // Try server config first
    if let Some(data) = billing_data {
        if !data.procedure_type_mappings.is_empty() {
            let key = format!("{:?}", proc);
            if let Some(code) = data.procedure_type_mappings.get(&key) {
                return code.clone();
            }
        }
    }
    // Fall back to hardcoded match
    match proc {
        ProcedureType::PapSmear => "G365A",
        ProcedureType::IudInsertion => "G378A",
        ProcedureType::IudRemoval => "G552A",
        ProcedureType::LesionExcisionSmall => "R048A",
        ProcedureType::LesionExcisionMedium => "R094A",
        ProcedureType::LesionExcisionLarge => "R094A",
        ProcedureType::AbscessDrainage => "Z101A",
        ProcedureType::SkinBiopsy => "Z113A",
        ProcedureType::CryotherapySingle => "Z117A",
        ProcedureType::CryotherapyMultiple => "Z117A",
        ProcedureType::ElectrocoagulationSingle => "Z159A",
        ProcedureType::ElectrocoagulationMultiple => "Z160A",
        ProcedureType::BenignExcisionSmall => "Z125A",
        ProcedureType::BenignExcisionMedium => "Z125A",
        ProcedureType::LacerationRepairSimpleSmall => "Z154A",
        ProcedureType::LacerationRepairSimpleLarge => "Z175A",
        ProcedureType::LacerationRepairComplex => "Z176A",
        ProcedureType::EpistaxisCautery => "Z314A",
        ProcedureType::EpistaxisPacking => "Z315A",
        ProcedureType::Sigmoidoscopy => "Z535A",
        ProcedureType::Anoscopy => "Z543A",
        ProcedureType::HemorrhoidIncision => "Z545A",
        ProcedureType::CornealForeignBody => "Z847A",
        ProcedureType::Immunization => "G538A",
        ProcedureType::ImmunizationFlu => "G590A",
        ProcedureType::ImmunizationTdap => "G847A",
        ProcedureType::ImmunizationHepB => "G842A",
        ProcedureType::ImmunizationHpv => "G843A",
        ProcedureType::ImmunizationMmr => "G845A",
        ProcedureType::ImmunizationPneumococcal => "G846A",
        ProcedureType::ImmunizationVaricella => "G848A",
        ProcedureType::ImmunizationPediatric => "G840A",
        ProcedureType::InjectionSoleReason => "G373A", // sole reason for visit injection
        ProcedureType::JointInjection => "G370A",
        ProcedureType::JointInjectionAdditional => "G371A",
        ProcedureType::TriggerPointInjection => "G384A",
        ProcedureType::TriggerPointAdditional => "G385A",
        ProcedureType::ImInjectionWithVisit => "G372A",
        ProcedureType::IntralesionalSmall => "G375A",
        ProcedureType::IntralesionalLarge => "G377A",
        ProcedureType::IntravenousAdmin => "G379A",
        ProcedureType::NerveBlockPeripheral => "G231A",
        ProcedureType::NerveBlockParavertebral => "G228A",
        ProcedureType::NerveBlockAdditional => "G223A",
        ProcedureType::EarSyringing => "G420A",
        ProcedureType::Tonometry => "G435A",
        ProcedureType::NailDebridement => "Z110A",
        ProcedureType::NailExcisionSingle => "Z128A",
        ProcedureType::NailExcisionMultiple => "Z129A",
        ProcedureType::ForeignBodyRemoval => "Z114A",
        ProcedureType::BiopsyWithSutures => "Z116A",
        ProcedureType::WoundCatheterization => "Z611A",
        ProcedureType::GroupThreeExcisionFace => "Z122A",
        ProcedureType::GroupThreeExcisionOther => "Z125A",
        ProcedureType::GroupOneExcisionSingle => "Z156A",
        ProcedureType::GroupOneExcisionTwo => "Z157A",
        ProcedureType::GroupOneExcisionThree => "Z158A",
        ProcedureType::NevusExcision => "Z162A",
        ProcedureType::PapSmearRepeat => "G394A",
        ProcedureType::ElectrocoagThreeOrMore => "Z161A",
    }.to_string()
}

// ── Condition type mapping ─────────────────────────────────────────────────

/// Return the snake_case key for `ConditionType`, matching the serde rename
/// and the keys the LLM places in `conditionEvidence`.
fn condition_type_to_key(cond: &ConditionType) -> &'static str {
    match cond {
        ConditionType::DiabetesManagement => "diabetes_management",
        ConditionType::SmokingCessation => "smoking_cessation",
        ConditionType::StiManagement => "sti_management",
        ConditionType::ChfManagement => "chf_management",
        ConditionType::Neurocognitive => "neurocognitive",
        ConditionType::HomeCare => "home_care",
        ConditionType::SmokingCessationFollowUp => "smoking_cessation_follow_up",
        ConditionType::PrimaryMentalHealth => "primary_mental_health",
        ConditionType::Psychotherapy => "psychotherapy",
        ConditionType::HivPrimaryCare => "hiv_primary_care",
        ConditionType::InsulinTherapySupport => "insulin_therapy_support",
        ConditionType::DiabeticAssessment => "diabetic_assessment",
        ConditionType::CounsellingAdditional => "counselling_additional",
        ConditionType::FibromyalgiaCare => "fibromyalgia_care",
        ConditionType::IddPrimaryCare => "idd_primary_care",
        ConditionType::OpioidWithdrawalManagement => "opioid_withdrawal_management",
    }
}

fn condition_type_to_codes(cond: &ConditionType, billing_data: BillingDataRef<'_>) -> Vec<String> {
    // Try server config first
    if let Some(data) = billing_data {
        if !data.condition_type_mappings.is_empty() {
            let key = format!("{:?}", cond);
            if let Some(codes) = data.condition_type_mappings.get(&key) {
                return codes.clone();
            }
        }
    }
    // Fall back to hardcoded match
    match cond {
        ConditionType::DiabetesManagement => vec!["Q040A".to_string()],
        ConditionType::SmokingCessation => vec!["Q042A".to_string()],
        ConditionType::StiManagement => vec!["K028A".to_string()],
        ConditionType::ChfManagement => vec!["Q050A".to_string()],
        ConditionType::Neurocognitive => vec!["K032A".to_string()],
        ConditionType::HomeCare => vec!["K070A".to_string()],
        ConditionType::SmokingCessationFollowUp => vec!["K039A".to_string()],
        ConditionType::PrimaryMentalHealth => vec!["K005A".to_string()],
        ConditionType::Psychotherapy => vec!["K007A".to_string()],
        ConditionType::HivPrimaryCare => vec!["K022A".to_string()],
        ConditionType::InsulinTherapySupport => vec!["K029A".to_string()],
        ConditionType::DiabeticAssessment => vec!["K030A".to_string()],
        ConditionType::CounsellingAdditional => vec!["K033A".to_string()],
        ConditionType::FibromyalgiaCare => vec!["K037A".to_string()],
        ConditionType::IddPrimaryCare => vec!["K125A".to_string()],
        ConditionType::OpioidWithdrawalManagement => vec!["K005A".to_string()],
    }
}

/// Cross-validate condition evidence: ensure the evidence text contains at least
/// one keyword relevant to the condition. This catches LLM hallucinations where
/// evidence is fabricated for conditions not actually present in the SOAP.
fn validate_condition_evidence(cond: &ConditionType, evidence: &str) -> bool {
    let evidence_lower = evidence.to_lowercase();
    let keywords: &[&str] = match cond {
        ConditionType::DiabeticAssessment | ConditionType::DiabetesManagement
        | ConditionType::InsulinTherapySupport => &[
            "diabet", "a1c", "glucose", "insulin", "metformin", "hyperglycemia",
            "hypoglycemia", "blood sugar",
        ],
        ConditionType::ChfManagement => &[
            "heart failure", "chf", "fluid", "diuretic", "ejection", "edema",
            "cardiomyopathy",
        ],
        ConditionType::OpioidWithdrawalManagement => &[
            "opioid", "methadone", "suboxone", "buprenorphine", "naloxone",
            "withdrawal", "tapering", "opiate",
        ],
        ConditionType::StiManagement => &["sti", "chlamydia", "gonorrhea", "syphilis", "hiv", "sexual"],
        ConditionType::Neurocognitive => &["cognitive", "mmse", "moca", "dementia", "alzheimer"],
        ConditionType::IddPrimaryCare => &["intellectual", "developmental", "autism", "down syndrome", "cerebral palsy", "spina bifida", "fetal alcohol"],
        // For conditions where evidence text is already sufficient proof, accept any non-empty evidence
        _ => return true,
    };
    keywords.iter().any(|kw| evidence_lower.contains(kw))
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn basket_to_category(basket: Basket) -> String {
    match basket {
        Basket::In => "in_basket".to_string(),
        Basket::Out => "out_of_basket".to_string(),
    }
}

fn make_billing_code(
    ohip: &OhipCode,
    confidence: BillingConfidence,
    is_after_hours: bool,
) -> BillingCode {
    let category = basket_to_category(ohip.basket);

    let billable_amount_cents = match ohip.basket {
        Basket::In => ohip.ffs_rate_cents * ohip.shadow_pct as u32 / 100,
        Basket::Out => ohip.ffs_rate_cents, // full FFS
    };

    let after_hours = is_after_hours && ohip.after_hours_eligible;
    let after_hours_premium_cents = if after_hours {
        ohip.ffs_rate_cents * 50 / 100
    } else {
        0
    };

    BillingCode {
        code: ohip.code.to_string(),
        description: ohip.description.to_string(),
        fee_cents: ohip.ffs_rate_cents,
        category,
        shadow_pct: ohip.shadow_pct,
        billable_amount_cents,
        confidence,
        auto_extracted: true,
        after_hours,
        after_hours_premium_cents,
        quantity: 1,
    }
}

// ── Tray fee qualification ────────────────────────────────────────────────

/// Procedure codes that qualify for the E542A general tray fee when performed
/// outside of hospital. Covers surgical/procedural Z-codes, R-codes, and select
/// G-codes (biopsies, excisions, lacerations, injections, nail procedures, etc.).
/// Immunizations and ear syringing do NOT qualify.
fn is_tray_fee_qualifying(code: &str, billing_data: BillingDataRef<'_>) -> bool {
    // Try server config first
    if let Some(data) = billing_data {
        if !data.tray_fee_qualifying_codes.is_empty() {
            return data.tray_fee_qualifying_codes.iter().any(|c| c == code);
        }
    }
    // Fall back to hardcoded list
    matches!(
        code,
        // Excisions, biopsies, lacerations (Z series)
        "Z101A" | "Z110A" | "Z113A" | "Z114A" | "Z116A" | "Z117A"
        | "Z122A" | "Z125A" | "Z128A" | "Z129A"
        | "Z154A" | "Z156A" | "Z157A" | "Z158A"
        | "Z159A" | "Z160A" | "Z161A" | "Z162A"
        | "Z175A" | "Z176A"
        | "Z314A" | "Z315A"     // Epistaxis cautery/packing
        | "Z535A" | "Z543A" | "Z545A"  // Sigmoidoscopy, anoscopy, hemorrhoid
        | "Z611A"               // Catheterization
        | "Z847A"               // Corneal foreign body
        // Malignant excisions (R series)
        | "R048A" | "R094A"
        // Injections into joints/trigger points/nerve blocks (G series)
        | "G370A" | "G375A" | "G377A"
        | "G384A"
        | "G228A" | "G231A"
        // IUD procedures
        | "G378A" | "G552A"
    )
}

// ── Companion code helpers ───────────────────────────────────────────────

/// Apply hardcoded companion code rules (pap tray fee, smoking cessation).
fn apply_hardcoded_companion_rules(
    procedure_codes: &[String],
    condition_codes: &[String],
    ctx: &RuleEngineContext,
    codes: &mut Vec<BillingCode>,
) {
    // Pap tray fee (E430A) — with G365A outside hospital
    if !ctx.is_hospital && procedure_codes.iter().any(|c| c == "G365A") {
        if let Some(ohip) = ohip_codes::get_code("E430A") {
            codes.push(make_billing_code(ohip, BillingConfidence::High, false));
        }
    }

    // Smoking cessation initial discussion (E079A) — with Q042A
    if condition_codes.iter().any(|c| c == "Q042A") {
        if let Some(ohip) = ohip_codes::get_code("E079A") {
            codes.push(make_billing_code(ohip, BillingConfidence::Medium, false));
        }
    }
}

/// Apply companion code rules from server-provided billing data.
fn apply_companion_rules_from_data(
    data: &crate::server_config::BillingData,
    procedure_codes: &[String],
    condition_codes: &[String],
    ctx: &RuleEngineContext,
    codes: &mut Vec<BillingCode>,
) {
    let all_trigger_codes: Vec<&str> = procedure_codes
        .iter()
        .chain(condition_codes.iter())
        .map(|s| s.as_str())
        .collect();

    for rule in &data.companion_rules {
        // Check condition: "not_hospital" means skip if hospital
        if rule.condition == "not_hospital" && ctx.is_hospital {
            continue;
        }

        // Check if the trigger code is present in procedure or condition codes
        if all_trigger_codes.contains(&rule.trigger_code.as_str()) {
            if let Some(ohip) = ohip_codes::get_code(&rule.companion_code) {
                codes.push(make_billing_code(ohip, BillingConfidence::Medium, false));
            }
        }
    }
}

// ── Counselling unit calculation (GP54) ───────────────────────────────────

/// Calculate counselling/PMH units from session duration.
/// Per SOB GP54 time table: 1 unit = 20 min, 2 units = 46 min, 3 units = 76 min, etc.
/// Pattern: first unit at 20 min, second at 46 min, then +30 min per additional unit.
const COUNSELLING_UNIT_THRESHOLDS: &[u64] = &[20, 46, 76, 106, 136, 166, 196, 226];

fn counselling_units_from_duration(duration_ms: u64, billing_data: BillingDataRef<'_>) -> u8 {
    let minutes = duration_ms / 60_000;

    // Use server-provided thresholds if available
    let thresholds: &[u64] = if let Some(data) = billing_data {
        if !data.counselling_unit_thresholds.is_empty() {
            &data.counselling_unit_thresholds
        } else {
            COUNSELLING_UNIT_THRESHOLDS
        }
    } else {
        COUNSELLING_UNIT_THRESHOLDS
    };

    let mut units: u8 = 0;
    for &threshold in thresholds {
        if minutes >= threshold {
            units += 1;
        } else {
            break;
        }
    }
    // If longer than all thresholds, extrapolate at +30 min per unit
    if units == thresholds.len() as u8 && !thresholds.is_empty() {
        let beyond = minutes - thresholds[thresholds.len() - 1];
        units = units.saturating_add((beyond / 30) as u8);
    }
    units.max(1) // At least 1 unit if the visit happened
}

// ── Diagnostic code resolution ────────────────────────────────────────────

/// Set diagnostic code + description on a billing record from a DiagnosticCode reference.
fn set_diagnostic(record: &mut BillingRecord, dc: &diagnostic_codes::DiagnosticCode) {
    record.diagnostic_code = Some(dc.code.to_string());
    record.diagnostic_description = Some(dc.description.to_string());
}

/// Billing-code-implied diagnostic codes: when a specific K/Q code is present,
/// it strongly implies a particular diagnosis family.
fn billing_code_implied_diagnostic(codes: &[String], billing_data: BillingDataRef<'_>) -> Option<String> {
    // Try server config first
    if let Some(data) = billing_data {
        if !data.code_implied_diagnostics.is_empty() {
            for c in codes {
                if let Some(diag) = data.code_implied_diagnostics.get(c.as_str()) {
                    return Some(diag.clone());
                }
            }
            return None;
        }
    }
    // Fall back to hardcoded mapping
    for c in codes {
        match c.as_str() {
            "K030A" | "Q040A" => return Some("250".to_string()), // diabetes
            "Q050A" => return Some("428".to_string()),            // CHF
            "K028A" => return Some("099".to_string()),            // STI
            _ => {}
        }
    }
    None
}

/// Valid IDD diagnostic codes per SOB.
const IDD_CODES_K133: &[&str] = &["299", "319", "343", "758"];
const IDD_CODES_K125: &[&str] = &["299", "319", "343", "741", "758"];

/// Resolve the diagnostic code for a billing record via a 4-stage pipeline:
/// 1. Try the LLM's `suggestedDiagnosticCode` — cross-validated against primaryDiagnosis
/// 2. Fall back to text-matching `primaryDiagnosis` against the 562-code database
/// 3. Apply billing-code signals (K030A→250, Q050A→428) as fallback
/// 4. Enforce SOB constraints (K133A/K125A require IDD codes)
fn resolve_diagnostic_code(
    features: &ClinicalFeatures,
    billing_codes: &[String],
    assessment_code: &str,
    record: &mut BillingRecord,
    billing_data: BillingDataRef<'_>,
) {
    // Stage 1: try the explicit code suggestion, cross-validated against primaryDiagnosis.
    // The LLM's numeric code suggestion is wrong ~27% of the time (e.g. suggesting 491
    // "bronchitis" for a knee pain encounter). Cross-validate: at least one significant
    // word from the code's description must appear in the primaryDiagnosis text.
    if let Some(ref suggested) = features.suggested_diagnostic_code {
        if let Some(dc) = diagnostic_codes::get_diagnostic_code(suggested.trim()) {
            // Cross-validate against primaryDiagnosis when available.
            // Accept the code if: (a) no primaryDiagnosis to check against, or
            // (b) at least one significant word from the code description appears
            // in the primaryDiagnosis text.
            let cross_valid = match features.primary_diagnosis.as_ref() {
                None => true, // no text to cross-check, accept the code
                Some(diag) => {
                    let diag_lower = diag.to_lowercase();
                    dc.description.to_lowercase().split_whitespace()
                        .filter(|w| w.len() >= 4)
                        .any(|w| diag_lower.contains(w))
                }
            };
            if cross_valid {
                set_diagnostic(record, dc);
            }
            // If not cross-valid, fall through to Stage 2 (text match is more reliable)
        }
    }

    // Stage 2: if no code yet, resolve from plain-text primaryDiagnosis
    if record.diagnostic_code.is_none() {
        if let Some(ref text) = features.primary_diagnosis {
            if let Some(dc) = diagnostic_codes::match_diagnosis_text(text.trim()) {
                set_diagnostic(record, dc);
            }
        }
    }

    // Stage 3: if still no code, use billing-code-implied diagnosis as fallback
    if record.diagnostic_code.is_none() {
        if let Some(implied) = billing_code_implied_diagnostic(billing_codes, billing_data) {
            if let Some(dc) = diagnostic_codes::get_diagnostic_code(&implied) {
                set_diagnostic(record, dc);
            }
        }
    }

    // Stage 4: SOB constraint — K133A/K125A require IDD diagnostic codes
    let idd_code_in_billing = assessment_code == "K133A"
        || billing_codes.iter().any(|c| c == "K125A");
    if idd_code_in_billing {
        let allowed = if billing_codes.iter().any(|c| c == "K125A") {
            IDD_CODES_K125
        } else {
            IDD_CODES_K133
        };
        let is_valid = record
            .diagnostic_code
            .as_deref()
            .map_or(false, |c| allowed.contains(&c));
        if !is_valid {
            record.diagnostic_code = None;
            record.diagnostic_description = None;
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn default_features() -> ClinicalFeatures {
        ClinicalFeatures {
            visit_type: VisitType::MinorAssessment,
            procedures: vec![],
            conditions: vec![],
            setting: EncounterSetting::InOffice,
            is_new_patient: false,
            is_after_hours: false,
            patient_count: None,
            estimated_duration_minutes: Some(10),
            confidence: 0.90,
            suggested_diagnostic_code: None,
            primary_diagnosis: None,
            condition_evidence: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_minor_assessment_mapping() {
        let features = default_features();
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);
        assert_eq!(record.status, BillingStatus::Draft);
        assert_eq!(record.codes.len(), 1);
        assert_eq!(record.codes[0].code, "A001A");
        // A001A: 2680 cents * 30% = 804 cents (integer truncation)
        assert_eq!(record.codes[0].billable_amount_cents, 804);
        assert_eq!(record.codes[0].category, "in_basket");
    }

    #[test]
    fn test_general_assessment_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::GeneralAssessment;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None, None);
        assert_eq!(record.codes[0].code, "A003A");
        // A003A: 9560 * 30% = 2868
        assert_eq!(record.codes[0].billable_amount_cents, 2868);
    }

    #[test]
    fn test_general_reassessment_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::GeneralReassessment;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_200_000, None, None);
        assert_eq!(record.codes[0].code, "A004A");
    }

    #[test]
    fn test_intermediate_assessment_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::IntermediateAssessment;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None, None);
        assert_eq!(record.codes[0].code, "A007A");
    }

    #[test]
    fn test_mini_assessment_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::MiniAssessment;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 300_000, None, None);
        assert_eq!(record.codes[0].code, "A008A");
    }

    #[test]
    fn test_prenatal_major_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PrenatalMajor;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None, None);
        assert_eq!(record.codes[0].code, "P003A");
        // Out-of-basket: full FFS = 9385
        assert_eq!(record.codes[0].billable_amount_cents, 9385);
        assert_eq!(record.codes[0].category, "out_of_basket");
    }

    #[test]
    fn test_prenatal_minor_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PrenatalMinor;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None, None);
        assert_eq!(record.codes[0].code, "P004A");
        assert_eq!(record.codes[0].billable_amount_cents, 4455);
    }

    #[test]
    fn test_palliative_care_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PalliativeCare;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None, None);
        assert_eq!(record.codes[0].code, "K023A");
        assert_eq!(record.codes[0].billable_amount_cents, 8525);
    }

    #[test]
    fn test_counselling_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::Counselling;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None, None);
        assert_eq!(record.codes[0].code, "K013A");
    }

    #[test]
    fn test_shared_appointment_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::SharedAppointment;
        features.patient_count = Some(3);
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 3_600_000, None, None);
        assert_eq!(record.codes[0].code, "K140A");
    }

    #[test]
    fn test_well_baby_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::WellBabyVisit;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None, None);
        assert_eq!(record.codes[0].code, "A007A");
    }

    #[test]
    fn test_periodic_health_child_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthChild;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None, None);
        assert_eq!(record.codes[0].code, "K017A");
    }

    #[test]
    fn test_periodic_health_adolescent_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthAdolescent;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None, None);
        assert_eq!(record.codes[0].code, "K130A");
    }

    #[test]
    fn test_periodic_health_adult_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthAdult;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None, None);
        assert_eq!(record.codes[0].code, "K131A");
    }

    #[test]
    fn test_periodic_health_senior_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthSenior;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None, None);
        assert_eq!(record.codes[0].code, "K132A");
    }

    #[test]
    fn test_periodic_health_idd_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthIdd;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None, None);
        assert_eq!(record.codes[0].code, "K133A");
    }

    #[test]
    fn test_procedure_50_shadow_rate() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::SkinBiopsy];
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None, None);
        // Assessment + procedure + E542A tray fee = 3 codes
        assert_eq!(record.codes.len(), 3);
        let biopsy = &record.codes[1];
        assert_eq!(biopsy.code, "Z113A");
        assert_eq!(biopsy.shadow_pct, 50);
        // Z113A: 3245 * 50% = 1622
        assert_eq!(biopsy.billable_amount_cents, 1622);
        assert_eq!(record.codes[2].code, "E542A");
    }

    #[test]
    fn test_multiple_procedures() {
        let mut features = default_features();
        features.procedures = vec![
            ProcedureType::PapSmear,
            ProcedureType::CryotherapyMultiple,
        ];
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_200_000, None, None);
        // Assessment + 2 procedures + E542A tray fee + E430A pap tray = 5 codes
        assert!(record.codes.iter().any(|c| c.code == "G365A"));
        assert!(record.codes.iter().any(|c| c.code == "Z117A"));
        assert!(record.codes.iter().any(|c| c.code == "E542A"), "General tray fee for cryotherapy");
        assert!(record.codes.iter().any(|c| c.code == "E430A"), "Pap tray fee for G365A");
    }

    #[test]
    fn test_conditions_out_of_basket() {
        let mut features = default_features();
        features.conditions = vec![ConditionType::DiabetesManagement];
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_200_000, None, None);
        // Assessment + condition = 2 codes
        assert_eq!(record.codes.len(), 2);
        let dm = &record.codes[1];
        assert_eq!(dm.code, "Q040A");
        assert_eq!(dm.category, "out_of_basket");
        assert_eq!(dm.billable_amount_cents, 6570); // full FFS
    }

    #[test]
    fn test_after_hours_premium() {
        let mut features = default_features();
        features.is_after_hours = true;
        // A001A is after-hours eligible
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);

        // Should have assessment code + Q012A premium
        assert_eq!(record.codes.len(), 2);

        let assessment = &record.codes[0];
        assert_eq!(assessment.code, "A001A");
        assert!(assessment.after_hours);
        // Premium = 2680 * 50% = 1340
        assert_eq!(assessment.after_hours_premium_cents, 1340);

        let premium = &record.codes[1];
        assert_eq!(premium.code, "Q012A");
    }

    #[test]
    fn test_after_hours_not_eligible_procedure() {
        let mut features = default_features();
        features.is_after_hours = true;
        features.procedures = vec![ProcedureType::SkinBiopsy];
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None, None);

        // Assessment (after-hours) + procedure + E542A tray fee + Q012A premium = 4 codes
        assert_eq!(record.codes.len(), 4);

        let biopsy = record.codes.iter().find(|c| c.code == "Z113A").unwrap();
        assert!(!biopsy.after_hours); // procedures not after-hours eligible
        assert_eq!(biopsy.after_hours_premium_cents, 0);
        assert!(record.codes.iter().any(|c| c.code == "Q012A"));
        assert!(record.codes.iter().any(|c| c.code == "E542A"));
    }

    #[test]
    fn test_time_rounding_7_min() {
        // 7 minutes = 0 units (below 8-min threshold)
        let features = default_features();
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 7 * 60 * 1000, None, None);
        assert!(record.time_entries.is_empty());
    }

    #[test]
    fn test_time_rounding_8_min() {
        // 8 minutes = 1 unit (>= 8 remainder rounds up)
        let features = default_features();
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 8 * 60 * 1000, None, None);
        assert_eq!(record.time_entries.len(), 1);
        assert_eq!(record.time_entries[0].billable_units, 1);
    }

    #[test]
    fn test_time_rounding_15_min() {
        // 15 minutes = 1 unit (exact boundary)
        let features = default_features();
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 15 * 60 * 1000, None, None);
        assert_eq!(record.time_entries.len(), 1);
        assert_eq!(record.time_entries[0].billable_units, 1);
    }

    #[test]
    fn test_time_rounding_23_min() {
        // 23 minutes: 15 + 8 remainder = 2 units
        let features = default_features();
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 23 * 60 * 1000, None, None);
        assert_eq!(record.time_entries.len(), 1);
        assert_eq!(record.time_entries[0].billable_units, 2);
    }

    #[test]
    fn test_time_rounding_30_min() {
        // 30 minutes = 2 units (exact boundary)
        let features = default_features();
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 30 * 60 * 1000, None, None);
        assert_eq!(record.time_entries.len(), 1);
        assert_eq!(record.time_entries[0].billable_units, 2);
    }

    #[test]
    fn test_time_entry_telephone_remote() {
        let mut features = default_features();
        features.setting = EncounterSetting::TelephoneRemote;
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 15 * 60 * 1000, None, None);
        assert_eq!(record.time_entries.len(), 1);
        assert_eq!(record.time_entries[0].code, "Q311A");
        assert_eq!(record.time_entries[0].rate_per_15min_cents, 1700);
        assert_eq!(record.time_entries[0].billable_amount_cents, 1700);
    }

    #[test]
    fn test_total_calculation() {
        let mut features = default_features();
        features.visit_type = VisitType::GeneralReassessment;
        features.conditions = vec![ConditionType::DiabetesManagement];
        // 20 minutes = 2 units (15 + 5 remainder < 8 → 1 unit... wait: 20/15=1 remainder 5 < 8 → 1 unit)
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 20 * 60 * 1000, None, None);

        // A004A shadow: 3935 * 30% = 1180
        let a004_shadow = 3935 * 30 / 100;
        // Q040A out-of-basket: 6570
        let q040_full = 6570;
        // Q310A: 1 unit * $20 = 2000
        let time = 2000;

        assert_eq!(record.total_shadow_cents, a004_shadow);
        assert_eq!(record.total_out_of_basket_cents, q040_full);
        assert_eq!(record.total_time_based_cents, time);
        assert_eq!(
            record.total_amount_cents,
            a004_shadow + q040_full + time
        );
    }

    #[test]
    fn test_patient_name_preserved() {
        let features = default_features();
        let record = map_features_to_billing(
            &features,
            "session-123",
            "2026-04-05",
            600_000,
            Some("John Doe"),
            None,
        );
        assert_eq!(record.patient_name, Some("John Doe".to_string()));
        assert_eq!(record.session_id, "session-123");
        assert_eq!(record.date, "2026-04-05");
    }

    #[test]
    fn test_extracted_at_populated() {
        let features = default_features();
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);
        assert!(record.extracted_at.is_some());
        // Should be valid RFC3339
        let ts = record.extracted_at.unwrap();
        assert!(ts.contains("T"));
    }

    #[test]
    fn test_confidence_high() {
        let mut features = default_features();
        features.confidence = 0.90;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);
        assert_eq!(record.codes[0].confidence, BillingConfidence::High);
    }

    #[test]
    fn test_confidence_medium() {
        let mut features = default_features();
        features.confidence = 0.70;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);
        assert_eq!(record.codes[0].confidence, BillingConfidence::Medium);
    }

    #[test]
    fn test_confidence_low() {
        let mut features = default_features();
        features.confidence = 0.50;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);
        assert_eq!(record.codes[0].confidence, BillingConfidence::Low);
    }

    #[test]
    fn test_tray_fee_auto_added_for_procedure() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::SkinBiopsy]; // Z113A qualifies for tray fee
        let record = map_features_to_billing_with_context(
            &features, "s1", "2026-04-05", 900_000, None,
            &RuleEngineContext { is_hospital: false, ..Default::default() },
            None,
        );
        // Assessment + procedure + E542A tray fee = 3 codes
        assert!(record.codes.iter().any(|c| c.code == "E542A"), "Tray fee should be auto-added for skin biopsy");
    }

    #[test]
    fn test_tray_fee_not_added_in_hospital() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::SkinBiopsy];
        let record = map_features_to_billing_with_context(
            &features, "s1", "2026-04-05", 900_000, None,
            &RuleEngineContext { is_hospital: true, ..Default::default() },
            None,
        );
        assert!(!record.codes.iter().any(|c| c.code == "E542A"), "No tray fee in hospital");
    }

    #[test]
    fn test_tray_fee_not_added_for_immunization() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::ImmunizationFlu]; // G590A does NOT qualify
        let record = map_features_to_billing_with_context(
            &features, "s1", "2026-04-05", 600_000, None,
            &RuleEngineContext { is_hospital: false, ..Default::default() },
            None,
        );
        assert!(!record.codes.iter().any(|c| c.code == "E542A"), "No tray fee for immunization");
    }

    #[test]
    fn test_pap_tray_fee_auto_added() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::PapSmear]; // G365A
        let record = map_features_to_billing_with_context(
            &features, "s1", "2026-04-05", 900_000, None,
            &RuleEngineContext { is_hospital: false, ..Default::default() },
            None,
        );
        assert!(record.codes.iter().any(|c| c.code == "E430A"), "Pap tray fee should be auto-added");
        // G365A also qualifies for general tray fee — but Pap tray E430A is the specific one
    }

    #[test]
    fn test_pap_tray_fee_not_in_hospital() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::PapSmear];
        let record = map_features_to_billing_with_context(
            &features, "s1", "2026-04-05", 900_000, None,
            &RuleEngineContext { is_hospital: true, ..Default::default() },
            None,
        );
        assert!(!record.codes.iter().any(|c| c.code == "E430A"), "No Pap tray in hospital");
    }

    #[test]
    fn test_smoking_cessation_addon() {
        let mut features = default_features();
        features.conditions = vec![ConditionType::SmokingCessation]; // maps to Q042A
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None, None);
        assert!(record.codes.iter().any(|c| c.code == "Q042A"), "Smoking cessation Q042A present");
        assert!(record.codes.iter().any(|c| c.code == "E079A"), "E079A should be auto-added with Q042A");
    }

    #[test]
    fn test_joint_injection_tray_fee() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::JointInjection]; // G370A qualifies
        let record = map_features_to_billing_with_context(
            &features, "s1", "2026-04-05", 900_000, None,
            &RuleEngineContext { is_hospital: false, ..Default::default() },
            None,
        );
        assert!(record.codes.iter().any(|c| c.code == "G370A"));
        assert!(record.codes.iter().any(|c| c.code == "E542A"), "Tray fee for joint injection");
    }

    #[test]
    fn test_all_visit_types_have_valid_codes() {
        let visit_types = [
            VisitType::MinorAssessment,
            VisitType::IntermediateAssessment,
            VisitType::GeneralAssessment,
            VisitType::GeneralReassessment,
            VisitType::MiniAssessment,
            VisitType::PrenatalMajor,
            VisitType::PrenatalMinor,
            VisitType::PalliativeCare,
            VisitType::Counselling,
            VisitType::SharedAppointment,
            VisitType::WellBabyVisit,
            VisitType::Consultation,
            VisitType::RepeatConsultation,
            VisitType::LimitedConsultation,
            VisitType::VirtualVideo,
            VisitType::VirtualPhone,
            VisitType::HouseCall,
            VisitType::EmergencyDeptEquiv,
            VisitType::PeriodicHealthChild,
            VisitType::PeriodicHealthAdolescent,
            VisitType::PeriodicHealthAdult,
            VisitType::PeriodicHealthSenior,
            VisitType::PeriodicHealthIdd,
        ];
        for vt in &visit_types {
            let code = visit_type_to_code(vt, None);
            assert!(
                ohip_codes::get_code(&code).is_some(),
                "VisitType {:?} maps to {} which is not in OHIP_CODES",
                vt,
                code
            );
        }
    }

    #[test]
    fn test_all_procedure_types_have_valid_codes() {
        let procedures = [
            ProcedureType::PapSmear,
            ProcedureType::IudInsertion,
            ProcedureType::IudRemoval,
            ProcedureType::LesionExcisionSmall,
            ProcedureType::LesionExcisionMedium,
            ProcedureType::LesionExcisionLarge,
            ProcedureType::AbscessDrainage,
            ProcedureType::SkinBiopsy,
            ProcedureType::CryotherapySingle,
            ProcedureType::CryotherapyMultiple,
            ProcedureType::ElectrocoagulationSingle,
            ProcedureType::ElectrocoagulationMultiple,
            ProcedureType::BenignExcisionSmall,
            ProcedureType::BenignExcisionMedium,
            ProcedureType::LacerationRepairSimpleSmall,
            ProcedureType::LacerationRepairSimpleLarge,
            ProcedureType::LacerationRepairComplex,
            ProcedureType::EpistaxisCautery,
            ProcedureType::EpistaxisPacking,
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
        for p in &procedures {
            let code = procedure_type_to_code(p, None);
            assert!(
                ohip_codes::get_code(&code).is_some(),
                "ProcedureType {:?} maps to {} which is not in OHIP_CODES",
                p,
                code
            );
        }
    }

    #[test]
    fn test_k_code_suppressed_without_evidence() {
        let mut features = default_features();
        features.conditions = vec![ConditionType::DiabeticAssessment];
        // No evidence provided — K030A should be suppressed
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_200_000, None, None);
        assert!(
            !record.codes.iter().any(|c| c.code == "K030A"),
            "K030A should be suppressed without evidence"
        );
    }

    #[test]
    fn test_k_code_emitted_with_evidence() {
        let mut features = default_features();
        features.conditions = vec![ConditionType::DiabeticAssessment];
        features.condition_evidence.insert(
            "diabetic_assessment".to_string(),
            "A1C 7.8%, foot exam normal, increased metformin".to_string(),
        );
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_200_000, None, None);
        assert!(
            record.codes.iter().any(|c| c.code == "K030A"),
            "K030A should be emitted when evidence is provided"
        );
    }

    #[test]
    fn test_k_code_suppressed_with_blank_evidence() {
        let mut features = default_features();
        features.conditions = vec![ConditionType::DiabeticAssessment];
        features.condition_evidence.insert(
            "diabetic_assessment".to_string(),
            "   ".to_string(), // whitespace-only
        );
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_200_000, None, None);
        assert!(
            !record.codes.iter().any(|c| c.code == "K030A"),
            "K030A should be suppressed with blank evidence"
        );
    }

    #[test]
    fn test_q_code_not_gated_by_evidence() {
        let mut features = default_features();
        features.conditions = vec![ConditionType::DiabetesManagement];
        // No evidence — but Q040A is a Q code, not K, so it should still emit
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_200_000, None, None);
        assert!(
            record.codes.iter().any(|c| c.code == "Q040A"),
            "Q codes should not be gated by evidence"
        );
    }

    #[test]
    fn test_k133_accepts_idd_diagnostic_code() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthIdd;
        for code in ["299", "319", "343", "758"] {
            features.suggested_diagnostic_code = Some(code.to_string());
            let record = map_features_to_billing(&features, "s1", "2026-04-05", 3_600_000, None, None);
            assert_eq!(
                record.diagnostic_code.as_deref(),
                Some(code),
                "K133A should accept IDD diagnostic code {code}"
            );
        }
    }

    #[test]
    fn test_k133_rejects_non_idd_diagnostic_code() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthIdd;
        features.suggested_diagnostic_code = Some("250".to_string());
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 3_600_000, None, None);
        assert_eq!(
            record.diagnostic_code, None,
            "K133A should reject non-IDD diagnostic code 250"
        );
    }

    #[test]
    fn test_k133_no_diagnostic_code_stays_none() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthIdd;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 3_600_000, None, None);
        assert_eq!(record.diagnostic_code, None);
    }

    #[test]
    fn test_all_condition_types_have_valid_codes() {
        let conditions = all_condition_types();
        for c in &conditions {
            let codes = condition_type_to_codes(c, None);
            for code_str in &codes {
                assert!(
                    ohip_codes::get_code(code_str).is_some(),
                    "ConditionType {:?} maps to {} which is not in OHIP_CODES",
                    c,
                    code_str
                );
            }
        }
    }

    #[test]
    fn test_condition_type_to_key_matches_serde() {
        // Guards against condition_type_to_key diverging from serde rename_all.
        for c in &all_condition_types() {
            let serde_key = serde_json::to_value(c)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            assert_eq!(
                condition_type_to_key(c),
                serde_key,
                "condition_type_to_key({:?}) does not match serde serialization",
                c
            );
        }
    }

    fn all_condition_types() -> Vec<ConditionType> {
        vec![
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
            ConditionType::IddPrimaryCare,
        ]
    }

    #[test]
    fn test_k125_emitted_with_evidence() {
        let mut features = default_features();
        features.conditions = vec![ConditionType::IddPrimaryCare];
        features.condition_evidence.insert(
            "idd_primary_care".to_string(),
            "Patient with Down syndrome, annual IDD care review".to_string(),
        );
        features.suggested_diagnostic_code = Some("758".to_string());
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None, None);
        assert!(record.codes.iter().any(|c| c.code == "K125A"));
        assert_eq!(record.diagnostic_code.as_deref(), Some("758"));
    }

    #[test]
    fn test_k125_accepts_spina_bifida_741() {
        let mut features = default_features();
        features.conditions = vec![ConditionType::IddPrimaryCare];
        features.condition_evidence.insert(
            "idd_primary_care".to_string(),
            "Spina bifida patient, IDD primary care".to_string(),
        );
        features.suggested_diagnostic_code = Some("741".to_string());
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None, None);
        assert!(record.codes.iter().any(|c| c.code == "K125A"));
        assert_eq!(record.diagnostic_code.as_deref(), Some("741"));
    }

    #[test]
    fn test_k125_rejects_non_idd_diagnostic() {
        let mut features = default_features();
        features.conditions = vec![ConditionType::IddPrimaryCare];
        features.condition_evidence.insert(
            "idd_primary_care".to_string(),
            "Patient with developmental disability, annual review".to_string(),
        );
        features.suggested_diagnostic_code = Some("496".to_string());
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None, None);
        assert!(record.codes.iter().any(|c| c.code == "K125A"));
        assert_eq!(record.diagnostic_code, None, "Non-IDD diagnostic should be cleared");
    }

    #[test]
    fn test_k133_does_not_accept_741() {
        // K133A's SOB list doesn't include 741 (spina bifida) — only K125A does
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthIdd;
        features.suggested_diagnostic_code = Some("741".to_string());
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 3_600_000, None, None);
        assert_eq!(record.diagnostic_code, None, "K133A should not accept 741");
    }

    // ── Counselling unit + mutual exclusion tests ──────────────────────────

    #[test]
    fn test_counselling_units_gp54_table() {
        // Exact SOB GP54 thresholds
        assert_eq!(counselling_units_from_duration(10 * 60_000, None), 1);  // below minimum
        assert_eq!(counselling_units_from_duration(19 * 60_000, None), 1);  // just below 20
        assert_eq!(counselling_units_from_duration(20 * 60_000, None), 1);  // 1 unit
        assert_eq!(counselling_units_from_duration(45 * 60_000, None), 1);  // just below 46
        assert_eq!(counselling_units_from_duration(46 * 60_000, None), 2);  // 2 units
        assert_eq!(counselling_units_from_duration(74 * 60_000, None), 2);  // below 76
        assert_eq!(counselling_units_from_duration(76 * 60_000, None), 3);  // 3 units
        assert_eq!(counselling_units_from_duration(106 * 60_000, None), 4); // 4 units
        assert_eq!(counselling_units_from_duration(136 * 60_000, None), 5); // 5 units
    }

    #[test]
    fn test_counselling_visit_sets_quantity() {
        let mut features = default_features();
        features.visit_type = VisitType::Counselling;
        // 74 minutes → 2 units per GP54
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 74 * 60 * 1000, None, None);
        assert_eq!(record.codes[0].code, "K013A");
        assert_eq!(record.codes[0].quantity, 2);
    }

    #[test]
    fn test_counselling_exhausted_uses_k033() {
        let mut features = default_features();
        features.visit_type = VisitType::Counselling;
        let record = map_features_to_billing_with_context(
            &features, "s1", "2026-04-05", 74 * 60 * 1000, None,
            &RuleEngineContext { counselling_exhausted: true, ..Default::default() },
            None,
        );
        assert_eq!(record.codes[0].code, "K033A");
        assert_eq!(record.codes[0].quantity, 2);
    }

    #[test]
    fn test_counselling_auto_split_k013_k033() {
        let mut features = default_features();
        features.visit_type = VisitType::Counselling;
        // 106 minutes → 4 units: should split into K013A (3) + K033A (1)
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 106 * 60 * 1000, None, None);
        assert_eq!(record.codes.len(), 2, "Should have K013A + K033A");
        assert_eq!(record.codes[0].code, "K013A");
        assert_eq!(record.codes[0].quantity, 3, "K013A capped at 3");
        assert_eq!(record.codes[1].code, "K033A");
        assert_eq!(record.codes[1].quantity, 1, "K033A gets overflow");
    }

    #[test]
    fn test_counselling_auto_split_5_units() {
        let mut features = default_features();
        features.visit_type = VisitType::Counselling;
        // 136 minutes → 5 units: K013A (3) + K033A (2)
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 136 * 60 * 1000, None, None);
        assert_eq!(record.codes[0].code, "K013A");
        assert_eq!(record.codes[0].quantity, 3);
        assert_eq!(record.codes[1].code, "K033A");
        assert_eq!(record.codes[1].quantity, 2);
    }

    #[test]
    fn test_counselling_no_split_at_3_units() {
        let mut features = default_features();
        features.visit_type = VisitType::Counselling;
        // 76 minutes → exactly 3 units: no split needed
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 76 * 60 * 1000, None, None);
        assert_eq!(record.codes.len(), 1);
        assert_eq!(record.codes[0].code, "K013A");
        assert_eq!(record.codes[0].quantity, 3);
    }

    #[test]
    fn test_k005_suppressed_when_counselling() {
        let mut features = default_features();
        features.visit_type = VisitType::Counselling;
        features.conditions = vec![ConditionType::PrimaryMentalHealth];
        features.condition_evidence.insert(
            "primary_mental_health".to_string(),
            "counselling for adjustment reaction".to_string(),
        );
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 74 * 60 * 1000, None, None);
        assert!(record.codes.iter().any(|c| c.code == "K013A"), "K013A should be present");
        assert!(!record.codes.iter().any(|c| c.code == "K005A"), "K005A should be suppressed when K013A is the visit type");
    }

    // ── Diagnostic code resolution tests ──────────────────────────────────

    #[test]
    fn test_diagnosis_resolved_from_primary_text() {
        let mut features = default_features();
        features.primary_diagnosis = Some("diabetes mellitus".to_string());
        // No suggestedDiagnosticCode — should resolve from text
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);
        assert_eq!(record.diagnostic_code.as_deref(), Some("250"));
    }

    #[test]
    fn test_suggested_code_takes_precedence_when_consistent() {
        let mut features = default_features();
        features.suggested_diagnostic_code = Some("401".to_string());
        features.primary_diagnosis = Some("essential hypertension".to_string());
        // Consistent: 401 = "Essential hypertension" matches "essential hypertension"
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);
        assert_eq!(record.diagnostic_code.as_deref(), Some("401"));
    }

    #[test]
    fn test_suggested_code_rejected_when_inconsistent() {
        let mut features = default_features();
        features.suggested_diagnostic_code = Some("491".to_string()); // bronchitis
        features.primary_diagnosis = Some("knee osteoarthritis".to_string());
        // Inconsistent: 491 "chronic bronchitis" doesn't match "knee osteoarthritis"
        // Falls through to text match → 715 (osteoarthritis)
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);
        assert_eq!(record.diagnostic_code.as_deref(), Some("715"));
    }

    #[test]
    fn test_suggested_code_accepted_without_primary_diagnosis() {
        let mut features = default_features();
        features.suggested_diagnostic_code = Some("401".to_string());
        features.primary_diagnosis = None;
        // No primaryDiagnosis to cross-check → accept the suggested code
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);
        assert_eq!(record.diagnostic_code.as_deref(), Some("401"));
    }

    #[test]
    fn test_billing_code_implies_diagnosis_fallback() {
        let mut features = default_features();
        features.conditions = vec![ConditionType::DiabeticAssessment];
        features.condition_evidence.insert(
            "diabetic_assessment".to_string(),
            "A1C review, foot exam".to_string(),
        );
        // No suggestedDiagnosticCode, no primaryDiagnosis — K030A implies 250
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_200_000, None, None);
        assert_eq!(
            record.diagnostic_code.as_deref(),
            Some("250"),
            "K030A should imply diagnostic code 250"
        );
    }

    #[test]
    fn test_primary_diagnosis_copd() {
        let mut features = default_features();
        features.primary_diagnosis = Some("COPD with progressive dyspnea".to_string());
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);
        assert_eq!(record.diagnostic_code.as_deref(), Some("496"));
    }

    #[test]
    fn test_no_diagnosis_info_leaves_none() {
        let features = default_features();
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None, None);
        assert_eq!(record.diagnostic_code, None);
    }
}
