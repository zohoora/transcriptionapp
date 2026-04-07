//! Tauri commands for FHO+ billing management.

use crate::billing::{
    BillingDaySummary, BillingMonthSummary, BillingRecord,
    calculate_daily_caps, calculate_monthly_caps,
};
use crate::commands::CommandError;
use crate::commands::physicians::{SharedActivePhysician, SharedProfileClient};
use crate::config::Config;
use crate::llm_client::LLMClient;
use crate::local_archive;
use chrono::{Datelike, Duration};
use serde::Deserialize;
use tauri::State;
use tracing::{debug, info, warn};

/// Physician-provided billing context that supplements LLM extraction.
#[derive(Deserialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct BillingContext {
    /// "in_office", "phone_office", "phone_home", "video", "home_visit"
    #[serde(default = "default_setting")]
    pub visit_setting: String,
    /// "adult", "child_0_1", "child_2_15", "adolescent", "senior", "idd"
    #[serde(default = "default_age")]
    pub patient_age: String,
    /// Formal consultation referral received
    #[serde(default)]
    pub referral_received: bool,
    /// K013 3-unit annual limit reached — use K033 instead
    #[serde(default)]
    pub counselling_exhausted: bool,
    /// None = auto-detect from time, Some(true/false) = manual override
    #[serde(default)]
    pub after_hours_override: Option<bool>,
}

fn default_setting() -> String {
    "in_office".to_string()
}
fn default_age() -> String {
    "adult".to_string()
}

/// Build prompt hint text from billing context selections.
pub fn build_context_hints(ctx: &BillingContext) -> String {
    let mut hints = Vec::new();

    match ctx.visit_setting.as_str() {
        "phone_office" => hints.push("Visit was conducted by TELEPHONE from the physician's office. Use assessment codes, not A101/A102.".to_string()),
        "phone_home" => hints.push("Visit was conducted by TELEPHONE from outside the office (physician at home). Use A102 (limited virtual care phone) or Q311 for time tracking.".to_string()),
        "video" => hints.push("Visit was conducted by VIDEO telemedicine. Use A101 (limited virtual care video).".to_string()),
        "home_visit" => hints.push("Visit was a HOME VISIT to the patient's residence. Use A900 (complex house call assessment).".to_string()),
        _ => {} // in_office is default, no hint needed
    }

    match ctx.patient_age.as_str() {
        "child_0_1" => hints.push("Patient is an infant (0-1 years old). For periodic health visit use K017 (child).".to_string()),
        "child_2_15" => hints.push("Patient is a child (2-15 years old). For periodic health visit use K017 (child).".to_string()),
        "adolescent" => hints.push("Patient is an adolescent (16-17 years old). For periodic health visit use K130 (adolescent).".to_string()),
        "senior" => hints.push("Patient is 65 years or older. For periodic health visit use K132 (65+).".to_string()),
        "idd" => hints.push("Patient is an adult with intellectual/developmental disability. For periodic health visit use K133 (IDD).".to_string()),
        _ => {} // adult 18-64 is default
    }

    if ctx.referral_received {
        hints.push("A formal written REFERRAL was received from another physician for this visit. Use A005 (consultation) instead of a regular assessment.".to_string());
    }

    if ctx.counselling_exhausted {
        hints.push("The patient has EXHAUSTED their 3 K013 counselling units for this year. Use K033 (additional counselling units) instead of K013.".to_string());
    }

    if let Some(ah) = ctx.after_hours_override {
        if ah {
            hints.push("This visit is AFTER HOURS. Apply the after-hours indicator.".to_string());
        } else {
            hints.push("This visit is during regular hours. Do NOT mark as after hours.".to_string());
        }
    }

    hints.join("\n")
}

#[tauri::command]
pub fn get_session_billing(
    session_id: String,
    date: String,
) -> Result<Option<BillingRecord>, CommandError> {
    debug!("Loading billing record: {} on {}", session_id, date);
    Ok(local_archive::get_billing_record(&session_id, &super::parse_date(&date)?)?)
}

