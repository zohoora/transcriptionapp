//! Model downloading and management.
//!
//! This module handles automatic downloading of required models:
//! - Whisper models for transcription (from ggerganov/whisper.cpp)
//! - ECAPA-TDNN model for speaker diarization

use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, info};

use crate::config::Config;

/// Base URL for Whisper GGML models
const WHISPER_BASE_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// URL for the speaker embedding ONNX model
/// Using WeSpeaker ResNet34 model from ONNX Community (~26MB)
const SPEAKER_MODEL_URL: &str =
    "https://huggingface.co/onnx-community/wespeaker-voxceleb-resnet34-LM/resolve/main/onnx/model.onnx";

/// URL for the GTCRN speech enhancement model (~523KB)
const ENHANCEMENT_MODEL_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/speech-enhancement-models/gtcrn_simple.onnx";

/// URL for the emotion detection model (wav2small ~120KB when available)
/// Note: This model needs to be exported from the wav2small repo
const EMOTION_MODEL_URL: &str =
    "https://huggingface.co/dkounadis/wav2small/resolve/main/wav2small.onnx";

/// URL for the YAMNet audio classification model (~3MB)
/// YAMNet detects 521 audio event classes including coughs, sneezes, throat clearing
const YAMNET_MODEL_URL: &str =
    "https://huggingface.co/onnx-community/yamnet/resolve/main/onnx/model.onnx";

/// Errors that can occur during model operations
#[derive(Debug, Error)]
pub enum ModelError {
    #[error("Failed to download model: {0}")]
    DownloadError(String),

    #[error("Failed to create directory: {0}")]
    DirectoryError(String),

    #[error("Failed to write model file: {0}")]
    WriteError(String),

    #[error("Model not found: {0}")]
    NotFound(String),

    #[error("Invalid model name: {0}")]
    InvalidModel(String),

    #[error("Network error: {0}")]
    NetworkError(String),
}

/// Available Whisper model sizes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhisperModel {
    Tiny,
    Base,
    Small,
    Medium,
    Large,
}

impl WhisperModel {
    /// Get model name as string
    pub fn name(&self) -> &'static str {
        match self {
            WhisperModel::Tiny => "tiny",
            WhisperModel::Base => "base",
            WhisperModel::Small => "small",
            WhisperModel::Medium => "medium",
            WhisperModel::Large => "large",
        }
    }

    /// Get the filename for this model
    pub fn filename(&self) -> String {
        format!("ggml-{}.bin", self.name())
    }

    /// Get the download URL for this model
    pub fn url(&self) -> String {
        format!("{}/ggml-{}.bin", WHISPER_BASE_URL, self.name())
    }

    /// Get approximate file size in bytes (for progress estimation)
    pub fn approximate_size(&self) -> u64 {
        match self {
            WhisperModel::Tiny => 75_000_000,    // ~75 MB
            WhisperModel::Base => 142_000_000,   // ~142 MB
            WhisperModel::Small => 466_000_000,  // ~466 MB
            WhisperModel::Medium => 1_500_000_000, // ~1.5 GB
            WhisperModel::Large => 2_900_000_000,  // ~2.9 GB
        }
    }

    /// Parse model name from string
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "tiny" => Some(WhisperModel::Tiny),
            "base" => Some(WhisperModel::Base),
            "small" => Some(WhisperModel::Small),
            "medium" => Some(WhisperModel::Medium),
            "large" => Some(WhisperModel::Large),
            _ => None,
        }
    }
}

/// Model download progress
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub model_name: String,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
    pub percent: f32,
}

