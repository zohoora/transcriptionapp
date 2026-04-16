//! Benchmark runner: loads test cases from `tests/fixtures/benchmarks/*.json`
//! and runs them against the live LLM Router, asserting documented accuracy targets.
//!
//! This is the assertion-based companion to `docs/benchmarks/*.md`.
//! Each fixture file declares a task, model, accuracy targets, and test cases
//! with expected outputs. The runner issues fresh LLM calls (with --trials for
//! non-determinism handling) and checks pass/fail against the targets.
//!
//! Usage:
//!   cargo run --bin benchmark_runner -- clinical_content_check
//!   cargo run --bin benchmark_runner -- --all --trials 3 --fail-on-regression

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

use transcription_app_lib::config::Config;
use transcription_app_lib::encounter_detection::parse_clinical_content_check;
use transcription_app_lib::llm_client::LLMClient;

#[derive(Debug, Deserialize)]
struct BenchmarkFile {
    task: String,
    model: String,
    targets: Targets,
    test_cases: Vec<TestCase>,
}

#[derive(Debug, Deserialize)]
struct Targets {
    #[serde(default)]
    overall_accuracy_pct: Option<f64>,
    #[serde(default)]
    clinical_recall_pct: Option<f64>,
    #[serde(default)]
    non_clinical_precision_pct: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct TestCase {
    id: String,
    name: String,
    difficulty: String,
    input: String,
    #[serde(default)]
    expected_clinical: Option<bool>,
}

fn print_usage(program: &str) {
    eprintln!("Usage: {} [TASK_NAME | --all] [OPTIONS]", program);
    eprintln!();
    eprintln!("Run benchmark test cases from tests/fixtures/benchmarks/*.json");
    eprintln!("against the live LLM Router and assert accuracy targets.");
    eprintln!();
    eprintln!("Tasks (matched by filename minus .json):");
    eprintln!("  clinical_content_check");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --all                 Run all benchmark files");
    eprintln!("  --trials N            Run each test case N times for non-determinism (default: 1)");
    eprintln!("  --fail-on-regression  Exit non-zero if any target is not met");
    eprintln!("  --help                Show this help");
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("benchmarks")
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

    let mut task_filter: Option<String> = None;
    let mut all = false;
    let mut trials: u32 = 1;
    let mut fail_on_regression = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--all" => all = true,
            "--trials" => {
                i += 1;
                trials = args[i].parse().expect("Invalid trials count");
            }
            "--fail-on-regression" => fail_on_regression = true,
            "--help" => {
                print_usage(program);
                return ExitCode::SUCCESS;
            }
            other => {
                if other.starts_with('-') {
                    eprintln!("Unknown option: {}", other);
                    return ExitCode::from(1);
                }
                task_filter = Some(other.to_string());
            }
        }
        i += 1;
    }

    // Discover benchmark files
    let dir = fixtures_dir();
    if !dir.exists() {
        eprintln!("Benchmark fixtures directory not found: {}", dir.display());
        return ExitCode::from(1);
    }
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if all || task_filter.as_deref() == Some(stem) {
                    files.push(path);
                }
            }
        }
    }
    files.sort();

    if files.is_empty() {
        eprintln!("No benchmark files matched");
        return ExitCode::from(1);
    }

    let config = Config::load_or_default();
    let mut any_regression = false;

    for file in &files {
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to read {}: {e}", file.display());
                continue;
            }
        };
        let bench: BenchmarkFile = match serde_json::from_str(&content) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Failed to parse {}: {e}", file.display());
                continue;
            }
        };

        println!("\n=== Benchmark: {} (model={}) ===", bench.task, bench.model);

        let client = match LLMClient::new(
            &config.llm_router_url,
            &config.llm_api_key,
            &config.llm_client_id,
            &config.fast_model,
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("LLM client init failed: {e}");
                return ExitCode::from(1);
            }
        };

        let regression = match bench.task.as_str() {
            "clinical_content_check" => {
                run_clinical_content_check(&client, &bench, trials).await
            }
            other => {
                eprintln!("Task not yet implemented in runner: {other}");
                false
            }
        };
        if regression {
            any_regression = true;
        }
    }

    if fail_on_regression && any_regression {
        eprintln!("\nREGRESSION: one or more accuracy targets not met");
        return ExitCode::from(2);
    }

    ExitCode::SUCCESS
}

