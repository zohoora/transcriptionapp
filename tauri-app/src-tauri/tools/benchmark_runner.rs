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
use transcription_app_lib::encounter_detection::{
    build_clinical_content_check_prompt, build_encounter_detection_prompt,
    multi_patient_split_prompt, parse_clinical_content_check, parse_encounter_detection,
    parse_multi_patient_detection, parse_multi_patient_split,
};
use transcription_app_lib::encounter_merge::{build_encounter_merge_prompt, parse_merge_check};
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
    #[serde(default)]
    true_positive_rate_pct: Option<f64>,
    #[serde(default)]
    false_positive_rate_pct_max: Option<f64>,
    #[serde(default)]
    true_same_encounter_recall_pct: Option<f64>,
    #[serde(default)]
    true_different_encounter_recall_pct: Option<f64>,
    #[serde(default)]
    multi_recall_pct: Option<f64>,
    #[serde(default)]
    single_specificity_pct: Option<f64>,
    #[serde(default)]
    exact_match_pct: Option<f64>,
    #[serde(default)]
    within_2_lines_pct: Option<f64>,
    #[serde(default)]
    within_5_lines_pct: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct TestCase {
    id: String,
    #[allow(dead_code)]
    name: String,
    difficulty: String,
    #[serde(default)]
    input: Option<String>,
    // Clinical content check
    #[serde(default)]
    expected_clinical: Option<bool>,
    // Encounter detection
    #[serde(default)]
    expected_complete: Option<bool>,
    #[serde(default)]
    expected_end_segment_index: Option<u64>,
    #[serde(default)]
    min_confidence: Option<f64>,
    // Encounter merge
    #[serde(default)]
    prev_tail: Option<String>,
    #[serde(default)]
    curr_head: Option<String>,
    #[serde(default)]
    patient_name: Option<String>,
    #[serde(default)]
    expected_same_encounter: Option<bool>,
    // Multi-patient detection
    #[serde(default)]
    expected_multiple_patients: Option<bool>,
    // Multi-patient split
    #[serde(default)]
    expected_line_index: Option<usize>,
    #[serde(default)]
    expected_line_index_acceptable: Option<Vec<usize>>,
    #[serde(default)]
    expected_no_boundary: Option<bool>,
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
            "clinical_content_check" => run_clinical_content_check(&client, &bench, trials).await,
            "encounter_detection" => run_encounter_detection(&client, &bench, trials).await,
            "encounter_merge" => run_encounter_merge(&client, &bench, trials).await,
            "multi_patient_detection" => run_multi_patient_detection(&client, &bench, trials).await,
            "multi_patient_split" => run_multi_patient_split(&client, &bench, trials).await,
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
        let input = match tc.input.as_ref() {
            Some(s) => s,
            None => continue,
        };
        let (system, user) = build_clinical_content_check_prompt(input, None);

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

async fn run_encounter_detection(
    client: &LLMClient,
    bench: &BenchmarkFile,
    trials: u32,
) -> bool {
    let mut total = 0;
    let mut correct = 0;
    let mut tp = 0; // true positive (correctly identified split)
    let mut fp = 0; // false positive (incorrectly split)
    let mut tn = 0;
    let mut fn_ = 0;

    for tc in &bench.test_cases {
        let expected_complete = match tc.expected_complete {
            Some(e) => e,
            None => continue,
        };
        let input = match tc.input.as_ref() {
            Some(s) => s,
            None => continue,
        };
        let (system, user) = build_encounter_detection_prompt(input, None, None);

        let mut votes: Vec<bool> = Vec::new();
        for _ in 0..trials {
            match client.generate(&bench.model, &system, &user, "encounter_detection_bench").await {
                Ok(response) => match parse_encounter_detection(&response) {
                    Ok(parsed) => votes.push(parsed.complete),
                    Err(e) => eprintln!("  {} parse error: {e}", tc.id),
                },
                Err(e) => eprintln!("  {} LLM error: {e}", tc.id),
            }
        }

        let predicted = if votes.is_empty() {
            None
        } else {
            let true_count = votes.iter().filter(|v| **v).count();
            Some(true_count * 2 >= votes.len())
        };

        total += 1;
        let pass = predicted == Some(expected_complete);
        if pass { correct += 1; }

        // Confusion matrix
        match (expected_complete, predicted) {
            (true, Some(true)) => tp += 1,
            (true, Some(false)) => fn_ += 1,
            (false, Some(true)) => fp += 1,
            (false, Some(false)) => tn += 1,
            (_, None) => {}
        }

        let symbol = if pass { "✓" } else { "✗" };
        let pred_str = match predicted {
            Some(true) => "true",
            Some(false) => "false",
            None => "ERROR",
        };
        println!(
            "  {} {} ({}): expected={}, predicted={} [{} trials]",
            symbol, tc.id, tc.difficulty, expected_complete, pred_str, votes.len()
        );
    }

    let overall = correct as f64 / total.max(1) as f64 * 100.0;
    let tpr = if tp + fn_ > 0 { tp as f64 / (tp + fn_) as f64 * 100.0 } else { 100.0 };
    let fpr = if fp + tn > 0 { fp as f64 / (fp + tn) as f64 * 100.0 } else { 0.0 };

    println!();
    println!("  Overall accuracy: {:.1}% ({}/{})", overall, correct, total);
    println!("  TPR (split recall): {:.1}%   FPR: {:.1}%", tpr, fpr);

    let mut regression = false;
    if let Some(target) = bench.targets.overall_accuracy_pct {
        if overall < target {
            println!("  ✗ overall_accuracy {:.1}% < target {:.1}%", overall, target);
            regression = true;
        }
    }
    if let Some(target) = bench.targets.true_positive_rate_pct {
        if tpr < target {
            println!("  ✗ TPR {:.1}% < target {:.1}%", tpr, target);
            regression = true;
        }
    }
    if let Some(max) = bench.targets.false_positive_rate_pct_max {
        if fpr > max {
            println!("  ✗ FPR {:.1}% > max {:.1}%", fpr, max);
            regression = true;
        }
    }
    regression
}

async fn run_encounter_merge(client: &LLMClient, bench: &BenchmarkFile, trials: u32) -> bool {
    let mut total = 0;
    let mut correct = 0;
    let mut same_total = 0;
    let mut same_correct = 0;
    let mut diff_total = 0;
    let mut diff_correct = 0;

    for tc in &bench.test_cases {
        let expected = match tc.expected_same_encounter {
            Some(e) => e,
            None => continue,
        };
        let prev = match tc.prev_tail.as_ref() {
            Some(s) => s,
            None => continue,
        };
        let curr = match tc.curr_head.as_ref() {
            Some(s) => s,
            None => continue,
        };
        let (system, user) = build_encounter_merge_prompt(prev, curr, tc.patient_name.as_deref(), None);

        let mut votes: Vec<bool> = Vec::new();
        for _ in 0..trials {
            match client.generate(&bench.model, &system, &user, "encounter_merge_bench").await {
                Ok(response) => match parse_merge_check(&response) {
                    Ok(parsed) => votes.push(parsed.same_encounter),
                    Err(e) => eprintln!("  {} parse error: {e}", tc.id),
                },
                Err(e) => eprintln!("  {} LLM error: {e}", tc.id),
            }
        }

        let predicted = if votes.is_empty() {
            None
        } else {
            let true_count = votes.iter().filter(|v| **v).count();
            Some(true_count * 2 >= votes.len())
        };

        total += 1;
        let pass = predicted == Some(expected);
        if pass { correct += 1; }

        if expected {
            same_total += 1;
            if predicted == Some(true) { same_correct += 1; }
        } else {
            diff_total += 1;
            if predicted == Some(false) { diff_correct += 1; }
        }

        let symbol = if pass { "✓" } else { "✗" };
        let pred_str = match predicted {
            Some(true) => "true",
            Some(false) => "false",
            None => "ERROR",
        };
        println!(
            "  {} {} ({}): expected_same={}, predicted={} [{} trials]",
            symbol, tc.id, tc.difficulty, expected, pred_str, votes.len()
        );
    }

    let overall = correct as f64 / total.max(1) as f64 * 100.0;
    let same_recall = if same_total > 0 { same_correct as f64 / same_total as f64 * 100.0 } else { 100.0 };
    let diff_recall = if diff_total > 0 { diff_correct as f64 / diff_total as f64 * 100.0 } else { 100.0 };

    println!();
    println!("  Overall accuracy: {:.1}% ({}/{})", overall, correct, total);
    println!("  True same recall: {:.1}% ({}/{})", same_recall, same_correct, same_total);
    println!("  True different recall: {:.1}% ({}/{})", diff_recall, diff_correct, diff_total);

    let mut regression = false;
    if let Some(target) = bench.targets.overall_accuracy_pct {
        if overall < target {
            println!("  ✗ overall_accuracy {:.1}% < target {:.1}%", overall, target);
            regression = true;
        }
    }
    if let Some(target) = bench.targets.true_same_encounter_recall_pct {
        if same_recall < target {
            println!("  ✗ true_same_recall {:.1}% < target {:.1}%", same_recall, target);
            regression = true;
        }
    }
    if let Some(target) = bench.targets.true_different_encounter_recall_pct {
        if diff_recall < target {
            println!("  ✗ true_different_recall {:.1}% < target {:.1}%", diff_recall, target);
            regression = true;
        }
    }
    regression
}

async fn run_multi_patient_detection(
    client: &LLMClient,
    bench: &BenchmarkFile,
    trials: u32,
) -> bool {
    use transcription_app_lib::encounter_detection::MULTI_PATIENT_DETECT_PROMPT;
    let mut total = 0;
    let mut correct = 0;
    let mut multi_total = 0;
    let mut multi_correct = 0;
    let mut single_total = 0;
    let mut single_correct = 0;

    for tc in &bench.test_cases {
        let expected_multi = match tc.expected_multiple_patients {
            Some(e) => e,
            None => continue,
        };
        let input = match tc.input.as_ref() {
            Some(s) => s,
            None => continue,
        };

        let user = format!("Transcript:\n{}", input);
        let mut votes: Vec<bool> = Vec::new();
        for _ in 0..trials {
            match client.generate(&bench.model, MULTI_PATIENT_DETECT_PROMPT, &user, "multi_patient_bench").await {
                Ok(response) => match parse_multi_patient_detection(&response) {
                    Ok(parsed) => votes.push(parsed.patient_count >= 2),
                    Err(e) => eprintln!("  {} parse error: {e}", tc.id),
                },
                Err(e) => eprintln!("  {} LLM error: {e}", tc.id),
            }
        }

        let predicted = if votes.is_empty() {
            None
        } else {
            let multi_count = votes.iter().filter(|v| **v).count();
            Some(multi_count * 2 >= votes.len())
        };

        total += 1;
        let pass = predicted == Some(expected_multi);
        if pass { correct += 1; }

        if expected_multi {
            multi_total += 1;
            if predicted == Some(true) { multi_correct += 1; }
        } else {
            single_total += 1;
            if predicted == Some(false) { single_correct += 1; }
        }

        let symbol = if pass { "✓" } else { "✗" };
        let pred_str = match predicted {
            Some(true) => "multi",
            Some(false) => "single",
            None => "ERROR",
        };
        println!(
            "  {} {} ({}): expected={}, predicted={} [{} trials]",
            symbol, tc.id, tc.difficulty,
            if expected_multi { "multi" } else { "single" },
            pred_str, votes.len()
        );
    }

    let overall = correct as f64 / total.max(1) as f64 * 100.0;
    let multi_recall = if multi_total > 0 { multi_correct as f64 / multi_total as f64 * 100.0 } else { 100.0 };
    let single_specificity = if single_total > 0 { single_correct as f64 / single_total as f64 * 100.0 } else { 100.0 };

    println!();
    println!("  Overall accuracy: {:.1}% ({}/{})", overall, correct, total);
    println!("  Multi recall: {:.1}% ({}/{})", multi_recall, multi_correct, multi_total);
    println!("  Single specificity: {:.1}% ({}/{})", single_specificity, single_correct, single_total);

    let mut regression = false;
    if let Some(target) = bench.targets.overall_accuracy_pct {
        if overall < target {
            println!("  ✗ overall_accuracy {:.1}% < target {:.1}%", overall, target);
            regression = true;
        }
    }
    if let Some(target) = bench.targets.multi_recall_pct {
        if multi_recall < target {
            println!("  ✗ multi_recall {:.1}% < target {:.1}%", multi_recall, target);
            regression = true;
        }
    }
    if let Some(target) = bench.targets.single_specificity_pct {
        if single_specificity < target {
            println!("  ✗ single_specificity {:.1}% < target {:.1}%", single_specificity, target);
            regression = true;
        }
    }
    regression
}

async fn run_multi_patient_split(
    client: &LLMClient,
    bench: &BenchmarkFile,
    trials: u32,
) -> bool {
    let mut total = 0;
    let mut exact = 0;
    let mut within_2 = 0;
    let mut within_5 = 0;
    let mut correct_overall = 0;
    let mut no_boundary_total = 0;
    let mut no_boundary_correct = 0;

    for tc in &bench.test_cases {
        let input = match tc.input.as_ref() {
            Some(s) => s,
            None => continue,
        };

        let system = multi_patient_split_prompt(None);
        let user = format!("Transcript:\n{}", input);

        let mut votes: Vec<Option<usize>> = Vec::new();
        for _ in 0..trials {
            match client.generate(&bench.model, &system, &user, "multi_patient_split_bench").await {
                Ok(response) => match parse_multi_patient_split(&response) {
                    Ok(parsed) => votes.push(parsed.line_index),
                    Err(e) => eprintln!("  {} parse error: {e}", tc.id),
                },
                Err(e) => eprintln!("  {} LLM error: {e}", tc.id),
            }
        }

        // Take majority for "no boundary" (None) vs median for line_index
        if votes.is_empty() {
            continue;
        }
        let none_votes = votes.iter().filter(|v| v.is_none()).count();
        let predicted_no_boundary = none_votes * 2 >= votes.len();
        let predicted_line: Option<usize> = if predicted_no_boundary {
            None
        } else {
            let mut nums: Vec<usize> = votes.iter().filter_map(|v| *v).collect();
            nums.sort();
            Some(nums[nums.len() / 2])
        };

        total += 1;
        let symbol;
        let detail;

        if tc.expected_no_boundary == Some(true) {
            // No-boundary case
            no_boundary_total += 1;
            if predicted_no_boundary {
                correct_overall += 1;
                no_boundary_correct += 1;
                symbol = "✓";
                detail = "no_boundary".to_string();
            } else {
                symbol = "✗";
                detail = format!("predicted_line={}", predicted_line.unwrap());
            }
        } else if let Some(expected_line) = tc.expected_line_index {
            let acceptable = tc.expected_line_index_acceptable.clone()
                .unwrap_or_else(|| vec![expected_line]);
            let predicted = match predicted_line {
                Some(l) => l,
                None => {
                    symbol = "✗";
                    detail = "got_no_boundary".to_string();
                    println!("  {} {} ({}): expected_line={}, predicted=NONE", symbol, tc.id, tc.difficulty, expected_line);
                    continue;
                }
            };
            let diff = (predicted as i64 - expected_line as i64).abs();
            if acceptable.contains(&predicted) {
                exact += 1;
                within_2 += 1;
                within_5 += 1;
                correct_overall += 1;
                symbol = "✓";
                detail = format!("predicted={} (in acceptable range)", predicted);
            } else if diff <= 2 {
                within_2 += 1;
                within_5 += 1;
                correct_overall += 1;
                symbol = "≈";
                detail = format!("predicted={} (within ±2 of {})", predicted, expected_line);
            } else if diff <= 5 {
                within_5 += 1;
                symbol = "~";
                detail = format!("predicted={} (within ±5 of {})", predicted, expected_line);
            } else {
                symbol = "✗";
                detail = format!("predicted={} (off by {})", predicted, diff);
            }
        } else {
            continue;
        }

        println!("  {} {} ({}): {}", symbol, tc.id, tc.difficulty, detail);
    }

    let overall = correct_overall as f64 / total.max(1) as f64 * 100.0;
    let exact_pct = exact as f64 / total.max(1) as f64 * 100.0;
    let within_2_pct = within_2 as f64 / total.max(1) as f64 * 100.0;
    let within_5_pct = within_5 as f64 / total.max(1) as f64 * 100.0;
    let no_boundary_pct = if no_boundary_total > 0 {
        no_boundary_correct as f64 / no_boundary_total as f64 * 100.0
    } else { 100.0 };

    println!();
    println!("  Overall correct: {:.1}% ({}/{})", overall, correct_overall, total);
    println!("  Exact match: {:.1}% ({}/{})", exact_pct, exact, total);
    println!("  Within ±2 lines: {:.1}% ({}/{})", within_2_pct, within_2, total);
    println!("  Within ±5 lines: {:.1}% ({}/{})", within_5_pct, within_5, total);
    if no_boundary_total > 0 {
        println!("  No-boundary recall: {:.1}% ({}/{})", no_boundary_pct, no_boundary_correct, no_boundary_total);
    }

    let mut regression = false;
    if let Some(target) = bench.targets.overall_accuracy_pct {
        if overall < target {
            println!("  ✗ overall {:.1}% < target {:.1}%", overall, target);
            regression = true;
        }
    }
    if let Some(target) = bench.targets.exact_match_pct {
        if exact_pct < target {
            println!("  ✗ exact_match {:.1}% < target {:.1}%", exact_pct, target);
            regression = true;
        }
    }
    if let Some(target) = bench.targets.within_2_lines_pct {
        if within_2_pct < target {
            println!("  ✗ within_2_lines {:.1}% < target {:.1}%", within_2_pct, target);
            regression = true;
        }
    }
    if let Some(target) = bench.targets.within_5_lines_pct {
        if within_5_pct < target {
            println!("  ✗ within_5_lines {:.1}% < target {:.1}%", within_5_pct, target);
            regression = true;
        }
    }
    regression
}
