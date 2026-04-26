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
use std::path::PathBuf;
use std::process::ExitCode;

use transcription_app_lib::config::Config;
use transcription_app_lib::continuous_mode::MIN_SENSOR_HYBRID_WORDS;
use transcription_app_lib::encounter_detection::{
    DetectionEvalContext, DetectionOutcome, EncounterDetectionResult, evaluate_detection,
};
use transcription_app_lib::local_archive;
use transcription_app_lib::replay_bundle::{find_replay_bundles, ReplayBundle};
use transcription_app_lib::replay_fetch::ArchiveFetcher;

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
    eprintln!("  --date YYYY-MM-DD   Replay all sessions for the date, fetching from profile");
    eprintln!("                      service when not in the local archive (multi-room)");
    eprintln!("  --override K=V      Override a DetectionEvalContext field for what-if analysis");
    eprintln!("                      Supported: hybrid_confirm_window_secs, hybrid_min_words_for_sensor_split,");
    eprintln!("                                 merge_back_count, min_sensor_hybrid_words,");
    eprintln!("                                 sensor_continuous_present=true|false,");
    eprintln!("                                 manual_triggered=true|false");
    eprintln!("  --mismatches        Only show bundles where replayed decision differs from actual");
    eprintln!("  --fail-on-mismatch  Exit non-zero if agreement drops below threshold (default: 99.0%)");
    eprintln!("  --threshold PCT     Set the agreement threshold for --fail-on-mismatch (e.g. 95.0)");
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
    /// What-if: force the `sensor_continuous_present` gate on or off for all
    /// checks, regardless of the bundle's captured value. Useful for
    /// evaluating how the 0.99 threshold gate would affect historical data.
    sensor_continuous_present: Option<bool>,
    /// What-if: force `manual_triggered` for all checks. Rarely needed since
    /// manual triggers short-circuit the LLM and don't produce bundle checks,
    /// but exposed for completeness/debugging.
    manual_triggered: Option<bool>,
}

fn parse_override(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid override format: '{}' (expected KEY=VALUE)", s));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
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

    // Read captured trigger state from the bundle (schema v2+). v1 bundles
    // have these defaulted to false via #[serde(default)], so old replay
    // results match pre-v2 CLI behavior unless the user opts into an override.
    let sensor_triggered = check.loop_state.sensor_triggered;
    let manual_triggered = overrides
        .manual_triggered
        .unwrap_or(check.loop_state.manual_triggered);
    let sensor_continuous_present = overrides
        .sensor_continuous_present
        .unwrap_or(check.loop_state.sensor_continuous_present);

    DetectionEvalContext {
        detection_result,
        buffer_age_mins,
        merge_back_count,
        word_count: check.word_count,
        cleaned_word_count: check.cleaned_word_count,
        consecutive_llm_failures: check.loop_state.consecutive_failures,
        manual_triggered,
        sensor_triggered,
        is_hybrid_mode,
        sensor_absent_secs,
        hybrid_confirm_window_secs,
        hybrid_min_words_for_sensor_split,
        sensor_continuous_present,
        server_thresholds: None,
    }
}

