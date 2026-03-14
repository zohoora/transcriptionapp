//! Detection Replay CLI
//!
//! Reads replay bundles from the archive, reconstructs `DetectionEvalContext` from
//! captured data, runs `evaluate_detection()`, and compares to actual outcomes.
//!
//! Usage:
//!   cargo run --bin detection_replay_cli -- ~/.transcriptionapp/archive/2026/03/12/
//!   cargo run --bin detection_replay_cli -- ~/.transcriptionapp/archive/2026/03/12/ --override hybrid_confirm_window_secs=120
//!   cargo run --bin detection_replay_cli -- --all

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use transcription_app_lib::config::Config;
use transcription_app_lib::encounter_detection::{
    DetectionEvalContext, DetectionOutcome, EncounterDetectionResult, evaluate_detection,
};
use transcription_app_lib::local_archive;
use transcription_app_lib::replay_bundle::ReplayBundle;

fn print_usage(program: &str) {
    eprintln!("Detection Replay CLI");
    eprintln!();
    eprintln!("Replays archived encounter detection decisions through the pure evaluate_detection()");
    eprintln!("function and compares to actual outcomes.");
    eprintln!();
    eprintln!("Usage: {} [options] [archive_path]", program);
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  archive_path        Path to archive directory (e.g., ~/.transcriptionapp/archive/2026/03/12/)");
    eprintln!("                      Recursively finds all replay_bundle.json files");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --all               Replay all bundles in ~/.transcriptionapp/archive/");
    eprintln!("  --override K=V      Override a DetectionEvalContext field for what-if analysis");
    eprintln!("                      Supported: hybrid_confirm_window_secs, hybrid_min_words_for_sensor_split,");
    eprintln!("                                 merge_back_count");
    eprintln!("  --mismatches        Only show bundles where replayed decision differs from actual");
    eprintln!("  --help              Show this help");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {} ~/.transcriptionapp/archive/2026/03/12/", program);
    eprintln!("  {} --all", program);
    eprintln!("  {} --all --override hybrid_confirm_window_secs=120", program);
    eprintln!("  {} --all --mismatches", program);
}

/// Parsed --override values
#[derive(Default)]
struct Overrides {
    hybrid_confirm_window_secs: Option<u64>,
    hybrid_min_words_for_sensor_split: Option<usize>,
    merge_back_count: Option<usize>,
}

fn parse_override(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid override format: '{}' (expected KEY=VALUE)", s));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Recursively find all replay_bundle.json files under a directory
fn find_replay_bundles(dir: &Path) -> Vec<PathBuf> {
    let mut bundles = Vec::new();
    if !dir.is_dir() {
        return bundles;
    }
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return bundles,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            bundles.extend(find_replay_bundles(&path));
        } else if path.file_name().map_or(false, |n| n == "replay_bundle.json") {
            bundles.push(path);
        }
    }
    bundles.sort();
    bundles
}

