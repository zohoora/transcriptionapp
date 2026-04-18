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
pub mod audio_processing;
pub mod audio;
pub mod audio_upload_queue;
pub mod billing;
pub mod biomarkers;
pub mod checklist;
pub mod co2_calibration;
mod commands;
pub mod config;
pub mod continuous_mode;
pub mod continuous_mode_flush_on_stop;
pub mod transcript_buffer;
pub mod encounter_detection;
pub mod encounter_merge;
pub mod patient_name_tracker;
pub mod debug_storage;
pub mod diarization;
pub mod local_archive;
pub mod enhancement;
pub mod listening;
pub mod gemini_client;
pub mod harness;
pub mod llm_backend;
pub mod llm_client;
pub mod run_context;
pub mod mcp;
pub mod medplum;
pub mod models;
pub mod ollama;
pub mod permissions;
mod pipeline;
pub mod pipeline_log;
pub mod presence_sensor;
pub mod preprocessing;
pub mod replay_bundle;
pub mod segment_log;
pub mod server_sync;
pub mod shadow_observer;
pub mod day_log;
pub mod performance_summary;
pub mod screenshot;
pub mod screenshot_task;
pub mod encounter_pipeline;
pub mod continuous_mode_events;
pub mod shadow_log;
pub mod speaker_profiles;
#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod e2e_tests;
#[cfg(test)]
mod pipeline_tests;
#[cfg(test)]
mod soak_tests;
#[cfg(test)]
mod stress_tests;
pub mod session;
pub mod transcription;
pub mod vad;
pub mod encounter_experiment;
pub mod vision_experiment;
pub mod whisper_server;
pub mod room_config;
pub mod profile_client;
pub mod physician_cache;
pub mod server_config;
pub mod server_config_resolve;

use commands::PipelineState;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager, WindowEvent};
use tracing::{info, warn};

/// Application version
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Timeout for graceful pipeline shutdown on window close
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3);

/// Recursively search a directory for a file matching `libonnxruntime.*.dylib`.
/// Returns the first match found, or `None`.
fn find_ort_dylib_recursive(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_ort_dylib_recursive(&path) {
                return Some(found);
            }
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("libonnxruntime.") && name.ends_with(".dylib") {
                return Some(path);
            }
        }
    }
    None
}

