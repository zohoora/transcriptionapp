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
    eprintln!("                                 merge_back_count, min_sensor_hybrid_words");
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
    min_sensor_hybrid_words: Option<usize>,
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

/// Check if a detection check would be skipped by the pre-check guard that prevents
/// micro-splits from sensor flicker in hybrid mode. In production, sensor-triggered
/// wakeups skip the LLM call entirely when word count is below the minimum.
fn should_skip_precheck(
    check: &transcription_app_lib::replay_bundle::DetectionCheck,
    config: &serde_json::Value,
    overrides: &Overrides,
) -> bool {
    let is_hybrid = config
        .get("encounter_detection_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("hybrid")
        == "hybrid";
    let min_words = overrides.min_sensor_hybrid_words.unwrap_or(500);
    is_hybrid && check.sensor_context.departed && check.word_count < min_words
}

/// Determine actual outcome from the bundle's split_decision and outcome fields.
///
/// Intermediate checks that led to a split-then-merge-back are detected by
/// merge_back_count increasing between consecutive checks.
fn actual_outcome_str(bundle: &ReplayBundle, check_idx: usize, total_checks: usize) -> String {
    if check_idx < total_checks - 1 {
        // Detect intermediate split→merge-back: if the NEXT check's merge_back_count
        // is higher, this check triggered a split that was subsequently merged back.
        let current_mbc = bundle.detection_checks[check_idx].loop_state.merge_back_count;
        let next_mbc = bundle.detection_checks[check_idx + 1].loop_state.merge_back_count;
        if next_mbc > current_mbc {
            return "SplitMergedBack".to_string();
        }
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

/// Check if replayed outcome matches actual.
/// "SplitMergedBack" counts as agreement with a Split replay — the detection
/// logic correctly identified a split, and the merge-back was a separate decision.
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
                        "min_sensor_hybrid_words" => {
                            overrides.min_sensor_hybrid_words =
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
        || overrides.min_sensor_hybrid_words.is_some()
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
        if let Some(v) = overrides.min_sensor_hybrid_words {
            eprintln!("  min_sensor_hybrid_words = {}", v);
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
            let actual = actual_outcome_str(&bundle, idx, num_checks);

            // Pre-check guard: sensor-triggered checks in hybrid mode with
            // insufficient words would now be skipped before evaluate_detection()
            let (replayed_str, agree) =
                if should_skip_precheck(check, &bundle.config, &overrides) {
                    let min_words = overrides.min_sensor_hybrid_words.unwrap_or(500);
                    let s = format!(
                        "Skipped(sensor_precheck, {}w<{})",
                        check.word_count, min_words
                    );
                    (s, actual == "NoSplit")
                } else {
                    let ctx = build_eval_context(check, &bundle.config, &overrides);
                    let (outcome, _new_failures) = evaluate_detection(&ctx);
                    let agree = outcomes_agree(&outcome, &actual);
                    (format_outcome(&outcome), agree)
                };

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
                replayed_str,
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

#[cfg(test)]
mod tests {
    use super::*;
    use transcription_app_lib::replay_bundle::*;

    /// Helper to create a minimal ReplayBundle with the given detection checks and split.
    fn make_bundle(
        checks: Vec<DetectionCheck>,
        split: Option<SplitDecision>,
    ) -> ReplayBundle {
        ReplayBundle {
            schema_version: 1,
            config: serde_json::json!({
                "encounter_detection_mode": "hybrid",
                "hybrid_confirm_window_secs": 180,
                "hybrid_min_words_for_sensor_split": 500,
            }),
            segments: vec![],
            sensor_transitions: vec![],
            vision_results: vec![],
            detection_checks: checks,
            split_decision: split,
            clinical_check: None,
            merge_check: None,
            soap_result: None,
            name_tracker: None,
            outcome: None,
        }
    }

    /// Helper to create a detection check with specific LLM response.
    fn make_check(
        complete: Option<bool>,
        confidence: Option<f64>,
        end_index: Option<u64>,
        word_count: usize,
        merge_back_count: u32,
        buffer_age_secs: f64,
        success: bool,
    ) -> DetectionCheck {
        let mut check = DetectionCheck::new(
            (0, 100),
            word_count,
            word_count,
            SensorContext::new(false, false),
            String::new(),
            String::new(),
            500,
            0,
            merge_back_count,
            buffer_age_secs,
            None,
        );
        check.success = success;
        check.parsed_complete = complete;
        check.parsed_confidence = confidence;
        check.parsed_end_index = end_index;
        check
    }

    // ---- actual_outcome_str tests ----

    #[test]
    fn test_last_check_with_split_decision() {
        let bundle = make_bundle(
            vec![make_check(Some(true), Some(0.95), Some(50), 1000, 0, 600.0, true)],
            Some(SplitDecision {
                ts: "2026-03-12T10:00:00Z".into(),
                trigger: "hybrid_llm".into(),
                word_count: 1000,
                cleaned_word_count: 1000,
                end_segment_index: Some(50),
            }),
        );
        assert_eq!(actual_outcome_str(&bundle, 0, 1), "Split(hybrid_llm)");
    }

    #[test]
    fn test_intermediate_check_no_merge_back() {
        let checks = vec![
            make_check(None, None, None, 500, 0, 300.0, false), // LLM failed
            make_check(Some(true), Some(0.95), Some(50), 1000, 0, 600.0, true),
        ];
        let bundle = make_bundle(checks, Some(SplitDecision {
            ts: "2026-03-12T10:00:00Z".into(),
            trigger: "hybrid_llm".into(),
            word_count: 1000,
            cleaned_word_count: 1000,
            end_segment_index: Some(50),
        }));
        assert_eq!(actual_outcome_str(&bundle, 0, 2), "NoSplit");
    }

    #[test]
    fn test_intermediate_check_with_merge_back() {
        let checks = vec![
            make_check(Some(true), Some(0.95), Some(30), 600, 0, 250.0, true), // split→merge-back
            make_check(Some(true), Some(0.92), Some(80), 1500, 1, 900.0, true), // final split
        ];
        let bundle = make_bundle(checks, Some(SplitDecision {
            ts: "2026-03-12T10:00:00Z".into(),
            trigger: "hybrid_llm".into(),
            word_count: 1500,
            cleaned_word_count: 1500,
            end_segment_index: Some(80),
        }));
        // Check 0 had merge_back_count=0, check 1 has merge_back_count=1 → SplitMergedBack
        assert_eq!(actual_outcome_str(&bundle, 0, 2), "SplitMergedBack");
    }

    #[test]
    fn test_multiple_merge_backs() {
        let checks = vec![
            make_check(Some(true), Some(0.95), Some(10), 400, 0, 200.0, true), // split→merge-back
            make_check(Some(true), Some(0.92), Some(20), 500, 1, 300.0, true), // split→merge-back
            make_check(Some(true), Some(0.95), Some(50), 1000, 2, 600.0, true), // final split
        ];
        let bundle = make_bundle(checks, Some(SplitDecision {
            ts: "2026-03-12T10:00:00Z".into(),
            trigger: "hybrid_llm".into(),
            word_count: 1000,
            cleaned_word_count: 1000,
            end_segment_index: Some(50),
        }));
        assert_eq!(actual_outcome_str(&bundle, 0, 3), "SplitMergedBack");
        assert_eq!(actual_outcome_str(&bundle, 1, 3), "SplitMergedBack");
        assert_eq!(actual_outcome_str(&bundle, 2, 3), "Split(hybrid_llm)");
    }

    // ---- outcomes_agree tests ----

    #[test]
    fn test_split_agrees_with_split() {
        let outcome = DetectionOutcome::Split {
            end_segment_index: Some(50),
            confidence: 0.95,
            trigger: "llm".into(),
        };
        assert!(outcomes_agree(&outcome, "Split(hybrid_llm)"));
        assert!(outcomes_agree(&outcome, "SplitMergedBack"));
    }

    #[test]
    fn test_nosplit_agrees_with_nosplit() {
        assert!(outcomes_agree(&DetectionOutcome::NoSplit, "NoSplit"));
        assert!(outcomes_agree(&DetectionOutcome::NoResult, "NoSplit"));
        assert!(outcomes_agree(
            &DetectionOutcome::BelowThreshold { confidence: 0.5, threshold: 0.85 },
            "NoSplit"
        ));
    }

    #[test]
    fn test_split_disagrees_with_nosplit() {
        let outcome = DetectionOutcome::Split {
            end_segment_index: Some(50),
            confidence: 0.95,
            trigger: "llm".into(),
        };
        assert!(!outcomes_agree(&outcome, "NoSplit"));
    }

    // ---- build_eval_context + evaluate_detection end-to-end tests ----

    #[test]
    fn test_normal_llm_split() {
        let check = make_check(Some(true), Some(0.95), Some(50), 1000, 0, 600.0, true);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides::default();
        let ctx = build_eval_context(&check, &config, &overrides);
        let (outcome, _) = evaluate_detection(&ctx);
        match outcome {
            DetectionOutcome::Split { confidence, .. } => {
                assert!((confidence - 0.95).abs() < 0.001);
            }
            other => panic!("Expected Split, got {:?}", other),
        }
    }

    #[test]
    fn test_below_threshold_with_merge_back() {
        // merge_back_count=2 → threshold = 0.85 + 0.10 = 0.95
        // confidence 0.92 < 0.95 → BelowThreshold
        let check = make_check(Some(true), Some(0.92), Some(50), 1000, 2, 600.0, true);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides::default();
        let ctx = build_eval_context(&check, &config, &overrides);
        let (outcome, _) = evaluate_detection(&ctx);
        match outcome {
            DetectionOutcome::BelowThreshold { confidence, threshold } => {
                assert!((confidence - 0.92).abs() < 0.001);
                assert!((threshold - 0.95).abs() < 0.001);
            }
            other => panic!("Expected BelowThreshold, got {:?}", other),
        }
    }

    #[test]
    fn test_llm_failure_no_result() {
        let check = make_check(None, None, None, 500, 0, 300.0, false);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides::default();
        let ctx = build_eval_context(&check, &config, &overrides);
        let (outcome, failures) = evaluate_detection(&ctx);
        assert!(matches!(outcome, DetectionOutcome::NoResult));
        assert_eq!(failures, 1); // Incremented from 0
    }

    #[test]
    fn test_not_complete_returns_nosplit() {
        let check = make_check(Some(false), None, None, 800, 0, 400.0, true);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides::default();
        let ctx = build_eval_context(&check, &config, &overrides);
        let (outcome, failures) = evaluate_detection(&ctx);
        assert!(matches!(outcome, DetectionOutcome::NoSplit));
        assert_eq!(failures, 0); // Reset on confident "no"
    }

    #[test]
    fn test_absolute_word_cap_force_split() {
        let check = make_check(Some(false), None, None, 26000, 0, 7200.0, true);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides::default();
        let ctx = build_eval_context(&check, &config, &overrides);
        let (outcome, _) = evaluate_detection(&ctx);
        match outcome {
            DetectionOutcome::ForceSplit { trigger } => {
                assert_eq!(trigger, "absolute_word_cap");
            }
            other => panic!("Expected ForceSplit, got {:?}", other),
        }
    }

    #[test]
    fn test_hybrid_sensor_timeout_force_split() {
        let absent_time = (chrono::Utc::now() - chrono::Duration::seconds(200)).to_rfc3339();
        let mut check = make_check(Some(false), None, None, 800, 0, 600.0, true);
        check.loop_state.sensor_absent_since = Some(absent_time);
        let config = serde_json::json!({
            "encounter_detection_mode": "hybrid",
            "hybrid_confirm_window_secs": 180,
            "hybrid_min_words_for_sensor_split": 500,
        });
        let overrides = Overrides::default();
        let ctx = build_eval_context(&check, &config, &overrides);
        let (outcome, _) = evaluate_detection(&ctx);
        match outcome {
            DetectionOutcome::ForceSplit { trigger } => {
                assert_eq!(trigger, "hybrid_sensor_timeout");
            }
            other => panic!("Expected ForceSplit(hybrid_sensor_timeout), got {:?}", other),
        }
    }

    #[test]
    fn test_override_changes_threshold() {
        // Without override: merge_back=2 → threshold=0.95, confidence=0.93 → BelowThreshold
        let check = make_check(Some(true), Some(0.93), Some(50), 1000, 2, 600.0, true);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});

        let overrides_normal = Overrides::default();
        let ctx = build_eval_context(&check, &config, &overrides_normal);
        let (outcome, _) = evaluate_detection(&ctx);
        assert!(matches!(outcome, DetectionOutcome::BelowThreshold { .. }));

        // With override: reset merge_back_count=0 → threshold=0.85, confidence=0.93 → Split
        let overrides_reset = Overrides { merge_back_count: Some(0), ..Default::default() };
        let ctx = build_eval_context(&check, &config, &overrides_reset);
        let (outcome, _) = evaluate_detection(&ctx);
        assert!(matches!(outcome, DetectionOutcome::Split { .. }));
    }

    // ---- should_skip_precheck tests ----

    fn make_sensor_check(
        complete: Option<bool>,
        confidence: Option<f64>,
        end_index: Option<u64>,
        word_count: usize,
        merge_back_count: u32,
        buffer_age_secs: f64,
        success: bool,
        departed: bool,
    ) -> DetectionCheck {
        let mut check = DetectionCheck::new(
            (0, 100),
            word_count,
            word_count,
            SensorContext::new(departed, !departed),
            String::new(),
            String::new(),
            500,
            0,
            merge_back_count,
            buffer_age_secs,
            None,
        );
        check.success = success;
        check.parsed_complete = complete;
        check.parsed_confidence = confidence;
        check.parsed_end_index = end_index;
        check
    }

    #[test]
    fn test_precheck_skips_sensor_departed_low_words_hybrid() {
        let check = make_sensor_check(Some(true), Some(0.95), Some(50), 120, 0, 600.0, true, true);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides::default();
        assert!(should_skip_precheck(&check, &config, &overrides));
    }

    #[test]
    fn test_precheck_allows_sufficient_words() {
        let check = make_sensor_check(Some(true), Some(0.95), Some(50), 600, 0, 600.0, true, true);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides::default();
        assert!(!should_skip_precheck(&check, &config, &overrides));
    }

    #[test]
    fn test_precheck_allows_non_hybrid_mode() {
        let check = make_sensor_check(Some(true), Some(0.95), Some(50), 120, 0, 600.0, true, true);
        let config = serde_json::json!({"encounter_detection_mode": "sensor"});
        let overrides = Overrides::default();
        assert!(!should_skip_precheck(&check, &config, &overrides));
    }

    #[test]
    fn test_precheck_allows_sensor_not_departed() {
        let check =
            make_sensor_check(Some(true), Some(0.95), Some(50), 120, 0, 600.0, true, false);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides::default();
        assert!(!should_skip_precheck(&check, &config, &overrides));
    }

    #[test]
    fn test_precheck_override_changes_threshold() {
        let check = make_sensor_check(Some(true), Some(0.95), Some(50), 300, 0, 600.0, true, true);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});

        // Default 500: 300 words < 500 → skip
        assert!(should_skip_precheck(&check, &config, &Overrides::default()));

        // Override 200: 300 words >= 200 → allow
        let overrides_low = Overrides {
            min_sensor_hybrid_words: Some(200),
            ..Default::default()
        };
        assert!(!should_skip_precheck(&check, &config, &overrides_low));
    }

    #[test]
    fn test_long_buffer_lower_threshold() {
        // buffer_age > 20 mins → base threshold 0.70 instead of 0.85
        // confidence 0.75 should pass with 20+ min buffer but fail with <20 min
        let check_long = make_check(Some(true), Some(0.75), Some(50), 1000, 0, 1500.0, true); // 25 mins
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides::default();
        let ctx = build_eval_context(&check_long, &config, &overrides);
        let (outcome, _) = evaluate_detection(&ctx);
        assert!(matches!(outcome, DetectionOutcome::Split { .. }));

        let check_short = make_check(Some(true), Some(0.75), Some(50), 1000, 0, 600.0, true); // 10 mins
        let ctx = build_eval_context(&check_short, &config, &overrides);
        let (outcome, _) = evaluate_detection(&ctx);
        assert!(matches!(outcome, DetectionOutcome::BelowThreshold { .. }));
    }
}