/// Check if a detection check would be skipped by the pre-check guard that prevents
/// micro-splits from sensor flicker in hybrid mode. In production, sensor-triggered
/// wakeups skip the LLM call entirely when word count is below the minimum.
///
/// Uses the captured `loop_state.sensor_triggered` flag (schema v2+) as the
/// sole signal. We used to fall back to `sensor_context.departed` for v1
/// bundles, but that heuristic was wrong — `sensor_context.departed` is a
/// prompt hint that production sets whenever `sensor_absent_since.is_some()`,
/// not only when the current check was triggered by a sensor transition.
/// That mismatch caused the replay to silently skip checks production ran.
///
/// The word threshold is imported from `continuous_mode::MIN_SENSOR_HYBRID_WORDS`
/// so production and replay stay in lockstep.
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
    let min_words = overrides
        .min_sensor_hybrid_words
        .unwrap_or(MIN_SENSOR_HYBRID_WORDS);
    is_hybrid && check.loop_state.sensor_triggered && check.word_count < min_words
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let program = &args[0];

    if args.len() < 2 || args.contains(&"--help".to_string()) {
        print_usage(program);
        return if args.contains(&"--help".to_string()) {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }

    // Parse arguments
    let mut archive_path: Option<PathBuf> = None;
    let mut date_arg: Option<String> = None;
    let mut overrides = Overrides::default();
    let mut mismatches_only = false;
    let mut all_archives = false;
    let mut fail_on_mismatch = false;
    let mut threshold_pct: f64 = 99.0;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => {
                all_archives = true;
            }
            "--date" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --date requires a YYYY-MM-DD value");
                    return ExitCode::from(1);
                }
                date_arg = Some(args[i].clone());
            }
            "--mismatches" => {
                mismatches_only = true;
            }
            "--fail-on-mismatch" => {
                fail_on_mismatch = true;
            }
            "--threshold" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --threshold requires a percentage value");
                    return ExitCode::from(1);
                }
                threshold_pct = args[i].parse().expect("Invalid threshold (use a percent like 95.0)");
            }
            "--override" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --override requires a KEY=VALUE argument");
                    return ExitCode::from(1);
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
                        "sensor_continuous_present" => {
                            overrides.sensor_continuous_present = Some(
                                value
                                    .parse()
                                    .expect("Invalid bool value (use true|false)"),
                            );
                        }
                        "manual_triggered" => {
                            overrides.manual_triggered = Some(
                                value
                                    .parse()
                                    .expect("Invalid bool value (use true|false)"),
                            );
                        }
                        _ => {
                            eprintln!("Unknown override key: {}", key);
                            return ExitCode::from(1);
                        }
                    },
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        return ExitCode::from(1);
                    }
                }
            }
            "--help" => {
                print_usage(program);
                return ExitCode::SUCCESS;
            }
            other => {
                if other.starts_with('-') {
                    eprintln!("Unknown option: {}", other);
                    return ExitCode::from(1);
                }
                archive_path = Some(PathBuf::from(other));
            }
        }
        i += 1;
    }

    // Resolve sources: each entry is (display_label, ReplayBundle).
    //
    // Three modes (in priority order):
    //   --date YYYY-MM-DD   per-session iteration with local→server fallback
    //   --all               filesystem walk of ~/.transcriptionapp/archive/
    //   PATH                filesystem walk of the given directory
    enum SourceMode {
        Filesystem(PathBuf),
        Date(String),
    }
    let mode = if let Some(date) = date_arg {
        SourceMode::Date(date)
    } else if all_archives {
        SourceMode::Filesystem(
            local_archive::get_archive_dir().expect("Could not determine archive directory"),
        )
    } else if let Some(path) = archive_path {
        SourceMode::Filesystem(path)
    } else {
        eprintln!("Error: provide an archive path, --all, or --date YYYY-MM-DD");
        return ExitCode::from(1);
    };

    let sources: Vec<(String, ReplayBundle)> = match mode {
        SourceMode::Filesystem(search_path) => {
            if !search_path.exists() {
                eprintln!("Path does not exist: {}", search_path.display());
                return ExitCode::from(1);
            }
            let bundle_paths = find_replay_bundles(&search_path);
            if bundle_paths.is_empty() {
                eprintln!(
                    "No replay_bundle.json files found under {}",
                    search_path.display()
                );
                return ExitCode::SUCCESS;
            }
            let mut out: Vec<(String, ReplayBundle)> = Vec::new();
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
                let display = bundle_path
                    .strip_prefix(&search_path)
                    .unwrap_or(bundle_path)
                    .display()
                    .to_string();
                out.push((display, bundle));
            }
            out
        }
        SourceMode::Date(date) => {
            let fetcher = ArchiveFetcher::from_env().unwrap_or_else(|e| {
                eprintln!("warn: ArchiveFetcher init failed ({e}); falling back to local-only");
                ArchiveFetcher::local_only()
            });
            match fetcher.list_replay_bundles_for_date(&date).await {
                Ok(bundles) if bundles.is_empty() => {
                    eprintln!("No replay_bundle.json found for {} (local or server)", date);
                    return ExitCode::SUCCESS;
                }
                Ok(bundles) => bundles,
                Err(e) => {
                    eprintln!("Error: list bundles for {}: {}", date, e);
                    return ExitCode::from(1);
                }
            }
        }
    };

    // Print override info
    let has_any_override = overrides.hybrid_confirm_window_secs.is_some()
        || overrides.hybrid_min_words_for_sensor_split.is_some()
        || overrides.merge_back_count.is_some()
        || overrides.min_sensor_hybrid_words.is_some()
        || overrides.sensor_continuous_present.is_some()
        || overrides.manual_triggered.is_some();
    if has_any_override {
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
        if let Some(v) = overrides.sensor_continuous_present {
            eprintln!("  sensor_continuous_present = {}", v);
        }
        if let Some(v) = overrides.manual_triggered {
            eprintln!("  manual_triggered = {}", v);
        }
        eprintln!();
    }

    let mut total_bundles = 0;
    let mut total_checks = 0;
    let mut matches = 0;
    let mut mismatches = 0;

    for (display_path, bundle) in &sources {
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
                    let min_words = overrides
                        .min_sensor_hybrid_words
                        .unwrap_or(MIN_SENSOR_HYBRID_WORDS);
                    let s = format!(
                        "Skipped(sensor_precheck, {}w<{})",
                        check.word_count, min_words
                    );
                    (s, actual == "NoSplit")
                } else {
                    let ctx = build_eval_context(check, &bundle.config, &overrides);
                    let (outcome, _new_failures) = evaluate_detection(&ctx);
                    // TODO: simulating production's MIN_SPLIT_WORD_FLOOR
                    // (continuous_mode.rs:1488) requires per-segment word
                    // counts in `ReplaySegment` AND preserving leftover
                    // segments across `build_and_reset`. Both are bundle
                    // schema changes; until then a handful of historical
                    // checks will report Split where production NoSplit'd.
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
    let agreement_pct = if total_checks > 0 {
        let pct = matches as f64 / total_checks as f64 * 100.0;
        println!("Agreement: {:.1}%", pct);
        pct
    } else {
        100.0
    };

    // Regression gate: exit non-zero if below threshold
    if fail_on_mismatch && agreement_pct < threshold_pct {
        eprintln!();
        eprintln!(
            "REGRESSION: agreement {:.1}% is below threshold {:.1}%",
            agreement_pct, threshold_pct
        );
        eprintln!(
            "Run without --fail-on-mismatch and with --mismatches to investigate."
        );
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
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
            billing_result: None,
            name_tracker: None,
            outcome: None,
            multi_patient_detections: vec![],
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
            false, // sensor_continuous_present
            false, // sensor_triggered
            false, // manual_triggered
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
            false,    // sensor_continuous_present
            departed, // sensor_triggered (v2+: captured from production select branch)
            false,    // manual_triggered
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

    // ---- Drift-fix regression tests (schema v2) ----

    /// Build a check with the sensor_continuous_present flag set. Used to
    /// verify that schema v2 fields flow through build_eval_context correctly.
    fn make_check_with_continuous_present(
        confidence: f64,
        word_count: usize,
        merge_back_count: u32,
        buffer_age_secs: f64,
        sensor_continuous_present: bool,
    ) -> DetectionCheck {
        let mut check = DetectionCheck::new(
            (0, 100),
            word_count,
            word_count,
            SensorContext::new(false, true), // sensor present
            String::new(),
            String::new(),
            500,
            0,
            merge_back_count,
            buffer_age_secs,
            None,
            sensor_continuous_present,
            false,
            false,
        );
        check.success = true;
        check.parsed_complete = Some(true);
        check.parsed_confidence = Some(confidence);
        check.parsed_end_index = Some(50);
        check
    }

    #[test]
    fn test_sensor_continuous_present_blocks_high_confidence_split() {
        // Pre-fix bug: replay always set sensor_continuous_present=false, so a
        // 0.95 confidence split would succeed. Production would block it
        // because the 0.99 gate is raised when the sensor has remained Present
        // since the last split (couples/family visit scenario).
        let check = make_check_with_continuous_present(0.95, 1000, 0, 600.0, true);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides::default();
        let ctx = build_eval_context(&check, &config, &overrides);
        assert!(
            ctx.sensor_continuous_present,
            "sensor_continuous_present should flow through from loop_state"
        );
        let (outcome, _) = evaluate_detection(&ctx);
        match outcome {
            DetectionOutcome::BelowThreshold { confidence, threshold } => {
                assert!((confidence - 0.95).abs() < 0.001);
                assert!(
                    (threshold - 0.99).abs() < 0.001,
                    "threshold should be raised to 0.99, got {}",
                    threshold
                );
            }
            other => panic!(
                "Expected BelowThreshold (sensor_continuous_present gate), got {:?}",
                other
            ),
        }
    }

    #[test]
    fn test_sensor_continuous_present_false_allows_normal_split() {
        // When sensor_continuous_present=false, a 0.95 confidence split should
        // succeed normally under the base 0.85 threshold.
        let check = make_check_with_continuous_present(0.95, 1000, 0, 600.0, false);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides::default();
        let ctx = build_eval_context(&check, &config, &overrides);
        let (outcome, _) = evaluate_detection(&ctx);
        assert!(matches!(outcome, DetectionOutcome::Split { .. }));
    }

    #[test]
    fn test_override_forces_sensor_continuous_present_on() {
        // What-if: "how would this historical data replay if the gate were on?"
        let check = make_check_with_continuous_present(0.90, 1000, 0, 600.0, false);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides {
            sensor_continuous_present: Some(true),
            ..Default::default()
        };
        let ctx = build_eval_context(&check, &config, &overrides);
        assert!(ctx.sensor_continuous_present);
        let (outcome, _) = evaluate_detection(&ctx);
        assert!(
            matches!(outcome, DetectionOutcome::BelowThreshold { .. }),
            "Override should force the 0.99 gate on, blocking a 0.90 split"
        );
    }

    #[test]
    fn test_override_forces_sensor_continuous_present_off() {
        // What-if: "how would this replay if the gate weren't applied?"
        let check = make_check_with_continuous_present(0.90, 1000, 0, 600.0, true);
        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        let overrides = Overrides {
            sensor_continuous_present: Some(false),
            ..Default::default()
        };
        let ctx = build_eval_context(&check, &config, &overrides);
        assert!(!ctx.sensor_continuous_present);
        let (outcome, _) = evaluate_detection(&ctx);
        assert!(
            matches!(outcome, DetectionOutcome::Split { .. }),
            "Override should force the gate off, allowing a 0.90 split"
        );
    }

    #[test]
    fn test_v1_bundle_backward_compat() {
        // A schema v1 bundle (pre-v2) has no sensor_continuous_present field.
        // Serde's #[serde(default)] makes it deserialize as false, which
        // matches pre-drift-fix CLI behavior. This test documents that
        // expectation so future schema changes notice if they break it.
        let v1_bundle_json = r#"{
            "schema_version": 1,
            "config": {"encounter_detection_mode": "hybrid"},
            "segments": [],
            "sensor_transitions": [],
            "vision_results": [],
            "detection_checks": [{
                "ts": "2026-03-12T10:00:00Z",
                "segment_range": [0, 100],
                "word_count": 1000,
                "cleaned_word_count": 1000,
                "sensor_context": {"departed": false, "present": false, "unknown": true},
                "prompt_system": "",
                "prompt_user": "",
                "response_raw": null,
                "parsed_complete": true,
                "parsed_confidence": 0.95,
                "parsed_end_index": 50,
                "latency_ms": 500,
                "success": true,
                "error": null,
                "loop_state": {
                    "consecutive_failures": 0,
                    "merge_back_count": 0,
                    "buffer_age_secs": 600.0
                }
            }]
        }"#;
        let bundle: ReplayBundle = serde_json::from_str(v1_bundle_json)
            .expect("v1 bundle should deserialize via serde defaults");
        let check = &bundle.detection_checks[0];
        assert!(!check.loop_state.sensor_continuous_present);
        assert!(!check.loop_state.sensor_triggered);
        assert!(!check.loop_state.manual_triggered);
    }

    #[test]
    fn test_should_skip_precheck_uses_captured_sensor_triggered() {
        // v2+ bundle: sensor_triggered=true captured directly from production.
        // Word count below MIN_SENSOR_HYBRID_WORDS → should skip LLM call.
        let mut check = DetectionCheck::new(
            (0, 100),
            120, // below threshold
            120,
            SensorContext::new(false, true), // sensor present per prompt context
            String::new(),
            String::new(),
            500,
            0,
            0,
            600.0,
            None,
            false,
            true, // sensor_triggered (v2+ captured flag)
            false,
        );
        check.success = true;
        check.parsed_complete = Some(true);
        check.parsed_confidence = Some(0.95);
        check.parsed_end_index = Some(50);

        let config = serde_json::json!({"encounter_detection_mode": "hybrid"});
        assert!(
            should_skip_precheck(&check, &config, &Overrides::default()),
            "Should skip when sensor_triggered=true and word_count < MIN_SENSOR_HYBRID_WORDS"
        );
    }

    // ---- scanner picks up merged-away sibling files ----

    #[test]
    fn test_find_replay_bundles_includes_merged_siblings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let session = dir.path().join("2026").join("04").join("15").join("session-id");
        fs::create_dir_all(&session).unwrap();
        fs::write(session.join("replay_bundle.json"), "{}").unwrap();
        fs::write(session.join("replay_bundle.merged_abc12345.json"), "{}").unwrap();
        fs::write(session.join("replay_bundle.merged_def67890.json"), "{}").unwrap();
        fs::write(session.join("metadata.json"), "{}").unwrap();
        fs::write(session.join("transcript.txt"), "").unwrap();

        let bundles = find_replay_bundles(dir.path());
        assert_eq!(bundles.len(), 3, "should find canonical + 2 merged siblings");
        let names: Vec<String> = bundles
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
            .collect();
        assert!(names.contains(&"replay_bundle.json".to_string()));
        assert!(names.iter().any(|n| n == "replay_bundle.merged_abc12345.json"));
        assert!(names.iter().any(|n| n == "replay_bundle.merged_def67890.json"));
        // Ensure unrelated files are not picked up
        assert!(!names.contains(&"metadata.json".to_string()));
        assert!(!names.contains(&"transcript.txt".to_string()));
    }
}
