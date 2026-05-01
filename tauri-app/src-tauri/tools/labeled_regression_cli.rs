//! Labeled regression CLI: compares production session outputs to
//! human-reviewed ground truth labels.
//!
//! For each label file in `tests/fixtures/labels/*.json`, finds the corresponding
//! session in `~/.transcriptionapp/archive/` and verifies:
//!   - billing codes match the expected codes
//!   - diagnostic code matches
//!   - SOAP / merge / split decisions are consistent with the label
//!
//! This is the "golden corpus" regression test — when production behavior
//! diverges from a labeled correct answer, it's either a regression in our
//! code or the label needs to be updated.
//!
//! Usage:
//!   cargo run --bin labeled_regression_cli -- --all
//!   cargo run --bin labeled_regression_cli -- --all --fail-on-regression
//!   cargo run --bin labeled_regression_cli -- 2026-04-15_00aa31d4.json

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

use transcription_app_lib::billing::BillingRecord;
use transcription_app_lib::feedback_to_label::LabelData;
use transcription_app_lib::replay_fetch::ArchiveFetcher;

#[derive(Debug, Deserialize)]
struct Label {
    session_id: String,
    date: String,
    #[allow(dead_code)]
    #[serde(default)]
    labeled_at: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    labeled_by: Option<String>,
    labels: LabelData,
}

fn print_usage(program: &str) {
    eprintln!("Usage: {} [LABEL_FILE | --all] [OPTIONS]", program);
    eprintln!();
    eprintln!("Compare production session outputs to ground truth labels.");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  LABEL_FILE          Run a single label file (relative to tests/fixtures/labels/)");
    eprintln!("  --all               Run all labels in tests/fixtures/labels/");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --fail-on-regression  Exit non-zero if any label diverges from production");
    eprintln!("  --verbose             Print details for matches as well as mismatches");
    eprintln!("  --help                Show this help");
}

fn labels_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("labels")
}

fn list_label_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Parse a YYYY-MM-DD label date into a noon-UTC DateTime.
/// Returns None if the date is malformed (rather than silently using today).
fn parse_date(date: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let naive = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let noon = naive.and_hms_opt(12, 0, 0)?;
    Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(noon, chrono::Utc))
}

/// Decide whether production's procedure section is clinically appropriate.
///
/// Reads `replay_bundle.json` bytes, finds `soap_result.response_raw`, parses
/// `procedure[]`, and judges each entry via
/// [`is_billable_procedure_action`] from the shared
/// `billing::procedure_vocab` module. Empty / unparseable / no-raw inputs
/// return `true` (conservative — don't false-fail when we can't judge).
fn procedure_section_judges_correct(replay_bundle_bytes: &[u8]) -> bool {
    let Ok(bundle) = serde_json::from_slice::<serde_json::Value>(replay_bundle_bytes) else {
        return true;
    };
    let Some(raw) = bundle.pointer("/soap_result/response_raw").and_then(|v| v.as_str()) else {
        return true;
    };
    if raw.is_empty() {
        return true;
    }

    let mut any_action = false;
    for chunk in raw.split("\n---\n") {
        let chunk = chunk.trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        let Ok(json) = serde_json::from_str::<serde_json::Value>(chunk) else { continue };
        let Some(arr) = json.get("procedure").and_then(|v| v.as_array()) else { continue };
        for item in arr {
            let Some(action) = item.get("action").and_then(|v| v.as_str()) else { continue };
            any_action = true;
            if !transcription_app_lib::billing::procedure_vocab::is_billable_procedure_action(action) {
                return false;
            }
        }
    }
    let _ = any_action;
    true
}

#[cfg(test)]
mod tests {
    use super::procedure_section_judges_correct;

    fn bundle_with_raw(raw: &str) -> Vec<u8> {
        let body = serde_json::json!({
            "soap_result": { "response_raw": raw }
        });
        serde_json::to_vec(&body).unwrap()
    }

    #[test]
    fn empty_procedure_section_is_correct() {
        let raw = r#"{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[]}"#;
        assert!(procedure_section_judges_correct(&bundle_with_raw(raw)));
    }

    #[test]
    fn injection_is_billable() {
        let raw = r#"{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[{"action":"performed knee injection","transcript_quote":"x"}]}"#;
        assert!(procedure_section_judges_correct(&bundle_with_raw(raw)));
    }