async fn run_clinical_content_check(
    client: &LLMClient,
    bench: &BenchmarkFile,
    trials: u32,
) -> bool {
    use transcription_app_lib::encounter_detection::build_clinical_content_check_prompt;

    let mut total = 0;
    let mut correct = 0;
    let mut clinical_total = 0;
    let mut clinical_correct = 0;
    let mut non_clinical_predicted = 0;
    let mut non_clinical_correct = 0;

    for tc in &bench.test_cases {
        let expected = match tc.expected_clinical {
            Some(e) => e,
            None => continue,
        };
        let (system, user) = build_clinical_content_check_prompt(&tc.input, None);

        // Run trials, take majority
        let mut votes: Vec<bool> = Vec::new();
        for _ in 0..trials {
            match client
                .generate(&bench.model, &system, &user, "clinical_check_bench")
                .await
            {
                Ok(response) => match parse_clinical_content_check(&response) {
                    Ok(parsed) => votes.push(parsed.clinical),
                    Err(e) => eprintln!("  {} parse error: {e}", tc.id),
                },
                Err(e) => eprintln!("  {} LLM error: {e}", tc.id),
            }
        }
        let predicted = if votes.is_empty() {
            None
        } else {
            let clinical_votes = votes.iter().filter(|v| **v).count();
            Some(clinical_votes * 2 >= votes.len())
        };

        total += 1;
        let pass = predicted == Some(expected);
        if pass {
            correct += 1;
        }

        // Track clinical recall (TPR) and non-clinical precision
        if expected {
            clinical_total += 1;
            if predicted == Some(true) {
                clinical_correct += 1;
            }
        }
        if predicted == Some(false) {
            non_clinical_predicted += 1;
            if !expected {
                non_clinical_correct += 1;
            }
        }

        let symbol = if pass { "✓" } else { "✗" };
        let pred_str = match predicted {
            Some(true) => "true",
            Some(false) => "false",
            None => "ERROR",
        };
        println!(
            "  {} {} ({}): expected={}, predicted={} [{} trials]",
            symbol, tc.id, tc.difficulty, expected, pred_str, votes.len()
        );
    }

    let overall = correct as f64 / total.max(1) as f64 * 100.0;
    let recall = if clinical_total > 0 {
        clinical_correct as f64 / clinical_total as f64 * 100.0
    } else {
        100.0
    };
    let precision = if non_clinical_predicted > 0 {
        non_clinical_correct as f64 / non_clinical_predicted as f64 * 100.0
    } else {
        100.0
    };

    println!();
    println!("  Overall accuracy: {:.1}% ({}/{})", overall, correct, total);
    println!("  Clinical recall: {:.1}% ({}/{})", recall, clinical_correct, clinical_total);
    println!("  Non-clinical precision: {:.1}% ({}/{})", precision, non_clinical_correct, non_clinical_predicted);

    let mut regression = false;
    if let Some(target) = bench.targets.overall_accuracy_pct {
        if overall < target {
            println!("  ✗ overall_accuracy {:.1}% < target {:.1}%", overall, target);
            regression = true;
        }
    }
    if let Some(target) = bench.targets.clinical_recall_pct {
        if recall < target {
            println!("  ✗ clinical_recall {:.1}% < target {:.1}%", recall, target);
            regression = true;
        }
    }
    if let Some(target) = bench.targets.non_clinical_precision_pct {
        if precision < target {
            println!("  ✗ non_clinical_precision {:.1}% < target {:.1}%", precision, target);
            regression = true;
        }
    }
    regression
}
