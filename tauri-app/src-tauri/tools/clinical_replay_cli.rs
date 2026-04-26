//! Clinical content check replay CLI: re-issues archived clinical-check prompts
//! through the current LLM Router and compares parsed results to what was archived.
//!
//! Catches regressions in:
//!   - Clinical content check prompt template changes
//!   - LLM model changes
//!   - parse_clinical_content_check() output handling
//!
//! Usage:
//!   cargo run --bin clinical_replay_cli -- --all
//!   cargo run --bin clinical_replay_cli -- --all --trials 3 --fail-on-mismatch --threshold 90.0

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use transcription_app_lib::config::Config;
use transcription_app_lib::encounter_detection::{
    build_clinical_content_check_prompt, parse_clinical_content_check,
};
use transcription_app_lib::llm_client::LLMClient;
use transcription_app_lib::local_archive;
use transcription_app_lib::replay_bundle::{find_replay_bundles, ReplayBundle};
use transcription_app_lib::replay_fetch::ArchiveFetcher;

const DEFAULT_THRESHOLD: f64 = 90.0;
const DEFAULT_TRIALS: u32 = 1;

fn print_usage(program: &str) {
    eprintln!("Usage: {} [PATH | --all] [OPTIONS]", program);
    eprintln!();
    eprintln!("Replay archived clinical-check decisions and verify the current prompt+model");
    eprintln!("agree with what was archived. The current bundles only contain the BOOLEAN");
    eprintln!("outcome (is_clinical), not the full prompt — so we re-build the prompt from");
    eprintln!("the bundle's transcript and compare classification.");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  PATH                Search this directory for replay_bundle.json files");
    eprintln!("  --all               Replay all bundles in ~/.transcriptionapp/archive/");
    eprintln!("  --date YYYY-MM-DD   Replay every session on that date (server fallback)");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --trials N          Run each check N times, take majority vote (default: 1)");
    eprintln!("  --fail-on-mismatch  Exit non-zero if agreement drops below threshold");
    eprintln!("  --threshold PCT     Set the agreement threshold (default: 90.0)");
    eprintln!("  --mismatches        Only show bundles where the replayed result differs");
    eprintln!("  --model NAME        Override the model (default: from config)");
    eprintln!("  --help              Show this help");
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
                if i >= args.len() { eprintln!("--date needs a YYYY-MM-DD value"); return ExitCode::from(1); }
                date_arg = Some(args[i].clone());
            }
            "--mismatches" => mismatches_only = true,
            "--fail-on-mismatch" => fail_on_mismatch = true,
            "--trials" => {
                i += 1;
                if i >= args.len() { eprintln!("--trials needs a value"); return ExitCode::from(1); }
                trials = args[i].parse().expect("Invalid trials");
            }
            "--threshold" => {
                i += 1;
                if i >= args.len() { eprintln!("--threshold needs a value"); return ExitCode::from(1); }
                threshold_pct = args[i].parse().expect("Invalid threshold");
            }
            "--model" => {
                i += 1;
                if i >= args.len() { eprintln!("--model needs a value"); return ExitCode::from(1); }
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
            local_archive::get_archive_dir().expect("Could not determine archive directory")
        } else if let Some(ref path) = archive_path {
            path.clone()
        } else {
            eprintln!("Error: provide an archive path, --all, or --date YYYY-MM-DD");
            return ExitCode::from(1);
        };
        let bundle_paths = find_replay_bundles(&search_path);
        if bundle_paths.is_empty() {
            eprintln!("No replay bundles found");
            return ExitCode::SUCCESS;
        }
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

    eprintln!("Clinical-check replay (model={}, trials={}, bundles={})",
        model, trials, sources.len());
    eprintln!();

    let mut total = 0;
    let mut matches = 0;
    let mut mismatches = 0;

    for (display, bundle) in sources {
        let archived_clinical = match bundle.clinical_check.as_ref() {
            Some(c) if c.success => c.is_clinical,
            _ => continue, // skip bundles without a clinical check or where it failed
        };

        // Reconstruct the transcript from segments to feed into the same prompt
        let transcript: String = bundle
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if transcript.split_whitespace().count() < 100 {
            // Production skips clinical check below MIN_WORDS_FOR_CLINICAL_CHECK
            continue;
        }

        let (system, user) = build_clinical_content_check_prompt(&transcript, None);

        let mut votes: Vec<bool> = Vec::new();
        for _ in 0..trials {
            match client.generate(&model, &system, &user, "clinical_replay").await {
                Ok(response) => match parse_clinical_content_check(&response) {
                    Ok(parsed) => votes.push(parsed.clinical),
                    Err(e) => eprintln!("  parse error: {e}"),
                },
                Err(e) => eprintln!("  LLM error: {e}"),
            }
        }

        let majority = if votes.is_empty() {
            None
        } else {
            let true_count = votes.iter().filter(|d| **d).count();
            Some(true_count * 2 >= votes.len())
        };
        let agree = majority == Some(archived_clinical);

        total += 1;
        if agree { matches += 1; } else { mismatches += 1; }

        if mismatches_only && agree { continue; }
        let status = if agree { "MATCH" } else { "MISMATCH" };
        let majority_str = match majority {
            Some(true) => "true",
            Some(false) => "false",
            None => "ERROR",
        };
        let trial_str: Vec<String> = votes.iter().map(|d| d.to_string()).collect();
        println!(
            "Bundle: {} [{}] archived={}, majority={}, trials=[{}]",
            display, status, archived_clinical, majority_str, trial_str.join(",")
        );
    }

    println!();
    println!("────────────────────────────────────────────");
    println!("Bundles checked: {}  Match: {}  Mismatch: {}", total, matches, mismatches);
    let agreement = if total > 0 {
        let pct = matches as f64 / total as f64 * 100.0;
        println!("Agreement: {:.1}%", pct);
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