#[tauri::command]
pub fn save_session_billing(
    session_id: String,
    date: String,
    record: BillingRecord,
) -> Result<(), CommandError> {
    info!("Saving billing record for session: {}", session_id);
    local_archive::save_billing_record(&session_id, &super::parse_date(&date)?, &record)?;
    Ok(())
}

#[tauri::command]
pub fn confirm_session_billing(
    session_id: String,
    date: String,
) -> Result<BillingRecord, CommandError> {
    info!("Confirming billing for session: {}", session_id);
    let parsed_date = super::parse_date(&date)?;

    let mut record = local_archive::get_billing_record(&session_id, &parsed_date)?
        .ok_or_else(|| CommandError::NotFound("No billing record found".into()))?;

    record.status = crate::billing::BillingStatus::Confirmed;
    record.confirmed_at = Some(chrono::Utc::now().to_rfc3339());

    local_archive::save_billing_record(&session_id, &parsed_date, &record)?;
    Ok(record)
}

/// Extract billing codes from a session's SOAP note via LLM + rule engine.
/// Called on-demand from the billing tab "Extract Billing Codes" button.
/// Tries local archive first, then server (session may be from another machine).
/// Accepts optional `context` with physician-provided billing hints.
#[tauri::command]
pub async fn extract_billing_codes(
    session_id: String,
    date: String,
    context: Option<BillingContext>,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<BillingRecord, CommandError> {
    info!("Extracting billing codes for session: {}", session_id);
    let parsed_date = super::parse_date(&date)?;

    // Build context hints string (empty if no context provided)
    let context_hints = context
        .as_ref()
        .map(|c| build_context_hints(c))
        .unwrap_or_default();
    if !context_hints.is_empty() {
        debug!("Billing context hints:\n{}", context_hints);
    }

    // Load session details — try local first, then server
    let details = match local_archive::get_session(&session_id, &date) {
        Ok(d) => d,
        Err(_) => {
            // Try server (session may exist on a different machine)
            let physician_id = active_physician.read().await.as_ref().map(|p| p.id.clone());
            let client = profile_client.read().await.clone();
            if let (Some(phys_id), Some(client)) = (physician_id, client) {
                match client.get_session(&phys_id, &session_id).await {
                    Ok(d) => d,
                    Err(e) => {
                        warn!("Server fetch also failed for billing extraction: {e}");
                        return Err(CommandError::NotFound(
                            format!("Session not found locally or on server: {}", session_id)
                        ));
                    }
                }
            } else {
                return Err(CommandError::NotFound(
                    format!("Session not found locally and no server configured: {}", session_id)
                ));
            }
        }
    };
    let soap = details.soap_note
        .ok_or_else(|| CommandError::NotFound("No SOAP note found for billing extraction".into()))?;
    let transcript = details.transcript
        .as_deref()
        .unwrap_or("");

    let config = Config::load_or_default();
    let client = LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &config.fast_model,
    ).map_err(|e| CommandError::Network(e))?;

    let duration_ms = details.metadata.duration_ms.unwrap_or(0);

    // After-hours: use override from context if provided, else auto-detect
    let after_hours = context
        .as_ref()
        .and_then(|c| c.after_hours_override)
        .unwrap_or_else(|| crate::encounter_pipeline::is_after_hours(&parsed_date));

    let logger = std::sync::Arc::new(std::sync::Mutex::new(
        crate::pipeline_log::PipelineLogger::new(),
    ));

    let record = crate::encounter_pipeline::extract_and_archive_billing(
        &client,
        &config.fast_model,
        &soap,
        transcript,
        &context_hints,
        &session_id,
        &parsed_date,
        duration_ms,
        details.metadata.patient_name.as_deref(),
        after_hours,
        &logger,
    ).await
    .map_err(CommandError::Other)?;

    Ok(record)
}

