//! Vision SOAP Prompt Experiment CLI
//!
//! A standalone tool to run vision prompt experiments without the full Tauri app.
//!
//! Usage:
//!   cargo run --bin vision_experiment_cli -- <transcript_path> <image_path> [strategies...]
//!
//! Examples:
//!   cargo run --bin vision_experiment_cli -- transcript.txt ehr.jpg
//!   cargo run --bin vision_experiment_cli -- transcript.txt ehr.jpg p1 p2 p5

use base64::Engine;
use std::env;
use std::fs;

use transcription_app_lib::config::Config;
use transcription_app_lib::llm_client::LLMClient;
use transcription_app_lib::vision_experiment::{
    experiments_dir, generate_summary_report, run_experiment, save_result, ExperimentParams,
    PromptStrategy,
};

fn parse_strategy(s: &str) -> Option<PromptStrategy> {
    match s.to_lowercase().as_str() {
        "current" | "p0" | "baseline" => Some(PromptStrategy::Current),
        "negative" | "p1" => Some(PromptStrategy::NegativeFraming),
        "flip" | "p2" => Some(PromptStrategy::FlipDefault),
        "twostep" | "p3" => Some(PromptStrategy::TwoStepReasoning),
        "prominent" | "p4" => Some(PromptStrategy::ProminentPlacement),
        "examples" | "p5" => Some(PromptStrategy::ConcreteExamples),
        "minimal" | "p6" => Some(PromptStrategy::MinimalImage),
        "transcript" | "p7" => Some(PromptStrategy::TranscriptOnly),
        "aggressive" | "p8" => Some(PromptStrategy::AggressiveExamples),
        "anchor" | "p9" => Some(PromptStrategy::ExplicitAnchor),
        "hybrid" | "p10" => Some(PromptStrategy::HybridLookup),
        "verified" | "p11" => Some(PromptStrategy::VerifiedSteps),
        "all" => None, // Special case: run all
        "phase2" => None, // Special case: handled separately
        "phase3" => None, // Special case: handled separately
        _ => {
            eprintln!("Unknown strategy: {}. Use: p0-p11, or keywords: all, phase2, phase3", s);
            None
        }
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
    if args.len() < 3 {
        eprintln!("Vision SOAP Prompt Experiment CLI");
        eprintln!("");
        eprintln!("Usage: {} <transcript_path> <image_path> [strategies...]", args[0]);
        eprintln!("");
        eprintln!("Strategies:");
        eprintln!("  p0/current   - Current production prompt (baseline)");
        eprintln!("  p1/negative  - Negative framing (list what NOT to include)");
        eprintln!("  p2/flip      - Flip default (ignore image EXCEPT...)");
        eprintln!("  p3/twostep   - Two-step reasoning");
        eprintln!("  p4/prominent - Prominent placement (rule first)");
        eprintln!("  p5/examples  - Concrete examples [BEST from phase 1]");
        eprintln!("  p6/minimal   - Minimal image instruction");
        eprintln!("  p7/transcript- Transcript only (ignore image)");
        eprintln!("  p8/aggressive- Aggressive examples with specific anchors");
        eprintln!("  p9/anchor    - Explicit anchor points for extraction");
        eprintln!("  all          - Run all strategies (default if none specified)");
        eprintln!("  phase2       - Run phase 2 strategies (p5 + new variations)");
        eprintln!("");
        eprintln!("Examples:");
        eprintln!("  {} transcript.txt ehr.jpg", args[0]);
        eprintln!("  {} transcript.txt ehr.jpg p1 p5", args[0]);
        std::process::exit(1);
    }

    let transcript_path = &args[1];
    let image_path = &args[2];

    // Parse strategies
    let strategies: Vec<PromptStrategy> = if args.len() > 3 {
        // Check for special keywords
        let first_arg = args[3].to_lowercase();
        if first_arg == "phase2" {
            PromptStrategy::phase2()
        } else if first_arg == "phase3" {
            PromptStrategy::phase3()
        } else {
            args[3..]
                .iter()
                .filter_map(|s| parse_strategy(s))
                .collect()
        }
    } else {
        PromptStrategy::all()
    };

    // Use all if no valid strategies specified
    let strategies = if strategies.is_empty() {
        PromptStrategy::all()
    } else {
        strategies
    };

    // Load transcript
    let transcript = fs::read_to_string(transcript_path)
        .map_err(|e| format!("Failed to read transcript '{}': {}", transcript_path, e))?;
    println!("Loaded transcript: {} chars, {} words",
             transcript.len(),
             transcript.split_whitespace().count());

    // Load and encode image
    let image_data = fs::read(image_path)
        .map_err(|e| format!("Failed to read image '{}': {}", image_path, e))?;
    let image_base64 = base64::engine::general_purpose::STANDARD.encode(&image_data);
    println!(
        "Loaded image: {} bytes -> {} base64 chars",
        image_data.len(),
        image_base64.len()
    );

    // Create LLM client
    let config = Config::load_or_default();
    println!("LLM Router: {}", config.llm_router_url);

    let client = LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &config.fast_model,
    )?;

    println!("\n{}", "=".repeat(80));
    println!("RUNNING EXPERIMENTS");
    println!("{}", "=".repeat(80));
    println!(
        "\nTesting {} strategies with temperature 0.3\n",
        strategies.len()
    );

    let mut results = Vec::new();
    let mut best_score = i32::MIN;
    let mut best_strategy = String::new();

    for strategy in &strategies {
        let params = ExperimentParams {
            strategy: *strategy,
            temperature: 0.3,
            max_tokens: 2000,
            image_first: false,
        };

        println!("\n{}", strategy.name());
        println!("{}", "-".repeat(60));

        match run_experiment(&client, "vision-model", &transcript, &image_base64, &params).await {
            Ok(result) => {
                let score = result.score.total_score;

                // Color-code the score
                let score_indicator = if score >= 3 {
                    "EXCELLENT"
                } else if score >= 1 {
                    "GOOD"
                } else if score >= 0 {
                    "FAIR"
                } else {
                    "POOR"
                };

                println!("Score: {} ({})", score, score_indicator);
                println!(
                    "  Patient name (Julie): {}",
                    if result.score.has_patient_name {
                        "YES"
                    } else {
                        "no"
                    }
                );
                println!(
                    "  Medication (Wegovy):  {}",
                    if result.score.has_medication_name {
                        "YES"
                    } else {
                        "no"
                    }
                );
                println!(
                    "  Weight issue:         {}",
                    if result.score.has_weight_issue {
                        "YES"
                    } else {
                        "no"
                    }
                );

                if !result.score.irrelevant_inclusions.is_empty() {
                    println!(
                        "  IRRELEVANT items:     {:?}",
                        result.score.irrelevant_inclusions
                    );
                }

                println!("  Time: {}ms", result.generation_time_ms);
                println!("  Words: {}", result.score.word_count);

                // Track best
                if score > best_score {
                    best_score = score;
                    best_strategy = strategy.name().to_string();
                }

                // Save result
                match save_result(&result) {
                    Ok(path) => println!("  Saved: {}", path.display()),
                    Err(e) => eprintln!("  Failed to save: {}", e),
                }

                // Show first few lines of output for debugging
                let preview: String = result.raw_response
                    .lines()
                    .take(5)
                    .collect::<Vec<_>>()
                    .join("\n");
                println!("\n  Preview:\n  {}", preview.replace('\n', "\n  "));

                results.push(result);
            }
            Err(e) => {
                eprintln!("  ERROR: {}", e);
            }
        }
    }

    // Generate summary
    println!("\n{}", "=".repeat(80));
    println!("SUMMARY");
    println!("{}", "=".repeat(80));

    if !results.is_empty() {
        println!("\nBest Strategy: {} (score: {})\n", best_strategy, best_score);

        // Print detailed report
        let report = generate_summary_report(&results);
        println!("{}", report);

        // Save report
        let report_path = experiments_dir().join("summary.md");
        fs::write(&report_path, &report)?;
        println!("\nReport saved to: {}", report_path.display());
    } else {
        println!("\nNo successful experiments to summarize.");
    }

    Ok(())
}
