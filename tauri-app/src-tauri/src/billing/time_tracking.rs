use super::clinical_features::EncounterSetting;
use super::types::*;
use std::collections::HashMap;

/// Calculate a Q310 or Q311 time entry from the encounter duration in milliseconds.
///
/// - Q310 (Direct Patient Care, $20/15 min) for InOffice, HomeVisit, TelephoneInOffice, Video
/// - Q311 (Telephone Remote, $17/15 min) for TelephoneRemote
///
/// Rounding: total_minutes / 15 with 8+ minute remainder rounding up to the next unit.
pub fn calculate_direct_care_time(duration_ms: u64, setting: &EncounterSetting) -> TimeEntry {
    let total_minutes = (duration_ms / 1000 / 60) as u16;
    let billable_units = round_to_15min_units(total_minutes);

    let (code, description, rate_cents) = match setting {
        EncounterSetting::TelephoneRemote => ("Q311", "Telephone Remote", 1700u32),
        _ => ("Q310", "Direct Patient Care", 2000u32),
    };

    let billable_amount_cents = billable_units as u32 * rate_cents;

    TimeEntry {
        code: code.to_string(),
        description: description.to_string(),
        rate_per_15min_cents: rate_cents,
        minutes: total_minutes,
        billable_units,
        billable_amount_cents,
        auto_calculated: true,
    }
}

/// Round minutes to 15-minute billing units.
/// Remainder of 8+ minutes rounds up to the next unit.
fn round_to_15min_units(minutes: u16) -> u16 {
    let full_units = minutes / 15;
    let remainder = minutes % 15;
    if remainder >= 8 {
        full_units + 1
    } else {
        full_units
    }
}

/// Aggregate daily time from billing records and compute cap status.
pub fn calculate_daily_caps(records: &[BillingRecord]) -> DailyCapStatus {
    let mut total_minutes: u32 = 0;
    for rec in records {
        for te in &rec.time_entries {
            total_minutes += te.minutes as u32;
        }
    }

    let hours_used = total_minutes as f32 / 60.0;
    let percentage = hours_used / DAILY_HOUR_LIMIT;
    let warning_level = get_warning_level(percentage);

    DailyCapStatus {
        hours_used,
        hours_limit: DAILY_HOUR_LIMIT,
        percentage,
        warning_level,
    }
}

/// Calculate 28-day rolling window caps from daily summaries.
pub fn calculate_monthly_caps(daily_summaries: &[BillingDaySummary]) -> MonthlyCapStatus {
    // Take only the most recent 28 days
    let window: &[BillingDaySummary] = if daily_summaries.len() > MONTHLY_WINDOW_DAYS as usize {
        &daily_summaries[daily_summaries.len() - MONTHLY_WINDOW_DAYS as usize..]
    } else {
        daily_summaries
    };

    let mut total_hours: f32 = 0.0;
    let mut hours_by_code: HashMap<String, f32> = HashMap::new();

    for day in window {
        total_hours += day.total_time_hours;
        for (code, &hours) in &day.time_hours_by_code {
            *hours_by_code.entry(code.clone()).or_insert(0.0) += hours;
        }
    }

    let q312_hours = hours_by_code.get("Q312").copied().unwrap_or(0.0);
    let q313_hours = hours_by_code.get("Q313").copied().unwrap_or(0.0);

    // Indirect + admin ratio: (Q312 + Q313) / total
    let indirect_admin_ratio = if total_hours > 0.0 {
        (q312_hours + q313_hours) / total_hours
    } else {
        0.0
    };

    // Admin-alone ratio: Q313 / total
    let admin_ratio = if total_hours > 0.0 {
        q313_hours / total_hours
    } else {
        0.0
    };

    let hours_percentage = total_hours / MONTHLY_HOUR_LIMIT;

    // Warning level: worst of hours, indirect ratio, admin ratio
    let hours_warning = get_warning_level(hours_percentage);
    let indirect_warning =
        get_warning_level(indirect_admin_ratio / INDIRECT_ADMIN_RATIO_LIMIT);
    let admin_warning = get_warning_level(admin_ratio / ADMIN_ALONE_RATIO_LIMIT);

    let warning_level = worst_warning(&[hours_warning, indirect_warning, admin_warning]);

    // Projected cap hit date
    let days_count = window.len() as f32;
    let projected_cap_hit_date = if total_hours > 0.0 && days_count > 0.0 {
        let avg_daily = total_hours / days_count;
        if avg_daily > 0.0 {
            let remaining_hours = MONTHLY_HOUR_LIMIT - total_hours;
            if remaining_hours > 0.0 {
                let days_to_cap = (remaining_hours / avg_daily).ceil() as i64;
                let today = chrono::Utc::now().date_naive();
                if let Some(cap_date) =
                    today.checked_add_signed(chrono::TimeDelta::days(days_to_cap))
                {
                    Some(cap_date.format("%Y-%m-%d").to_string())
                } else {
                    None
                }
            } else {
                // Already at or over cap
                Some(chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string())
            }
        } else {
            None
        }
    } else {
        None
    };

    MonthlyCapStatus {
        hours_used: total_hours,
        hours_limit: MONTHLY_HOUR_LIMIT,
        hours_percentage,
        indirect_admin_ratio,
        indirect_admin_limit: INDIRECT_ADMIN_RATIO_LIMIT,
        admin_ratio,
        admin_limit: ADMIN_ALONE_RATIO_LIMIT,
        warning_level,
        projected_cap_hit_date,
    }
}