#[tauri::command]
pub fn get_daily_billing_summary(
    date: String,
) -> Result<BillingDaySummary, CommandError> {
    debug!("Loading daily billing summary for {}", date);
    let parsed_date = super::parse_date(&date)?;

    // list_sessions_by_date takes a &str date (YYYY-MM-DD)
    let sessions = local_archive::list_sessions_by_date(&date)
        .map_err(CommandError::Other)?;

    let mut records = Vec::new();
    for summary in &sessions {
        if let Ok(Some(record)) = local_archive::get_billing_record(&summary.session_id, &parsed_date) {
            records.push(record);
        }
    }

    let cap_status = calculate_daily_caps(&records);

    let mut total_shadow = 0u32;
    let mut total_oob = 0u32;
    let mut total_time = 0u32;
    let mut confirmed = 0u32;
    let mut draft = 0u32;
    let mut time_hours: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let mut total_time_hours = 0.0f32;

    for r in &records {
        total_shadow += r.total_shadow_cents;
        total_oob += r.total_out_of_basket_cents;
        total_time += r.total_time_based_cents;
        match r.status {
            crate::billing::BillingStatus::Confirmed => confirmed += 1,
            crate::billing::BillingStatus::Draft => draft += 1,
        }
        for te in &r.time_entries {
            let hours = te.minutes as f32 / 60.0;
            *time_hours.entry(te.code.clone()).or_insert(0.0) += hours;
            total_time_hours += hours;
        }
    }

    Ok(BillingDaySummary {
        date: date.clone(),
        encounter_count: records.len() as u32,
        total_shadow_cents: total_shadow,
        total_out_of_basket_cents: total_oob,
        total_time_based_cents: total_time,
        total_amount_cents: total_shadow + total_oob + total_time,
        time_hours_by_code: time_hours,
        total_time_hours,
        confirmed_count: confirmed,
        draft_count: draft,
        cap_status,
        encounters: records,
    })
}

#[tauri::command]
pub fn get_monthly_billing_summary(
    end_date: String,
) -> Result<BillingMonthSummary, CommandError> {
    debug!("Loading monthly billing summary ending {}", end_date);
    let parsed_end = super::parse_date(&end_date)?;

    let mut daily_summaries = Vec::new();

    // Walk backwards 28 days
    for day_offset in 0..28i64 {
        let day = parsed_end - Duration::days(day_offset);
        let date_str = format!("{:04}-{:02}-{:02}", day.year(), day.month(), day.day());

        // Try to get daily summary -- silently skip if no sessions
        match get_daily_billing_summary(date_str) {
            Ok(summary) => daily_summaries.push(summary),
            Err(_) => {} // No sessions that day, skip
        }
    }

    // Reverse so earliest date is first
    daily_summaries.reverse();

    let cap_status = calculate_monthly_caps(&daily_summaries);

    let mut total_shadow = 0u32;
    let mut total_oob = 0u32;
    let mut total_time = 0u32;
    let mut total_hours = 0.0f32;
    let mut hours_by_code: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

    for ds in &daily_summaries {
        total_shadow += ds.total_shadow_cents;
        total_oob += ds.total_out_of_basket_cents;
        total_time += ds.total_time_based_cents;
        total_hours += ds.total_time_hours;
        for (code, hours) in &ds.time_hours_by_code {
            *hours_by_code.entry(code.clone()).or_insert(0.0) += hours;
        }
    }

    let indirect_admin_ratio = if total_hours > 0.0 {
        let q312 = hours_by_code.get("Q312A").copied().unwrap_or(0.0);
        let q313 = hours_by_code.get("Q313A").copied().unwrap_or(0.0);
        (q312 + q313) / total_hours
    } else {
        0.0
    };

    let admin_ratio = if total_hours > 0.0 {
        hours_by_code.get("Q313A").copied().unwrap_or(0.0) / total_hours
    } else {
        0.0
    };

    let start_date = parsed_end - Duration::days(27);

    Ok(BillingMonthSummary {
        period_start: format!("{:04}-{:02}-{:02}", start_date.year(), start_date.month(), start_date.day()),
        period_end: end_date,
        daily_summaries,
        total_shadow_cents: total_shadow,
        total_out_of_basket_cents: total_oob,
        total_time_based_cents: total_time,
        total_amount_cents: total_shadow + total_oob + total_time,
        total_hours,
        hours_by_code,
        indirect_admin_ratio,
        admin_ratio,
        cap_status,
    })
}

