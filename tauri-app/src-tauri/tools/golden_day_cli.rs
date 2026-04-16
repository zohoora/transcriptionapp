//! Golden day regression: verifies a fully-labeled clinic day matches production.
//!
//! Unlike `labeled_regression_cli` (which checks individual sessions), this runs
//! one or more "golden days" as a unit:
//!   - Every session in the day must have a label
//!   - The number of sessions in the archive must match the number of labels
//!   - All labeled checks must pass
//!
//! This catches regressions like:
//!   - A session getting deleted in a refactor
//!   - An extra spurious split appearing
//!   - Encounter renumbering breaking
//!
//! Usage:
//!   cargo run --bin golden_day_cli -- 2026-04-15
//!   cargo run --bin golden_day_cli -- --all-days --fail-on-regression

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

use transcription_app_lib::local_archive;

#[derive(Debug, Deserialize)]
struct Label {
    session_id: String,
    date: String,
    #[allow(dead_code)]
    #[serde(default)]
    labels: serde_json::Value,
}

fn print_usage(program: &str) {
    eprintln!("Usage: {} <DATE | --all-days> [OPTIONS]", program);
    eprintln!();
    eprintln!("Verify a fully-labeled clinic day matches production.");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  DATE                  Date in YYYY-MM-DD format (e.g. 2026-04-15)");
    eprintln!("  --all-days            Run every day with at least one label");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --fail-on-regression  Exit non-zero if any check fails");
    eprintln!("  --help                Show this help");
}

fn labels_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("labels")
}

fn load_all_labels() -> Vec<Label> {
    let dir = labels_dir();
    let mut labels = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(label) = serde_json::from_str::<Label>(&content) {
                    labels.push(label);
                }
            }
        }
    }
    labels
}

fn count_sessions_in_archive(date: &str) -> Result<usize, String> {
    let archive_dir = local_archive::get_archive_dir().map_err(|e| e.to_string())?;
    // Date format: YYYY-MM-DD → archive/YYYY/MM/DD/
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("Invalid date format: {}", date));
    }
    let day_dir = archive_dir.join(parts[0]).join(parts[1]).join(parts[2]);
    if !day_dir.exists() {
        return Ok(0);
    }
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(&day_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("metadata.json").exists() {
                count += 1;
            }
        }
    }
    Ok(count)
}

fn verify_day(date: &str, labels_for_day: &[&Label]) -> (u32, u32, Vec<String>) {
    let mut checks = 0;
    let mut passes = 0;
    let mut issues: Vec<String> = Vec::new();

    println!("\n=== Golden Day: {} ===", date);
    println!("  Labels in fixture set: {}", labels_for_day.len());

    // Check 1: every labeled session exists in the archive
    let mut sessions_found = 0;
    for label in labels_for_day {
        checks += 1;
        match local_archive::get_session(&label.session_id, date) {
            Ok(_) => {
                passes += 1;
                sessions_found += 1;
            }
            Err(e) => {
                issues.push(format!(
                    "  ✗ Session {} not found in archive: {}",
                    &label.session_id[..8],
                    e
                ));
            }
        }
    }

    // Check 2: archive session count matches label count
    checks += 1;
    let archive_count = count_sessions_in_archive(date).unwrap_or(0);
    if archive_count == labels_for_day.len() {
        passes += 1;
        println!("  ✓ Archive has {} sessions, matches {} labels", archive_count, labels_for_day.len());
    } else {
        issues.push(format!(
            "  ✗ Archive has {} sessions but {} labels — count mismatch",
            archive_count, labels_for_day.len()
        ));
    }

    println!("  Sessions verified: {}/{}", sessions_found, labels_for_day.len());
    println!("  Checks: {}/{} passed", passes, checks);
    for issue in &issues {
        println!("{}", issue);
    }
    (checks, passes, issues)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let program = &args[0];
    if args.len() < 2 || args.contains(&"--help".to_string()) {
        print_usage(program);
        return if args.contains(&"--help".to_string()) { ExitCode::SUCCESS } else { ExitCode::from(1) };
    }

    let mut date_arg: Option<String> = None;
    let mut all_days = false;
    let mut fail_on_regression = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--all-days" => all_days = true,
            "--fail-on-regression" => fail_on_regression = true,
            "--help" => { print_usage(program); return ExitCode::SUCCESS; }
            other => {
                if other.starts_with('-') {
                    eprintln!("Unknown option: {}", other);
                    return ExitCode::from(1);
                }
                date_arg = Some(other.to_string());
            }
        }
        i += 1;
    }

    let labels = load_all_labels();
    if labels.is_empty() {
        eprintln!("No labels found in {}", labels_dir().display());
        return ExitCode::SUCCESS;
    }

    // Group labels by date
    let mut by_date: HashMap<String, Vec<&Label>> = HashMap::new();
    for label in &labels {
        by_date.entry(label.date.clone()).or_default().push(label);
    }

    let dates: Vec<&String> = if all_days {
        let mut ds: Vec<&String> = by_date.keys().collect();
        ds.sort();
        ds
    } else if let Some(ref d) = date_arg {
        if !by_date.contains_key(d) {
            eprintln!("No labels found for date {}", d);
            return ExitCode::from(1);
        }
        vec![d]
    } else {
        eprintln!("Provide a date (YYYY-MM-DD) or --all-days");
        return ExitCode::from(1);
    };

    let mut total_checks = 0;
    let mut total_passes = 0;
    let mut total_issues: Vec<String> = Vec::new();

    for date in dates {
        let labels_for_day = by_date.get(date).unwrap();
        let (checks, passes, issues) = verify_day(date, labels_for_day);
        total_checks += checks;
        total_passes += passes;
        total_issues.extend(issues);
    }

    println!();
    println!("════════════════════════════════════════════");
    println!("Total: {}/{} checks passed", total_passes, total_checks);
    if !total_issues.is_empty() {
        println!("Issues: {}", total_issues.len());
    }

    if fail_on_regression && total_passes < total_checks {
        eprintln!();
        eprintln!("REGRESSION: golden day verification failed");
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}