/// Determine the warning level from a percentage (0.0 = 0%, 1.0 = 100%).
pub fn get_warning_level(percentage: f32) -> CapWarning {
    if percentage >= 1.0 {
        CapWarning::Exceeded
    } else if percentage >= CRITICAL_THRESHOLD {
        CapWarning::Critical
    } else if percentage >= WARNING_THRESHOLD {
        CapWarning::Warning
    } else {
        CapWarning::Normal
    }
}

/// Return the most severe warning from a list.
fn worst_warning(warnings: &[CapWarning]) -> CapWarning {
    fn severity(w: &CapWarning) -> u8 {
        match w {
            CapWarning::Normal => 0,
            CapWarning::Warning => 1,
            CapWarning::Critical => 2,
            CapWarning::Exceeded => 3,
        }
    }

    warnings
        .iter()
        .max_by_key(|w| severity(w))
        .cloned()
        .unwrap_or(CapWarning::Normal)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Time rounding tests ────────────────────────────────────────────────

    #[test]
    fn test_round_0_min() {
        assert_eq!(round_to_15min_units(0), 0);
    }

    #[test]
    fn test_round_7_min() {
        // 7 remainder < 8 → 0 units
        assert_eq!(round_to_15min_units(7), 0);
    }

    #[test]
    fn test_round_8_min() {
        // 8 remainder >= 8 → 1 unit
        assert_eq!(round_to_15min_units(8), 1);
    }

    #[test]
    fn test_round_14_min() {
        // 14 remainder >= 8 → 1 unit
        assert_eq!(round_to_15min_units(14), 1);
    }

    #[test]
    fn test_round_15_min() {
        // Exactly 1 unit, 0 remainder
        assert_eq!(round_to_15min_units(15), 1);
    }

    #[test]
    fn test_round_22_min() {
        // 1 full unit + 7 remainder < 8 → 1 unit
        assert_eq!(round_to_15min_units(22), 1);
    }

    #[test]
    fn test_round_23_min() {
        // 1 full unit + 8 remainder >= 8 → 2 units
        assert_eq!(round_to_15min_units(23), 2);
    }

    #[test]
    fn test_round_30_min() {
        // Exactly 2 units
        assert_eq!(round_to_15min_units(30), 2);
    }

    #[test]
    fn test_round_45_min() {
        assert_eq!(round_to_15min_units(45), 3);
    }

    #[test]
    fn test_round_53_min() {
        // 3 full + 8 → 4
        assert_eq!(round_to_15min_units(53), 4);
    }

    #[test]
    fn test_round_60_min() {
        assert_eq!(round_to_15min_units(60), 4);
    }

    // ── calculate_direct_care_time tests ───────────────────────────────────

    #[test]
    fn test_direct_care_in_office() {
        let te = calculate_direct_care_time(15 * 60 * 1000, &EncounterSetting::InOffice);
        assert_eq!(te.code, "Q310");
        assert_eq!(te.rate_per_15min_cents, 2000);
        assert_eq!(te.minutes, 15);
        assert_eq!(te.billable_units, 1);
        assert_eq!(te.billable_amount_cents, 2000);
        assert!(te.auto_calculated);
    }

    #[test]
    fn test_direct_care_telephone_remote() {
        let te = calculate_direct_care_time(15 * 60 * 1000, &EncounterSetting::TelephoneRemote);
        assert_eq!(te.code, "Q311");
        assert_eq!(te.rate_per_15min_cents, 1700);
        assert_eq!(te.billable_amount_cents, 1700);
    }

    #[test]
    fn test_direct_care_home_visit() {
        let te = calculate_direct_care_time(30 * 60 * 1000, &EncounterSetting::HomeVisit);
        assert_eq!(te.code, "Q310");
        assert_eq!(te.billable_units, 2);
        assert_eq!(te.billable_amount_cents, 4000);
    }

    #[test]
    fn test_direct_care_video() {
        let te = calculate_direct_care_time(23 * 60 * 1000, &EncounterSetting::Video);
        assert_eq!(te.code, "Q310");
        assert_eq!(te.billable_units, 2);
    }

    #[test]
    fn test_direct_care_zero_duration() {
        let te = calculate_direct_care_time(0, &EncounterSetting::InOffice);
        assert_eq!(te.billable_units, 0);
        assert_eq!(te.billable_amount_cents, 0);
    }

    // ── Daily cap tests ────────────────────────────────────────────────────

    fn make_record_with_time(minutes: u16) -> BillingRecord {
        BillingRecord {
            session_id: "s1".into(),
            date: "2026-04-05".into(),
            patient_name: None,
            status: BillingStatus::Draft,
            codes: vec![],
            time_entries: vec![TimeEntry {
                code: "Q310".into(),
                description: "Direct Patient Care".into(),
                rate_per_15min_cents: 2000,
                minutes,
                billable_units: round_to_15min_units(minutes),
                billable_amount_cents: round_to_15min_units(minutes) as u32 * 2000,
                auto_calculated: true,
            }],
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
    fn test_daily_caps_normal() {
        let records = vec![make_record_with_time(120)]; // 2 hours
        let caps = calculate_daily_caps(&records);
        assert_eq!(caps.hours_used, 2.0);
        assert_eq!(caps.hours_limit, 14.0);
        assert!((caps.percentage - 2.0 / 14.0).abs() < 0.001);
        assert_eq!(caps.warning_level, CapWarning::Normal);
    }

    #[test]
    fn test_daily_caps_warning() {
        // 80% of 14h = 11.2h = 672 min
        let records = vec![make_record_with_time(672)];
        let caps = calculate_daily_caps(&records);
        assert_eq!(caps.warning_level, CapWarning::Warning);
    }

    #[test]
    fn test_daily_caps_critical() {
        // 95% of 14h = 13.3h = 798 min
        let records = vec![make_record_with_time(798)];
        let caps = calculate_daily_caps(&records);
        assert_eq!(caps.warning_level, CapWarning::Critical);
    }

    #[test]
    fn test_daily_caps_exceeded() {
        // 14h+ = 840+ min
        let records = vec![make_record_with_time(841)];
        let caps = calculate_daily_caps(&records);
        assert_eq!(caps.warning_level, CapWarning::Exceeded);
    }

    #[test]
    fn test_daily_caps_empty() {
        let records: Vec<BillingRecord> = vec![];
        let caps = calculate_daily_caps(&records);
        assert_eq!(caps.hours_used, 0.0);
        assert_eq!(caps.warning_level, CapWarning::Normal);
    }

    #[test]
    fn test_daily_caps_multiple_records() {
        let records = vec![
            make_record_with_time(60),
            make_record_with_time(90),
            make_record_with_time(30),
        ];
        let caps = calculate_daily_caps(&records);
        assert_eq!(caps.hours_used, 3.0); // 180 min = 3h
    }

    // ── Monthly cap tests ──────────────────────────────────────────────────

    fn make_day_summary(date: &str, total_hours: f32, q312_hours: f32, q313_hours: f32) -> BillingDaySummary {
        let mut time_hours_by_code = HashMap::new();
        let q310_hours = total_hours - q312_hours - q313_hours;
        if q310_hours > 0.0 {
            time_hours_by_code.insert("Q310".to_string(), q310_hours);
        }
        if q312_hours > 0.0 {
            time_hours_by_code.insert("Q312".to_string(), q312_hours);
        }
        if q313_hours > 0.0 {
            time_hours_by_code.insert("Q313".to_string(), q313_hours);
        }

        BillingDaySummary {
            date: date.to_string(),
            encounter_count: 1,
            encounters: vec![],
            total_shadow_cents: 0,
            total_out_of_basket_cents: 0,
            total_time_based_cents: 0,
            total_amount_cents: 0,
            time_hours_by_code,
            total_time_hours: total_hours,
            confirmed_count: 0,
            draft_count: 1,
            cap_status: DailyCapStatus {
                hours_used: total_hours,
                hours_limit: DAILY_HOUR_LIMIT,
                percentage: total_hours / DAILY_HOUR_LIMIT,
                warning_level: CapWarning::Normal,
            },
        }
    }

    #[test]
    fn test_monthly_caps_normal() {
        let summaries: Vec<BillingDaySummary> = (0..20)
            .map(|i| make_day_summary(&format!("2026-03-{:02}", i + 1), 8.0, 1.0, 0.2))
            .collect();
        let caps = calculate_monthly_caps(&summaries);
        // 20 days * 8h = 160h < 240h → normal
        assert_eq!(caps.hours_used, 160.0);
        assert_eq!(caps.hours_limit, 240.0);
    }

    #[test]
    fn test_monthly_caps_window_28_days() {
        // 35 days — should only use last 28
        let summaries: Vec<BillingDaySummary> = (0..35)
            .map(|i| make_day_summary(&format!("2026-03-{:02}", (i % 28) + 1), 8.0, 0.0, 0.0))
            .collect();
        let caps = calculate_monthly_caps(&summaries);
        // 28 * 8 = 224
        assert_eq!(caps.hours_used, 224.0);
    }

    #[test]
    fn test_monthly_caps_indirect_admin_ratio() {
        // 10h total per day, 2h Q312, 0.5h Q313 → ratio = 2.5/10 = 0.25
        let summaries = vec![make_day_summary("2026-04-01", 10.0, 2.0, 0.5)];
        let caps = calculate_monthly_caps(&summaries);
        assert!((caps.indirect_admin_ratio - 0.25).abs() < 0.001);
        assert!((caps.admin_ratio - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_monthly_caps_admin_ratio_exceeded() {
        // 10h total, 0h Q312, 1h Q313 → admin_ratio = 0.10, limit 0.05 → exceeded
        let summaries = vec![make_day_summary("2026-04-01", 10.0, 0.0, 1.0)];
        let caps = calculate_monthly_caps(&summaries);
        assert!((caps.admin_ratio - 0.10).abs() < 0.001);
        // 0.10 / 0.05 = 2.0 → exceeded
        assert_eq!(caps.warning_level, CapWarning::Exceeded);
    }

    #[test]
    fn test_monthly_caps_empty() {
        let summaries: Vec<BillingDaySummary> = vec![];
        let caps = calculate_monthly_caps(&summaries);
        assert_eq!(caps.hours_used, 0.0);
        assert_eq!(caps.indirect_admin_ratio, 0.0);
        assert_eq!(caps.admin_ratio, 0.0);
        assert_eq!(caps.warning_level, CapWarning::Normal);
        assert!(caps.projected_cap_hit_date.is_none());
    }

    #[test]
    fn test_monthly_caps_projected_date() {
        // 10h/day for 10 days = 100h used, 140h remaining, avg 10h/day → 14 more days
        let summaries: Vec<BillingDaySummary> = (0..10)
            .map(|i| make_day_summary(&format!("2026-04-{:02}", i + 1), 10.0, 0.0, 0.0))
            .collect();
        let caps = calculate_monthly_caps(&summaries);
        assert!(caps.projected_cap_hit_date.is_some());
    }

    // ── Warning level tests ────────────────────────────────────────────────

    #[test]
    fn test_warning_level_normal() {
        assert_eq!(get_warning_level(0.0), CapWarning::Normal);
        assert_eq!(get_warning_level(0.5), CapWarning::Normal);
        assert_eq!(get_warning_level(0.79), CapWarning::Normal);
    }

    #[test]
    fn test_warning_level_warning() {
        assert_eq!(get_warning_level(0.80), CapWarning::Warning);
        assert_eq!(get_warning_level(0.90), CapWarning::Warning);
        assert_eq!(get_warning_level(0.94), CapWarning::Warning);
    }

    #[test]
    fn test_warning_level_critical() {
        assert_eq!(get_warning_level(0.95), CapWarning::Critical);
        assert_eq!(get_warning_level(0.99), CapWarning::Critical);
    }

    #[test]
    fn test_warning_level_exceeded() {
        assert_eq!(get_warning_level(1.0), CapWarning::Exceeded);
        assert_eq!(get_warning_level(1.5), CapWarning::Exceeded);
    }

    #[test]
    fn test_worst_warning() {
        assert_eq!(
            worst_warning(&[CapWarning::Normal, CapWarning::Normal]),
            CapWarning::Normal
        );
        assert_eq!(
            worst_warning(&[CapWarning::Normal, CapWarning::Warning]),
            CapWarning::Warning
        );
        assert_eq!(
            worst_warning(&[CapWarning::Warning, CapWarning::Critical]),
            CapWarning::Critical
        );
        assert_eq!(
            worst_warning(&[CapWarning::Normal, CapWarning::Exceeded]),
            CapWarning::Exceeded
        );
    }
}