#[tauri::command]
pub fn export_billing_csv(
    start_date: String,
    end_date: String,
) -> Result<String, CommandError> {
    info!("Exporting billing CSV from {} to {}", start_date, end_date);
    let start = super::parse_date(&start_date)?;
    let end = super::parse_date(&end_date)?;

    let mut csv = String::from("date,session_id,patient_name,encounter_number,code,description,fee,shadow_rate,billable_amount,category,time_minutes,status,confirmed_at\n");

    let mut current = start;
    while current <= end {
        let date_str = format!("{:04}-{:02}-{:02}", current.year(), current.month(), current.day());

        if let Ok(sessions) = local_archive::list_sessions_by_date(&date_str) {
            for session in &sessions {
                if let Ok(Some(record)) = local_archive::get_billing_record(&session.session_id, &current) {
                    let patient = record.patient_name.as_deref().unwrap_or("");
                    let status = match record.status {
                        crate::billing::BillingStatus::Draft => "draft",
                        crate::billing::BillingStatus::Confirmed => "confirmed",
                    };
                    let confirmed = record.confirmed_at.as_deref().unwrap_or("");
                    let enc_num = session.encounter_number.map(|n| n.to_string()).unwrap_or_default();

                    // Billing codes
                    for code in &record.codes {
                        csv.push_str(&format!(
                            "{},{},{},{},{},{},{:.2},{},{:.2},{},,,{}\n",
                            date_str,
                            session.session_id,
                            escape_csv(patient),
                            enc_num,
                            code.code,
                            escape_csv(&code.description),
                            code.fee_cents as f64 / 100.0,
                            code.shadow_pct,
                            code.billable_amount_cents as f64 / 100.0,
                            code.category,
                            status,
                        ));
                    }

                    // Time entries
                    for te in &record.time_entries {
                        csv.push_str(&format!(
                            "{},{},{},{},{},{},{:.2},,{:.2},time_based,{},{},{}\n",
                            date_str,
                            session.session_id,
                            escape_csv(patient),
                            enc_num,
                            te.code,
                            escape_csv(&te.description),
                            te.rate_per_15min_cents as f64 / 100.0 * 4.0, // hourly rate
                            te.billable_amount_cents as f64 / 100.0,
                            te.minutes,
                            status,
                            confirmed,
                        ));
                    }
                }
            }
        }

        current = current + Duration::days(1);
    }

    Ok(csv)
}

/// Search result for OHIP code lookup
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OhipCodeSearchResult {
    pub code: String,
    pub description: String,
    pub fee_cents: u32,
    pub category: String,
    pub shadow_pct: u8,
    pub basket: String,
}

/// Search OHIP codes by code prefix or description substring.
#[tauri::command]
pub fn search_ohip_codes(query: String) -> Vec<OhipCodeSearchResult> {
    debug!("Searching OHIP codes: {}", query);
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for ohip in crate::billing::ohip_codes::all_codes() {
        let code_matches = ohip.code.to_lowercase().contains(&query_lower);
        let desc_matches = ohip.description.to_lowercase().contains(&query_lower);

        if code_matches || desc_matches {
            results.push(OhipCodeSearchResult {
                code: ohip.code.to_string(),
                description: ohip.description.to_string(),
                fee_cents: ohip.ffs_rate_cents,
                category: match ohip.category {
                    crate::billing::ohip_codes::CodeCategory::Assessment => "Assessment",
                    crate::billing::ohip_codes::CodeCategory::Counselling => "Counselling",
                    crate::billing::ohip_codes::CodeCategory::Procedure => "Procedure",
                    crate::billing::ohip_codes::CodeCategory::ChronicDisease => "Chronic Disease",
                    crate::billing::ohip_codes::CodeCategory::Screening => "Screening",
                    crate::billing::ohip_codes::CodeCategory::Premium => "Premium",
                    crate::billing::ohip_codes::CodeCategory::TimeBased => "Time-Based",
                    crate::billing::ohip_codes::CodeCategory::Immunization => "Immunization",
                }.to_string(),
                shadow_pct: ohip.shadow_pct,
                basket: match ohip.basket {
                    crate::billing::ohip_codes::Basket::In => "in_basket",
                    crate::billing::ohip_codes::Basket::Out => "out_of_basket",
                }.to_string(),
            });
        }

        if results.len() >= 15 {
            break;
        }
    }

    results
}

