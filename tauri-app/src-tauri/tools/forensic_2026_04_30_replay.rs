//! Forensic 2026-04-30 corpus replay validator.
//!
//! For each label fixture, fetches the archived `billing_result.response_raw`
//! from `replay_bundle.json`, re-parses it to `ClinicalFeatures`, runs the
//! current (patched) rule engine, and compares the resulting billing codes
//! + diagnostic code to the label expectations.
//!
//! Outputs a per-session match/mismatch table + summary. Run twice (once
//! against unpatched HEAD, once against the patched branch) to get a true
//! before/after delta. Or run once on the patched branch and compare counts
//! to `labeled_regression_cli --all` (which scores production billing.json
//! against the same labels — that's the unpatched baseline by definition).
//!
//! Usage:
//!   cargo run --release --bin forensic_2026_04_30_replay -- --all
//!   cargo run --release --bin forensic_2026_04_30_replay -- --date 2026-04-30
//!   cargo run --release --bin forensic_2026_04_30_replay -- --date 2026-04-29 --verbose

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

use transcription_app_lib::billing::clinical_features::{
    augment_procedures_from_soap_text, parse_billing_extraction, ClinicalFeatures,
};
use transcription_app_lib::billing::rule_engine::{
    condition_keyword_guard, map_features_to_billing_with_tools_model, RuleEngineContext,
};
use transcription_app_lib::billing::types::ResolvedDiagnostic;
use transcription_app_lib::feedback_to_label::LabelData;
use transcription_app_lib::replay_fetch::ArchiveFetcher;

#[derive(Debug, Deserialize)]
struct Label {
    session_id: String,
    date: String,
    labels: LabelData,
}

fn labels_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("labels")
}

fn parse_date(date: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let naive = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let noon = naive.and_hms_opt(12, 0, 0)?;
    Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
        noon,
        chrono::Utc,
    ))
}