/// Set up ONNX Runtime path from bundled location if not already set.
/// This allows the app to work without requiring ORT_DYLIB_PATH to be set externally.
///
/// SAFETY: Uses `std::env::set_var` (unsafe in Rust 2024+). This function runs
/// during single-threaded startup before any other threads are spawned.
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
                                    unsafe { std::env::set_var("ORT_DYLIB_PATH", &path_str) };
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
        // Walk the venv directory tree to find the dylib (native Rust, no shell-out)
        if let Some(found) = find_ort_dylib_recursive(&venv_path) {
            let path_str = found.to_string_lossy().to_string();
            unsafe { std::env::set_var("ORT_DYLIB_PATH", &path_str) };
            eprintln!("Using ORT from venv: {}", path_str);
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
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
            app.manage(pipeline_state.clone());

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
            app.manage(continuous_mode_state.clone());

            // Initialize room config
            let room_config = room_config::RoomConfig::load().unwrap_or(None);
            let server_urls = room_config.as_ref().map(|rc| rc.all_server_urls());
            let room_id_for_merge = room_config.as_ref().and_then(|rc| rc.room_id.clone());
            let profile_api_key = room_config.as_ref().and_then(|rc| rc.profile_api_key.clone());
            let shared_room_config: commands::SharedRoomConfig = Arc::new(tokio::sync::RwLock::new(room_config));
            app.manage(shared_room_config);

            // Initialize profile client with all server URLs (primary + fallbacks)
            let profile_client = server_urls
                .filter(|urls| !urls.is_empty())
                .map(|urls| profile_client::ProfileClient::new(&urls, profile_api_key));

            // Startup: probe URLs to find the best one, then merge settings + sync speakers.
            // Non-blocking — app launches immediately with cached config and profiles.
            if let Some(ref client) = profile_client {
                // URL probe + settings merge (sequential: probe first, then merge uses best URL)
                let client_startup = client.clone();
                let room_id = room_id_for_merge;
                tauri::async_runtime::spawn(async move {
                    client_startup.select_best_url().await;
                    match client_startup.merge_server_settings(room_id.as_deref()).await {
                        Ok(true) => info!("Startup settings merge complete"),
                        Ok(false) => info!("Startup settings merge: no changes from server"),
                        Err(e) => warn!("Startup settings merge failed (using local config): {e}"),
                    }
                });
                // Speaker profile sync (runs concurrently with probe+merge)
                let client_speakers = client.clone();
                tauri::async_runtime::spawn(async move {
                    client_speakers.select_best_url().await;
                    match commands::physicians::do_sync_speaker_profiles(&client_speakers).await {
                        Ok(msg) => info!("Startup speaker sync: {msg}"),
                        Err(e) => warn!("Startup speaker sync failed: {e}"),
                    }
                });
            }

            let shared_profile_client: commands::SharedProfileClient = Arc::new(tokio::sync::RwLock::new(profile_client));
            app.manage(shared_profile_client.clone());

            // Server-configurable data (prompts, billing rules, detection thresholds).
            // Starts with compiled defaults; async fetch updates in background.
            let shared_server_config: commands::SharedServerConfig =
                Arc::new(tokio::sync::RwLock::new(server_config::compiled_defaults()));
            app.manage(shared_server_config.clone());

            // Async: fetch server config in background (non-blocking)
            {
                let client_for_config = shared_profile_client.clone();
                let config_state = shared_server_config.clone();
                tauri::async_runtime::spawn(async move {
                    let client_guard = client_for_config.read().await;
                    if let Some(ref client) = *client_guard {
                        let config = server_config::load_server_config(client).await;
                        let source = format!("{:?}", config.source);
                        let version = config.version;
                        *config_state.write().await = config;
                        info!(version, source = %source, "Server config loaded");
                    }
                });
            }

            // Initialize active physician
            let shared_active_physician: commands::SharedActivePhysician = Arc::new(tokio::sync::RwLock::new(None));
            app.manage(shared_active_physician);

            // Audio upload queue — persists pending uploads across restarts
            let audio_queue = Arc::new(tokio::sync::Mutex::new(
                audio_upload_queue::AudioUploadQueue::load(),
            ));
            app.manage(audio_queue.clone());

            // CO2 calibration state
            let calibration_state: commands::calibration::SharedCalibrationState =
                Arc::new(std::sync::Mutex::new(None));
            app.manage(calibration_state);

            // Spawn background audio upload task
            let upload_client = shared_profile_client.clone();
            let upload_pipeline = pipeline_state.clone();
            let upload_continuous = continuous_mode_state.clone();
            tauri::async_runtime::spawn(audio_upload_queue::audio_upload_task(
                audio_queue,
                upload_client,
                upload_pipeline,
                upload_continuous,
            ));

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

            // Request all permissions upfront so the user isn't interrupted during recording
            std::thread::spawn(|| {
                // Small delay to let the main window render first
                std::thread::sleep(std::time::Duration::from_millis(500));

                // 1. Microphone permission
                let mic_status = permissions::check_microphone_permission();
                info!("Startup permission check — Microphone: {}", mic_status);
                if mic_status == permissions::MicrophoneAuthStatus::NotDetermined {
                    permissions::request_microphone_permission();
                    // Wait for user to respond before showing next dialog
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }

                // 2. Screen Recording permission (CGPreflightScreenCaptureAccess / CGRequestScreenCaptureAccess)
                let screen_ok = screenshot::check_screen_recording_permission();
                info!("Startup permission check — Screen Recording: {}", screen_ok);
                if !screen_ok {
                    warn!("Screen Recording permission not granted — screen captures will show blank content for other apps. Grant permission in System Settings → Privacy & Security → Screen Recording.");
                    // Trigger the system prompt so the user sees it
                    let _ = screenshot::request_screen_recording_permission();
                }

            });

            info!("App setup complete");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_input_devices,
            commands::get_settings,
            commands::set_settings,
            commands::clear_user_edited_field,
            commands::get_operational_defaults,
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
            commands::generate_patient_handout,
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
            commands::save_patient_handout,
            commands::get_patient_handout,
            // Billing commands
            commands::get_session_billing,
            commands::save_session_billing,
            commands::confirm_session_billing,
            commands::extract_billing_codes,
            commands::get_daily_billing_summary,
            commands::get_monthly_billing_summary,
            commands::export_billing_csv,
            commands::search_ohip_codes,
            commands::search_diagnostic_codes,
            commands::read_local_audio_file,
            // Session cleanup commands (history window)
            commands::delete_local_session,
            commands::split_local_session,
            commands::merge_local_sessions,
            commands::update_session_patient_name,
            commands::renumber_local_encounters,
            commands::get_session_transcript_lines,
            commands::suggest_split_points,
            commands::get_session_feedback,
            commands::save_session_feedback,
            commands::get_session_soap_note,
            commands::delete_patient_from_session,
            commands::rename_patient_label,
            commands::merge_patient_soaps,
            commands::generate_clinical_feedback,
            // Clinical chat commands
            commands::clinical_chat_send,
            // MIIS (Medical Illustration Image Server) commands
            commands::miis_suggest,
            commands::miis_send_usage,
            commands::generate_ai_image,
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
            commands::get_continuous_transcript,
            commands::trigger_new_patient,
            commands::set_continuous_encounter_notes,
            commands::get_current_encounter_transcript,
            commands::list_serial_ports,
            // Physician selection and room config
            commands::get_room_config,
            commands::save_room_config,
            commands::test_profile_server,
            commands::get_physicians,
            commands::select_physician,
            commands::get_active_physician,
            commands::deselect_physician,
            commands::sync_speaker_profiles,
            // Physician admin commands (CRUD)
            commands::create_physician,
            commands::update_physician,
            commands::delete_physician,
            // Room commands
            commands::get_rooms,
            commands::create_room,
            commands::update_room,
            commands::delete_room,
            commands::sync_settings_from_server,
            commands::sync_infrastructure_settings,
            commands::sync_room_settings,
            // Audio upload (manual batch processing)
            commands::check_audio_ffmpeg,
            commands::process_audio_upload,
            // CO2 calibration
            commands::start_co2_calibration,
            commands::stop_co2_calibration,
            commands::advance_calibration_phase,
            commands::get_calibration_status,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                // Only exit the app when the main window is closed
                // Other windows (like history) should just close normally
                if window.label() != "main" {
                    info!("Closing secondary window: {}", window.label());
                    // Hide window before destruction to flush pending WebKit layout
                    // callbacks. Without this, WebKit::WebPageProxy::dispatchSetObscuredContentInsets()
                    // can fire on a deallocated webview → SIGSEGV.
                    let _ = window.hide();
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
