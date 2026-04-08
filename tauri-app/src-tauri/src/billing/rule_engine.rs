use super::clinical_features::*;
use super::diagnostic_codes;
use super::ohip_codes::{self, Basket, OhipCode};
use super::types::*;

/// Context flags that influence companion code selection in the rule engine.
/// Populated from the `BillingContext` in commands/billing.rs.
#[derive(Debug, Default)]
pub struct RuleEngineContext {
    /// True = procedure done in hospital (no tray fees). False = out-of-hospital / office (default).
    pub is_hospital: bool,
}

/// Map extracted clinical features to a draft billing record with OHIP codes.
pub fn map_features_to_billing(
    features: &ClinicalFeatures,
    session_id: &str,
    date: &str,
    duration_ms: u64,
    patient_name: Option<&str>,
) -> BillingRecord {
    map_features_to_billing_with_context(features, session_id, date, duration_ms, patient_name, &RuleEngineContext::default())
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
) -> BillingRecord {
    let mut codes: Vec<BillingCode> = Vec::new();

    // 1. Visit type -> assessment code
    let assessment_code = visit_type_to_code(&features.visit_type);
    if let Some(ohip) = ohip_codes::get_code(assessment_code) {
        let confidence = if features.confidence >= 0.85 {
            BillingConfidence::High
        } else if features.confidence >= 0.65 {
            BillingConfidence::Medium
        } else {
            BillingConfidence::Low
        };
        codes.push(make_billing_code(ohip, confidence, features.is_after_hours));
    }

    // 2. Procedures -> procedure codes
    let mut procedure_codes: Vec<&str> = Vec::new();
    for proc in &features.procedures {
        let proc_code = procedure_type_to_code(proc);
        if let Some(ohip) = ohip_codes::get_code(proc_code) {
            codes.push(make_billing_code(ohip, BillingConfidence::High, false));
            procedure_codes.push(proc_code);
        }
    }

    // 3. Conditions -> K/Q codes
    let mut condition_codes: Vec<&str> = Vec::new();
    for cond in &features.conditions {
        let cond_codes = condition_type_to_codes(cond);
        for code_str in cond_codes {
            if let Some(ohip) = ohip_codes::get_code(code_str) {
                codes.push(make_billing_code(ohip, BillingConfidence::Medium, false));
                condition_codes.push(code_str);
            }
        }
    }

    // 4. Companion codes — auto-add related codes based on what was extracted

    // 4a. Tray fee (E542A) — for qualifying procedures performed outside hospital
    if !ctx.is_hospital {
        let tray_qualifying = procedure_codes.iter().any(|c| is_tray_fee_qualifying(c));
        if tray_qualifying {
            if let Some(ohip) = ohip_codes::get_code("E542A") {
                codes.push(make_billing_code(ohip, BillingConfidence::High, false));
            }
        }
    }

    // 4b. Pap tray fee (E430A) — with G365A outside hospital
    if !ctx.is_hospital && procedure_codes.iter().any(|c| *c == "G365A") {
        if let Some(ohip) = ohip_codes::get_code("E430A") {
            codes.push(make_billing_code(ohip, BillingConfidence::High, false));
        }
    }

    // 4c. Smoking cessation initial discussion (E079A) — with Q042A
    if condition_codes.iter().any(|c| *c == "Q042A") {
        if let Some(ohip) = ohip_codes::get_code("E079A") {
            codes.push(make_billing_code(ohip, BillingConfidence::Medium, false));
        }
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
        super::time_tracking::calculate_direct_care_time(duration_ms, &features.setting);
    let time_entries = if time_entry.billable_units > 0 {
        vec![time_entry]
    } else {
        vec![]
    };

    // 7. Build record and calculate totals
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

    // Validate and set diagnostic code from LLM suggestion
    if let Some(ref suggested) = features.suggested_diagnostic_code {
        let code_str = suggested.trim();
        if let Some(dc) = diagnostic_codes::get_diagnostic_code(code_str) {
            record.diagnostic_code = Some(dc.code.to_string());
            record.diagnostic_description = Some(dc.description.to_string());
        }
        // Invalid code silently ignored — physician can set manually in the UI
    }

    record.recalculate_totals();

    record
}

// ── Visit type mapping ─────────────────────────────────────────────────────

fn visit_type_to_code(vt: &VisitType) -> &'static str {
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
    }
}

// ── Procedure type mapping ─────────────────────────────────────────────────

fn procedure_type_to_code(proc: &ProcedureType) -> &'static str {
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
    }
}

// ── Condition type mapping ─────────────────────────────────────────────────

