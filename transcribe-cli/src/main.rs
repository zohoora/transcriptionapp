mod audio;
mod config;
mod transcription;
mod vad;

use anyhow::Result;
use clap::Parser;
use cpal::traits::DeviceTrait;
use ringbuf::traits::Split;
use ringbuf::HeapRb;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use audio::{
    calculate_ring_buffer_capacity, get_device, list_input_devices, run_processor,
    select_input_config, AudioCapture, ProcessorConfig, ProcessorMessage,
};
use config::Config;
use transcription::{ProviderType, SessionRecord, WhisperProvider};
use vad::VadConfig;

/// Headless CLI for offline speech transcription using Whisper
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the Whisper model file (.bin)
    #[arg(short, long)]
    model: Option<PathBuf>,

    /// Input device ID (use "default" or run with --list-devices)
    #[arg(short, long, default_value = "default")]
    device: String,

    /// Language code (e.g., "en", "auto")
    #[arg(short, long, default_value = "en")]
    language: String,

    /// Output format: "paragraphs" or "single"
    #[arg(short, long, default_value = "paragraphs")]
    output: String,

    /// List available input devices and exit
    #[arg(long)]
    list_devices: bool,

    /// Number of threads for Whisper inference
    #[arg(long, default_value = "4")]
    threads: i32,

    /// VAD threshold (0.0 - 1.0)
    #[arg(long, default_value = "0.5")]
    vad_threshold: f32,

    /// Silence duration (ms) to end an utterance
    #[arg(long, default_value = "500")]
    silence_ms: u32,

    /// Maximum utterance length (ms)
    #[arg(long, default_value = "25000")]
    max_utterance_ms: u32,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .init();

    // Handle --list-devices
    if args.list_devices {
        return list_devices_and_exit();
    }

    // Determine model path
    let model_path = match &args.model {
        Some(path) => path.clone(),
        None => {
            let config = Config::default();
            config.get_model_path()?
        }
    };

    info!("Transcription CLI starting...");
    info!("Model: {:?}", model_path);
    info!("Device: {}", args.device);
    info!("Language: {}", args.language);

    // Check if model exists
    if !model_path.exists() {
        error!("Model file not found: {:?}", model_path);
        eprintln!("\nModel file not found: {:?}", model_path);
        eprintln!("\nPlease download a Whisper model and place it at the expected location.");
        eprintln!("You can download models from:");
        eprintln!("  https://huggingface.co/ggerganov/whisper.cpp/tree/main");
        eprintln!("\nRecommended for most users: ggml-small.bin");
        eprintln!("\nPlace the model file at: {:?}", model_path);
        eprintln!("Or specify a custom path with: --model /path/to/model.bin");
        return Ok(());
    }

    // Load Whisper model
    info!("Loading Whisper model...");
    let whisper = WhisperProvider::new(&model_path, &args.language, args.threads)?;
    info!("Model loaded successfully");

    // Get audio device
    let device_id = if args.device == "default" {
        None
    } else {
        Some(args.device.as_str())
    };
    let device = get_device(device_id)?;
    let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
    info!("Using audio device: {}", device_name);

    // Get input configuration (includes both StreamConfig and SampleFormat)
    let selected = select_input_config(&device)?;
    let sample_rate = selected.config.sample_rate.0;
    info!(
        "Audio config: {} Hz, {} channels, format {:?}",
        sample_rate, selected.config.channels, selected.sample_format
    );

    // Create ring buffer
    let capacity = calculate_ring_buffer_capacity(sample_rate);
    let ring_buffer = HeapRb::<f32>::new(capacity);
    let (producer, consumer) = ring_buffer.split();
    debug!("Ring buffer capacity: {} samples ({} seconds)", capacity, capacity / sample_rate as usize);

    // Create audio capture
    let capture = AudioCapture::new(&device, &selected.config, selected.sample_format, producer)?;

    // Create processor config
    let vad_config = VadConfig::from_ms(
        args.vad_threshold,
        300, // pre-roll
        250, // min speech
        args.silence_ms,
        args.max_utterance_ms,
    );
    let processor_config = ProcessorConfig {
        device_sample_rate: sample_rate,
        vad_config,
        status_interval_ms: 1000,
    };

    // Create channels
    let (tx, mut rx) = mpsc::channel::<ProcessorMessage>(32);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = stop_flag.clone();

    // Spawn processor thread
    let processor_handle = std::thread::spawn(move || {
        run_processor(consumer, processor_config, tx, stop_flag_clone);
    });

    // Set up Ctrl+C handler
    let stop_flag_ctrlc = stop_flag.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Received Ctrl+C, stopping...");
        stop_flag_ctrlc.store(true, Ordering::SeqCst);
    });

    // Start capture
    capture.start()?;
    println!("\nRecording... Press Ctrl+C to stop.\n");

    // Create session record
    let mut session = SessionRecord::new(
        ProviderType::Whisper,
        args.language.clone(),
        device_name,
    );

    // Process messages
    let mut context = String::new();
    let mut last_audio_ms = 0u64;

    while let Some(msg) = rx.recv().await {
        match msg {
            ProcessorMessage::Utterance(utterance) => {
                debug!(
                    "Processing utterance: {}ms - {}ms",
                    utterance.start_ms, utterance.end_ms
                );

                // Transcribe
                let context_ref = if context.is_empty() {
                    None
                } else {
                    Some(context.as_str())
                };

                match whisper.transcribe(&utterance, context_ref) {
                    Ok(segment) => {
                        if !segment.text.is_empty() {
                            // Print segment
                            println!(
                                "[{:02}:{:02}.{:03} - {:02}:{:02}.{:03}] {}",
                                segment.start_ms / 60000,
                                (segment.start_ms % 60000) / 1000,
                                segment.start_ms % 1000,
                                segment.end_ms / 60000,
                                (segment.end_ms % 60000) / 1000,
                                segment.end_ms % 1000,
                                segment.text
                            );

                            // Update context
                            context.push(' ');
                            context.push_str(&segment.text);

                            // Keep context reasonable size
                            if context.len() > 1000 {
                                let words: Vec<&str> = context.split_whitespace().collect();
                                context = words
                                    .into_iter()
                                    .rev()
                                    .take(100)
                                    .collect::<Vec<_>>()
                                    .into_iter()
                                    .rev()
                                    .collect::<Vec<_>>()
                                    .join(" ");
                            }

                            session.add_segment(segment);
                        }
                    }
                    Err(e) => {
                        error!("Transcription error: {}", e);
                    }
                }
            }

            ProcessorMessage::Status {
                audio_clock_ms,
                pending_count,
                is_speech_active,
            } => {
                if audio_clock_ms > last_audio_ms + 5000 {
                    debug!(
                        "Status: {}s recorded, {} pending, speech: {}",
                        audio_clock_ms / 1000,
                        pending_count,
                        is_speech_active
                    );
                    last_audio_ms = audio_clock_ms;
                }

                if pending_count > 3 {
                    warn!("Processing is behind: {} utterances queued", pending_count);
                }
            }

            ProcessorMessage::Error(e) => {
                error!("Processor error: {}", e);
            }

            ProcessorMessage::Stopped => {
                info!("Processor stopped");
                break;
            }
        }
    }

    // Stop capture
    capture.stop()?;

    // Wait for processor thread
    let _ = processor_handle.join();

    // Finalize session
    session.finalize();

    // Print summary
    println!("\n--- Session Summary ---");
    println!("Duration: {:.1}s", session.total_duration_ms as f64 / 1000.0);
    println!("Speech: {:.1}s", session.speech_duration_ms as f64 / 1000.0);
    println!("Segments: {}", session.segments.len());

    if capture.overflow_count() > 0 {
        warn!("Audio overflows detected: {}", capture.overflow_count());
    }

    // Print final transcript
    if !session.segments.is_empty() {
        println!("\n--- Transcript ---\n");
        let transcript = if args.output == "single" {
            session.transcript_single()
        } else {
            session.transcript_paragraphs()
        };
        println!("{}", transcript);
    }

    info!("Session complete");
    Ok(())
}

fn list_devices_and_exit() -> Result<()> {
    println!("Available input devices:\n");

    match list_input_devices() {
        Ok(devices) => {
            if devices.is_empty() {
                println!("  No input devices found.");
            } else {
                for device in devices {
                    let default_marker = if device.is_default { " (default)" } else { "" };
                    println!("  - {}{}", device.name, default_marker);
                }
            }
        }
        Err(e) => {
            error!("Failed to list devices: {}", e);
            println!("  Error: {}", e);
        }
    }

    Ok(())
}
