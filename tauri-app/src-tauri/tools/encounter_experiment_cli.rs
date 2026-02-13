//! Encounter Detection Experiment CLI
//!
//! A standalone tool to replay archived transcripts through different encounter
//! detection/merge prompts and parameters.
//!
//! Usage:
//!   cargo run --bin encounter_experiment_cli
//!   cargo run --bin encounter_experiment_cli -- p1 p3 --threshold 0.9
//!   cargo run --bin encounter_experiment_cli -- --merge-only
//!   cargo run --bin encounter_experiment_cli -- --date 2026/02/10
//!
//! The default experiment matrix runs ~24 LLM calls (~2 min runtime).

use std::env;
use std::fs;
use std::path::PathBuf;

use transcription_app_lib::config::Config;
use transcription_app_lib::encounter_experiment::*;
use transcription_app_lib::llm_client::LLMClient;

fn print_usage(program: &str) {
    eprintln!("Encounter Detection Experiment CLI");
    eprintln!();
    eprintln!("Usage: {} [options] [strategies...]", program);
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --date YYYY/MM/DD   Archive date to load (default: 2026/02/10)");
    eprintln!("  --threshold N       Confidence threshold (default: 0.7)");
    eprintln!("  --patient NAME      Known patient name for P3/M1 strategies");
    eprintln!("  --merge-only        Only run merge experiments (skip detection)");
    eprintln!("  --detect-only       Only run detection experiments (skip merge)");
    eprintln!("  --model ALIAS       LLM model alias to use (default: config fast_model)");
    eprintln!("  --help              Show this help");
    eprintln!();
    eprintln!("Detection Strategies:");
    eprintln!("  p0/baseline       Current production prompt");
    eprintln!("  p1/conservative   Require explicit patient departure");
    eprintln!("  p2/context        Same-patient context (typical visit structure)");
    eprintln!("  p3/name           Patient-name-aware detection");
    eprintln!("  all               Run all strategies (default)");
    eprintln!();
    eprintln!("Merge Strategies (run automatically on archived encounter pairs):");
    eprintln!("  m0                Baseline merge prompt");
    eprintln!("  m1                Patient-name-weighted merge");
    eprintln!("  m2                Hallucination-filtered merge");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {} p1 p3 --threshold 0.9", program);
    eprintln!("  {} --merge-only --patient 'Buckland, Deborah Ann'", program);
}

