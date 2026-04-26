//! Merge-check replay CLI: re-issues archived merge-check prompts through the
//! current LLM Router and compares parsed results to what was archived.
//!
//! Catches regressions in:
//!   - Merge-check prompt template changes
//!   - LLM model changes
//!   - parse_merge_check() output handling
//!
//! Unlike `detection_replay_cli` (pure-function replay), this requires a live
//! LLM Router because the merge check is a real LLM call. To account for
//! non-determinism, run with `--trials N` to retry failures.
//!
//! Usage:
//!   cargo run --bin merge_replay_cli -- --all
//!   cargo run --bin merge_replay_cli -- ~/.transcriptionapp/archive/2026/04/15/
//!   cargo run --bin merge_replay_cli -- --all --trials 3 --fail-on-mismatch --threshold 80.0

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use transcription_app_lib::config::Config;
use transcription_app_lib::encounter_merge::parse_merge_check;
use transcription_app_lib::llm_client::LLMClient;
use transcription_app_lib::local_archive;
use transcription_app_lib::replay_bundle::{find_replay_bundles, ReplayBundle};
use transcription_app_lib::replay_fetch::ArchiveFetcher;

const DEFAULT_THRESHOLD: f64 = 75.0; // LLM non-determinism: ~40% flip rate per docs
const DEFAULT_TRIALS: u32 = 1;

fn print_usage(program: &str) {
    eprintln!("Usage: {} [PATH | --all] [OPTIONS]", program);
    eprintln!();
    eprintln!("Replay archived merge-check LLM calls and verify the current prompt+model");
    eprintln!("produces matching results.");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  PATH                Search this directory for replay_bundle.json files");
    eprintln!("  --all               Replay all bundles in ~/.transcriptionapp/archive/");
    eprintln!("  --date YYYY-MM-DD   Replay every session on that date (server fallback)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --trials N          Run each merge-check N times, take majority vote (default: 1)");
    eprintln!("  --fail-on-mismatch  Exit non-zero if agreement drops below threshold");
    eprintln!("  --threshold PCT     Set the agreement threshold (default: 75.0)");
    eprintln!("  --mismatches        Only show bundles where the replayed result differs");
    eprintln!("  --model NAME        Override the merge-check model (default: from config)");
    eprintln!("  --help              Show this help");
}

#[derive(Debug)]
struct ReplayResult {
    display: String,
    archived_decision: bool,
    replayed_decisions: Vec<bool>,
    majority_decision: Option<bool>,
    agree: bool,
    error: Option<String>,
}