#[derive(Default)]
struct Stats {
    sessions: u32,
    sessions_with_billing_data: u32,
    billing_checks: u32,
    billing_pass: u32,
    dx_checks: u32,
    dx_pass: u32,
    misses: Vec<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let mut date_filter: Option<String> = None;
    let mut all = false;
    let mut verbose = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => all = true,
            "--verbose" | "-v" => verbose = true,
            "--date" => {
                i += 1;
                date_filter = args.get(i).cloned();
            }
            "--help" | "-h" => {
                eprintln!("Usage: {} [--all | --date YYYY-MM-DD] [--verbose]", args[0]);
                return ExitCode::SUCCESS;
            }
            _ => {}
        }
        i += 1;
    }

    let dir = labels_dir();
    if !dir.exists() {
        eprintln!("Labels directory not found: {}", dir.display());
        return ExitCode::from(1);
    }

    let mut label_files = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Some(filter) = &date_filter {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !name.starts_with(filter) {
                        continue;
                    }
                } else if !all {
                    continue;
                }
                label_files.push(p);
            }
        }
    }
    label_files.sort();

    if label_files.is_empty() {
        eprintln!("No label files matched (use --all or --date YYYY-MM-DD)");
        return ExitCode::from(1);
    }

    let fetcher = ArchiveFetcher::from_env().unwrap_or_else(|_| ArchiveFetcher::local_only());

    let mut stats = Stats::default();
    let mut would_fix: Vec<String> = Vec::new();

    for file in &label_files {
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let label: Label = match serde_json::from_str(&content) {
            Ok(l) => l,
            Err(_) => continue,
        };
        let label_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        stats.sessions += 1;

        // Fetch the archived replay bundle to get the LLM's billing extraction
        let parsed_date = match parse_date(&label.date) {
            Some(d) => d,
            None => continue,
        };
        let bundle_bytes = match fetcher
            .fetch_replay_bundle_raw(&label.session_id, &parsed_date)
            .await
        {
            Ok(Some(b)) => b,
            _ => continue,
        };
        let bundle: serde_json::Value = match serde_json::from_slice(&bundle_bytes) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Extract LLM clinical features from billing_result.response_raw
        let raw = bundle
            .pointer("/billing_result/response_raw")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if raw.is_empty() {
            continue;
        }
        stats.sessions_with_billing_data += 1;

        let mut features: ClinicalFeatures = match parse_billing_extraction(raw) {
            Ok(f) => f,
            Err(_) => continue,
        };

        // Apply condition_keyword_guard (matches production)
        let soap_text = bundle
            .pointer("/soap_result/response_raw")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let (kept, _) = condition_keyword_guard(&features.conditions, soap_text);
        features.conditions = kept;

        // 2026-04-30 Class E: production runs augment_procedures_from_soap_text
        // in extract_and_archive_billing on the rendered SOAP procedure section.
        // Mirror that here so Class E patches are exercised in replay.
        // Extract Procedure section from the raw SOAP JSON's procedure[] field.
        let soap_proc_text = extract_soap_procedure_section_text(soap_text);
        let _augmented = augment_procedures_from_soap_text(&mut features, &soap_proc_text);

        // Synthesize a ResolvedDiagnostic from the archived diagnostic_tools_model
        // step output if present (this is what production passes to Stage 0).
        let tools_resolved: Option<ResolvedDiagnostic> = extract_tools_model(&bundle);

        // Build a context (transcript-aware procedure validation)
        let transcript = String::new(); // we don't replay transcripts here — keep validate-allow
        let mut ctx = RuleEngineContext::default();
        if !transcript.is_empty() {
            ctx.transcript = Some(transcript);
        }

        // Synthetic 25-min duration; only matters for K005-style codes
        let duration_ms = 25 * 60 * 1000;

        let record = map_features_to_billing_with_tools_model(
            &features,
            &label.session_id,
            &label.date,
            duration_ms,
            None,
            &ctx,
            None,
            tools_resolved.as_ref(),
        );

        let actual_codes: Vec<String> = record
            .codes
            .iter()
            .map(|c| c.code.clone())
            .chain(record.time_entries.iter().map(|t| t.code.clone()))
            .collect();
        let actual_dx = record.diagnostic_code.clone().unwrap_or_default();

        // Compare to label
        let mut session_billing_pass = true;
        let mut session_dx_pass = true;
        let mut local_misses: Vec<String> = Vec::new();

        if let Some(expected_codes) = &label.labels.billing_codes_expected {
            stats.billing_checks += 1;
            if expected_codes.iter().all(|c| actual_codes.contains(c)) {
                stats.billing_pass += 1;
            } else {
                session_billing_pass = false;
                let mut exp = expected_codes.clone();
                exp.sort();
                let mut act = actual_codes.clone();
                act.sort();
                local_misses.push(format!("billing: expected {:?}; got {:?}", exp, act));
            }
        }

        if let Some(unexpected_codes) = &label.labels.billing_codes_unexpected {
            for code in unexpected_codes {
                if actual_codes.contains(code) {
                    session_billing_pass = false;
                    local_misses.push(format!("billing: code {} present but flagged unexpected", code));
                }
            }
        }

        if let Some(expected_dx) = &label.labels.diagnostic_code_expected {
            stats.dx_checks += 1;
            // Honor `diagnostic_code_acceptable` — match against expected OR any acceptable
            let acceptable: Vec<String> = label
                .labels
                .diagnostic_code_acceptable
                .clone()
                .unwrap_or_default();
            let is_pass = &actual_dx == expected_dx || acceptable.contains(&actual_dx);
            if is_pass {
                stats.dx_pass += 1;
            } else {
                session_dx_pass = false;
                local_misses.push(format!(
                    "dx: expected {} (acceptable: {:?}); got {}",
                    expected_dx, acceptable, actual_dx
                ));
            }
        }

        let prod_billing = fetcher
            .fetch_billing(&label.session_id, &parsed_date)
            .await
            .unwrap_or(None);

        if !local_misses.is_empty() {
            for miss in &local_misses {
                stats.misses.push(format!("{}: {}", label_name, miss));
            }
            if verbose {
                println!("✗ {} — {} miss(es)", label_name, local_misses.len());
                for m in &local_misses {
                    println!("    {}", m);
                }
            }
        } else if verbose {
            println!("✓ {}", label_name);
        }

        // Compute "would-fix" — production output that didn't pass label, but our patched
        // engine output now matches.
        if let Some(prod) = prod_billing {
            let prod_codes: Vec<String> = prod.codes.iter().map(|c| c.code.clone())
                .chain(prod.time_entries.iter().map(|t| t.code.clone())).collect();
            let prod_dx = prod.diagnostic_code.clone().unwrap_or_default();
            let prod_billing_pass = label
                .labels
                .billing_codes_expected
                .as_ref()
                .map_or(true, |exp| exp.iter().all(|c| prod_codes.contains(c)));
            let prod_dx_pass = label
                .labels
                .diagnostic_code_expected
                .as_ref()
                .map_or(true, |exp| &prod_dx == exp);
            let new_billing_pass = label
                .labels
                .billing_codes_expected
                .as_ref()
                .map_or(true, |exp| exp.iter().all(|c| actual_codes.contains(c)));
            let new_dx_pass = label
                .labels
                .diagnostic_code_expected
                .as_ref()
                .map_or(true, |exp| &actual_dx == exp);

            if (!prod_billing_pass && new_billing_pass) || (!prod_dx_pass && new_dx_pass) {
                let mut summary = format!("WOULD FIX {}", label_name);
                if !prod_billing_pass && new_billing_pass {
                    summary.push_str(&format!(" — billing prod={:?}→ours={:?}", prod_codes, actual_codes));
                }
                if !prod_dx_pass && new_dx_pass {
                    summary.push_str(&format!(" — dx prod={}→ours={}", prod_dx, actual_dx));
                }
                would_fix.push(summary);
            }
            if (prod_billing_pass && !new_billing_pass) || (prod_dx_pass && !new_dx_pass) {
                let mut summary = format!("WOULD REGRESS {}", label_name);
                if prod_billing_pass && !new_billing_pass {
                    summary.push_str(&format!(" — billing prod={:?}→ours={:?}", prod_codes, actual_codes));
                }
                if prod_dx_pass && !new_dx_pass {
                    summary.push_str(&format!(" — dx prod={}→ours={}", prod_dx, actual_dx));
                }
                would_fix.push(summary);
            }
        }

        let _ = (session_billing_pass, session_dx_pass);
    }

    println!();
    println!("─────────────────────────────────────────────");
    println!("Forensic 2026-04-30 replay (patched rule engine)");
    println!("─────────────────────────────────────────────");
    println!("Sessions: {}", stats.sessions);
    println!("Sessions with replay-bundle billing data: {}", stats.sessions_with_billing_data);
    println!(
        "Billing-code checks: {}/{} pass",
        stats.billing_pass, stats.billing_checks
    );
    println!(
        "Diagnostic-code checks: {}/{} pass",
        stats.dx_pass, stats.dx_checks
    );
    println!();
    println!("WOULD-FIX vs production (true positive of patches):");
    let fix_count = would_fix.iter().filter(|s| s.starts_with("WOULD FIX")).count();
    let regress_count = would_fix.iter().filter(|s| s.starts_with("WOULD REGRESS")).count();
    for line in &would_fix {
        println!("  {}", line);
    }
    println!();
    println!("SUMMARY: would_fix={} would_regress={}", fix_count, regress_count);
    ExitCode::SUCCESS
}