/// Download a file from URL to the specified path
fn download_file(url: &str, dest_path: &Path) -> Result<(), ModelError> {
    info!("Downloading from {} to {:?}", url, dest_path);

    // Create parent directory if needed
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| ModelError::DirectoryError(e.to_string()))?;
    }

    // Download using blocking reqwest
    let response = reqwest::blocking::Client::new()
        .get(url)
        .send()
        .map_err(|e| ModelError::NetworkError(e.to_string()))?;

    if !response.status().is_success() {
        return Err(ModelError::DownloadError(format!(
            "HTTP {} for {}",
            response.status(),
            url
        )));
    }

    let total_size = response.content_length();
    info!(
        "Download started, total size: {}",
        total_size
            .map(|s| format!("{:.1} MB", s as f64 / 1_000_000.0))
            .unwrap_or_else(|| "unknown".to_string())
    );

    // Create temporary file for download
    let temp_path = dest_path.with_extension("download");
    let mut file = File::create(&temp_path)
        .map_err(|e| ModelError::WriteError(e.to_string()))?;

    // Download with progress tracking
    let bytes = response
        .bytes()
        .map_err(|e| ModelError::NetworkError(e.to_string()))?;

    file.write_all(&bytes)
        .map_err(|e| ModelError::WriteError(e.to_string()))?;

    file.flush()
        .map_err(|e| ModelError::WriteError(e.to_string()))?;

    // Rename temp file to final destination
    fs::rename(&temp_path, dest_path)
        .map_err(|e| ModelError::WriteError(e.to_string()))?;

    info!("Download complete: {:?}", dest_path);
    Ok(())
}

/// Download a Whisper model if not already present
pub fn ensure_whisper_model(model: WhisperModel) -> Result<PathBuf> {
    let models_dir = Config::models_dir()?;
    let model_path = models_dir.join(model.filename());

    if model_path.exists() {
        debug!("Whisper model already exists: {:?}", model_path);
        return Ok(model_path);
    }

    info!("Downloading Whisper {} model...", model.name());
    download_file(&model.url(), &model_path)
        .context(format!("Failed to download Whisper {} model", model.name()))?;

    Ok(model_path)
}

/// Download the speaker diarization model if not already present
pub fn ensure_speaker_model() -> Result<PathBuf> {
    let models_dir = Config::models_dir()?;
    let model_path = models_dir.join("speaker_embedding.onnx");

    if model_path.exists() {
        debug!("Speaker model already exists: {:?}", model_path);
        return Ok(model_path);
    }

    info!("Downloading speaker embedding model...");
    download_file(SPEAKER_MODEL_URL, &model_path)
        .context("Failed to download speaker embedding model")?;

    Ok(model_path)
}

/// Check if a Whisper model is available locally
pub fn is_whisper_model_available(model: WhisperModel) -> bool {
    if let Ok(models_dir) = Config::models_dir() {
        let model_path = models_dir.join(model.filename());
        model_path.exists()
    } else {
        false
    }
}

/// Check if the speaker model is available locally
pub fn is_speaker_model_available() -> bool {
    if let Ok(models_dir) = Config::models_dir() {
        // Check for the new name or legacy name
        let model_path = models_dir.join("speaker_embedding.onnx");
        let legacy_path = models_dir.join("voxceleb_ECAPA512_LM.onnx");
        model_path.exists() || legacy_path.exists()
    } else {
        false
    }
}

/// Download the speech enhancement model if not already present
pub fn ensure_enhancement_model() -> Result<PathBuf> {
    let models_dir = Config::models_dir()?;
    let model_path = models_dir.join("gtcrn_simple.onnx");

    if model_path.exists() {
        debug!("Enhancement model already exists: {:?}", model_path);
        return Ok(model_path);
    }

    info!("Downloading speech enhancement model...");
    download_file(ENHANCEMENT_MODEL_URL, &model_path)
        .context("Failed to download speech enhancement model")?;

    Ok(model_path)
}

/// Check if the enhancement model is available locally
pub fn is_enhancement_model_available() -> bool {
    if let Ok(models_dir) = Config::models_dir() {
        let model_path = models_dir.join("gtcrn_simple.onnx");
        model_path.exists()
    } else {
        false
    }
}

/// Get the path to the enhancement model
pub fn get_enhancement_model_path() -> Result<PathBuf> {
    let models_dir = Config::models_dir()?;
    Ok(models_dir.join("gtcrn_simple.onnx"))
}