#[tokio::main]
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

    let mut archive_path: Option<PathBuf> = None;
    let mut date_arg: Option<String> = None;
    let mut all_archives = false;
    let mut trials: u32 = DEFAULT_TRIALS;
    let mut fail_on_mismatch = false;
    let mut threshold_pct = DEFAULT_THRESHOLD;
    let mut mismatches_only = false;
    let mut model_override: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => all_archives = true,
            "--date" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --date requires a YYYY-MM-DD value");
                    return ExitCode::from(1);
                }
                date_arg = Some(args[i].clone());
            }
            "--mismatches" => mismatches_only = true,
            "--fail-on-mismatch" => fail_on_mismatch = true,
            "--trials" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --trials requires a number");
                    return ExitCode::from(1);
                }
                trials = args[i].parse().expect("Invalid trials count");
            }
            "--threshold" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --threshold requires a percentage");
                    return ExitCode::from(1);
                }
                threshold_pct = args[i].parse().expect("Invalid threshold");
            }
            "--model" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --model requires a name");
                    return ExitCode::from(1);
                }
                model_override = Some(args[i].clone());
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

    // Resolve sources to (display, ReplayBundle).
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
            Err(e) => {
                eprintln!("Error: list bundles for {}: {}", date, e);
                return ExitCode::from(1);
            }
        }
    } else {
        let search_path = if all_archives {
            local_archive::get_archive_dir().expect("Could not determine archive directory")
        } else if let Some(ref path) = archive_path {
            path.clone()
        } else {
            eprintln!("Error: provide an archive path, --all, or --date YYYY-MM-DD");
            return ExitCode::from(1);
        };
        if !search_path.exists() {
            eprintln!("Path does not exist: {}", search_path.display());
            return ExitCode::from(1);
        }
        let bundle_paths = find_replay_bundles(&search_path);
        if bundle_paths.is_empty() {
            eprintln!("No replay_bundle.json files found");
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
    };

    // Build LLM client from config
    let config = Config::load_or_default();
    let model = model_override.unwrap_or_else(|| config.fast_model.clone());
    let client = match LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &config.fast_model,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create LLM client: {e}");
            return ExitCode::from(1);
        }
    };

    eprintln!("Merge-check replay (model={}, trials={}, bundles={})",
        model, trials, sources.len());
    eprintln!();

    let mut results: Vec<ReplayResult> = Vec::new();
    let mut bundles_with_merge_check = 0;

    for (display, bundle) in sources {
        let merge_check = match bundle.merge_check {
            Some(m) => m,
            None => continue, // skip bundles without a merge check
        };
        let archived = match merge_check.parsed_same_encounter {
            Some(d) => d,
            None => continue, // skip if archived parse failed
        };

        bundles_with_merge_check += 1;

        // Re-issue the merge check N times
        let mut replayed: Vec<bool> = Vec::new();
        let mut last_error: Option<String> = None;
        for _ in 0..trials {
            match client
                .generate(&model, &merge_check.prompt_system, &merge_check.prompt_user, "merge_check_replay")
                .await
            {
                Ok(response) => match parse_merge_check(&response) {
                    Ok(parsed) => replayed.push(parsed.same_encounter),
                    Err(e) => last_error = Some(format!("parse: {e}")),
                },
                Err(e) => last_error = Some(format!("LLM: {e}")),
            }
        }

        let majority = if replayed.is_empty() {
            None
        } else {
            let true_count = replayed.iter().filter(|d| **d).count();
            Some(true_count * 2 >= replayed.len())
        };
        let agree = majority == Some(archived);

        results.push(ReplayResult {
            display: display.clone(),
            archived_decision: archived,
            replayed_decisions: replayed,
            majority_decision: majority,
            agree,
            error: last_error,
        });
    }

    // Print results
    let mut matches = 0;
    let mut mismatches = 0;
    for r in &results {
        if r.agree {
            matches += 1;
        } else {
            mismatches += 1;
        }
        if mismatches_only && r.agree {
            continue;
        }
        let status = if r.agree { "MATCH" } else { "MISMATCH" };
        let majority_str = match r.majority_decision {
            Some(true) => "true",
            Some(false) => "false",
            None => "ERROR",
        };
        let trial_str: Vec<String> = r.replayed_decisions.iter().map(|d| d.to_string()).collect();
        println!(
            "Bundle: {} [{}] archived={}, majority={}, trials=[{}]",
            r.display, status, r.archived_decision, majority_str, trial_str.join(",")
        );
        if let Some(ref e) = r.error {
            println!("  last_error: {}", e);
        }
    }

    println!();
    println!("────────────────────────────────────────────");
    println!(
        "Bundles with merge_check: {}  Match: {}  Mismatch: {}",
        bundles_with_merge_check, matches, mismatches
    );
    let agreement = if bundles_with_merge_check > 0 {
        let pct = matches as f64 / bundles_with_merge_check as f64 * 100.0;
        println!("Agreement: {:.1}%", pct);
        pct
    } else {
        100.0
    };

    if fail_on_mismatch && agreement < threshold_pct {
        eprintln!();
        eprintln!(
            "REGRESSION: agreement {:.1}% is below threshold {:.1}%",
            agreement, threshold_pct
        );
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}
