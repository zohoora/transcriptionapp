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
mod commands;
pub mod config;
mod pipeline;
#[cfg(test)]
mod command_tests;
#[cfg(test)]
mod pipeline_tests;
#[cfg(test)]
mod stress_tests;
pub mod session;
pub mod transcription;
pub mod vad;

use commands::PipelineState;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tracing::info;
use tracing_subscriber::EnvFilter;

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

            // Initialize pipeline state
            let pipeline_state = Mutex::new(PipelineState::default());
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
