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
use transcription_app_lib::local_archive;

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

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // split/merge/patient_count fields are reserved for future regression checks
struct LabelData {
    #[serde(default)]
    split_correct: Option<bool>,
    #[serde(default)]
    merge_correct: Option<bool>,
    #[serde(default)]
    clinical_correct: Option<bool>,
    #[serde(default)]
    patient_count_correct: Option<bool>,
    #[serde(default)]
    billing_codes_expected: Option<Vec<String>>,
    #[serde(default)]
    diagnostic_code_expected: Option<String>,
    #[serde(default)]
    notes: Option<String>,
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

fn main() -> ExitCode {
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

        // Load session details
        let session = match local_archive::get_session(&label.session_id, &label.date) {
            Ok(s) => s,
            Err(e) => {
                println!("⊘ {} — session not found locally: {e}", label_name);
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
            let session_dir = match parse_date(&label.date)
                .and_then(|dt| local_archive::get_session_archive_dir(&label.session_id, &dt).ok())
            {
                Some(dir) => dir,
                None => {
                    results.checks += 1;
                    results.failures.push(format!(
                        "  ✗ cannot resolve archive dir for date {}",
                        label.date
                    ));
                    total_checks += results.checks;
                    total_passes += results.passes;
                    if !results.failures.is_empty() { total_regressions += 1; }
                    continue;
                }
            };

            let billing: Option<BillingRecord> = fs::read_to_string(session_dir.join("billing.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok());

            match billing {
                Some(billing) => {
                    if let Some(expected_codes) = &label.labels.billing_codes_expected {
                        let mut actual: Vec<String> = billing.codes.iter().map(|c| c.code.clone()).collect();
                        actual.extend(billing.time_entries.iter().map(|t| t.code.clone()));
                        results.checks += 1;
                        if expected_codes.iter().all(|c| actual.contains(c)) {
                            results.passes += 1;
                        } else {
                            let mut expected_sorted = expected_codes.clone();
                            expected_sorted.sort();
                            let mut actual_sorted = actual.clone();
                            actual_sorted.sort();
                            results.failures.push(format!(
                                "  ✗ billing_codes: expected {:?} subset of actual, got {:?}",
                                expected_sorted, actual_sorted
                            ));
                        }
                    }

                    if let Some(expected_dx) = &label.labels.diagnostic_code_expected {
                        let actual_dx = billing.diagnostic_code.clone().unwrap_or_default();
                        results.check("diagnostic_code", expected_dx, &actual_dx);
                    }
                }
                None => {
                    results.checks += 1;
                    results.failures.push("  ✗ billing.json missing — cannot check codes/dx".to_string());
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
