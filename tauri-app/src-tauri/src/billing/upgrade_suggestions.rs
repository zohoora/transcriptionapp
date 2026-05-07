//! Heuristic predicates that surface OHIP code-swap suggestions on a finalized
//! `BillingRecord`. Pure — no LLM, no mutation; clinician applies via the
//! `apply_billing_upgrade` IPC command.

use super::clinical_features::ClinicalFeatures;
use super::ohip_codes;
use super::rule_engine::text_has_mental_health_keywords;
use super::types::{BillingConfidence, BillingRecord, UpgradeSuggestion};

pub fn compute_upgrade_suggestions(
    record: &BillingRecord,
    features: &ClinicalFeatures,
) -> Vec<UpgradeSuggestion> {
    let mut out = Vec::new();
    if let Some(s) = check_a004_to_a007(record, features) {
        out.push(s);
    }
    if let Some(s) = check_k005_to_k013(record, features) {
        out.push(s);
    }
    out
}

/// FFS rate delta in cents (`to - from`). Returns 0 when either code is missing
/// from the corpus — partial deltas would be misleading in the UI.
pub(crate) fn fee_delta_cents(from_code: &str, to_code: &str) -> i32 {
    let (Some(f), Some(t)) = (ohip_codes::get_code(from_code), ohip_codes::get_code(to_code))
    else {
        return 0;
    };
    t.ffs_rate_cents as i32 - f.ffs_rate_cents as i32
}

pub(crate) fn build_suggestion(from: &str, to: &str, reasoning: &str) -> UpgradeSuggestion {
    UpgradeSuggestion {
        from_code: from.to_string(),
        to_code: to.to_string(),
        fee_delta_cents: fee_delta_cents(from, to),
        reasoning: reasoning.to_string(),
    }
}

/// Replace the first BillingCode whose `code == from_code` with a fresh
/// BillingCode for `to_code`. Preserves the original quantity + after_hours
/// flag; sets `auto_extracted = false` so the audit can distinguish
/// clinician-applied upgrades. Recomputes totals.
pub fn apply_upgrade_in_record(
    record: &mut BillingRecord,
    from_code: &str,
    to_code: &str,
) -> Result<(), String> {
    let to_ohip = ohip_codes::get_code(to_code)
        .ok_or_else(|| format!("to_code {to_code} not in OHIP corpus"))?;
    let pos = record
        .codes
        .iter()
        .position(|c| c.code == from_code)
        .ok_or_else(|| format!("from_code {from_code} not present in record.codes"))?;
    let preserved_qty = record.codes[pos].quantity;
    let preserved_after_hours = record.codes[pos].after_hours;

    let mut new_code = super::rule_engine::make_billing_code(
        to_ohip,
        BillingConfidence::High,
        preserved_after_hours,
    );
    new_code.quantity = preserved_qty;
    new_code.auto_extracted = false;

    record.codes[pos] = new_code;
    record.recalculate_totals();
    Ok(())
}

// ── Predicates (clinical decisions — owned by clinic operators) ────────────

/// A004A → A007A. Suggest unless the visit looks comprehensive — A004
/// expects multi-problem / multi-system / 20-30 min, while A007 fits the
/// everyday focused follow-up and pays $5.20 more.
fn check_a004_to_a007(
    record: &BillingRecord,
    features: &ClinicalFeatures,
) -> Option<UpgradeSuggestion> {
    if !record.has_code("A004A") {
        return None;
    }

    let conditions = features.conditions.len();
    let procedures = features.procedures.len();
    let duration = features.estimated_duration_minutes.unwrap_or(0);
    let looks_comprehensive = conditions >= 3 || duration >= 25 || procedures >= 2;
    if looks_comprehensive {
        return None;
    }

    Some(build_suggestion(
        "A004A",
        "A007A",
        "Focused follow-up — A007A intermediate assessment fits 1-2 problems / ≤20 min and pays $5.20 more than A004A.",
    ))
}

