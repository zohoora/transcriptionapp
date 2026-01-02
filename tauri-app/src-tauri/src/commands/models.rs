//! Model download and status commands

use crate::checklist::{self, ChecklistResult};
use crate::config::{Config, ModelStatus};
use crate::models::{self, ModelInfo, WhisperModel};
use crate::{activity_log};
use tracing::{error, info};

/// Check if the Whisper model is available
#[tauri::command]
pub fn check_model_status() -> ModelStatus {
    check_model_status_internal()
}

fn check_model_status_internal() -> ModelStatus {
    let config = Config::load_or_default();

    match config.get_model_path() {
        Ok(path) => {
            if path.exists() {
                ModelStatus {
                    available: true,
                    path: Some(path.to_string_lossy().to_string()),
                    error: None,
                }
            } else {
                ModelStatus {
                    available: false,
                    path: Some(path.to_string_lossy().to_string()),
                    error: Some(format!("Model file not found at {:?}", path)),
                }
            }
        }
        Err(e) => ModelStatus {
            available: false,
            path: None,
            error: Some(e.to_string()),
        },
    }
}

/// Get information about all required models
#[tauri::command]
pub fn get_model_info() -> Vec<ModelInfo> {
    let config = Config::load_or_default();
    models::get_model_info(&config.whisper_model)
}

/// Generic helper for downloading models in a blocking task
async fn download_model_async<F, E>(model_name: &str, download_fn: F) -> Result<String, String>
where
    F: FnOnce() -> Result<std::path::PathBuf, E> + Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    info!("Downloading {} model", model_name);

    let result = tokio::task::spawn_blocking(download_fn)
        .await
        .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Ok(path) => {
            info!("{} model downloaded to: {:?}", model_name, path);
            Ok(path.to_string_lossy().to_string())
        }
        Err(e) => {
            error!("Failed to download {} model: {}", model_name, e);
            Err(e.to_string())
        }
    }
}

/// Download a Whisper model
#[tauri::command]
pub async fn download_whisper_model(model_name: String) -> Result<String, String> {
    let model = WhisperModel::from_name(&model_name)
        .ok_or_else(|| format!("Invalid model name: {}", model_name))?;

    download_model_async("Whisper", move || models::ensure_whisper_model(model)).await
}

/// Download the speaker embedding model for diarization
#[tauri::command]
pub async fn download_speaker_model() -> Result<String, String> {
    download_model_async("speaker", models::ensure_speaker_model).await
}

/// Ensure all required models are downloaded
#[tauri::command]
pub async fn ensure_models() -> Result<(), String> {
    let config = Config::load_or_default();
    let whisper_model = config.whisper_model.clone();
    let need_diarization = config.diarization_enabled;

    info!(
        "Ensuring models are available: whisper={}, diarization={}",
        whisper_model, need_diarization
    );

    // Run download in blocking task to not block async runtime
    let result = tokio::task::spawn_blocking(move || {
        models::ensure_all_models(&whisper_model, need_diarization)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?;

    match result {
        Ok(()) => {
            info!("All required models are available");
            Ok(())
        }
        Err(e) => {
            error!("Failed to ensure models: {}", e);
            Err(e.to_string())
        }
    }
}

/// Run the launch sequence checklist
///
/// This verifies all requirements before starting a recording session:
/// - Audio input devices available
/// - Required models downloaded
/// - Configuration valid
#[tauri::command]
pub fn run_checklist() -> ChecklistResult {
    info!("Running launch sequence checklist");
    let config = Config::load_or_default();
    let result = checklist::run_all_checks(&config);

    // Log checklist result (counts only, no content)
    let (passed, failed, warnings) = result.checks.iter().fold((0, 0, 0), |(p, f, w), check| {
        match check.status {
            checklist::CheckStatus::Pass => (p + 1, f, w),
            checklist::CheckStatus::Fail => (p, f + 1, w),
            checklist::CheckStatus::Warning => (p, f, w + 1),
            checklist::CheckStatus::Skipped | checklist::CheckStatus::Pending => (p, f, w),
        }
    });
    activity_log::log_checklist_result(result.checks.len(), passed, failed, warnings);

    result
}

/// Download the speech enhancement model
#[tauri::command]
pub async fn download_enhancement_model() -> Result<String, String> {
    download_model_async("enhancement", models::ensure_enhancement_model).await
}

/// Download the emotion detection model
#[tauri::command]
pub async fn download_emotion_model() -> Result<String, String> {
    download_model_async("emotion", models::ensure_emotion_model).await
}

/// Download the YAMNet audio classification model (for cough detection)
#[tauri::command]
pub async fn download_yamnet_model() -> Result<String, String> {
    download_model_async("YAMNet", models::ensure_yamnet_model).await
}
