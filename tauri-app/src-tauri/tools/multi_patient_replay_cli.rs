//! Multi-patient detection replay CLI: re-issues archived multi-patient
//! detection prompts and verifies the patient_count classification matches.
//!
//! Multi-patient detection runs at up to three stages per encounter:
//!   - PreSoap: before generating SOAP, to decide single vs multi-patient SOAP
//!   - Retrospective: after a merge-back to detect couples/family visits incorrectly merged
//!   - Standalone: safety-net check on the final encounter
//!
//! The captured prompt + response is in `bundle.multi_patient_detections[]`.
//! This tool re-issues each prompt and compares the patient count.
//!
//! Usage:
//!   cargo run --bin multi_patient_replay_cli -- --all
//!   cargo run --bin multi_patient_replay_cli -- --all --trials 3 --fail-on-mismatch --threshold 80.0
//!   cargo run --bin multi_patient_replay_cli -- --all --stage retrospective

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use transcription_app_lib::config::Config;
use transcription_app_lib::encounter_detection::parse_multi_patient_detection;
use transcription_app_lib::llm_client::LLMClient;
use transcription_app_lib::local_archive;
use transcription_app_lib::replay_bundle::{find_replay_bundles, MultiPatientStage, ReplayBundle};
use transcription_app_lib::replay_fetch::ArchiveFetcher;

const DEFAULT_THRESHOLD: f64 = 80.0;
const DEFAULT_TRIALS: u32 = 1;

fn print_usage(program: &str) {
    eprintln!("Usage: {} [PATH | --all] [OPTIONS]", program);
    eprintln!();
    eprintln!("Replay archived multi-patient detection LLM calls.");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --all               Replay all bundles in ~/.transcriptionapp/archive/");
    eprintln!("  --date YYYY-MM-DD   Replay every session on that date (server fallback)");
    eprintln!("  --trials N          Run each detection N times, take majority (default: 1)");
    eprintln!("  --fail-on-mismatch  Exit non-zero if agreement drops below threshold");
    eprintln!("  --threshold PCT     Set the agreement threshold (default: 80.0)");
    eprintln!("  --mismatches        Only show mismatches");
    eprintln!("  --stage STAGE       Filter to one stage: pre_soap | retrospective | standalone");
    eprintln!("  --model NAME        Override the model (default: from config)");
    eprintln!("  --help              Show this help");
}

fn parse_stage_filter(s: &str) -> Option<MultiPatientStage> {
    match s {
        "pre_soap" => Some(MultiPatientStage::PreSoap),
        "retrospective" => Some(MultiPatientStage::Retrospective),
        "standalone" => Some(MultiPatientStage::Standalone),
        _ => None,
    }
}

fn count_threshold(archived: u32, replayed: u32) -> bool {
    // Treat "single patient" (1) and "multi-patient" (≥2) as the load-bearing distinction.
    // Exact count differences within multi-patient are noise.
    (archived <= 1) == (replayed <= 1)
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
    let mut fail_on_mismatch = false;
    let mut threshold_pct = DEFAULT_THRESHOLD;
    let mut mismatches_only = false;
    let mut stage_filter: Option<MultiPatientStage> = None;
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
            "--trials" => {
                i += 1;
                trials = args[i].parse().expect("Invalid trials");
            }
            "--threshold" => {
                i += 1;
                threshold_pct = args[i].parse().expect("Invalid threshold");
            }
            "--stage" => {
                i += 1;
                stage_filter = parse_stage_filter(&args[i]);
                if stage_filter.is_none() {
                    eprintln!("Invalid stage: {}. Use pre_soap | retrospective | standalone", args[i]);
                    return ExitCode::from(1);
                }
            }
            "--model" => {
                i += 1;
                model_override = Some(args[i].clone());
            }
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

    eprintln!("Multi-patient replay (model={}, trials={}, bundles={})",
        model, trials, sources.len());
    if let Some(ref s) = stage_filter {
        eprintln!("Stage filter: {:?}", s);
    }
    eprintln!();

    let mut total = 0;
    let mut matches = 0;
    let mut mismatches = 0;

    for (display, bundle) in &sources {
        for mp in &bundle.multi_patient_detections {
            // Apply stage filter if present
            if let Some(ref s) = stage_filter {
                if *s != mp.stage {
                    continue;
                }
            }
            let archived_count = match mp.parsed_patient_count {
                Some(c) => c,
                None => continue,
            };

            let mut votes: Vec<u32> = Vec::new();
            for _ in 0..trials {
                match client.generate(&model, &mp.system_prompt, &mp.user_prompt, "multi_patient_replay").await {
                    Ok(response) => match parse_multi_patient_detection(&response) {
                        Ok(parsed) => votes.push(parsed.patient_count),
                        Err(e) => eprintln!("  parse error: {e}"),
                    },
                    Err(e) => eprintln!("  LLM error: {e}"),
                }
            }

            // Take median (or majority on the multi/single boundary)
            let majority = if votes.is_empty() {
                None
            } else {
                let mut sorted = votes.clone();
                sorted.sort();
                Some(sorted[sorted.len() / 2])
            };
            let agree = majority.map_or(false, |m| count_threshold(archived_count, m));

            total += 1;
            if agree { matches += 1; } else { mismatches += 1; }

            if mismatches_only && agree { continue; }
            let status = if agree { "MATCH" } else { "MISMATCH" };
            let majority_str = match majority {
                Some(m) => m.to_string(),
                None => "ERROR".to_string(),
            };
            let trial_str: Vec<String> = votes.iter().map(|d| d.to_string()).collect();
            println!(
                "Bundle: {} stage={:?} [{}] archived={}, majority={}, trials=[{}]",
                display, mp.stage, status, archived_count, majority_str, trial_str.join(",")
            );
        }
    }

    println!();
    println!("────────────────────────────────────────────");
    println!("Detections checked: {}  Match: {}  Mismatch: {}", total, matches, mismatches);
    let agreement = if total > 0 {
        let pct = matches as f64 / total as f64 * 100.0;
        println!("Agreement (single vs multi): {:.1}%", pct);
        pct
    } else {
        100.0
    };

    if fail_on_mismatch && agreement < threshold_pct {
        eprintln!();
        eprintln!("REGRESSION: agreement {:.1}% is below threshold {:.1}%", agreement, threshold_pct);
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}
