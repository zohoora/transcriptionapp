//! # Transcription App Library
//!
//! This crate provides the core functionality for the Transcription App,
//! a real-time speech-to-text application built with Tauri.
//!
//! ## Architecture
//!
//! The application is organized into several modules:
//!
//! - [`audio`] - Audio capture and resampling from input devices
//! - [`config`] - Configuration management and settings persistence
//! - [`session`] - Recording session state machine and transcript management
//! - [`transcription`] - Transcription types (segments, utterances)
//! - [`vad`] - Voice Activity Detection and audio gating
//! - [`diarization`] - Speaker diarization using ONNX embeddings
//! - [`enhancement`] - Speech enhancement/denoising using GTCRN
//! - [`emotion`] - Speech emotion detection using wav2small
//!
//! ## Usage
//!
//! This library is primarily used by the Tauri application via IPC commands.
//! The main entry point is the [`run`] function which starts the application.
//!
//! ## Example
//!
//! ```no_run
//! // Start the Tauri application
//! transcription_app_lib::run();
//! ```

pub mod audio;
pub mod biomarkers;
pub mod checklist;
mod commands;
pub mod config;
pub mod diarization;
pub mod emotion;
pub mod enhancement;
pub mod models;
mod pipeline;
#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod pipeline_tests;
#[cfg(test)]
mod soak_tests;
#[cfg(test)]
mod stress_tests;
pub mod session;
pub mod transcription;
pub mod vad;

use commands::PipelineState;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Manager, WindowEvent};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

/// Timeout for graceful pipeline shutdown on window close
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    info!("Transcription App starting...");

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Initialize session manager (wrapped in Arc for sharing)
            let session_manager = Arc::new(Mutex::new(session::SessionManager::new()));
            app.manage(session_manager);

            // Initialize pipeline state (wrapped in Arc for sharing with async tasks)
            let pipeline_state = Arc::new(Mutex::new(PipelineState::default()));
            app.manage(pipeline_state);

            info!("App setup complete");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_input_devices,
            commands::get_settings,
            commands::set_settings,
            commands::start_session,
            commands::stop_session,
            commands::reset_session,
            commands::check_model_status,
            commands::get_model_info,
            commands::download_whisper_model,
            commands::download_speaker_model,
            commands::download_enhancement_model,
            commands::download_emotion_model,
            commands::download_yamnet_model,
            commands::ensure_models,
            commands::run_checklist,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                // Try graceful shutdown of the pipeline
                let mut graceful_success = true;

                if let Some(pipeline_state) = window.app_handle().try_state::<Arc<Mutex<PipelineState>>>() {
                    if let Ok(mut ps) = pipeline_state.lock() {
                        if let Some(handle) = ps.handle.take() {
                            info!("Stopping pipeline on window close");
                            handle.stop();

                            // Spawn a thread to join the pipeline and wait with timeout
                            let join_thread = std::thread::spawn(move || {
                                handle.join();
                            });

                            // Wait for join to complete with timeout
                            let start = std::time::Instant::now();
                            loop {
                                if join_thread.is_finished() {
                                    info!("Pipeline shutdown completed gracefully");
                                    break;
                                }
                                if start.elapsed() >= SHUTDOWN_TIMEOUT {
                                    warn!("Pipeline shutdown timed out after {:?}", SHUTDOWN_TIMEOUT);
                                    graceful_success = false;
                                    break;
                                }
                                std::thread::sleep(Duration::from_millis(50));
                            }
                        }
                    }
                }

                if graceful_success {
                    // Allow normal exit - Rust destructors will run
                    info!("Graceful exit");
                    // Note: We still use _exit(0) for now because ONNX Runtime
                    // can crash during normal destructor cleanup. Once the ONNX
                    // shutdown issue is fixed upstream, this can be removed.
                    // TODO: Remove forced exit once ort crate fixes shutdown
                    unsafe { libc::_exit(0) };
                } else {
                    // Timeout expired - force immediate termination
                    warn!("Forcing immediate exit due to shutdown timeout");
                    unsafe { libc::_exit(0) };
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
