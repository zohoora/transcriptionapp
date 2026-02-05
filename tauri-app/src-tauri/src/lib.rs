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
//! - [`preprocessing`] - Audio preprocessing (DC removal, high-pass filter, AGC)
//! - [`medplum`] - Medplum EMR integration for encounter management
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

pub mod activity_log;
pub mod audio;
pub mod biomarkers;
pub mod checklist;
mod commands;
pub mod config;
pub mod continuous_mode;
pub mod debug_storage;
pub mod diarization;
pub mod local_archive;
pub mod enhancement;
pub mod listening;
pub mod llm_client;
pub mod mcp;
pub mod medplum;
pub mod models;
pub mod ollama;
pub mod permissions;
mod pipeline;
pub mod preprocessing;
pub mod screenshot;
pub mod speaker_profiles;
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
pub mod vision_experiment;
pub mod whisper_server;

use commands::PipelineState;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager, WindowEvent};
use tracing::{info, warn};

/// Application version
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Timeout for graceful pipeline shutdown on window close
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// Set up ONNX Runtime path from bundled location if not already set
/// This allows the app to work without requiring ORT_DYLIB_PATH to be set externally
fn setup_bundled_ort() {
    // If ORT_DYLIB_PATH is already set, use it
    if std::env::var("ORT_DYLIB_PATH").is_ok() {
        return;
    }

    // Try to find bundled ONNX Runtime in the app bundle
    // On macOS: MyApp.app/Contents/Frameworks/libonnxruntime.*.dylib
    if let Ok(exe_path) = std::env::current_exe() {
        // Navigate from Contents/MacOS/app-binary to Contents/Frameworks/
        if let Some(macos_dir) = exe_path.parent() {
            if let Some(contents_dir) = macos_dir.parent() {
                let frameworks_dir = contents_dir.join("Frameworks");

                // Look for libonnxruntime.*.dylib
                if frameworks_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&frameworks_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                if name.starts_with("libonnxruntime.") && name.ends_with(".dylib") {
                                    let path_str = path.to_string_lossy().to_string();
                                    std::env::set_var("ORT_DYLIB_PATH", &path_str);
                                    eprintln!("Using bundled ONNX Runtime: {}", path_str);
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: try the default venv location
    let home = dirs::home_dir().unwrap_or_default();
    let venv_path = home.join(".transcriptionapp/ort-venv");
    if venv_path.exists() {
        // Find the dylib in the venv
        if let Ok(output) = std::process::Command::new("find")
            .args([
                venv_path.to_string_lossy().as_ref(),
                "-name",
                "libonnxruntime.*.dylib",
            ])
            .output()
        {
            if let Ok(path) = String::from_utf8(output.stdout) {
                let path = path.trim();
                if !path.is_empty() && std::path::Path::new(path).exists() {
                    std::env::set_var("ORT_DYLIB_PATH", path);
                    eprintln!("Using ORT from venv: {}", path);
                }
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Set up ONNX Runtime path before anything else
    setup_bundled_ort();

    // Initialize activity logging (file + console)
    if let Err(e) = activity_log::init_logging() {
        eprintln!("Failed to initialize logging: {}", e);
        // Fall back to basic console logging
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .init();
    }

    // Log application start
    activity_log::log_app_start(APP_VERSION);
    info!("Transcription App starting...");

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // When another instance tries to launch, focus the existing window
            // and emit any deep link URL to the frontend
            info!("Single instance callback triggered with args: {:?}", argv);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
            // Check if any argument is a deep link URL
            for arg in argv {
                if arg.starts_with("fabricscribe://") {
                    // Parse URL for logging (without sensitive params)
                    let has_code = arg.contains("code=");
                    let has_state = arg.contains("state=");
                    let path = arg.split('?').next().unwrap_or("")
                        .trim_start_matches("fabricscribe://");
                    activity_log::log_deep_link("fabricscribe", path, has_code, has_state);

                    // Log path only, not query params (may contain OAuth code/state)
                    info!("Deep link received: {} (has_params={})", path, arg.contains('?'));
                    let _ = app.emit("deep-link", arg);
                }
            }
        }))
        .setup(|app| {
            // Initialize session manager (wrapped in Arc for sharing)
            let session_manager = Arc::new(Mutex::new(session::SessionManager::new()));
            app.manage(session_manager.clone());

            // Initialize pipeline state (wrapped in Arc for sharing with async tasks)
            let pipeline_state = Arc::new(Mutex::new(PipelineState::default()));
            app.manage(pipeline_state);

            // Initialize Medplum client state (lazy initialization)
            let medplum_client = commands::create_medplum_client();
            app.manage(medplum_client);

            // Initialize listening state for auto-session detection (Arc-wrapped for callback sharing)
            let listening_state: commands::SharedListeningState = Arc::new(Mutex::new(Default::default()));
            app.manage(listening_state);

            // Initialize screen capture state
            let screen_capture_state: commands::SharedScreenCaptureState = Arc::new(Mutex::new(Default::default()));
            app.manage(screen_capture_state);

            // Initialize continuous mode state
            let continuous_mode_state: commands::SharedContinuousModeState = Arc::new(Mutex::new(None));
            app.manage(continuous_mode_state);

            // Start MCP server on port 7101 for IT Admin Coordinator
            let mcp_session = session_manager.clone();
            tauri::async_runtime::spawn(async move {
                info!("Starting MCP server on port 7101");
                mcp::start_mcp_server(mcp_session).await;
            });

            // Resize window to match screen height (sidebar mode)
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(Some(monitor)) = window.current_monitor() {
                    let monitor_size = monitor.size();
                    let scale_factor = monitor.scale_factor();

                    // Convert physical pixels to logical pixels
                    let screen_height = (monitor_size.height as f64 / scale_factor) as u32;

                    // Use full screen height minus some padding for menu bar/dock
                    // macOS menu bar is ~25px, dock can vary but ~70px is typical
                    let padding = 100u32;
                    let window_height = screen_height.saturating_sub(padding);

                    // Keep width at 320px (sidebar width)
                    let window_width = 320u32;

                    // Position window on the right side of the screen
                    let screen_width = (monitor_size.width as f64 / scale_factor) as i32;
                    let x_position = screen_width - window_width as i32 - 10; // 10px from right edge
                    let y_position = 30i32; // Below menu bar

                    use tauri::{LogicalSize, LogicalPosition};
                    if let Err(e) = window.set_size(LogicalSize::new(window_width, window_height)) {
                        warn!("Failed to set window size: {}", e);
                    }
                    if let Err(e) = window.set_position(LogicalPosition::new(x_position, y_position)) {
                        warn!("Failed to set window position: {}", e);
                    }

                    info!("Window resized to {}x{} at ({}, {})",
                          window_width, window_height, x_position, y_position);
                }
            }

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
            commands::get_audio_file_path,
            commands::reset_silence_timer,
            commands::check_model_status,
            commands::get_model_info,
            commands::download_whisper_model,
            commands::download_speaker_model,
            commands::download_enhancement_model,
            commands::download_yamnet_model,
            commands::ensure_models,
            commands::run_checklist,
            commands::get_whisper_models,
            commands::download_whisper_model_by_id,
            commands::test_whisper_model,
            commands::is_model_downloaded,
            commands::check_ollama_status,
            commands::list_ollama_models,
            commands::prewarm_ollama_model,
            commands::generate_soap_note,
            commands::generate_soap_note_auto_detect,
            commands::generate_predictive_hint,
            // Medplum EMR commands
            commands::medplum_get_auth_state,
            commands::medplum_try_restore_session,
            commands::medplum_start_auth,
            commands::medplum_handle_callback,
            commands::medplum_logout,
            commands::medplum_refresh_token,
            commands::medplum_search_patients,
            commands::medplum_create_encounter,
            commands::medplum_complete_encounter,
            commands::medplum_get_encounter_history,
            commands::medplum_get_encounter_details,
            commands::medplum_get_audio_data,
            commands::medplum_sync_encounter,
            commands::medplum_quick_sync,
            commands::medplum_add_soap_to_encounter,
            commands::medplum_multi_patient_quick_sync,
            commands::medplum_check_connection,
            // Whisper server commands (remote transcription)
            commands::check_whisper_server_status,
            commands::list_whisper_server_models,
            // Permission commands (microphone access)
            commands::check_microphone_permission,
            commands::request_microphone_permission,
            commands::open_microphone_settings,
            // Listening mode commands (auto-session detection)
            commands::start_listening,
            commands::stop_listening,
            commands::get_listening_status,
            // Speaker profile commands (enrollment)
            commands::list_speaker_profiles,
            commands::get_speaker_profile,
            commands::create_speaker_profile,
            commands::update_speaker_profile,
            commands::reenroll_speaker_profile,
            commands::delete_speaker_profile,
            // Local archive commands (session history)
            commands::get_local_session_dates,
            commands::get_local_sessions_by_date,
            commands::get_local_session_details,
            commands::save_local_soap_note,
            // Clinical chat commands
            commands::clinical_chat_send,
            // MIIS (Medical Illustration Image Server) commands
            commands::miis_suggest,
            commands::miis_send_usage,
            // Screen capture commands
            commands::check_screen_recording_permission,
            commands::open_screen_recording_settings,
            commands::start_screen_capture,
            commands::stop_screen_capture,
            commands::get_screen_capture_status,
            commands::get_screenshot_paths,
            commands::get_screenshot_thumbnails,
            // Vision SOAP (experimental)
            commands::generate_vision_soap_note,
            // Vision prompt experiments
            commands::run_vision_experiments,
            commands::get_vision_experiment_results,
            commands::get_vision_experiment_report,
            commands::list_vision_experiment_strategies,
            // Continuous charting mode
            commands::start_continuous_mode,
            commands::stop_continuous_mode,
            commands::get_continuous_mode_status,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                // Only exit the app when the main window is closed
                // Other windows (like history) should just close normally
                if window.label() != "main" {
                    info!("Closing secondary window: {}", window.label());
                    return;
                }

                // Main window close - stop continuous mode if active
                if let Some(continuous_state) = window.app_handle().try_state::<commands::SharedContinuousModeState>() {
                    if let Ok(state) = continuous_state.lock() {
                        if let Some(ref handle) = *state {
                            info!("Stopping continuous mode on window close");
                            handle.stop();
                        }
                    }
                }

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
                    activity_log::log_app_shutdown("graceful");
                    info!("Graceful exit");
                    // WORKAROUND: Forced exit to avoid ONNX Runtime crash during cleanup
                    //
                    // The ONNX Runtime library can crash during normal destructor
                    // cleanup when sessions are dropped. This appears to be a thread
                    // safety issue in the ort crate's shutdown handling.
                    //
                    // Tracking: https://github.com/pykeio/ort/issues
                    // Related: ort crate version 2.0.0-rc.9
                    //
                    // Once the ort crate fixes graceful shutdown, this workaround
                    // can be removed and normal exit() used instead.
                    unsafe { libc::_exit(0) };
                } else {
                    // Timeout expired - force immediate termination
                    activity_log::log_app_shutdown("forced_timeout");
                    warn!("Forcing immediate exit due to shutdown timeout");
                    unsafe { libc::_exit(0) };
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