fn parse_detection_strategy(s: &str) -> Option<DetectionStrategy> {
    match s.to_lowercase().as_str() {
        "p0" | "baseline" => Some(DetectionStrategy::Baseline),
        "p1" | "conservative" => Some(DetectionStrategy::Conservative),
        "p2" | "context" => Some(DetectionStrategy::SamePatientContext),
        "p3" | "name" => Some(DetectionStrategy::PatientNameAware),
        _ => None,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = env::args().collect();

    // Parse arguments
    let mut date = "2026/02/10".to_string();
    let mut threshold = 0.7_f64;
    let mut patient_name: Option<String> = None;
    let mut model_override: Option<String> = None;
    let mut nothink = false;
    let mut merge_only = false;
    let mut detect_only = false;
    let mut strategies: Vec<DetectionStrategy> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_usage(&args[0]);
                return Ok(());
            }
            "--date" => {
                i += 1;
                if i < args.len() {
                    date = args[i].clone();
                }
            }
            "--threshold" => {
                i += 1;
                if i < args.len() {
                    threshold = args[i].parse().unwrap_or(0.7);
                }
            }
            "--patient" => {
                i += 1;
                if i < args.len() {
                    patient_name = Some(args[i].clone());
                }
            }
            "--model" => {
                i += 1;
                if i < args.len() {
                    model_override = Some(args[i].clone());
                }
            }
            "--nothink" => nothink = true,
            "--merge-only" => merge_only = true,
            "--detect-only" => detect_only = true,
            s => {
                if let Some(strategy) = parse_detection_strategy(s) {
                    strategies.push(strategy);
                } else if s == "all" {
                    strategies = DetectionStrategy::all();
                } else if !s.starts_with('-') {
                    eprintln!("Unknown strategy: {}", s);
                }
            }
        }
        i += 1;
    }

    // Default: all strategies
    if strategies.is_empty() && !merge_only {
        strategies = DetectionStrategy::all();
    }

    // Load transcripts from archive
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let archive_path = home.join(".transcriptionapp").join("archive").join(&date);

    println!("{}", "=".repeat(80));
    println!("ENCOUNTER DETECTION EXPERIMENT");
    println!("{}", "=".repeat(80));
    println!();

    let transcripts = match load_archived_transcripts(&archive_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to load transcripts: {}", e);
            eprintln!("Archive path: {}", archive_path.display());
            std::process::exit(1);
        }
    };

    println!("Loaded {} archived encounters from {}:", transcripts.len(), date);
    for (id, _text, wc, enc_num) in &transcripts {
        let enc_label = enc_num
            .map(|n| format!(" (encounter #{})", n))
            .unwrap_or_default();
        println!(
            "  {} - {} words{}",
            &id[..8],
            wc,
            enc_label
        );
    }

    // Check for hallucinations in each transcript
    println!("\nHallucination scan:");
    for (id, text, _, _) in &transcripts {
        let (_, report) = strip_hallucinations(text, 5);
        if report.repetitions.is_empty() {
            println!("  {} - clean", &id[..8]);
        } else {
            for rep in &report.repetitions {
                println!(
                    "  {} - FOUND: '{}' repeated {}x at position {} ({} -> {} words)",
                    &id[..8],
                    rep.word,
                    rep.original_count,
                    rep.position,
                    report.original_word_count,
                    report.cleaned_word_count,
                );
            }
        }
    }

    // Try to auto-detect patient name from metadata if not provided
    if patient_name.is_none() {
        for (id, _, _, _) in &transcripts {
            let metadata_path = archive_path.join(id).join("metadata.json");
            if let Ok(content) = fs::read_to_string(&metadata_path) {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(name) = value.get("patient_name").and_then(|n| n.as_str()) {
                        if !name.is_empty() {
                            patient_name = Some(name.to_string());
                            println!("\nAuto-detected patient name from metadata: {}", name);
                            break;
                        }
                    }
                }
            }
        }
    }

    // Create LLM client
    let config = Config::load_or_default();
    let model = model_override.unwrap_or_else(|| config.fast_model.clone());
    println!("\nLLM Router: {}", config.llm_router_url);
    println!("Model: {}", model);

    let client = LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &model,
    )?;

    // Build combined transcript (all encounters merged)
    let combined_text: String = transcripts
        .iter()
        .map(|(_, text, _, _)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let combined_word_count = combined_text.split_whitespace().count();
    println!(
        "\nCombined transcript: {} words (should be ~{} if 1 encounter)",
        combined_word_count,
        combined_word_count
    );

    let mut all_detection_results = Vec::new();
    let mut all_merge_results = Vec::new();

    // ========================================================================
    // Mode A: Detection experiments on combined transcript
    // ========================================================================
    if !merge_only {
        println!("\n{}", "=".repeat(80));
        println!("MODE A: DETECTION ON COMBINED TRANSCRIPT");
        println!("(Correct answer: complete=false — the full transcript is one encounter)");
        println!("{}", "=".repeat(80));

        let formatted = format_transcript_as_segments(&combined_text);

        // Build experiment matrix
        let mut experiments: Vec<ExperimentConfig> = Vec::new();

        for strategy in &strategies {
            // Experiment without filter
            experiments.push(ExperimentConfig {
                detection_strategy: Some(*strategy),
                merge_strategy: None,
                confidence_threshold: threshold,
                hallucination_filter: false,
                patient_name: patient_name.clone(),
                nothink,
            });

            // Experiment with filter (except baseline without filter already covered)
            experiments.push(ExperimentConfig {
                detection_strategy: Some(*strategy),
                merge_strategy: None,
                confidence_threshold: threshold,
                hallucination_filter: true,
                patient_name: patient_name.clone(),
                nothink,
            });

            // Experiment with high threshold (for baseline)
            if *strategy == DetectionStrategy::Baseline {
                experiments.push(ExperimentConfig {
                    detection_strategy: Some(*strategy),
                    merge_strategy: None,
                    confidence_threshold: 0.9,
                    hallucination_filter: false,
                    patient_name: patient_name.clone(),
                    nothink,
                });
                experiments.push(ExperimentConfig {
                    detection_strategy: Some(*strategy),
                    merge_strategy: None,
                    confidence_threshold: 0.9,
                    hallucination_filter: true,
                    patient_name: patient_name.clone(),
                    nothink,
                });
            }
        }

        println!(
            "\nRunning {} detection experiments...\n",
            experiments.len()
        );

        for (idx, exp_config) in experiments.iter().enumerate() {
            let strategy = exp_config
                .detection_strategy
                .unwrap_or(DetectionStrategy::Baseline);

            println!(
                "[{}/{}] {} (threshold={:.1}, filter={})",
                idx + 1,
                experiments.len(),
                strategy.name(),
                exp_config.confidence_threshold,
                if exp_config.hallucination_filter {
                    "ON"
                } else {
                    "off"
                },
            );

            match run_detection_experiment(
                &client,
                &model,
                &formatted,
                exp_config,
            )
            .await
            {
                Ok(result) => {
                    let verdict = if result.detected_complete {
                        "INCORRECT (premature split)"
                    } else {
                        "CORRECT (no split)"
                    };
                    println!(
                        "  -> complete={}, confidence={:.2}, {}ms — {}",
                        result.detected_complete,
                        result.confidence,
                        result.generation_time_ms,
                        verdict,
                    );

                    if let Some(ref parsed) = result.parsed {
                        if parsed.complete {
                            if let Some(end_idx) = parsed.end_segment_index {
                                println!("  -> end_segment_index={}", end_idx);
                            }
                        }
                    }

                    // Save result
                    if let Err(e) = save_detection_result(&result) {
                        eprintln!("  Failed to save: {}", e);
                    }

                    all_detection_results.push(result);
                }
                Err(e) => {
                    eprintln!("  ERROR: {}", e);
                }
            }
        }
    }

    // ========================================================================
    // Mode C: Merge experiments on archived encounter pairs
    // ========================================================================
    if !detect_only && transcripts.len() >= 2 {
        println!("\n{}", "=".repeat(80));
        println!("MODE C: MERGE CHECK ON ARCHIVED ENCOUNTER PAIRS");
        println!("(Correct answer: same_encounter=true for all pairs)");
        println!("{}", "=".repeat(80));

        let merge_strategies = MergeStrategy::all();

        for pair_idx in 0..transcripts.len() - 1 {
            let (id_a, text_a, _, enc_a) = &transcripts[pair_idx];
            let (id_b, text_b, _, enc_b) = &transcripts[pair_idx + 1];

            let pair_label = format!(
                "enc{}→enc{}",
                enc_a.unwrap_or((pair_idx + 1) as u32),
                enc_b.unwrap_or((pair_idx + 2) as u32),
            );

            println!(
                "\nPair: {} ({} → {})",
                pair_label,
                &id_a[..8],
                &id_b[..8],
            );

            // Extract tail/head excerpts
            let prev_tail = extract_tail(text_a, 500);
            let curr_head = extract_head(text_b, 500);

            println!(
                "  prev_tail: {} words, curr_head: {} words",
                prev_tail.split_whitespace().count(),
                curr_head.split_whitespace().count(),
            );

            for merge_strategy in &merge_strategies {
                let exp_config = ExperimentConfig {
                    detection_strategy: None,
                    merge_strategy: Some(*merge_strategy),
                    confidence_threshold: threshold,
                    hallucination_filter: *merge_strategy == MergeStrategy::HallucinationFiltered,
                    patient_name: patient_name.clone(),
                    nothink,
                };

                println!("  [{}]", merge_strategy.name());

                match run_merge_experiment(
                    &client,
                    &model,
                    &prev_tail,
                    &curr_head,
                    &pair_label,
                    &exp_config,
                )
                .await
                {
                    Ok(result) => {
                        let verdict = if result.same_encounter {
                            "CORRECT (same encounter)"
                        } else {
                            "INCORRECT (different)"
                        };
                        let reason = result.reason.as_deref().unwrap_or("-");
                        println!(
                            "    -> same={}, {}ms — {}",
                            result.same_encounter, result.generation_time_ms, verdict,
                        );
                        println!("    -> reason: {}", reason);

                        if let Err(e) = save_merge_result(&result) {
                            eprintln!("    Failed to save: {}", e);
                        }

                        all_merge_results.push(result);
                    }
                    Err(e) => {
                        eprintln!("    ERROR: {}", e);
                    }
                }
            }
        }
    }

    // ========================================================================
    // Summary
    // ========================================================================
    println!("\n{}", "=".repeat(80));
    println!("SUMMARY");
    println!("{}", "=".repeat(80));

    let report = generate_summary_report(&all_detection_results, &all_merge_results);
    println!("\n{}", report);

    // Save report
    let report_dir = experiments_dir();
    fs::create_dir_all(&report_dir)?;
    let report_path = report_dir.join("summary.md");
    fs::write(&report_path, &report)?;
    println!("Report saved to: {}", report_path.display());

    // Final verdict
    let detection_correct = all_detection_results
        .iter()
        .filter(|r| !r.detected_complete)
        .count();
    let merge_correct = all_merge_results
        .iter()
        .filter(|r| r.same_encounter)
        .count();

    println!("\n{}", "-".repeat(80));
    println!(
        "Detection: {} / {} correct (no premature split)",
        detection_correct,
        all_detection_results.len()
    );
    println!(
        "Merge: {} / {} correct (same encounter)",
        merge_correct,
        all_merge_results.len()
    );

    if detection_correct == all_detection_results.len()
        && merge_correct == all_merge_results.len()
    {
        println!("\nAll experiments passed — configuration correctly identifies 1 encounter.");
    } else {
        println!("\nSome experiments failed — see report for details.");
    }

    Ok(())
}