/// Extract the rendered SOAP's procedure section from the LLM JSON.
/// The SOAP `response_raw` is JSON with a `procedure[]` array of `{action, transcript_quote}`
/// objects. We concatenate the action strings as the augment input.
fn extract_soap_procedure_section_text(soap_raw: &str) -> String {
    // Strip markdown fences
    let stripped = soap_raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(stripped) else {
        return String::new();
    };
    let Some(arr) = json.get("procedure").and_then(|v| v.as_array()) else {
        return String::new();
    };
    let mut buf = String::new();
    for item in arr {
        if let Some(action) = item.get("action").and_then(|v| v.as_str()) {
            buf.push_str("• ");
            buf.push_str(action);
            buf.push('\n');
        }
    }
    buf
}

/// Extract the tools-model resolved diagnostic from the replay bundle, if
/// present. Production passes `Some(&ResolvedDiagnostic)` to the rule engine
/// when the diagnostic_tools_model step succeeded; we mirror that.
fn extract_tools_model(bundle: &serde_json::Value) -> Option<ResolvedDiagnostic> {
    // Schema v5 stores diagnostic_tools_model as a top-level field with a
    // ResolvedDiagnostic-like shape. Older bundles may lack it.
    let v = bundle.pointer("/diagnostic_tools_model")?;
    let code = v.get("code")?.as_str()?.to_string();
    let description = v.get("description").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let evidence = v.get("evidence").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let reasoning = v.get("reasoning").and_then(|x| x.as_str()).unwrap_or("").to_string();
    Some(ResolvedDiagnostic {
        code,
        description,
        evidence,
        reasoning,
    })
}