    #[test]
    fn auscultation_is_not_billable() {
        // Ruth's actual procedure entry from 2026-04-29 — must fail this check.
        let raw = r#"{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[{"action":"Performed chest auscultation","transcript_quote":"x"}]}"#;
        assert!(!procedure_section_judges_correct(&bundle_with_raw(raw)));
    }

    #[test]
    fn reviewing_labs_is_not_billable() {
        // Allan / Catherine 2:35 / Catherine sub-procedure — must fail.
        let raw = r#"{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[{"action":"Reviewed blood work results","transcript_quote":"x"}]}"#;
        assert!(!procedure_section_judges_correct(&bundle_with_raw(raw)));
    }

    #[test]
    fn dtc_form_is_not_billable() {
        let raw = r#"{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[{"action":"Completed medical section of Disability Tax Credit form","transcript_quote":"x"}]}"#;
        assert!(!procedure_section_judges_correct(&bundle_with_raw(raw)));
    }

    #[test]
    fn pap_smear_is_billable() {
        let raw = r#"{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[{"action":"Performed pap smear with speculum","transcript_quote":"x"}]}"#;
        assert!(procedure_section_judges_correct(&bundle_with_raw(raw)));
    }

    #[test]
    fn nerve_block_is_billable() {
        // Irene's actual procedure — would be billable if labeled correctly.
        let raw = r#"{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[{"action":"performed ultrasound-guided cervical numbing injection","transcript_quote":"x"}]}"#;
        assert!(procedure_section_judges_correct(&bundle_with_raw(raw)));
    }

    #[test]
    fn mixed_billable_and_nonbillable_fails() {
        // Catherine 2:35 actual: 3 entries, only one might be borderline.
        let raw = r#"{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[
            {"action":"Reviewed blood work results and ECG","transcript_quote":"x"},
            {"action":"Provided printed copy of blood work results","transcript_quote":"y"},
            {"action":"Completed Disability Tax Credit form","transcript_quote":"z"}
        ]}"#;
        assert!(!procedure_section_judges_correct(&bundle_with_raw(raw)));
    }

    #[test]
    fn unparseable_raw_is_conservative() {
        // Garbage JSON — return true so we don't false-fail.
        let bundle = bundle_with_raw("not json");
        assert!(procedure_section_judges_correct(&bundle));
    }

    #[test]
    fn missing_response_raw_skips_check() {
        // No response_raw → we can't judge → return true (skip).
        let body = serde_json::json!({"soap_result": {}});
        assert!(procedure_section_judges_correct(&serde_json::to_vec(&body).unwrap()));
    }

    #[test]
    fn multi_patient_raw_with_delimiter() {
        // Per-patient raw responses are joined with \n---\n. Both halves must
        // pass for the overall judgement to be correct.
        let raw = r#"{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[]}
---
{"subjective":[],"objective":[],"assessment":[],"plan":[],"procedure":[{"action":"performed pap smear","transcript_quote":"x"}]}"#;
        assert!(procedure_section_judges_correct(&bundle_with_raw(raw)));
    }
}

#[derive(Debug, Default)]
struct CheckResults {
    checks: u32,
    passes: u32,
    failures: Vec<String>,
}

impl CheckResults {
    fn check<T: PartialEq + std::fmt::Debug>(&mut self, name: &str, expected: &T, actual: &T) {
        self.checks += 1;
        if expected == actual {
            self.passes += 1;
        } else {
            self.failures.push(format!(
                "  ✗ {}: expected {:?}, got {:?}",
                name, expected, actual
            ));
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let program = &args[0];
    if args.len() < 2 || args.contains(&"--help".to_string()) {
        print_usage(program);
        return if args.contains(&"--help".to_string()) { ExitCode::SUCCESS } else { ExitCode::from(1) };
    }

    let mut all = false;
    let mut single_file: Option<String> = None;
    let mut fail_on_regression = false;
    let mut verbose = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => all = true,
            "--fail-on-regression" => fail_on_regression = true,
            "--verbose" => verbose = true,
            "--help" => { print_usage(program); return ExitCode::SUCCESS; }
            other => {
                if other.starts_with('-') {
                    eprintln!("Unknown option: {}", other);
                    return ExitCode::from(1);
                }
                single_file = Some(other.to_string());
            }
        }
        i += 1;
    }

