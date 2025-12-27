use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::debug;

/// Settings exposed to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub whisper_model: String,
    pub language: String,
    pub input_device_id: Option<String>,
    pub output_format: String,
    pub vad_threshold: f32,
    pub silence_to_flush_ms: u32,
    pub max_utterance_ms: u32,
    // Diarization settings
    pub diarization_enabled: bool,
    pub max_speakers: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            whisper_model: "small".to_string(),
            language: "en".to_string(),
            input_device_id: None,
            output_format: "paragraphs".to_string(),
            vad_threshold: 0.5,
            silence_to_flush_ms: 500,
            max_utterance_ms: 25000,
            diarization_enabled: false,
            max_speakers: 10,
        }
    }
}

/// Validation error for settings
#[derive(Debug, Clone)]
pub struct SettingsValidationError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for SettingsValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl Settings {
    /// Valid whisper model names
    const VALID_MODELS: &'static [&'static str] = &["tiny", "base", "small", "medium", "large"];

    /// Valid output formats
    const VALID_OUTPUT_FORMATS: &'static [&'static str] = &["paragraphs", "single_paragraph"];

    /// Validate settings and return errors if any
    pub fn validate(&self) -> Vec<SettingsValidationError> {
        let mut errors = Vec::new();

        // Validate whisper model
        if !Self::VALID_MODELS.contains(&self.whisper_model.as_str()) {
            errors.push(SettingsValidationError {
                field: "whisper_model".to_string(),
                message: format!(
                    "Invalid model '{}'. Must be one of: {}",
                    self.whisper_model,
                    Self::VALID_MODELS.join(", ")
                ),
            });
        }

        // Validate VAD threshold (0.0 - 1.0)
        if !(0.0..=1.0).contains(&self.vad_threshold) {
            errors.push(SettingsValidationError {
                field: "vad_threshold".to_string(),
                message: format!(
                    "VAD threshold {} is out of range. Must be between 0.0 and 1.0",
                    self.vad_threshold
                ),
            });
        }

        // Validate silence_to_flush_ms (reasonable range: 100-5000ms)
        if self.silence_to_flush_ms < 100 || self.silence_to_flush_ms > 5000 {
            errors.push(SettingsValidationError {
                field: "silence_to_flush_ms".to_string(),
                message: format!(
                    "Silence duration {}ms is out of range. Must be between 100 and 5000ms",
                    self.silence_to_flush_ms
                ),
            });
        }

        // Validate max_utterance_ms (must be < 30s for Whisper, and > silence_to_flush)
        if self.max_utterance_ms > 29000 {
            errors.push(SettingsValidationError {
                field: "max_utterance_ms".to_string(),
                message: format!(
                    "Max utterance {}ms exceeds Whisper's 30s limit. Must be <= 29000ms",
                    self.max_utterance_ms
                ),
            });
        }
        if self.max_utterance_ms < self.silence_to_flush_ms {
            errors.push(SettingsValidationError {
                field: "max_utterance_ms".to_string(),
                message: format!(
                    "Max utterance {}ms must be greater than silence duration {}ms",
                    self.max_utterance_ms, self.silence_to_flush_ms
                ),
            });
        }

        // Validate max_speakers (reasonable range: 1-20)
        if self.max_speakers < 1 || self.max_speakers > 20 {
            errors.push(SettingsValidationError {
                field: "max_speakers".to_string(),
                message: format!(
                    "Max speakers {} is out of range. Must be between 1 and 20",
                    self.max_speakers
                ),
            });
        }

        // Validate output format
        if !Self::VALID_OUTPUT_FORMATS.contains(&self.output_format.as_str()) {
            // Just warn, don't fail - allow flexibility
            debug!(
                "Unusual output format '{}'. Expected one of: {}",
                self.output_format,
                Self::VALID_OUTPUT_FORMATS.join(", ")
            );
        }

        errors
    }

    /// Check if settings are valid
    pub fn is_valid(&self) -> bool {
        self.validate().is_empty()
    }
}

/// Model availability status for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStatus {
    pub available: bool,
    pub path: Option<String>,
    pub error: Option<String>,
}

/// Internal configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    pub whisper_model: String,
    pub language: String,
    pub input_device_id: Option<String>,
    pub output_format: String,
    pub vad_threshold: f32,
    pub vad_pre_roll_ms: u32,
    pub silence_to_flush_ms: u32,
    pub max_utterance_ms: u32,
    pub model_path: Option<PathBuf>,
    // Diarization settings
    #[serde(default)]
    pub diarization_enabled: bool,
    #[serde(default = "default_max_speakers")]
    pub max_speakers: usize,
    #[serde(default = "default_similarity_threshold")]
    pub speaker_similarity_threshold: f32,
    #[serde(default)]
    pub diarization_model_path: Option<PathBuf>,
}

