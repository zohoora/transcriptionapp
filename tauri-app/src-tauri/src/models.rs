//! Model downloading and management.
//!
//! This module handles automatic downloading of required models:
//! - Whisper models for transcription (from ggerganov/whisper.cpp)
//! - ECAPA-TDNN model for speaker diarization

use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{Read, Write};
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

/// URL for the YAMNet audio classification model (~3MB)
/// YAMNet detects 521 audio event classes including coughs, sneezes, throat clearing
/// Note: Original HuggingFace URL requires auth, using public GitHub source
const YAMNET_MODEL_URL: &str =
    "https://raw.githubusercontent.com/Choise-ieee/yamnet_onnx_cpp_audio_speech_classification/main/yamnet_3s.onnx";

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

/// Metadata for a Whisper model variant
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WhisperModelInfo {
    /// Internal identifier (e.g., "large-v3-turbo-q5_0")
    pub id: String,
    /// Display name for UI
    pub label: String,
    /// Model category for grouping
    pub category: String,
    /// Filename on disk
    pub filename: String,
    /// Download URL
    pub url: String,
    /// Approximate file size in bytes
    pub size_bytes: u64,
    /// Description of the model's characteristics
    pub description: String,
    /// Whether this model is downloaded
    #[serde(default)]
    pub downloaded: bool,
    /// Whether this is a recommended model
    #[serde(default)]
    pub recommended: bool,
    /// Whether this is an English-only model
    #[serde(default)]
    pub english_only: bool,
}

/// Get the list of all available Whisper models
/// Curated list for medical transcription: fast option + best quality options
pub fn get_all_whisper_models() -> Vec<WhisperModelInfo> {
    vec![
        // Fast option for testing/low-resource
        WhisperModelInfo {
            id: "small.en".into(),
            label: "Small (English)".into(),
            category: "Standard".into(),
            filename: "ggml-small.en.bin".into(),
            url: format!("{}/ggml-small.en.bin", WHISPER_BASE_URL),
            size_bytes: 466_000_000,
            description: "Fast and accurate for English. Good for testing.".into(),
            downloaded: false,
            recommended: false,
            english_only: true,
        },
        // Best quality - recommended for medical use
        WhisperModelInfo {
            id: "large-v3-turbo".into(),
            label: "Large v3 Turbo".into(),
            category: "Large".into(),
            filename: "ggml-large-v3-turbo.bin".into(),
            url: format!("{}/ggml-large-v3-turbo.bin", WHISPER_BASE_URL),
            size_bytes: 1_620_000_000,
            description: "Best for medical. Near large-v3 quality at 6x speed.".into(),
            downloaded: false,
            recommended: true,
            english_only: false,
        },
        // Quantized option - smaller download, slightly less accurate
        WhisperModelInfo {
            id: "large-v3-turbo-q5_0".into(),
            label: "Large v3 Turbo Q5".into(),
            category: "Quantized".into(),
            filename: "ggml-large-v3-turbo-q5_0.bin".into(),
            url: format!("{}/ggml-large-v3-turbo-q5_0.bin", WHISPER_BASE_URL),
            size_bytes: 574_000_000,
            description: "65% smaller download. Slightly less accurate.".into(),
            downloaded: false,
            recommended: false,
            english_only: false,
        },
    ]
}

/// Legacy enum for backward compatibility
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
            WhisperModel::Tiny => 75_000_000,
            WhisperModel::Base => 142_000_000,
            WhisperModel::Small => 466_000_000,
            WhisperModel::Medium => 1_500_000_000,
            WhisperModel::Large => 2_900_000_000,
        }
    }

    /// Parse model name from string (legacy support)
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "tiny" => Some(WhisperModel::Tiny),
            "base" => Some(WhisperModel::Base),
            "small" => Some(WhisperModel::Small),
            "medium" => Some(WhisperModel::Medium),
            "large" | "large-v1" | "large-v2" | "large-v3" | "large-v3-turbo" => Some(WhisperModel::Large),
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

/// Buffer size for streaming downloads (8KB)
const DOWNLOAD_BUFFER_SIZE: usize = 8 * 1024;

