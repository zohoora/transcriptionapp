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
use transcription_app_lib::experiment::labels::load_label_for_session;
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
    eprintln!("  --pairwise          Run pairwise detection on adjacent encounters (should split)");
    eprintln!("  --accumulation N,N  Words of next encounter to simulate (default: 200,500,0=full)");
    eprintln!("  --model ALIAS       LLM model alias to use (default: config fast_model)");
    eprintln!("  --sensor-departed   Inject sensor-departed prompt context for Baseline strategy");
    eprintln!("  --sensor-present    Inject sensor-confirmed-present prompt context for Baseline strategy");
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
    eprintln!("  {} --pairwise --date 2026/02/26 --model fast-model", program);
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
    let mut sensor_departed = false;
    let mut sensor_present = false;
    let mut merge_only = false;
    let mut detect_only = false;
    let mut pairwise = false;
    let mut accumulation_words: Vec<usize> = Vec::new(); // Words of next encounter to include
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
            "--sensor-departed" => sensor_departed = true,
            "--sensor-present" => sensor_present = true,
            "--merge-only" => merge_only = true,
            "--detect-only" => detect_only = true,
            "--pairwise" => pairwise = true,
            "--accumulation" => {
                i += 1;
                if i < args.len() {
                    // Parse comma-separated word counts, e.g., "200,500,1000"
                    for val in args[i].split(',') {
                        if let Ok(n) = val.trim().parse::<usize>() {
                            accumulation_words.push(n);
                        }
                    }
                }
            }
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

    // Load encounters from archive (continuous-mode bundles only)
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let archive_path = home.join(".transcriptionapp").join("archive").join(&date);

    println!("{}", "=".repeat(80));
    println!("ENCOUNTER DETECTION EXPERIMENT");
    println!("{}", "=".repeat(80));
    println!();

    let encounters = match load_archived_encounters(&archive_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to load encounters: {}", e);
            eprintln!("Archive path: {}", archive_path.display());
            std::process::exit(1);
        }
    };

    if encounters.is_empty() {
        eprintln!(
            "No continuous-mode encounters found at {}.",
            archive_path.display()
        );
        eprintln!(
            "(Session-mode archives without `replay_bundle.json` are silently skipped.)"
        );
        std::process::exit(1);
    }

    println!("Loaded {} continuous-mode encounters from {}:", encounters.len(), date);
    for enc in &encounters {
        let enc_label = enc
            .encounter_number
            .map(|n| format!(" (encounter #{})", n))
            .unwrap_or_default();
        println!(
            "  {} - {} words{}",
            &enc.session_id[..8.min(enc.session_id.len())],
            enc.word_count,
            enc_label
        );
    }

    // Check for hallucinations in each encounter
    println!("\nHallucination scan:");
    for enc in &encounters {
        let (_, report) = strip_hallucinations(&enc.plain_text, 5);
        let id_short = &enc.session_id[..8.min(enc.session_id.len())];
        if report.repetitions.is_empty() {
            println!("  {} - clean", id_short);
        } else {
            for rep in &report.repetitions {
                println!(
                    "  {} - FOUND: '{}' repeated {}x at position {} ({} -> {} words)",
                    id_short,
                    rep.word,
                    rep.original_count,
                    rep.position,
                    report.original_word_count,
                    report.cleaned_word_count,
                );
            }
        }
    }

    // Auto-detect patient name from the bundle's outcome if not provided
    if patient_name.is_none() {
        for enc in &encounters {
            if let Some(ref name) = enc.patient_name {
                if !name.is_empty() {
                    patient_name = Some(name.clone());
                    println!("\nAuto-detected patient name from bundle: {}", name);
                    break;
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

    // Build combined transcript (all encounters concatenated, plain text for stats)
    let combined_text: String = encounters
        .iter()
        .map(|e| e.plain_text.as_str())
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
    if !merge_only && !pairwise {
        println!("\n{}", "=".repeat(80));
        println!("MODE A: DETECTION ON COMBINED TRANSCRIPT");
        println!("(Correct answer: complete=false — the full transcript is one encounter)");
        println!("{}", "=".repeat(80));

        // Concatenate all encounter segments into one slice, re-indexing as
        // we go so the combined transcript has monotonically-increasing
        // indices. start_ms is left as-is since format_replay_segments_for_detection
        // re-bases elapsed time to the first segment's start_ms automatically.
        let mut combined_segments: Vec<transcription_app_lib::replay_bundle::ReplaySegment> =
            Vec::new();
        let mut next_index: u64 = 0;
        for enc in &encounters {
            for seg in &enc.segments {
                let mut s = seg.clone();
                s.index = next_index;
                next_index += 1;
                combined_segments.push(s);
            }
        }
        let formatted = format_replay_segments_for_detection(&combined_segments);

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
                sensor_departed,
                sensor_present,
            });

            // Experiment with filter (except baseline without filter already covered)
            experiments.push(ExperimentConfig {
                detection_strategy: Some(*strategy),
                merge_strategy: None,
                confidence_threshold: threshold,
                hallucination_filter: true,
                patient_name: patient_name.clone(),
                nothink,
                sensor_departed,
                sensor_present,
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
                    sensor_departed,
                    sensor_present,
                });
                experiments.push(ExperimentConfig {
                    detection_strategy: Some(*strategy),
                    merge_strategy: None,
                    confidence_threshold: 0.9,
                    hallucination_filter: true,
                    patient_name: patient_name.clone(),
                    nothink,
                    sensor_departed,
                    sensor_present,
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
    // Mode B: Pairwise detection on adjacent encounters
    // ========================================================================
    if pairwise && encounters.len() >= 2 {
        println!("\n{}", "=".repeat(80));
        println!("MODE B: PAIRWISE DETECTION ON ADJACENT ENCOUNTER PAIRS");
        println!("(Simulates production detection at different accumulation points)");
        println!("{}", "=".repeat(80));

        // load_archived_encounters already only returns continuous-mode bundles
        // (those with replay_bundle.json).
        let continuous: Vec<&ArchivedEncounter> = encounters.iter().collect();

        // Default accumulation points: simulate check at 200w, 500w, and full
        let accum_points = if accumulation_words.is_empty() {
            vec![200, 500, 0] // 0 = full
        } else {
            accumulation_words.clone()
        };

        if continuous.len() < 2 {
            println!("\nNeed at least 2 continuous-mode encounters for pairwise testing.");
        } else {
            let strategy = strategies.first().copied().unwrap_or(DetectionStrategy::Baseline);
            println!("\nUsing strategy: {}", strategy.name());
            println!(
                "Testing {} adjacent pairs at {} accumulation points...\n",
                continuous.len() - 1,
                accum_points.len(),
            );

            // Labels corpus uses YYYY-MM-DD; CLI takes YYYY/MM/DD. Translate
            // once per run so each pair's lookup is a cheap file stat.
            let labels_date = date.replace('/', "-");

            for pair_idx in 0..continuous.len() - 1 {
                let enc_a = continuous[pair_idx];
                let enc_b = continuous[pair_idx + 1];

                let name_a = enc_a.patient_name.as_deref().unwrap_or("unknown");
                let name_b = enc_b.patient_name.as_deref().unwrap_or("unknown");

                // Prefer the labeled corpus answer when present. enc_a's
                // `split_correct` field captures whether the production split
                // at enc_a's trailing edge was correct — exactly what Mode B
                // asks about. Falls back to the name-based heuristic when no
                // label has been recorded for the session yet.
                let label_a = load_label_for_session(&enc_a.session_id, &labels_date);
                let label_split_says = label_a
                    .as_ref()
                    .and_then(|l| l.labels.split_correct);
                let same_patient = match label_split_says {
                    Some(true) => false,  // split was correct → different patients
                    Some(false) => true,  // split was wrong → same patient
                    None => name_a == name_b,
                };
                let expected_source = if label_split_says.is_some() {
                    "label"
                } else {
                    "names"
                };
                let expected_label = if same_patient {
                    format!("complete=false (same patient, source={expected_source})")
                } else {
                    format!("complete=true (different patients, source={expected_source})")
                };

                println!(
                    "═══ Pair {}: enc#{} ({}, {}w) → enc#{} ({}, {}w) ═══",
                    pair_idx + 1,
                    enc_a.encounter_number.unwrap_or(0),
                    name_a,
                    enc_a.word_count,
                    enc_b.encounter_number.unwrap_or(0),
                    name_b,
                    enc_b.word_count,
                );
                println!("  Expected: {}", expected_label);

                for &accum in &accum_points {
                    // Concatenate enc_a's full segments with the first `accum`
                    // words' worth of enc_b's segments. This preserves bundle-
                    // level segment structure (timing + indices) so the
                    // formatter produces production-faithful prompts.
                    let mut combined_segments: Vec<
                        transcription_app_lib::replay_bundle::ReplaySegment,
                    > = enc_a.segments.clone();

                    // Take enough of enc_b's segments to reach `accum` words.
                    let mut words_taken = 0usize;
                    for seg in &enc_b.segments {
                        if accum != 0 && words_taken >= accum {
                            break;
                        }
                        words_taken += seg.text.split_whitespace().count();
                        combined_segments.push(seg.clone());
                    }

                    // Re-index so the combined slice has monotonically
                    // increasing indices starting at 0 (matches production's
                    // single-buffer view).
                    for (i, seg) in combined_segments.iter_mut().enumerate() {
                        seg.index = i as u64;
                    }

                    let next_words = words_taken;
                    let accum_label = if accum == 0 || accum >= enc_b.word_count {
                        format!("full ({}w)", next_words)
                    } else {
                        format!("{}w", next_words)
                    };

                    let combined_words = enc_a.word_count + next_words;
                    let formatted = format_replay_segments_for_detection(&combined_segments);
                    let formatted_words = formatted.split_whitespace().count();

                    print!(
                        "  @{}: {}w combined → {}w formatted",
                        accum_label, combined_words, formatted_words,
                    );

                    let exp_config = ExperimentConfig {
                        detection_strategy: Some(strategy),
                        merge_strategy: None,
                        confidence_threshold: threshold,
                        hallucination_filter: false,
                        patient_name: patient_name.clone(),
                        nothink,
                        sensor_departed,
                        sensor_present,
                    };

                    match run_detection_experiment(
                        &client,
                        &model,
                        &formatted,
                        &exp_config,
                    )
                    .await
                    {
                        Ok(result) => {
                            let correct = if same_patient {
                                !result.detected_complete
                            } else {
                                result.detected_complete
                            };
                            let verdict = if correct { "OK" } else { "MISS" };

                            println!(
                                " → complete={}, conf={:.2}, {}ms [{}]",
                                result.detected_complete,
                                result.confidence,
                                result.generation_time_ms,
                                verdict,
                            );

                            if let Err(e) = save_detection_result(&result) {
                                eprintln!("    Failed to save: {}", e);
                            }

                            all_detection_results.push(result);
                        }
                        Err(e) => {
                            println!();
                            eprintln!("    ERROR: {}", e);
                        }
                    }
                }
                println!();
            }
        }
    }

    // ========================================================================
    // Mode C: Merge experiments on archived encounter pairs
    // ========================================================================
    if !detect_only && !pairwise && encounters.len() >= 2 {
        println!("\n{}", "=".repeat(80));
        println!("MODE C: MERGE CHECK ON ARCHIVED ENCOUNTER PAIRS");
        println!("(Correct answer: same_encounter=true for all pairs)");
        println!("{}", "=".repeat(80));

        let merge_strategies = MergeStrategy::all();

        for pair_idx in 0..encounters.len() - 1 {
            let enc_a = &encounters[pair_idx];
            let enc_b = &encounters[pair_idx + 1];

            let pair_label = format!(
                "enc{}→enc{}",
                enc_a.encounter_number.unwrap_or((pair_idx + 1) as u32),
                enc_b.encounter_number.unwrap_or((pair_idx + 2) as u32),
            );

            println!(
                "\nPair: {} ({} → {})",
                pair_label,
                &enc_a.session_id[..8.min(enc_a.session_id.len())],
                &enc_b.session_id[..8.min(enc_b.session_id.len())],
            );

            // Extract tail/head excerpts from plain text
            let prev_tail = extract_tail(&enc_a.plain_text, 500);
            let curr_head = extract_head(&enc_b.plain_text, 500);

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
                    sensor_departed,
                    sensor_present,
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