/// Reconstruct DetectionEvalContext from a DetectionCheck in a replay bundle
fn build_eval_context(
    check: &transcription_app_lib::replay_bundle::DetectionCheck,
    config: &serde_json::Value,
    overrides: &Overrides,
) -> DetectionEvalContext {
    // Reconstruct detection_result from parsed fields
    let detection_result = if check.success {
        check.parsed_complete.map(|complete| EncounterDetectionResult {
            complete,
            end_segment_index: check.parsed_end_index,
            confidence: check.parsed_confidence,
        })
    } else {
        None // LLM error/timeout
    };

    let buffer_age_mins = (check.loop_state.buffer_age_secs / 60.0) as i64;
    let merge_back_count = overrides
        .merge_back_count
        .unwrap_or(check.loop_state.merge_back_count as usize);

    // Extract hybrid config from bundle config
    let detection_mode = config
        .get("encounter_detection_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("hybrid");
    let is_hybrid_mode = detection_mode == "hybrid";

    // Fall back to bundle config, then to app Config defaults (avoids magic numbers)
    let app_defaults = Config::load_or_default();
    let hybrid_confirm_window_secs = overrides.hybrid_confirm_window_secs.unwrap_or_else(|| {
        config
            .get("hybrid_confirm_window_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(app_defaults.hybrid_confirm_window_secs)
    });

    let hybrid_min_words_for_sensor_split =
        overrides
            .hybrid_min_words_for_sensor_split
            .unwrap_or_else(|| {
                config
                    .get("hybrid_min_words_for_sensor_split")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(app_defaults.hybrid_min_words_for_sensor_split)
            });

    // Compute sensor_absent_secs from loop_state.sensor_absent_since and check timestamp
    let sensor_absent_secs = check
        .loop_state
        .sensor_absent_since
        .as_ref()
        .and_then(|absent_str| {
            let absent_time = chrono::DateTime::parse_from_rfc3339(absent_str).ok()?;
            let check_time = chrono::DateTime::parse_from_rfc3339(&check.ts).ok()?;
            let diff = (check_time - absent_time).num_seconds();
            if diff >= 0 {
                Some(diff as u64)
            } else {
                None
            }
        });

    // Detect sensor_triggered: sensor_departed + not hybrid → pure sensor trigger
    let sensor_triggered = check.sensor_context.departed && !is_hybrid_mode;

    DetectionEvalContext {
        detection_result,
        buffer_age_mins,
        merge_back_count,
        word_count: check.word_count,
        cleaned_word_count: check.cleaned_word_count,
        consecutive_llm_failures: check.loop_state.consecutive_failures,
        manual_triggered: false, // bundles don't capture manual triggers
        sensor_triggered,
        is_hybrid_mode,
        sensor_absent_secs,
        hybrid_confirm_window_secs,
        hybrid_min_words_for_sensor_split,
    }
}

/// Determine actual outcome from the bundle's split_decision and outcome fields
fn actual_outcome_str(bundle: &ReplayBundle, check_idx: usize, total_checks: usize) -> String {
    // Only the last check in a bundle could have triggered a split
    if check_idx < total_checks - 1 {
        // Intermediate check — was not the one that triggered the split
        return "NoSplit".to_string();
    }
    // Last check: did the bundle end in a split?
    if let Some(ref split) = bundle.split_decision {
        format!("Split({})", split.trigger)
    } else if bundle.outcome.is_some() {
        "Split(unknown)".to_string()
    } else {
        "NoSplit".to_string()
    }
}

/// Format a DetectionOutcome for display
fn format_outcome(outcome: &DetectionOutcome) -> String {
    match outcome {
        DetectionOutcome::Split { confidence, trigger, .. } => {
            format!("Split({}, {:.2})", trigger, confidence)
        }
        DetectionOutcome::ForceSplit { trigger } => format!("ForceSplit({})", trigger),
        DetectionOutcome::BelowThreshold { confidence, threshold } => {
            format!("BelowThreshold({:.2}<{:.2})", confidence, threshold)
        }
        DetectionOutcome::NoSplit => "NoSplit".to_string(),
        DetectionOutcome::NoResult => "NoResult".to_string(),
    }
}

/// Check if replayed outcome matches actual
fn outcomes_agree(replayed: &DetectionOutcome, actual: &str) -> bool {
    match replayed {
        DetectionOutcome::Split { .. } | DetectionOutcome::ForceSplit { .. } => {
            actual.starts_with("Split") || actual.starts_with("ForceSplit")
        }
        DetectionOutcome::NoSplit
        | DetectionOutcome::NoResult
        | DetectionOutcome::BelowThreshold { .. } => {
            actual == "NoSplit"
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = &args[0];

    if args.len() < 2 || args.contains(&"--help".to_string()) {
        print_usage(program);
        std::process::exit(if args.contains(&"--help".to_string()) {
            0
        } else {
            1
        });
    }

    // Parse arguments
    let mut archive_path: Option<PathBuf> = None;
    let mut overrides = Overrides::default();
    let mut mismatches_only = false;
    let mut all_archives = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => {
                all_archives = true;
            }
            "--mismatches" => {
                mismatches_only = true;
            }
            "--override" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --override requires a KEY=VALUE argument");
                    std::process::exit(1);
                }
                match parse_override(&args[i]) {
                    Ok((key, value)) => match key.as_str() {
                        "hybrid_confirm_window_secs" => {
                            overrides.hybrid_confirm_window_secs =
                                Some(value.parse().expect("Invalid u64 value"));
                        }
                        "hybrid_min_words_for_sensor_split" => {
                            overrides.hybrid_min_words_for_sensor_split =
                                Some(value.parse().expect("Invalid usize value"));
                        }
                        "merge_back_count" => {
                            overrides.merge_back_count =
                                Some(value.parse().expect("Invalid usize value"));
                        }
                        _ => {
                            eprintln!("Unknown override key: {}", key);
                            std::process::exit(1);
                        }
                    },
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            "--help" => {
                print_usage(program);
                std::process::exit(0);
            }
            other => {
                if other.starts_with('-') {
                    eprintln!("Unknown option: {}", other);
                    std::process::exit(1);
                }
                archive_path = Some(PathBuf::from(other));
            }
        }
        i += 1;
    }

    // Determine search path
    let search_path = if all_archives {
        local_archive::get_archive_dir().expect("Could not determine archive directory")
    } else if let Some(ref path) = archive_path {
        path.clone()
    } else {
        eprintln!("Error: provide an archive path or use --all");
        std::process::exit(1);
    };

    if !search_path.exists() {
        eprintln!("Path does not exist: {}", search_path.display());
        std::process::exit(1);
    }

    // Find all replay bundles
    let bundle_paths = find_replay_bundles(&search_path);
    if bundle_paths.is_empty() {
        eprintln!("No replay_bundle.json files found under {}", search_path.display());
        std::process::exit(0);
    }

    // Print override info
    if overrides.hybrid_confirm_window_secs.is_some()
        || overrides.hybrid_min_words_for_sensor_split.is_some()
        || overrides.merge_back_count.is_some()
    {
        eprintln!("Overrides active:");
        if let Some(v) = overrides.hybrid_confirm_window_secs {
            eprintln!("  hybrid_confirm_window_secs = {}", v);
        }
        if let Some(v) = overrides.hybrid_min_words_for_sensor_split {
            eprintln!("  hybrid_min_words_for_sensor_split = {}", v);
        }
        if let Some(v) = overrides.merge_back_count {
            eprintln!("  merge_back_count = {}", v);
        }
        eprintln!();
    }

    let mut total_bundles = 0;
    let mut total_checks = 0;
    let mut matches = 0;
    let mut mismatches = 0;

    for bundle_path in &bundle_paths {
        let content = match fs::read_to_string(bundle_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("  Error reading {}: {}", bundle_path.display(), e);
                continue;
            }
        };

        let bundle: ReplayBundle = match serde_json::from_str(&content) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("  Error parsing {}: {}", bundle_path.display(), e);
                continue;
            }
        };

        if bundle.detection_checks.is_empty() {
            continue;
        }

        total_bundles += 1;
        let num_checks = bundle.detection_checks.len();
        let mut bundle_has_mismatch = false;
        let mut check_lines = Vec::new();

        for (idx, check) in bundle.detection_checks.iter().enumerate() {
            total_checks += 1;
            let ctx = build_eval_context(check, &bundle.config, &overrides);
            let (outcome, _new_failures) = evaluate_detection(&ctx);
            let actual = actual_outcome_str(&bundle, idx, num_checks);
            let agree = outcomes_agree(&outcome, &actual);

            if agree {
                matches += 1;
            } else {
                mismatches += 1;
                bundle_has_mismatch = true;
            }

            let symbol = if agree { "\u{2713}" } else { "\u{2717}" };
            check_lines.push(format!(
                "  Check {}/{}: actual={:<24} replayed={:<32} {}",
                idx + 1,
                num_checks,
                actual,
                format_outcome(&outcome),
                symbol
            ));
        }

        // Print results (filter if --mismatches)
        if !mismatches_only || bundle_has_mismatch {
            // Compute relative path for display
            let display_path = bundle_path
                .strip_prefix(&search_path)
                .unwrap_or(bundle_path)
                .display();
            let status = if bundle_has_mismatch {
                "MISMATCH"
            } else {
                "MATCH"
            };
            println!("Bundle: {} [{}]", display_path, status);
            for line in &check_lines {
                println!("{}", line);
            }
            println!();
        }
    }

    // Summary
    println!("────────────────────────────────────────────");
    println!(
        "Bundles: {}  Checks: {}  Match: {}  Mismatch: {}",
        total_bundles, total_checks, matches, mismatches
    );
    if total_checks > 0 {
        let pct = matches as f64 / total_checks as f64 * 100.0;
        println!("Agreement: {:.1}%", pct);
    }
}