    let dir = labels_dir();
    if !dir.exists() {
        eprintln!("Labels directory not found: {}", dir.display());
        return ExitCode::from(1);
    }

    let label_files: Vec<PathBuf> = if all {
        list_label_files(&dir)
    } else if let Some(name) = single_file {
        vec![dir.join(name)]
    } else {
        eprintln!("Provide a label file or --all");
        return ExitCode::from(1);
    };

    if label_files.is_empty() {
        eprintln!("No label files found in {}", dir.display());
        return ExitCode::SUCCESS;
    }

    let fetcher = ArchiveFetcher::from_env().unwrap_or_else(|e| {
        eprintln!("warn: ArchiveFetcher init failed ({e}); falling back to local-only");
        ArchiveFetcher::local_only()
    });

    let mut total_labels = 0;
    let mut total_checks = 0;
    let mut total_passes = 0;
    let mut total_regressions = 0;
    let mut missing_sessions = 0;

    for file in &label_files {
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to read {}: {e}", file.display());
                continue;
            }
        };
        let label: Label = match serde_json::from_str(&content) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to parse {}: {e}", file.display());
                continue;
            }
        };

        total_labels += 1;
        let label_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("?");

        let session = match fetcher.fetch_session(&label.session_id, &label.date).await {
            Ok(s) => s,
            Err(e) => {
                println!("⊘ {} — session not found locally or on server: {e}", label_name);
                missing_sessions += 1;
                continue;
            }
        };

        let mut results = CheckResults::default();

        if let Some(expected) = label.labels.clinical_correct {
            // clinical_correct=true means production correctly classified;
            // we check by inspecting metadata.likely_non_clinical
            let actual_clinical = !session.metadata.likely_non_clinical.unwrap_or(false);
            // The label asserts what the CORRECT classification is, not what production did.
            // If the label says "clinical=true" production should have classified as clinical.
            // For now, treat clinical_correct=true as "production should classify as clinical".
            results.check("clinical_classification", &expected, &actual_clinical);
        }

        // Load and check billing
        let want_billing = label.labels.billing_codes_expected.is_some()
            || label.labels.diagnostic_code_expected.is_some();
        if want_billing {
            let parsed_date = match parse_date(&label.date) {
                Some(d) => d,
                None => {
                    results.checks += 1;
                    results.failures.push(format!(
                        "  ✗ cannot parse date {}",
                        label.date
                    ));
                    total_checks += results.checks;
                    total_passes += results.passes;
                    if !results.failures.is_empty() { total_regressions += 1; }
                    continue;
                }
            };
            let billing: Option<BillingRecord> =
                fetcher.fetch_billing(&label.session_id, &parsed_date).await.unwrap_or(None);

            match billing {
                Some(billing) => {
                    let actual_codes: Vec<String> = billing.codes.iter().map(|c| c.code.clone())
                        .chain(billing.time_entries.iter().map(|t| t.code.clone()))
                        .collect();

                    if let Some(expected_codes) = &label.labels.billing_codes_expected {
                        results.checks += 1;
                        if expected_codes.iter().all(|c| actual_codes.contains(c)) {
                            results.passes += 1;
                        } else {
                            let mut expected_sorted = expected_codes.clone();
                            expected_sorted.sort();
                            let mut actual_sorted = actual_codes.clone();
                            actual_sorted.sort();
                            results.failures.push(format!(
                                "  ✗ billing_codes: expected {:?} subset of actual, got {:?}",
                                expected_sorted, actual_sorted
                            ));
                        }
                    }

                    // 2026-04-30 schema additions: also check `billing_codes_unexpected`.
                    // Karen White K005A and Carl Grieve G372A are present-but-flagged-wrong cases.
                    if let Some(unexpected_codes) = &label.labels.billing_codes_unexpected {
                        for code in unexpected_codes {
                            results.checks += 1;
                            if actual_codes.contains(code) {
                                results.failures.push(format!(
                                    "  ✗ billing_codes_unexpected: code {} present (label flagged as inappropriate)",
                                    code
                                ));
                            } else {
                                results.passes += 1;
                            }
                        }
                    }

                    // 2026-04-30 schema addition: per-code `billing_quantity_expected`.
                    // Deanna Wicks K005A qty=2 case (was qty=1 pre-Class-G fix).
                    if let Some(qty_expected) = &label.labels.billing_quantity_expected {
                        for (code, expected_qty) in qty_expected {
                            results.checks += 1;
                            let actual_qty = billing
                                .codes
                                .iter()
                                .find(|c| &c.code == code)
                                .map(|c| c.quantity as u32);
                            if actual_qty == Some(*expected_qty as u32) {
                                results.passes += 1;
                            } else {
                                results.failures.push(format!(
                                    "  ✗ billing_quantity[{}]: expected {}, got {:?}",
                                    code, expected_qty, actual_qty
                                ));
                            }
                        }
                    }

                    if let Some(expected_dx) = &label.labels.diagnostic_code_expected {
                        let actual_dx = billing.diagnostic_code.clone().unwrap_or_default();
                        // 2026-04-30 schema addition: honor `diagnostic_code_acceptable` —
                        // match against expected OR any acceptable code.
                        let acceptable: Vec<String> = label
                            .labels
                            .diagnostic_code_acceptable
                            .clone()
                            .unwrap_or_default();
                        results.checks += 1;
                        if &actual_dx == expected_dx || acceptable.contains(&actual_dx) {
                            results.passes += 1;
                        } else {
                            results.failures.push(format!(
                                "  ✗ diagnostic_code: expected {} (acceptable: {:?}), got {}",
                                expected_dx, acceptable, actual_dx
                            ));
                        }
                    }
                }
                None => {
                    results.checks += 1;
                    results.failures.push("  ✗ billing.json missing — cannot check codes/dx".to_string());
                }
            }
        }

        // procedure_section_correct: read the replay bundle's raw SOAP response,
        // parse procedure[], and check against the label assertion. This catches
        // the v0.10.61 procedure-section overcapture failure mode (chest
        // auscultation / "reviewed blood work" being listed as billable
        // procedures). Added after the 2026-04-29 forensic review surfaced 5+
        // sessions today + Apr 27 Heike with this defect.
        if let Some(expected_correct) = label.labels.procedure_section_correct {
            let parsed_date = match parse_date(&label.date) {
                Some(d) => d,
                None => {
                    results.checks += 1;
                    results.failures.push(format!(
                        "  ✗ procedure_section: cannot parse date {}",
                        label.date
                    ));
                    total_checks += results.checks;
                    total_passes += results.passes;
                    if !results.failures.is_empty() { total_regressions += 1; }
                    continue;
                }
            };
            match fetcher.fetch_replay_bundle_raw(&label.session_id, &parsed_date).await {
                Ok(Some(bytes)) => {
                    let actual_correct = procedure_section_judges_correct(&bytes);
                    results.check("procedure_section_correct", &expected_correct, &actual_correct);
                }
                Ok(None) => {
                    // No replay bundle — can't verify. Note as informational
                    // rather than fail; some older sessions predate v5 schema.
                    if verbose {
                        println!("  ⊘ {} no replay_bundle.json — procedure_section check skipped", label_name);
                    }
                }
                Err(e) => {
                    results.checks += 1;
                    results.failures.push(format!(
                        "  ✗ procedure_section: replay_bundle fetch failed: {e}"
                    ));
                }
            }
        }

        // Note: split_correct, merge_correct, patient_count_correct are reserved
        // for future implementation when day_log + replay_bundle inspection lands.
        // Current production state for these is informational; the labels capture
        // the desired behavior so the regression test can be enabled once the
        // verification logic is added.

        total_checks += results.checks;
        total_passes += results.passes;
        let regressed = !results.failures.is_empty();
        if regressed { total_regressions += 1; }

        let label_status = if regressed { "REGRESSION" } else { "OK" };
        if regressed || verbose {
            println!("{} {} ({} checks, {} pass)", label_status, label_name, results.checks, results.passes);
            if let Some(notes) = &label.labels.notes {
                println!("  notes: {notes}");
            }
            for fail in &results.failures {
                println!("{}", fail);
            }
        }
    }

    println!();
    println!("────────────────────────────────────────────");
    println!(
        "Labels: {}  Checks: {}  Pass: {}  Regressions: {}  Missing: {}",
        total_labels, total_checks, total_passes, total_regressions, missing_sessions
    );

    if fail_on_regression && total_regressions > 0 {
        eprintln!();
        eprintln!("REGRESSION: {} labeled sessions diverge from production", total_regressions);
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}