fn default_max_speakers() -> usize {
    10
}

fn default_similarity_threshold() -> f32 {
    0.5
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            whisper_model: "small".to_string(),
            language: "en".to_string(),
            input_device_id: None,
            output_format: "paragraphs".to_string(),
            vad_threshold: 0.5,
            vad_pre_roll_ms: 300,
            silence_to_flush_ms: 500,
            max_utterance_ms: 25000,
            model_path: None,
            diarization_enabled: false,
            max_speakers: 10,
            speaker_similarity_threshold: 0.5,
            diarization_model_path: None,
        }
    }
}

impl Config {
    /// Get the default config directory
    pub fn config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        Ok(home.join(".transcriptionapp"))
    }

    /// Get the config file path
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.json"))
    }

    /// Get the default models directory
    pub fn models_dir() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("models"))
    }

    /// Load config from file or return default
    pub fn load_or_default() -> Self {
        match Self::load() {
            Ok(config) => config,
            Err(e) => {
                debug!("Failed to load config, using default: {}", e);
                Self::default()
            }
        }
    }

    /// Load config from file
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: Config = serde_json::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Save config to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Get the model file path
    pub fn get_model_path(&self) -> Result<PathBuf> {
        if let Some(ref path) = self.model_path {
            Ok(path.clone())
        } else {
            let models_dir = Self::models_dir()?;
            let filename = format!("ggml-{}.bin", self.whisper_model);
            Ok(models_dir.join(filename))
        }
    }

    /// Get the diarization model file path
    /// Checks for both new name (speaker_embedding.onnx) and legacy name (voxceleb_ECAPA512_LM.onnx)
    pub fn get_diarization_model_path(&self) -> Result<PathBuf> {
        if let Some(ref path) = self.diarization_model_path {
            return Ok(path.clone());
        }

        let models_dir = Self::models_dir()?;

        // Check new name first
        let new_path = models_dir.join("speaker_embedding.onnx");
        if new_path.exists() {
            return Ok(new_path);
        }

        // Check legacy name
        let legacy_path = models_dir.join("voxceleb_ECAPA512_LM.onnx");
        if legacy_path.exists() {
            return Ok(legacy_path);
        }

        // Return new path for download
        Ok(new_path)
    }

    /// Convert to frontend Settings
    pub fn to_settings(&self) -> Settings {
        Settings {
            whisper_model: self.whisper_model.clone(),
            language: self.language.clone(),
            input_device_id: self.input_device_id.clone(),
            output_format: self.output_format.clone(),
            vad_threshold: self.vad_threshold,
            silence_to_flush_ms: self.silence_to_flush_ms,
            max_utterance_ms: self.max_utterance_ms,
            diarization_enabled: self.diarization_enabled,
            max_speakers: self.max_speakers,
        }
    }

    /// Update from frontend Settings
    pub fn update_from_settings(&mut self, settings: &Settings) {
        self.whisper_model = settings.whisper_model.clone();
        self.language = settings.language.clone();
        self.input_device_id = settings.input_device_id.clone();
        self.output_format = settings.output_format.clone();
        self.vad_threshold = settings.vad_threshold;
        self.silence_to_flush_ms = settings.silence_to_flush_ms;
        self.max_utterance_ms = settings.max_utterance_ms;
        self.diarization_enabled = settings.diarization_enabled;
        self.max_speakers = settings.max_speakers;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.whisper_model, "small");
        assert_eq!(config.language, "en");
    }

    #[test]
    fn test_settings_roundtrip() {
        let config = Config::default();
        let settings = config.to_settings();

        let mut config2 = Config::default();
        config2.update_from_settings(&settings);

        assert_eq!(config.whisper_model, config2.whisper_model);
        assert_eq!(config.language, config2.language);
    }

    #[test]
    fn test_default_values() {
        let config = Config::default();
        assert_eq!(config.output_format, "paragraphs");
        assert_eq!(config.vad_threshold, 0.5);
        assert_eq!(config.vad_pre_roll_ms, 300);
        assert_eq!(config.silence_to_flush_ms, 500);
        assert_eq!(config.max_utterance_ms, 25000);
        assert!(config.model_path.is_none());
        assert!(config.input_device_id.is_none());
    }

    #[test]
    fn test_get_model_path_default() {
        let config = Config::default();
        let path = config.get_model_path().unwrap();

        // Should end with ggml-small.bin
        assert!(path.to_string_lossy().ends_with("ggml-small.bin"));
    }

    #[test]
    fn test_get_model_path_custom() {
        let mut config = Config::default();
        config.model_path = Some(PathBuf::from("/custom/path/model.bin"));

        let path = config.get_model_path().unwrap();
        assert_eq!(path, PathBuf::from("/custom/path/model.bin"));
    }

    #[test]
    fn test_get_model_path_different_models() {
        let mut config = Config::default();

        config.whisper_model = "tiny".to_string();
        let path = config.get_model_path().unwrap();
        assert!(path.to_string_lossy().ends_with("ggml-tiny.bin"));

        config.whisper_model = "medium".to_string();
        let path = config.get_model_path().unwrap();
        assert!(path.to_string_lossy().ends_with("ggml-medium.bin"));

        config.whisper_model = "large".to_string();
        let path = config.get_model_path().unwrap();
        assert!(path.to_string_lossy().ends_with("ggml-large.bin"));
    }

    #[test]
    fn test_settings_all_fields() {
        let settings = Settings {
            whisper_model: "medium".to_string(),
            language: "fr".to_string(),
            input_device_id: Some("mic-1".to_string()),
            output_format: "sentences".to_string(),
            vad_threshold: 0.6,
            silence_to_flush_ms: 600,
            max_utterance_ms: 30000,
            diarization_enabled: true,
            max_speakers: 5,
        };

        let mut config = Config::default();
        config.update_from_settings(&settings);

        assert_eq!(config.whisper_model, "medium");
        assert_eq!(config.language, "fr");
        assert_eq!(config.input_device_id, Some("mic-1".to_string()));
        assert_eq!(config.output_format, "sentences");
        assert_eq!(config.vad_threshold, 0.6);
        assert_eq!(config.silence_to_flush_ms, 600);
        assert_eq!(config.max_utterance_ms, 30000);
        assert!(config.diarization_enabled);
        assert_eq!(config.max_speakers, 5);
    }

    #[test]
    fn test_to_settings_preserves_values() {
        let mut config = Config::default();
        config.whisper_model = "large".to_string();
        config.language = "de".to_string();
        config.vad_threshold = 0.7;

        let settings = config.to_settings();

        assert_eq!(settings.whisper_model, "large");
        assert_eq!(settings.language, "de");
        assert_eq!(settings.vad_threshold, 0.7);
    }

    #[test]
    fn test_config_dir() {
        let result = Config::config_dir();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.to_string_lossy().contains(".transcriptionapp"));
    }

    #[test]
    fn test_models_dir() {
        let result = Config::models_dir();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.to_string_lossy().contains("models"));
    }

    #[test]
    fn test_config_path() {
        let result = Config::config_path();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.to_string_lossy().ends_with("config.json"));
    }

    #[test]
    fn test_update_from_settings_none_device() {
        let settings = Settings {
            whisper_model: "small".to_string(),
            language: "en".to_string(),
            input_device_id: None,
            output_format: "paragraphs".to_string(),
            vad_threshold: 0.5,
            silence_to_flush_ms: 500,
            max_utterance_ms: 25000,
            diarization_enabled: false,
            max_speakers: 10,
        };

        let mut config = Config::default();
        config.input_device_id = Some("old-device".to_string());
        config.update_from_settings(&settings);

        assert!(config.input_device_id.is_none());
    }

    #[test]
    fn test_diarization_defaults() {
        let config = Config::default();
        assert!(!config.diarization_enabled);
        assert_eq!(config.max_speakers, 10);
        assert_eq!(config.speaker_similarity_threshold, 0.5);
        assert!(config.diarization_model_path.is_none());
    }

    #[test]
    fn test_get_diarization_model_path() {
        let config = Config::default();
        let path = config.get_diarization_model_path().unwrap();
        // New default is speaker_embedding.onnx, but also accepts legacy voxceleb_ECAPA512_LM.onnx
        assert!(
            path.to_string_lossy().ends_with("speaker_embedding.onnx")
                || path.to_string_lossy().ends_with("voxceleb_ECAPA512_LM.onnx")
        );
    }

    #[test]
    fn test_load_or_default_returns_default() {
        // When no config file exists, should return default
        let config = Config::load_or_default();
        assert_eq!(config.schema_version, 1);
    }
}
