//! Multi-patient SPLIT replay CLI: re-issues archived split-point prompts and
//! verifies the line_index decision matches.
//!
//! Per benchmark spec, line accuracy is fuzzy (±2 lines = correct, ±5 = acceptable).
//! Two modes:
//!   - **Captured replay** (v3+ bundles): re-issue the exact captured prompt,
//!     compare line_index within tolerance. **Note (2026-04):** production
//!     does not yet wire the multi-patient SPLIT prompt through
//!     `MultiPatientDetection.split_decision`. Until that's added in
//!     `continuous_mode.rs`, captured-mode runs will report "no captured split"
//!     for every bundle. Use `--synthetic` for sanity checks until then.
//!   - **Synthetic replay** (any bundle with multi_patient_detection ≥ 2):
//!     build a split prompt from the segments and ask the LLM. No ground truth
//!     comparison — just a sanity check that the split prompt produces a valid
//!     line_index. Works with v1/v2 bundles too.
//!
//! Usage:
//!   cargo run --bin multi_patient_split_replay_cli -- --all
//!   cargo run --bin multi_patient_split_replay_cli -- --all --tolerance 2 --fail-on-mismatch --threshold 70.0
//!   cargo run --bin multi_patient_split_replay_cli -- --all --synthetic   # use synthetic mode

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use transcription_app_lib::config::Config;
use transcription_app_lib::encounter_detection::{
    multi_patient_split_prompt, parse_multi_patient_split,
};
use transcription_app_lib::llm_client::LLMClient;
use transcription_app_lib::local_archive;
use transcription_app_lib::replay_bundle::{find_replay_bundles, ReplayBundle};
use transcription_app_lib::replay_fetch::ArchiveFetcher;

const DEFAULT_THRESHOLD: f64 = 70.0;
const DEFAULT_TRIALS: u32 = 1;
const DEFAULT_TOLERANCE: i64 = 2; // ±2 lines per benchmark spec