/// Download the emotion detection model if not already present
pub fn ensure_emotion_model() -> Result<PathBuf> {
    let models_dir = Config::models_dir()?;
    let model_path = models_dir.join("wav2small.onnx");

    if model_path.exists() {
        debug!("Emotion model already exists: {:?}", model_path);
        return Ok(model_path);
    }

    info!("Downloading emotion detection model...");
    download_file(EMOTION_MODEL_URL, &model_path)
        .context("Failed to download emotion detection model")?;

    Ok(model_path)
}

/// Check if the emotion model is available locally
pub fn is_emotion_model_available() -> bool {
    if let Ok(models_dir) = Config::models_dir() {
        let model_path = models_dir.join("wav2small.onnx");
        model_path.exists()
    } else {
        false
    }
}

/// Get the path to the emotion model
pub fn get_emotion_model_path() -> Result<PathBuf> {
    let models_dir = Config::models_dir()?;
    Ok(models_dir.join("wav2small.onnx"))
}

/// Download the YAMNet audio classification model if not already present
pub fn ensure_yamnet_model() -> Result<PathBuf> {
    let models_dir = Config::models_dir()?;
    let model_path = models_dir.join("yamnet.onnx");

    if model_path.exists() {
        debug!("YAMNet model already exists: {:?}", model_path);
        return Ok(model_path);
    }

    info!("Downloading YAMNet audio classification model...");
    download_file(YAMNET_MODEL_URL, &model_path)
        .context("Failed to download YAMNet model")?;

    Ok(model_path)
}

/// Check if the YAMNet model is available locally
pub fn is_yamnet_model_available() -> bool {
    if let Ok(models_dir) = Config::models_dir() {
        let model_path = models_dir.join("yamnet.onnx");
        model_path.exists()
    } else {
        false
    }
}

/// Get the path to the YAMNet model
pub fn get_yamnet_model_path() -> Result<PathBuf> {
    let models_dir = Config::models_dir()?;
    Ok(models_dir.join("yamnet.onnx"))
}

/// Get the path to the speaker model (checking both new and legacy names)
pub fn get_speaker_model_path() -> Result<PathBuf> {
    let models_dir = Config::models_dir()?;

    // Check new name first
    let model_path = models_dir.join("speaker_embedding.onnx");
    if model_path.exists() {
        return Ok(model_path);
    }

    // Check legacy name
    let legacy_path = models_dir.join("voxceleb_ECAPA512_LM.onnx");
    if legacy_path.exists() {
        return Ok(legacy_path);
    }

    // Return the new path (for download)
    Ok(model_path)
}

/// Ensure all required models are available, downloading if necessary
pub fn ensure_all_models(whisper_model: &str, need_diarization: bool) -> Result<()> {
    // Download Whisper model
    let model = WhisperModel::from_name(whisper_model)
        .ok_or_else(|| anyhow::anyhow!("Invalid whisper model: {}", whisper_model))?;
    ensure_whisper_model(model)?;

    // Download speaker model if diarization is enabled
    if need_diarization {
        ensure_speaker_model()?;
    }

    Ok(())
}

/// Model status for frontend
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub available: bool,
    pub path: Option<String>,
    pub size_bytes: Option<u64>,
    pub download_url: String,
}

