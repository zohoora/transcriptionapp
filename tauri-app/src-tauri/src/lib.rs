mod audio;
mod commands;
mod config;
mod session;
mod transcription;
mod vad;

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
            // Initialize session manager
            let session_manager = session::SessionManager::new();
            app.manage(std::sync::Mutex::new(session_manager));

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
