//! Bootstrap label fixtures from existing production billing data.
//!
//! For a given date, walks the archive and creates a label file for each
//! session. The label assumes production was correct (codes_expected = current
//! billing.json codes, diagnostic_code_expected = current dx). Human review
//! can then downgrade specific assertions to `null` for known-bad cases.
//!
//! This grows the labeled corpus without requiring full manual review of
//! each session — the labels lock in the current state, so any future
//! regression away from this state is flagged.
//!
//! Usage:
//!   cargo run --bin bootstrap_labels -- 2026-04-14
//!   cargo run --bin bootstrap_labels -- 2026-04-14 --overwrite   # replace existing labels
//!   cargo run --bin bootstrap_labels -- 2026-04-14 --dry-run     # show what would be created

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

use transcription_app_lib::billing::BillingRecord;
use transcription_app_lib::local_archive;

#[derive(Debug, Deserialize)]
struct Metadata {
    session_id: String,
    #[serde(default)]
    patient_name: Option<String>,
    #[serde(default)]
    word_count: Option<u64>,
    #[serde(default)]
    encounter_number: Option<u32>,
    #[serde(default)]
    likely_non_clinical: Option<bool>,
}

fn print_usage(program: &str) {
    eprintln!("Usage: {} <DATE> [OPTIONS]", program);
    eprintln!();
    eprintln!("Bootstrap label fixtures from existing production billing data.");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  DATE          Date in YYYY-MM-DD format (e.g. 2026-04-14)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --overwrite   Replace existing label files (default: skip)");
    eprintln!("  --dry-run     Show what would be created without writing");
    eprintln!("  --help        Show this help");
}

fn labels_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("labels")
}

fn day_archive_dir(date: &str) -> Result<PathBuf, String> {
    let archive = local_archive::get_archive_dir().map_err(|e| e.to_string())?;
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("Invalid date: {}", date));
    }
    Ok(archive.join(parts[0]).join(parts[1]).join(parts[2]))
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let program = &args[0];
    if args.len() < 2 || args.contains(&"--help".to_string()) {
        print_usage(program);
        return if args.contains(&"--help".to_string()) { ExitCode::SUCCESS } else { ExitCode::from(1) };
    }

    let mut date: Option<String> = None;
    let mut overwrite = false;
    let mut dry_run = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--overwrite" => overwrite = true,
            "--dry-run" => dry_run = true,
            "--help" => { print_usage(program); return ExitCode::SUCCESS; }
            other => {
                if other.starts_with('-') {
                    eprintln!("Unknown option: {}", other);
                    return ExitCode::from(1);
                }
                date = Some(other.to_string());
            }
        }
        i += 1;
    }

    let date = match date {
        Some(d) => d,
        None => { eprintln!("Provide a date (YYYY-MM-DD)"); return ExitCode::from(1); }
    };

    let day_dir = match day_archive_dir(&date) {
        Ok(d) => d,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
    };
    if !day_dir.exists() {
        eprintln!("Day directory does not exist: {}", day_dir.display());
        return ExitCode::from(1);
    }

    let labels_d = labels_dir();
    fs::create_dir_all(&labels_d).expect("create labels dir");

    let mut sessions_seen = 0;
    let mut labels_written = 0;
    let mut labels_skipped = 0;

    let entries = fs::read_dir(&day_dir).expect("read day dir");
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }

        let metadata_path = path.join("metadata.json");
        if !metadata_path.exists() { continue; }

        let metadata_content = match fs::read_to_string(&metadata_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let metadata: Metadata = match serde_json::from_str(&metadata_content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        sessions_seen += 1;
        let short_id = &metadata.session_id[..8.min(metadata.session_id.len())];
        let label_filename = format!("{}_{}.json", date, short_id);
        let label_path = labels_d.join(&label_filename);

        if label_path.exists() && !overwrite {
            labels_skipped += 1;
            println!("⊘ {} already exists (use --overwrite to replace)", label_filename);
            continue;
        }

        // Determine clinical classification from metadata
        let is_clinical = !metadata.likely_non_clinical.unwrap_or(false);

        // Load production billing.json (if present)
        let billing_path = path.join("billing.json");
        let billing_record: Option<BillingRecord> = if billing_path.exists() {
            fs::read_to_string(&billing_path).ok()
                .and_then(|s| serde_json::from_str::<BillingRecord>(&s).ok())
        } else {
            None
        };

        // Build labels object — only include billing assertions when billing exists
        let mut labels = serde_json::json!({
            "split_correct": true,
            "merge_correct": true,
            "clinical_correct": is_clinical,
            "patient_count_correct": true,
        });

        let dx_desc = if let Some(record) = billing_record {
            let mut codes: Vec<String> = record.codes.iter().map(|c| c.code.clone()).collect();
            codes.extend(record.time_entries.iter().map(|t| t.code.clone()));
            codes.sort();
            codes.dedup();
            labels["billing_codes_expected"] = serde_json::json!(codes);
            if let Some(dx) = record.diagnostic_code.clone() {
                labels["diagnostic_code_expected"] = serde_json::json!(dx);
            }
            record.diagnostic_description.clone()
        } else {
            None
        };

        labels["notes"] = serde_json::json!(format!(
            "Auto-bootstrapped from production. Patient: {}. Words: {}. Encounter #{}. {}{}",
            metadata.patient_name.as_deref().unwrap_or("-"),
            metadata.word_count.unwrap_or(0),
            metadata.encounter_number.unwrap_or(0),
            if is_clinical { "Clinical." } else { "Non-clinical (no SOAP/billing)." },
            dx_desc.as_ref().map(|d| format!(" Dx: {}", d)).unwrap_or_default(),
        ));

        let label = serde_json::json!({
            "session_id": metadata.session_id,
            "date": date,
            "labeled_at": chrono::Utc::now().to_rfc3339(),
            "labeled_by": "bootstrap_labels (auto from production)",
            "labels": labels,
        });

        if dry_run {
            println!("[DRY-RUN] would write {}", label_filename);
        } else {
            let json = serde_json::to_string_pretty(&label).unwrap();
            fs::write(&label_path, json).expect("write label");
            labels_written += 1;
            println!("✓ {}", label_filename);
        }
    }

    println!();
    println!("────────────────────────────────────────────");
    println!("Sessions seen: {}  Labels written: {}  Skipped (existed): {}",
        sessions_seen, labels_written, labels_skipped);
    if dry_run {
        println!("(dry-run — no files were actually written)");
    }
    ExitCode::SUCCESS
}