/// Escape a string for CSV output (quote if contains comma, newline, or quote)
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('\n') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_context_hints_defaults() {
        let ctx = BillingContext::default();
        let hints = build_context_hints(&ctx);
        assert!(hints.is_empty(), "Default context should produce no hints");
    }

    #[test]
    fn test_build_context_hints_video() {
        let ctx = BillingContext {
            visit_setting: "video".to_string(),
            ..Default::default()
        };
        let hints = build_context_hints(&ctx);
        assert!(hints.contains("VIDEO telemedicine"));
        assert!(hints.contains("A101"));
    }

    #[test]
    fn test_build_context_hints_phone_home() {
        let ctx = BillingContext {
            visit_setting: "phone_home".to_string(),
            ..Default::default()
        };
        let hints = build_context_hints(&ctx);
        assert!(hints.contains("TELEPHONE"));
        assert!(hints.contains("A102"));
    }

    #[test]
    fn test_build_context_hints_home_visit() {
        let ctx = BillingContext {
            visit_setting: "home_visit".to_string(),
            ..Default::default()
        };
        let hints = build_context_hints(&ctx);
        assert!(hints.contains("HOME VISIT"));
        assert!(hints.contains("A900"));
    }

    #[test]
    fn test_build_context_hints_senior() {
        let ctx = BillingContext {
            patient_age: "senior".to_string(),
            ..Default::default()
        };
        let hints = build_context_hints(&ctx);
        assert!(hints.contains("65 years or older"));
        assert!(hints.contains("K132"));
    }

    #[test]
    fn test_build_context_hints_child() {
        let ctx = BillingContext {
            patient_age: "child_2_15".to_string(),
            ..Default::default()
        };
        let hints = build_context_hints(&ctx);
        assert!(hints.contains("child (2-15"));
        assert!(hints.contains("K017"));
    }

    #[test]
    fn test_build_context_hints_idd() {
        let ctx = BillingContext {
            patient_age: "idd".to_string(),
            ..Default::default()
        };
        let hints = build_context_hints(&ctx);
        assert!(hints.contains("intellectual/developmental disability"));
        assert!(hints.contains("K133"));
    }

    #[test]
    fn test_build_context_hints_referral() {
        let ctx = BillingContext {
            referral_received: true,
            ..Default::default()
        };
        let hints = build_context_hints(&ctx);
        assert!(hints.contains("REFERRAL"));
        assert!(hints.contains("A005"));
    }

    #[test]
    fn test_build_context_hints_counselling_exhausted() {
        let ctx = BillingContext {
            counselling_exhausted: true,
            ..Default::default()
        };
        let hints = build_context_hints(&ctx);
        assert!(hints.contains("EXHAUSTED"));
        assert!(hints.contains("K033"));
    }

    #[test]
    fn test_build_context_hints_after_hours_override() {
        let ctx_yes = BillingContext {
            after_hours_override: Some(true),
            ..Default::default()
        };
        assert!(build_context_hints(&ctx_yes).contains("AFTER HOURS"));

        let ctx_no = BillingContext {
            after_hours_override: Some(false),
            ..Default::default()
        };
        assert!(build_context_hints(&ctx_no).contains("regular hours"));
    }

    #[test]
    fn test_build_context_hints_multiple() {
        let ctx = BillingContext {
            visit_setting: "video".to_string(),
            patient_age: "senior".to_string(),
            referral_received: true,
            counselling_exhausted: false,
            after_hours_override: None,
        };
        let hints = build_context_hints(&ctx);
        assert!(hints.contains("VIDEO"));
        assert!(hints.contains("65 years"));
        assert!(hints.contains("REFERRAL"));
        // Three hints, joined by newlines
        assert_eq!(hints.lines().count(), 3);
    }
}
