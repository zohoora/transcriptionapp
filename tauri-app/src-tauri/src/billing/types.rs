use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Cap / threshold constants ──────────────────────────────────────────────

pub const DAILY_HOUR_LIMIT: f32 = 14.0;
pub const MONTHLY_HOUR_LIMIT: f32 = 240.0;
pub const MONTHLY_WINDOW_DAYS: u32 = 28;
pub const INDIRECT_ADMIN_RATIO_LIMIT: f32 = 0.25;
pub const ADMIN_ALONE_RATIO_LIMIT: f32 = 0.05;
pub const WARNING_THRESHOLD: f32 = 0.80;
pub const CRITICAL_THRESHOLD: f32 = 0.95;

// ── Enums ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BillingStatus {
    Draft,
    Confirmed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BillingConfidence {
    High,
    Medium,
    Low,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CapWarning {
    Normal,
    Warning,
    Critical,
    Exceeded,
}

// ── Billing code (one line item) ───────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BillingCode {
    pub code: String,
    pub description: String,
    pub fee_cents: u32,
    pub category: String,
    pub shadow_pct: u8,
    pub billable_amount_cents: u32,
    pub confidence: BillingConfidence,
    pub auto_extracted: bool,
    pub after_hours: bool,
    pub after_hours_premium_cents: u32,
    /// Quantity (default 1). For add-on codes like G385A (max 2), can be >1.
    #[serde(default = "default_quantity")]
    pub quantity: u8,
}

fn default_quantity() -> u8 {
    1
}

// ── Time entry (Q310–Q313) ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntry {
    pub code: String,
    pub description: String,
    pub rate_per_15min_cents: u32,
    pub minutes: u16,
    pub billable_units: u16,
    pub billable_amount_cents: u32,
    pub auto_calculated: bool,
}

// ── Full billing record for one encounter ──────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BillingRecord {
    pub session_id: String,
    pub date: String,
    pub patient_name: Option<String>,
    pub status: BillingStatus,
    pub codes: Vec<BillingCode>,
    pub time_entries: Vec<TimeEntry>,
    pub total_shadow_cents: u32,
    pub total_out_of_basket_cents: u32,
    pub total_time_based_cents: u32,
    pub total_amount_cents: u32,
    pub confirmed_at: Option<String>,
    pub notes: Option<String>,
    pub extraction_model: Option<String>,
    pub extracted_at: Option<String>,
}

impl BillingRecord {
    /// Recalculate the aggregate totals from the individual codes and time entries.
    pub fn recalculate_totals(&mut self) {
        let mut shadow_cents: u32 = 0;
        let mut out_of_basket_cents: u32 = 0;

        for c in &self.codes {
            let qty = c.quantity.max(1) as u32;
            if c.category == "in_basket" {
                shadow_cents = shadow_cents.saturating_add(c.billable_amount_cents * qty);
                if c.after_hours {
                    shadow_cents = shadow_cents.saturating_add(c.after_hours_premium_cents * qty);
                }
            } else {
                // out_of_basket
                out_of_basket_cents = out_of_basket_cents.saturating_add(c.billable_amount_cents * qty);
                if c.after_hours {
                    out_of_basket_cents =
                        out_of_basket_cents.saturating_add(c.after_hours_premium_cents * qty);
                }
            }
        }

        let mut time_cents: u32 = 0;
        for t in &self.time_entries {
            time_cents = time_cents.saturating_add(t.billable_amount_cents);
        }

        self.total_shadow_cents = shadow_cents;
        self.total_out_of_basket_cents = out_of_basket_cents;
        self.total_time_based_cents = time_cents;
        self.total_amount_cents = shadow_cents
            .saturating_add(out_of_basket_cents)
            .saturating_add(time_cents);
    }
}

// ── Daily cap status ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DailyCapStatus {
    pub hours_used: f32,
    pub hours_limit: f32,
    pub percentage: f32,
    pub warning_level: CapWarning,
}