/// K005A → K013A. Same fee; this is a clinical-fit swap. K005 is reserved
/// for primary MH care; K013 covers general counselling time. Uses the same
/// MH keyword list (`rule_engine::MH_KEYWORDS`) that gates K005 admission so
/// the two checks can't drift.
fn check_k005_to_k013(
    record: &BillingRecord,
    features: &ClinicalFeatures,
) -> Option<UpgradeSuggestion> {
    if !record.has_code("K005A") {
        return None;
    }

    let primary_dx = features.primary_diagnosis.as_deref().unwrap_or("").trim();
    // Don't second-guess the rule-engine's existing K005 admit-side guard
    // when there's no primary_diagnosis to compare against.
    if primary_dx.is_empty() || text_has_mental_health_keywords(primary_dx) {
        return None;
    }

    Some(build_suggestion(
        "K005A",
        "K013A",
        "Primary diagnosis isn't mental-health framed. If the counselling time was lifestyle / chronic-disease / smoking cessation, K013 (general counselling, same $80/unit, 3/yr cap with K033 overflow) is the cleaner code.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::clinical_features::{ConditionType, EncounterSetting, ProcedureType, VisitType};
    use crate::billing::ohip_codes;
    use crate::billing::types::{
        BillingCode, BillingConfidence, BillingRecord, BillingStatus,
    };

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

    fn empty_record() -> BillingRecord {
        BillingRecord {
            session_id: "s1".into(),
            date: "2026-05-07".into(),
            patient_name: None,
            status: BillingStatus::Draft,
            codes: vec![],
            time_entries: vec![],
            total_shadow_cents: 0,
            total_out_of_basket_cents: 0,
            total_time_based_cents: 0,
            total_amount_cents: 0,
            confirmed_at: None,
            notes: None,
            extraction_model: None,
            extracted_at: None,
            diagnostic_code: None,
            diagnostic_description: None,
            diagnostic_evidence: None,
            diagnostic_reasoning: None,
            suggestions: vec![],
            applied_upgrades: vec![],
        }
    }

    fn billing_code(code: &str) -> BillingCode {
        let ohip = ohip_codes::get_code(code).expect("code in corpus");
        super::super::rule_engine::make_billing_code(ohip, BillingConfidence::High, false)
    }

    // ── Helpers ────────────────────────────────────────────────────────────

    #[test]
    fn fee_delta_a004_to_a007_is_plus_520_cents() {
        // A004A = $39.35, A007A = $44.55, delta = $5.20 = 520 cents.
        assert_eq!(fee_delta_cents("A004A", "A007A"), 520);
    }

    #[test]
    fn fee_delta_k005_to_k013_is_zero() {
        // K005A and K013A both bill at $80 in the current corpus.
        assert_eq!(fee_delta_cents("K005A", "K013A"), 0);
    }

    #[test]
    fn fee_delta_unknown_codes_returns_zero() {
        assert_eq!(fee_delta_cents("ZZZZ9", "ZZZZ8"), 0);
        assert_eq!(fee_delta_cents("A004A", "ZZZZ9"), 0);
    }

    #[test]
    fn build_suggestion_populates_fee_delta_from_corpus() {
        let s = build_suggestion("A004A", "A007A", "test");
        assert_eq!(s.from_code, "A004A");
        assert_eq!(s.to_code, "A007A");
        assert_eq!(s.fee_delta_cents, 520);
        assert_eq!(s.reasoning, "test");
    }

    // ── Compute behavior ──────────────────────────────────────────────────

    #[test]
    fn compute_returns_empty_when_no_relevant_codes() {
        assert!(compute_upgrade_suggestions(&empty_record(), &default_features()).is_empty());
    }

    #[test]
    fn compute_short_circuits_when_source_codes_absent() {
        let mut record = empty_record();
        record.codes.push(billing_code("A007A")); // already upgraded
        record.codes.push(billing_code("K013A")); // not K005A
        assert!(compute_upgrade_suggestions(&record, &default_features()).is_empty());
    }

    /// Ordering is fixed: A004→A007 first, K005→K013 second. Pins the contract.
    #[test]
    fn compute_ordering_is_stable() {
        let mut record = empty_record();
        record.codes.push(billing_code("A004A"));
        record.codes.push(billing_code("K005A"));
        let mut features = default_features();
        features.primary_diagnosis = Some("Type 2 diabetes — annual review".into());
        features.estimated_duration_minutes = Some(15);
        features.conditions = vec![ConditionType::DiabeticAssessment];

        let suggestions = compute_upgrade_suggestions(&record, &features);
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].from_code, "A004A");
        assert_eq!(suggestions[0].to_code, "A007A");
        assert_eq!(suggestions[1].from_code, "K005A");
        assert_eq!(suggestions[1].to_code, "K013A");
    }

    // ── A004 → A007 ────────────────────────────────────────────────────────

    #[test]
    fn a004_focused_visit_suggests_a007() {
        let mut record = empty_record();
        record.codes.push(billing_code("A004A"));
        let mut features = default_features();
        features.estimated_duration_minutes = Some(15);
        features.conditions = vec![ConditionType::DiabeticAssessment];

        let s = check_a004_to_a007(&record, &features).expect("focused visit fires");
        assert_eq!(s.fee_delta_cents, 520);
    }

    #[test]
    fn a004_long_visit_does_not_suggest() {
        let mut record = empty_record();
        record.codes.push(billing_code("A004A"));
        let mut features = default_features();
        features.estimated_duration_minutes = Some(28);
        assert!(check_a004_to_a007(&record, &features).is_none());
    }

    #[test]
    fn a004_many_conditions_does_not_suggest() {
        let mut record = empty_record();
        record.codes.push(billing_code("A004A"));
        let mut features = default_features();
        features.conditions = vec![
            ConditionType::DiabeticAssessment,
            ConditionType::ChfManagement,
            ConditionType::SmokingCessation,
        ];
        features.estimated_duration_minutes = Some(15);
        assert!(check_a004_to_a007(&record, &features).is_none());
    }

    #[test]
    fn a004_with_multiple_procedures_does_not_suggest() {
        let mut record = empty_record();
        record.codes.push(billing_code("A004A"));
        let mut features = default_features();
        features.procedures = vec![
            ProcedureType::JointInjection,
            ProcedureType::TriggerPointInjection,
        ];
        features.estimated_duration_minutes = Some(15);
        assert!(check_a004_to_a007(&record, &features).is_none());
    }

    // ── K005 → K013 ────────────────────────────────────────────────────────

    #[test]
    fn k005_with_non_mh_primary_dx_suggests_k013() {
        let mut record = empty_record();
        record.codes.push(billing_code("K005A"));
        let mut features = default_features();
        features.primary_diagnosis = Some("Hypertension management with med titration".into());

        let s = check_k005_to_k013(&record, &features).expect("non-MH dx fires");
        assert_eq!(s.fee_delta_cents, 0);
        assert_eq!(s.from_code, "K005A");
        assert_eq!(s.to_code, "K013A");
    }

    #[test]
    fn k005_with_mh_primary_dx_does_not_suggest() {
        // Each phrase must contain at least one of rule_engine::MH_KEYWORDS so
        // the upgrade-side stays aligned with the K005 admit-side guard.
        for dx in [
            "Generalized anxiety disorder",
            "Major depressive episode",
            "PTSD with intrusive symptoms",
            "Bipolar II — med review",
            "OCD with ruminations",
            "Anger management coaching",
            "Counselling for grief",
        ] {
            let mut record = empty_record();
            record.codes.push(billing_code("K005A"));
            let mut features = default_features();
            features.primary_diagnosis = Some(dx.into());
            assert!(
                check_k005_to_k013(&record, &features).is_none(),
                "expected NO suggestion for MH dx {dx:?}"
            );
        }
    }

    #[test]
    fn k005_with_no_primary_dx_does_not_suggest() {
        let mut record = empty_record();
        record.codes.push(billing_code("K005A"));
        assert!(check_k005_to_k013(&record, &default_features()).is_none());
    }

    // ── Apply ──────────────────────────────────────────────────────────────

    #[test]
    fn apply_swaps_code_and_preserves_quantity() {
        let mut record = empty_record();
        let mut k005 = billing_code("K005A");
        k005.quantity = 4;
        record.codes.push(k005);

        apply_upgrade_in_record(&mut record, "K005A", "K013A").unwrap();

        assert_eq!(record.codes.len(), 1);
        assert_eq!(record.codes[0].code, "K013A");
        assert_eq!(record.codes[0].quantity, 4);
        assert!(!record.codes[0].auto_extracted);
    }

    #[test]
    fn apply_recalculates_totals() {
        let mut record = empty_record();
        record.codes.push(billing_code("A004A"));
        record.recalculate_totals();
        let pre_total = record.total_amount_cents;
        apply_upgrade_in_record(&mut record, "A004A", "A007A").unwrap();
        assert!(record.total_amount_cents > pre_total);
    }

    #[test]
    fn apply_errors_when_from_code_missing() {
        let mut record = empty_record();
        let err = apply_upgrade_in_record(&mut record, "A004A", "A007A").unwrap_err();
        assert!(err.contains("not present"));
    }

    #[test]
    fn apply_errors_when_to_code_unknown() {
        let mut record = empty_record();
        record.codes.push(billing_code("A004A"));
        let err = apply_upgrade_in_record(&mut record, "A004A", "ZZZZ9").unwrap_err();
        assert!(err.contains("not in OHIP corpus"));
    }
}
