use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: u32,
    pub engine_mode: EngineMode,
    pub whisper_model: WhisperModelType,
    pub language: String,
    pub input_device_id: Option<String>,
    pub output_format: OutputFormat,

    // VAD tuning
    pub vad_threshold: f32,
    pub vad_pre_roll_ms: u32,
    pub silence_to_flush_ms: u32,
    pub max_utterance_ms: u32,

    // Model path
    pub model_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            engine_mode: EngineMode::Whisper,
            whisper_model: WhisperModelType::Small,
            language: "en".to_string(),
            input_device_id: None,
            output_format: OutputFormat::Paragraphs,
            vad_threshold: 0.5,
            vad_pre_roll_ms: 300,
            silence_to_flush_ms: 500,
            max_utterance_ms: 25000,
            model_path: None,
        }
    }
}

impl Config {
    /// Load config from file, or create default
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .context("Failed to read config file")?;
            serde_json::from_str(&content)
                .context("Failed to parse config file")
        } else {
            Ok(Self::default())
        }
    }

    /// Save config to file
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;
        std::fs::write(path, content)
            .context("Failed to write config file")
    }

    /// Get the default config directory
    pub fn default_config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Failed to get home directory")?;
        Ok(home.join(".transcriptionapp"))
    }

    /// Get the default models directory
    pub fn default_models_dir() -> Result<PathBuf> {
        Ok(Self::default_config_dir()?.join("models"))
    }

    /// Get the model file path
    pub fn get_model_path(&self) -> Result<PathBuf> {
        if let Some(ref path) = self.model_path {
            Ok(path.clone())
        } else {
            let models_dir = Self::default_models_dir()?;
            Ok(models_dir.join(self.whisper_model.filename()))
        }
    }
}

/// Engine mode (currently only Whisper is implemented)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineMode {
    Whisper,
    Apple, // Stub for future
}

impl Default for EngineMode {
    fn default() -> Self {
        Self::Whisper
    }
}

/// Whisper model type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WhisperModelType {
    Tiny,
    Base,
    Small,
    Medium,
    Large,
}

impl Default for WhisperModelType {
    fn default() -> Self {
        Self::Small
    }
}

impl WhisperModelType {
    pub fn filename(&self) -> &'static str {
        match self {
            Self::Tiny => "ggml-tiny.bin",
            Self::Base => "ggml-base.bin",
            Self::Small => "ggml-small.bin",
            Self::Medium => "ggml-medium.bin",
            Self::Large => "ggml-large.bin",
        }
    }
}

impl std::str::FromStr for WhisperModelType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tiny" => Ok(Self::Tiny),
            "base" => Ok(Self::Base),
            "small" => Ok(Self::Small),
            "medium" => Ok(Self::Medium),
            "large" => Ok(Self::Large),
            _ => Err(format!("Unknown model type: {}", s)),
        }
    }
}

/// Output format for transcript
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Paragraphs,
    SingleParagraph,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Paragraphs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.vad_threshold, 0.5);
        assert_eq!(config.language, "en");
    }

    #[test]
    fn test_model_filename() {
        assert_eq!(WhisperModelType::Small.filename(), "ggml-small.bin");
        assert_eq!(WhisperModelType::Tiny.filename(), "ggml-tiny.bin");
    }

    #[test]
    fn test_model_type_parse() {
        assert_eq!("small".parse::<WhisperModelType>().unwrap(), WhisperModelType::Small);
        assert_eq!("TINY".parse::<WhisperModelType>().unwrap(), WhisperModelType::Tiny);
    }
}