// ── Monthly (28-day rolling) cap status ────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyCapStatus {
    pub hours_used: f32,
    pub hours_limit: f32,
    pub hours_percentage: f32,
    pub indirect_admin_ratio: f32,
    pub indirect_admin_limit: f32,
    pub admin_ratio: f32,
    pub admin_limit: f32,
    pub warning_level: CapWarning,
    pub projected_cap_hit_date: Option<String>,
}

// ── Day summary ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BillingDaySummary {
    pub date: String,
    pub encounter_count: u32,
    pub encounters: Vec<BillingRecord>,
    pub total_shadow_cents: u32,
    pub total_out_of_basket_cents: u32,
    pub total_time_based_cents: u32,
    pub total_amount_cents: u32,
    pub time_hours_by_code: HashMap<String, f32>,
    pub total_time_hours: f32,
    pub confirmed_count: u32,
    pub draft_count: u32,
    pub cap_status: DailyCapStatus,
}

// ── Month summary ──────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BillingMonthSummary {
    pub period_start: String,
    pub period_end: String,
    pub daily_summaries: Vec<BillingDaySummary>,
    pub total_shadow_cents: u32,
    pub total_out_of_basket_cents: u32,
    pub total_time_based_cents: u32,
    pub total_amount_cents: u32,
    pub total_hours: f32,
    pub hours_by_code: HashMap<String, f32>,
    pub indirect_admin_ratio: f32,
    pub admin_ratio: f32,
    pub cap_status: MonthlyCapStatus,
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_code(category: &str, billable: u32, after_hours: bool, ah_premium: u32) -> BillingCode {
        BillingCode {
            code: "TEST".into(),
            description: "Test code".into(),
            fee_cents: 1000,
            category: category.into(),
            shadow_pct: 30,
            billable_amount_cents: billable,
            confidence: BillingConfidence::High,
            auto_extracted: true,
            after_hours,
            after_hours_premium_cents: ah_premium,
            quantity: 1,
        }
    }

    fn make_time_entry(code: &str, billable: u32) -> TimeEntry {
        TimeEntry {
            code: code.into(),
            description: "Test".into(),
            rate_per_15min_cents: 2000,
            minutes: 15,
            billable_units: 1,
            billable_amount_cents: billable,
            auto_calculated: true,
        }
    }

    fn empty_record() -> BillingRecord {
        BillingRecord {
            session_id: "s1".into(),
            date: "2026-04-05".into(),
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
        }
    }

    #[test]
    fn test_recalculate_totals_empty() {
        let mut rec = empty_record();
        rec.recalculate_totals();
        assert_eq!(rec.total_amount_cents, 0);
    }

    #[test]
    fn test_recalculate_totals_in_basket_only() {
        let mut rec = empty_record();
        rec.codes.push(make_code("in_basket", 300, false, 0));
        rec.codes.push(make_code("in_basket", 200, false, 0));
        rec.recalculate_totals();
        assert_eq!(rec.total_shadow_cents, 500);
        assert_eq!(rec.total_out_of_basket_cents, 0);
        assert_eq!(rec.total_amount_cents, 500);
    }

    #[test]
    fn test_recalculate_totals_out_of_basket() {
        let mut rec = empty_record();
        rec.codes.push(make_code("out_of_basket", 8035, false, 0));
        rec.recalculate_totals();
        assert_eq!(rec.total_out_of_basket_cents, 8035);
        assert_eq!(rec.total_shadow_cents, 0);
        assert_eq!(rec.total_amount_cents, 8035);
    }

    #[test]
    fn test_recalculate_totals_with_after_hours() {
        let mut rec = empty_record();
        rec.codes.push(make_code("in_basket", 300, true, 150));
        rec.recalculate_totals();
        // 300 + 150 after-hours premium
        assert_eq!(rec.total_shadow_cents, 450);
        assert_eq!(rec.total_amount_cents, 450);
    }

    #[test]
    fn test_recalculate_totals_mixed() {
        let mut rec = empty_record();
        rec.codes.push(make_code("in_basket", 300, false, 0));
        rec.codes.push(make_code("out_of_basket", 5000, false, 0));
        rec.time_entries.push(make_time_entry("Q310A", 2000));
        rec.time_entries.push(make_time_entry("Q310A", 2000));
        rec.recalculate_totals();
        assert_eq!(rec.total_shadow_cents, 300);
        assert_eq!(rec.total_out_of_basket_cents, 5000);
        assert_eq!(rec.total_time_based_cents, 4000);
        assert_eq!(rec.total_amount_cents, 9300);
    }

    #[test]
    fn test_serde_billing_status() {
        let json = serde_json::to_string(&BillingStatus::Draft).unwrap();
        assert_eq!(json, "\"draft\"");
        let json = serde_json::to_string(&BillingStatus::Confirmed).unwrap();
        assert_eq!(json, "\"confirmed\"");
    }

    #[test]
    fn test_serde_billing_confidence() {
        let json = serde_json::to_string(&BillingConfidence::High).unwrap();
        assert_eq!(json, "\"high\"");
        let json = serde_json::to_string(&BillingConfidence::Medium).unwrap();
        assert_eq!(json, "\"medium\"");
        let json = serde_json::to_string(&BillingConfidence::Low).unwrap();
        assert_eq!(json, "\"low\"");
    }

    #[test]
    fn test_serde_cap_warning() {
        for (variant, expected) in [
            (CapWarning::Normal, "\"normal\""),
            (CapWarning::Warning, "\"warning\""),
            (CapWarning::Critical, "\"critical\""),
            (CapWarning::Exceeded, "\"exceeded\""),
        ] {
            assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
        }
    }

    #[test]
    fn test_serde_camel_case_billing_code() {
        let code = make_code("in_basket", 300, false, 0);
        let json = serde_json::to_string(&code).unwrap();
        assert!(json.contains("\"feeCents\""));
        assert!(json.contains("\"billableAmountCents\""));
        assert!(json.contains("\"shadowPct\""));
        assert!(json.contains("\"autoExtracted\""));
        assert!(json.contains("\"afterHours\""));
        assert!(json.contains("\"afterHoursPremiumCents\""));
    }

    #[test]
    fn test_serde_camel_case_time_entry() {
        let te = make_time_entry("Q310A", 2000);
        let json = serde_json::to_string(&te).unwrap();
        assert!(json.contains("\"ratePer15minCents\""));
        assert!(json.contains("\"billableUnits\""));
        assert!(json.contains("\"billableAmountCents\""));
        assert!(json.contains("\"autoCalculated\""));
    }

    #[test]
    fn test_serde_roundtrip_billing_record() {
        let mut rec = empty_record();
        rec.codes.push(make_code("in_basket", 300, true, 150));
        rec.time_entries.push(make_time_entry("Q310A", 2000));
        rec.recalculate_totals();

        let json = serde_json::to_string(&rec).unwrap();
        let deser: BillingRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.total_amount_cents, rec.total_amount_cents);
        assert_eq!(deser.session_id, "s1");
    }

    #[test]
    fn test_constants() {
        assert_eq!(DAILY_HOUR_LIMIT, 14.0);
        assert_eq!(MONTHLY_HOUR_LIMIT, 240.0);
        assert_eq!(MONTHLY_WINDOW_DAYS, 28);
        assert_eq!(INDIRECT_ADMIN_RATIO_LIMIT, 0.25);
        assert_eq!(ADMIN_ALONE_RATIO_LIMIT, 0.05);
        assert!(WARNING_THRESHOLD < CRITICAL_THRESHOLD);
        assert!(CRITICAL_THRESHOLD < 1.0);
    }
}