fn condition_type_to_codes(cond: &ConditionType) -> Vec<&'static str> {
    match cond {
        ConditionType::DiabetesManagement => vec!["Q040A"],
        ConditionType::SmokingCessation => vec!["Q042A"],
        ConditionType::StiManagement => vec!["K028A"],
        ConditionType::ChfManagement => vec!["Q050A"],
        ConditionType::Neurocognitive => vec!["K032A"],
        ConditionType::HomeCare => vec!["K070A"],
        ConditionType::SmokingCessationFollowUp => vec!["K039A"],
        ConditionType::PrimaryMentalHealth => vec!["K005A"],
        ConditionType::Psychotherapy => vec!["K007A"],
        ConditionType::HivPrimaryCare => vec!["K022A"],
        ConditionType::InsulinTherapySupport => vec!["K029A"],
        ConditionType::DiabeticAssessment => vec!["K030A"],
        ConditionType::CounsellingAdditional => vec!["K033A"],
        ConditionType::FibromyalgiaCare => vec!["K037A"],
    }
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
fn is_tray_fee_qualifying(code: &str) -> bool {
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
        }
    }

    #[test]
    fn test_minor_assessment_mapping() {
        let features = default_features();
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None);
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
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None);
        assert_eq!(record.codes[0].code, "A003A");
        // A003A: 9560 * 30% = 2868
        assert_eq!(record.codes[0].billable_amount_cents, 2868);
    }

    #[test]
    fn test_general_reassessment_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::GeneralReassessment;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_200_000, None);
        assert_eq!(record.codes[0].code, "A004A");
    }

    #[test]
    fn test_intermediate_assessment_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::IntermediateAssessment;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None);
        assert_eq!(record.codes[0].code, "A007A");
    }

    #[test]
    fn test_mini_assessment_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::MiniAssessment;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 300_000, None);
        assert_eq!(record.codes[0].code, "A008A");
    }

    #[test]
    fn test_prenatal_major_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PrenatalMajor;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None);
        assert_eq!(record.codes[0].code, "P003A");
        // Out-of-basket: full FFS = 9385
        assert_eq!(record.codes[0].billable_amount_cents, 9385);
        assert_eq!(record.codes[0].category, "out_of_basket");
    }

    #[test]
    fn test_prenatal_minor_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PrenatalMinor;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None);
        assert_eq!(record.codes[0].code, "P004A");
        assert_eq!(record.codes[0].billable_amount_cents, 4455);
    }

    #[test]
    fn test_palliative_care_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PalliativeCare;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None);
        assert_eq!(record.codes[0].code, "K023A");
        assert_eq!(record.codes[0].billable_amount_cents, 8525);
    }

    #[test]
    fn test_counselling_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::Counselling;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None);
        assert_eq!(record.codes[0].code, "K013A");
    }

    #[test]
    fn test_shared_appointment_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::SharedAppointment;
        features.patient_count = Some(3);
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 3_600_000, None);
        assert_eq!(record.codes[0].code, "K140A");
    }

    #[test]
    fn test_well_baby_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::WellBabyVisit;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None);
        assert_eq!(record.codes[0].code, "A007A");
    }

    #[test]
    fn test_periodic_health_child_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthChild;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None);
        assert_eq!(record.codes[0].code, "K017A");
    }

    #[test]
    fn test_periodic_health_adolescent_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthAdolescent;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None);
        assert_eq!(record.codes[0].code, "K130A");
    }

    #[test]
    fn test_periodic_health_adult_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthAdult;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None);
        assert_eq!(record.codes[0].code, "K131A");
    }

    #[test]
    fn test_periodic_health_senior_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthSenior;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None);
        assert_eq!(record.codes[0].code, "K132A");
    }

    #[test]
    fn test_periodic_health_idd_mapping() {
        let mut features = default_features();
        features.visit_type = VisitType::PeriodicHealthIdd;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_800_000, None);
        assert_eq!(record.codes[0].code, "K133A");
    }

    #[test]
    fn test_procedure_50_shadow_rate() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::SkinBiopsy];
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None);
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
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_200_000, None);
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
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 1_200_000, None);
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
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None);

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
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None);

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
            map_features_to_billing(&features, "s1", "2026-04-05", 7 * 60 * 1000, None);
        assert!(record.time_entries.is_empty());
    }

    #[test]
    fn test_time_rounding_8_min() {
        // 8 minutes = 1 unit (>= 8 remainder rounds up)
        let features = default_features();
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 8 * 60 * 1000, None);
        assert_eq!(record.time_entries.len(), 1);
        assert_eq!(record.time_entries[0].billable_units, 1);
    }

    #[test]
    fn test_time_rounding_15_min() {
        // 15 minutes = 1 unit (exact boundary)
        let features = default_features();
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 15 * 60 * 1000, None);
        assert_eq!(record.time_entries.len(), 1);
        assert_eq!(record.time_entries[0].billable_units, 1);
    }

    #[test]
    fn test_time_rounding_23_min() {
        // 23 minutes: 15 + 8 remainder = 2 units
        let features = default_features();
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 23 * 60 * 1000, None);
        assert_eq!(record.time_entries.len(), 1);
        assert_eq!(record.time_entries[0].billable_units, 2);
    }

    #[test]
    fn test_time_rounding_30_min() {
        // 30 minutes = 2 units (exact boundary)
        let features = default_features();
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 30 * 60 * 1000, None);
        assert_eq!(record.time_entries.len(), 1);
        assert_eq!(record.time_entries[0].billable_units, 2);
    }

    #[test]
    fn test_time_entry_telephone_remote() {
        let mut features = default_features();
        features.setting = EncounterSetting::TelephoneRemote;
        let record =
            map_features_to_billing(&features, "s1", "2026-04-05", 15 * 60 * 1000, None);
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
            map_features_to_billing(&features, "s1", "2026-04-05", 20 * 60 * 1000, None);

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
        );
        assert_eq!(record.patient_name, Some("John Doe".to_string()));
        assert_eq!(record.session_id, "session-123");
        assert_eq!(record.date, "2026-04-05");
    }

    #[test]
    fn test_extracted_at_populated() {
        let features = default_features();
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None);
        assert!(record.extracted_at.is_some());
        // Should be valid RFC3339
        let ts = record.extracted_at.unwrap();
        assert!(ts.contains("T"));
    }

    #[test]
    fn test_confidence_high() {
        let mut features = default_features();
        features.confidence = 0.90;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None);
        assert_eq!(record.codes[0].confidence, BillingConfidence::High);
    }

    #[test]
    fn test_confidence_medium() {
        let mut features = default_features();
        features.confidence = 0.70;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None);
        assert_eq!(record.codes[0].confidence, BillingConfidence::Medium);
    }

    #[test]
    fn test_confidence_low() {
        let mut features = default_features();
        features.confidence = 0.50;
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 600_000, None);
        assert_eq!(record.codes[0].confidence, BillingConfidence::Low);
    }

    #[test]
    fn test_tray_fee_auto_added_for_procedure() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::SkinBiopsy]; // Z113A qualifies for tray fee
        let record = map_features_to_billing_with_context(
            &features, "s1", "2026-04-05", 900_000, None,
            &RuleEngineContext { is_hospital: false },
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
            &RuleEngineContext { is_hospital: true },
        );
        assert!(!record.codes.iter().any(|c| c.code == "E542A"), "No tray fee in hospital");
    }

    #[test]
    fn test_tray_fee_not_added_for_immunization() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::ImmunizationFlu]; // G590A does NOT qualify
        let record = map_features_to_billing_with_context(
            &features, "s1", "2026-04-05", 600_000, None,
            &RuleEngineContext { is_hospital: false },
        );
        assert!(!record.codes.iter().any(|c| c.code == "E542A"), "No tray fee for immunization");
    }

    #[test]
    fn test_pap_tray_fee_auto_added() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::PapSmear]; // G365A
        let record = map_features_to_billing_with_context(
            &features, "s1", "2026-04-05", 900_000, None,
            &RuleEngineContext { is_hospital: false },
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
            &RuleEngineContext { is_hospital: true },
        );
        assert!(!record.codes.iter().any(|c| c.code == "E430A"), "No Pap tray in hospital");
    }

    #[test]
    fn test_smoking_cessation_addon() {
        let mut features = default_features();
        features.conditions = vec![ConditionType::SmokingCessation]; // maps to Q042A
        let record = map_features_to_billing(&features, "s1", "2026-04-05", 900_000, None);
        assert!(record.codes.iter().any(|c| c.code == "Q042A"), "Smoking cessation Q042A present");
        assert!(record.codes.iter().any(|c| c.code == "E079A"), "E079A should be auto-added with Q042A");
    }

    #[test]
    fn test_joint_injection_tray_fee() {
        let mut features = default_features();
        features.procedures = vec![ProcedureType::JointInjection]; // G370A qualifies
        let record = map_features_to_billing_with_context(
            &features, "s1", "2026-04-05", 900_000, None,
            &RuleEngineContext { is_hospital: false },
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
            let code = visit_type_to_code(vt);
            assert!(
                ohip_codes::get_code(code).is_some(),
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
            let code = procedure_type_to_code(p);
            assert!(
                ohip_codes::get_code(code).is_some(),
                "ProcedureType {:?} maps to {} which is not in OHIP_CODES",
                p,
                code
            );
        }
    }

    #[test]
    fn test_all_condition_types_have_valid_codes() {
        let conditions = [
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
        ];
        for c in &conditions {
            let codes = condition_type_to_codes(c);
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
}
