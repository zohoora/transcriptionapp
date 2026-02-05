#!/bin/bash
# Run vision SOAP prompt experiments
#
# This script tests different prompt strategies for the vision SOAP generation
# to find the optimal approach for using EHR screenshots.
#
# Usage:
#   ./scripts/run-vision-experiments.sh
#
# Prerequisites:
#   - The app must be built: pnpm tauri build --debug
#   - LLM router must be running at the configured URL

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
APP_DIR="$HOME/.transcriptionapp"
EXPERIMENTS_DIR="$APP_DIR/debug/vision-experiments"
DEBUG_DIR="$APP_DIR/debug"

# Test data paths
TRANSCRIPT_PATH="$DEBUG_DIR/5abbe45b-7b02-4c1c-912f-652e64ec1904/transcript.txt"
IMAGE_PATH="/var/folders/m2/_kdc0bx12txclscx5kxcnm8m0000gp/T/transcriptionapp-screenshots-20260204-215815/capture-215900-216-thumb.jpg"

echo "=================================="
echo "Vision SOAP Prompt Experiments"
echo "=================================="
echo ""
echo "Transcript: $TRANSCRIPT_PATH"
echo "Image: $IMAGE_PATH"
echo "Output: $EXPERIMENTS_DIR"
echo ""

# Check prerequisites
if [ ! -f "$TRANSCRIPT_PATH" ]; then
    echo "ERROR: Transcript not found at $TRANSCRIPT_PATH"
    exit 1
fi

if [ ! -f "$IMAGE_PATH" ]; then
    echo "ERROR: Image not found at $IMAGE_PATH"
    exit 1
fi

# Create experiments directory
mkdir -p "$EXPERIMENTS_DIR"

echo "Starting experiments..."
echo ""

# Build and run the test binary
cd "$PROJECT_DIR/src-tauri"

# Create a simple Rust test binary to run experiments
cat > /tmp/vision_experiment_runner.rs << 'EOF'
use std::env;
use std::fs;
use std::path::PathBuf;

// Import the experiment module
use transcription_app::vision_experiment::{
    ExperimentParams, PromptStrategy, build_experiment_prompt, score_result,
    experiments_dir, save_result, generate_summary_report, run_experiment,
};
use transcription_app::llm_client::LLMClient;
use transcription_app::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <transcript_path> <image_path>", args[0]);
        std::process::exit(1);
    }

    let transcript_path = &args[1];
    let image_path = &args[2];

    // Load transcript
    let transcript = fs::read_to_string(transcript_path)?;
    println!("Loaded transcript: {} chars", transcript.len());

    // Load and encode image
    let image_data = fs::read(image_path)?;
    let image_base64 = base64::engine::general_purpose::STANDARD
        .encode(&image_data);
    println!("Loaded image: {} bytes -> {} base64 chars", image_data.len(), image_base64.len());

    // Create LLM client
    let config = Config::load_or_default();
    let client = LLMClient::new(
        &config.llm_router_url,
        &config.llm_api_key,
        &config.llm_client_id,
        &config.fast_model,
    )?;

    println!("\nRunning experiments...\n");
    println!("{:=<80}", "");

    let mut results = Vec::new();

    // Test all strategies with default temperature
    for strategy in PromptStrategy::all() {
        let params = ExperimentParams {
            strategy,
            temperature: 0.3,
            max_tokens: 2000,
            image_first: false,
        };

        println!("\n{}", strategy.name());
        println!("{:-<60}", "");

        match run_experiment(&client, "vision-model", &transcript, &image_base64, &params).await {
            Ok(result) => {
                println!("Score: {}", result.score.total_score);
                println!("  Patient name (Julie): {}", if result.score.has_patient_name { "YES" } else { "no" });
                println!("  Medication (Wegovy): {}", if result.score.has_medication_name { "YES" } else { "no" });
                println!("  Weight issue: {}", if result.score.has_weight_issue { "YES" } else { "no" });
                println!("  Irrelevant items: {:?}", result.score.irrelevant_inclusions);
                println!("  Time: {}ms", result.generation_time_ms);
                println!("  Words: {}", result.score.word_count);

                // Save result
                match save_result(&result) {
                    Ok(path) => println!("  Saved: {:?}", path),
                    Err(e) => eprintln!("  Failed to save: {}", e),
                }

                results.push(result);
            }
            Err(e) => {
                eprintln!("  ERROR: {}", e);
            }
        }
    }

    // Generate summary report
    println!("\n{:=<80}", "");
    println!("\nSUMMARY REPORT\n");
    let report = generate_summary_report(&results);
    println!("{}", report);

    // Save report
    let report_path = experiments_dir().join("summary.md");
    fs::write(&report_path, &report)?;
    println!("\nReport saved to: {:?}", report_path);

    Ok(())
}
EOF

echo "Note: The experiment runner is built into the app."
echo "To run experiments, use the Tauri command 'run_vision_experiments' from the frontend."
echo ""
echo "Alternatively, run the Rust tests:"
echo "  cd src-tauri && cargo test vision_experiment --features test"
echo ""
echo "Or invoke via the app's IPC:"
echo ""
echo "Example JavaScript (in app console):"
echo '  await window.__TAURI__.core.invoke("run_vision_experiments", {'
echo '    request: {'
echo "      transcript_path: \"$TRANSCRIPT_PATH\","
echo "      image_path: \"$IMAGE_PATH\","
echo '      strategies: [],'
echo '      temperatures: [0.3],'
echo '      test_image_order: false'
echo '    }'
echo '  })'
echo ""