fn print_usage(program: &str) {
    eprintln!("Usage: {} [PATH | --all] [OPTIONS]", program);
    eprintln!();
    eprintln!("Replay archived multi-patient SPLIT decisions (line_index boundary).");
    eprintln!();
    eprintln!("Modes:");
    eprintln!("  captured (default): use bundles' captured split_decision (schema v3+)");
    eprintln!("  --synthetic:        build new split prompts from segments (any bundle)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --all                 Replay all bundles in ~/.transcriptionapp/archive/");
    eprintln!("  --date YYYY-MM-DD     Replay every session on that date (server fallback)");
    eprintln!("  --trials N            Run each split N times, take median (default: 1)");
    eprintln!("  --tolerance N         Lines within ±N count as correct (default: 2)");
    eprintln!("  --fail-on-mismatch    Exit non-zero if agreement drops below threshold");
    eprintln!("  --threshold PCT       Agreement threshold (default: 70.0)");
    eprintln!("  --mismatches          Only show mismatches");
    eprintln!("  --synthetic           Synthetic mode (no ground truth)");
    eprintln!("  --model NAME          Override the model");
    eprintln!("  --help                Show this help");
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let program = &args[0];

    if args.len() < 2 || args.contains(&"--help".to_string()) {
        print_usage(program);
        return if args.contains(&"--help".to_string()) { ExitCode::SUCCESS } else { ExitCode::from(1) };
    }

    let mut archive_path: Option<PathBuf> = None;
    let mut date_arg: Option<String> = None;
    let mut all_archives = false;
    let mut trials: u32 = DEFAULT_TRIALS;
    let mut tolerance: i64 = DEFAULT_TOLERANCE;
    let mut fail_on_mismatch = false;
    let mut threshold_pct = DEFAULT_THRESHOLD;
    let mut mismatches_only = false;
    let mut synthetic = false;
    let mut model_override: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => all_archives = true,
            "--date" => {
                i += 1;
                if i >= args.len() { eprintln!("--date needs a YYYY-MM-DD value"); return ExitCode::from(1); }
                date_arg = Some(args[i].clone());
            }
            "--mismatches" => mismatches_only = true,
            "--fail-on-mismatch" => fail_on_mismatch = true,
            "--synthetic" => synthetic = true,
            "--trials" => { i += 1; trials = args[i].parse().expect("Invalid trials"); }
            "--tolerance" => { i += 1; tolerance = args[i].parse().expect("Invalid tolerance"); }
            "--threshold" => { i += 1; threshold_pct = args[i].parse().expect("Invalid threshold"); }
            "--model" => { i += 1; model_override = Some(args[i].clone()); }
            "--help" => { print_usage(program); return ExitCode::SUCCESS; }
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

    let sources: Vec<(String, ReplayBundle)> = if let Some(date) = date_arg {
        let fetcher = ArchiveFetcher::from_env().unwrap_or_else(|e| {
            eprintln!("warn: ArchiveFetcher init failed ({e}); falling back to local-only");
            ArchiveFetcher::local_only()
        });
        match fetcher.list_replay_bundles_for_date(&date).await {
            Ok(b) if b.is_empty() => {
                eprintln!("No replay_bundle.json found for {} (local or server)", date);
                return ExitCode::SUCCESS;
            }
            Ok(b) => b,
            Err(e) => { eprintln!("Error: list bundles for {}: {}", date, e); return ExitCode::from(1); }
        }
    } else {
        let search_path = if all_archives {
            local_archive::get_archive_dir().expect("Could not determine archive dir")
        } else if let Some(ref path) = archive_path {
            path.clone()
        } else {
            eprintln!("Provide PATH, --all, or --date YYYY-MM-DD");
            return ExitCode::from(1);
        };
        let bundle_paths = find_replay_bundles(&search_path);
        let mut out: Vec<(String, ReplayBundle)> = Vec::new();
        for bundle_path in &bundle_paths {
            let content = match fs::read_to_string(bundle_path) { Ok(c) => c, Err(_) => continue };
            let bundle: ReplayBundle = match serde_json::from_str(&content) { Ok(b) => b, Err(_) => continue };
            let display = bundle_path.strip_prefix(&search_path).unwrap_or(bundle_path).display().to_string();
            out.push((display, bundle));
        }
        out
    };

    let config = Config::load_or_default();
    let model = model_override.unwrap_or_else(|| config.fast_model.clone());
    let client = match LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &config.fast_model,
    ) {
        Ok(c) => c,
        Err(e) => { eprintln!("LLM init failed: {e}"); return ExitCode::from(1); }
    };

    let mode = if synthetic { "synthetic" } else { "captured" };
    eprintln!("Multi-patient SPLIT replay (mode={}, model={}, tolerance=±{}, bundles={})",
        mode, model, tolerance, sources.len());
    eprintln!();

    let mut total = 0;
    let mut matches = 0;
    let mut mismatches = 0;
    let mut synthetic_runs = 0;
    let mut bundles_skipped_no_split = 0;

    for (display, bundle) in &sources {
        // Find a multi-patient detection that found ≥2 patients
        let detection = bundle.multi_patient_detections.iter()
            .find(|mp| mp.parsed_patient_count.unwrap_or(0) >= 2);
        let detection = match detection {
            Some(d) => d,
            None => { bundles_skipped_no_split += 1; continue; }
        };

        if synthetic {
            // Synthetic: build a split prompt from the segments and ask the LLM.
            // No ground truth — just verify the split prompt produces a valid line_index.
            if bundle.segments.is_empty() { continue; }
            let formatted: Vec<String> = bundle.segments.iter().enumerate()
                .map(|(i, s)| format!("[{}] {}", i, s.text))
                .collect();
            let formatted_text = formatted.join("\n");

            let system = multi_patient_split_prompt(None);
            let user = format!("Transcript:\n{}", formatted_text);

            let mut votes: Vec<u32> = Vec::new();
            for _ in 0..trials {
                match client.generate(&model, &system, &user, "multi_patient_split_synthetic").await {
                    Ok(response) => match parse_multi_patient_split(&response) {
                        Ok(parsed) => {
                            if let Some(idx) = parsed.line_index {
                                votes.push(idx as u32);
                            }
                        }
                        Err(e) => eprintln!("  parse error: {e}"),
                    },
                    Err(e) => eprintln!("  LLM error: {e}"),
                }
            }

            synthetic_runs += 1;
            let result = if votes.is_empty() { "no_boundary".to_string() } else {
                let mut sorted = votes.clone();
                sorted.sort();
                format!("line_index={}", sorted[sorted.len() / 2])
            };
            println!("Bundle: {} [SYNTHETIC] {} (segments={})", display, result, bundle.segments.len());
            continue;
        }

        // Captured mode: only works for v3+ bundles
        let archived_split = match &detection.split_decision {
            Some(s) => s,
            None => {
                bundles_skipped_no_split += 1;
                continue;
            }
        };
        let archived_line = match archived_split.parsed_line_index {
            Some(l) => l,
            None => continue,
        };

        let mut votes: Vec<u32> = Vec::new();
        for _ in 0..trials {
            match client.generate(&model, &archived_split.system_prompt, &archived_split.user_prompt, "multi_patient_split_replay").await {
                Ok(response) => match parse_multi_patient_split(&response) {
                    Ok(parsed) => {
                        if let Some(idx) = parsed.line_index {
                            votes.push(idx as u32);
                        }
                    }
                    Err(_) => {}
                },
                Err(_) => {}
            }
        }

        let median = if votes.is_empty() {
            None
        } else {
            let mut sorted = votes.clone();
            sorted.sort();
            Some(sorted[sorted.len() / 2])
        };
        let agree = median.map_or(false, |m| {
            let diff = (m as i64 - archived_line as i64).abs();
            diff <= tolerance
        });

        total += 1;
        if agree { matches += 1; } else { mismatches += 1; }

        if mismatches_only && agree { continue; }
        let status = if agree { "MATCH" } else { "MISMATCH" };
        let median_str = median.map(|m| m.to_string()).unwrap_or_else(|| "ERROR".to_string());
        let trial_str: Vec<String> = votes.iter().map(|d| d.to_string()).collect();
        println!(
            "Bundle: {} [{}] archived_line={}, median={}, trials=[{}]",
            display, status, archived_line, median_str, trial_str.join(",")
        );
    }

    println!();
    println!("────────────────────────────────────────────");
    if synthetic {
        println!("Synthetic runs: {}  Bundles without ≥2-patient detection: {}",
            synthetic_runs, bundles_skipped_no_split);
        // Synthetic mode doesn't have agreement to assert
    } else {
        println!("Splits checked: {}  Match: {}  Mismatch: {}  Bundles without captured split: {}",
            total, matches, mismatches, bundles_skipped_no_split);
        let agreement = if total > 0 {
            let pct = matches as f64 / total as f64 * 100.0;
            println!("Agreement (within ±{} lines): {:.1}%", tolerance, pct);
            pct
        } else {
            // No v3 bundles yet — emit guidance
            println!("(No v3 bundles with captured split_decision yet — run --synthetic for sanity check)");
            100.0
        };

        if fail_on_mismatch && agreement < threshold_pct {
            eprintln!();
            eprintln!("REGRESSION: agreement {:.1}% is below threshold {:.1}%", agreement, threshold_pct);
            return ExitCode::from(2);
        }
    }

    ExitCode::SUCCESS
}