/// Get information about all models
pub fn get_model_info(whisper_model: &str) -> Vec<ModelInfo> {
    let mut models = Vec::new();

    // Whisper model info
    if let Some(model) = WhisperModel::from_name(whisper_model) {
        let available = is_whisper_model_available(model);
        let path = if available {
            Config::models_dir()
                .ok()
                .map(|d| d.join(model.filename()).to_string_lossy().to_string())
        } else {
            None
        };

        models.push(ModelInfo {
            name: format!("Whisper {}", model.name()),
            available,
            path,
            size_bytes: Some(model.approximate_size()),
            download_url: model.url(),
        });
    }

    // Speaker model info
    let speaker_available = is_speaker_model_available();
    let speaker_path = if speaker_available {
        get_speaker_model_path().ok().map(|p| p.to_string_lossy().to_string())
    } else {
        None
    };

    models.push(ModelInfo {
        name: "Speaker Embedding".to_string(),
        available: speaker_available,
        path: speaker_path,
        size_bytes: Some(40_000_000), // ~40 MB estimate
        download_url: SPEAKER_MODEL_URL.to_string(),
    });

    // Enhancement model info
    let enhancement_available = is_enhancement_model_available();
    let enhancement_path = if enhancement_available {
        get_enhancement_model_path().ok().map(|p| p.to_string_lossy().to_string())
    } else {
        None
    };

    models.push(ModelInfo {
        name: "Speech Enhancement (GTCRN)".to_string(),
        available: enhancement_available,
        path: enhancement_path,
        size_bytes: Some(523_638), // ~523 KB
        download_url: ENHANCEMENT_MODEL_URL.to_string(),
    });

    // Emotion model info
    let emotion_available = is_emotion_model_available();
    let emotion_path = if emotion_available {
        get_emotion_model_path().ok().map(|p| p.to_string_lossy().to_string())
    } else {
        None
    };

    models.push(ModelInfo {
        name: "Emotion Detection (wav2small)".to_string(),
        available: emotion_available,
        path: emotion_path,
        size_bytes: Some(120_000), // ~120 KB
        download_url: EMOTION_MODEL_URL.to_string(),
    });

    // YAMNet model info (for biomarker cough detection)
    let yamnet_available = is_yamnet_model_available();
    let yamnet_path = if yamnet_available {
        get_yamnet_model_path().ok().map(|p| p.to_string_lossy().to_string())
    } else {
        None
    };

    models.push(ModelInfo {
        name: "YAMNet (Cough Detection)".to_string(),
        available: yamnet_available,
        path: yamnet_path,
        size_bytes: Some(3_000_000), // ~3 MB
        download_url: YAMNET_MODEL_URL.to_string(),
    });

    models
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whisper_model_names() {
        assert_eq!(WhisperModel::Tiny.name(), "tiny");
        assert_eq!(WhisperModel::Base.name(), "base");
        assert_eq!(WhisperModel::Small.name(), "small");
        assert_eq!(WhisperModel::Medium.name(), "medium");
        assert_eq!(WhisperModel::Large.name(), "large");
    }

    #[test]
    fn test_whisper_model_from_name() {
        assert_eq!(WhisperModel::from_name("tiny"), Some(WhisperModel::Tiny));
        assert_eq!(WhisperModel::from_name("SMALL"), Some(WhisperModel::Small));
        assert_eq!(WhisperModel::from_name("invalid"), None);
    }

    #[test]
    fn test_whisper_model_urls() {
        let tiny = WhisperModel::Tiny;
        assert!(tiny.url().contains("ggml-tiny.bin"));
        assert!(tiny.url().starts_with("https://"));
    }

    #[test]
    fn test_whisper_model_filenames() {
        assert_eq!(WhisperModel::Tiny.filename(), "ggml-tiny.bin");
        assert_eq!(WhisperModel::Large.filename(), "ggml-large.bin");
    }

    /// Test downloading the tiny Whisper model
    /// This test is ignored by default as it downloads ~75MB
    #[test]
    #[ignore]
    fn test_download_whisper_tiny() {
        let result = ensure_whisper_model(WhisperModel::Tiny);
        assert!(result.is_ok(), "Failed to download: {:?}", result.err());
        let path = result.unwrap();
        assert!(path.exists(), "Downloaded file does not exist");
        // Tiny model should be at least 70MB
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 70_000_000, "File too small: {} bytes", metadata.len());
    }

    /// Test downloading the speaker embedding model
    /// This test is ignored by default as it downloads ~26MB
    #[test]
    #[ignore]
    fn test_download_speaker_model() {
        let result = ensure_speaker_model();
        assert!(result.is_ok(), "Failed to download: {:?}", result.err());
        let path = result.unwrap();
        assert!(path.exists(), "Downloaded file does not exist");
        // Speaker model should be at least 20MB
        let metadata = std::fs::metadata(&path).unwrap();
        assert!(metadata.len() > 20_000_000, "File too small: {} bytes", metadata.len());
    }
}