/// Download a file from URL to the specified path using streaming
/// to avoid loading large files into memory
fn download_file(url: &str, dest_path: &Path) -> Result<(), ModelError> {
    info!("Downloading from {} to {:?}", url, dest_path);

    // Create parent directory if needed
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| ModelError::DirectoryError(e.to_string()))?;
    }

    // Download using blocking reqwest with User-Agent header
    // (some servers reject requests without User-Agent)
    let mut response = reqwest::blocking::Client::builder()
        .user_agent("transcription-app/0.1")
        .build()
        .map_err(|e| ModelError::NetworkError(e.to_string()))?
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

    // Stream download in chunks to avoid OOM for large files
    let mut buffer = vec![0u8; DOWNLOAD_BUFFER_SIZE];
    let mut downloaded: u64 = 0;
    let mut last_progress_log: u64 = 0;

    loop {
        let bytes_read = response.read(&mut buffer)
            .map_err(|e| ModelError::NetworkError(e.to_string()))?;

        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])
            .map_err(|e| ModelError::WriteError(e.to_string()))?;

        downloaded += bytes_read as u64;

        // Log progress every 50MB
        if downloaded - last_progress_log >= 50_000_000 {
            if let Some(total) = total_size {
                let percent = (downloaded as f64 / total as f64) * 100.0;
                info!("Download progress: {:.1}% ({:.1} MB / {:.1} MB)",
                    percent,
                    downloaded as f64 / 1_000_000.0,
                    total as f64 / 1_000_000.0
                );
            } else {
                info!("Downloaded: {:.1} MB", downloaded as f64 / 1_000_000.0);
            }
            last_progress_log = downloaded;
        }
    }

    file.flush()
        .map_err(|e| ModelError::WriteError(e.to_string()))?;

    // Rename temp file to final destination
    fs::rename(&temp_path, dest_path)
        .map_err(|e| ModelError::WriteError(e.to_string()))?;

    info!("Download complete: {:?} ({:.1} MB)", dest_path, downloaded as f64 / 1_000_000.0);
    Ok(())
}

/// Download a Whisper model if not already present (legacy)
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

/// Get all available whisper models with their download status
pub fn get_whisper_models_with_status() -> Vec<WhisperModelInfo> {
    let models_dir = Config::models_dir().ok();

    get_all_whisper_models()
        .into_iter()
        .map(|mut model| {
            if let Some(ref dir) = models_dir {
                let path = dir.join(&model.filename);
                model.downloaded = path.exists();
            }
            model
        })
        .collect()
}

/// Find a model by ID
pub fn find_model_by_id(model_id: &str) -> Option<WhisperModelInfo> {
    get_all_whisper_models()
        .into_iter()
        .find(|m| m.id == model_id)
}

/// Get the filename for a model ID
pub fn get_model_filename(model_id: &str) -> Option<String> {
    find_model_by_id(model_id).map(|m| m.filename)
}

/// Check if a specific whisper model (by ID) is downloaded
pub fn is_whisper_model_downloaded(model_id: &str) -> bool {
    if let Some(model) = find_model_by_id(model_id) {
        if let Ok(models_dir) = Config::models_dir() {
            let path = models_dir.join(&model.filename);
            return path.exists();
        }
    }
    false
}

/// Download a whisper model by ID
pub fn download_whisper_model_by_id(model_id: &str) -> Result<PathBuf> {
    let model = find_model_by_id(model_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown model: {}", model_id))?;

    let models_dir = Config::models_dir()?;
    let model_path = models_dir.join(&model.filename);

    if model_path.exists() {
        info!("Model {} already exists at {:?}", model_id, model_path);
        return Ok(model_path);
    }

    info!("Downloading {} model ({:.1} MB)...", model.label, model.size_bytes as f64 / 1_000_000.0);
    download_file(&model.url, &model_path)
        .context(format!("Failed to download {} model", model.label))?;

    Ok(model_path)
}

/// Test a whisper model by loading it
pub fn test_whisper_model(model_id: &str) -> Result<bool> {
    let model = find_model_by_id(model_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown model: {}", model_id))?;

    let models_dir = Config::models_dir()?;
    let model_path = models_dir.join(&model.filename);

    if !model_path.exists() {
        return Ok(false);
    }

    // Basic validation: check file size is reasonable
    let metadata = fs::metadata(&model_path)
        .context("Failed to read model file metadata")?;

    // Model should be at least 50% of expected size
    let min_size = model.size_bytes / 2;
    if metadata.len() < min_size {
        info!("Model {} file size {} is less than expected minimum {}",
            model_id, metadata.len(), min_size);
        return Ok(false);
    }

    // Check file starts with GGML magic bytes
    let mut file = File::open(&model_path)
        .context("Failed to open model file")?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .context("Failed to read model magic bytes")?;

    // GGML magic bytes - can be big-endian or little-endian depending on version
    let valid_magic = magic == [0x67, 0x67, 0x6d, 0x6c] // "ggml" (big-endian)
        || magic == [0x67, 0x67, 0x6a, 0x74] // "ggjt" (older format)
        || magic == [0x6c, 0x6d, 0x67, 0x67]; // "lmgg" (little-endian, newer whisper.cpp models)

    if !valid_magic {
        info!("Model {} has invalid magic bytes: {:?}", model_id, magic);
        return Ok(false);
    }

    info!("Model {} validated successfully", model_id);
    Ok(true)
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
