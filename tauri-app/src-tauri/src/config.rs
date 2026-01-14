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
    // LLM Router settings for SOAP note generation
    #[serde(default = "default_llm_router_url")]
    pub llm_router_url: String,
    #[serde(default = "default_llm_api_key")]
    pub llm_api_key: String,
    #[serde(default = "default_llm_client_id")]
    pub llm_client_id: String,
    #[serde(default = "default_soap_model")]
    pub soap_model: String,
    #[serde(default = "default_fast_model")]
    pub fast_model: String,
    // Medplum EMR settings
    #[serde(default = "default_medplum_url")]
    pub medplum_server_url: String,
    #[serde(default = "default_medplum_client_id")]
    pub medplum_client_id: String,
    #[serde(default = "default_medplum_auto_sync")]
    pub medplum_auto_sync: bool,
    // Whisper server settings (for remote transcription)
    #[serde(default = "default_whisper_mode")]
    pub whisper_mode: String,
    #[serde(default = "default_whisper_server_url")]
    pub whisper_server_url: String,
    #[serde(default = "default_whisper_server_model")]
    pub whisper_server_model: String,
    // SOAP note generation preferences (persisted)
    #[serde(default = "default_soap_detail_level")]
    pub soap_detail_level: u8,
    #[serde(default = "default_soap_format")]
    pub soap_format: String,
    #[serde(default)]
    pub soap_custom_instructions: String,
    // Auto-session detection settings
    #[serde(default)]
    pub auto_start_enabled: bool,
    #[serde(default = "default_greeting_sensitivity")]
    pub greeting_sensitivity: Option<f32>,
    #[serde(default = "default_min_speech_duration_ms")]
    pub min_speech_duration_ms: Option<u32>,
    // Debug storage (development only - stores PHI locally)
    #[serde(default = "default_debug_storage_enabled")]
    pub debug_storage_enabled: bool,
}

fn default_debug_storage_enabled() -> bool {
    true // Enabled by default for debugging - DISABLE IN PRODUCTION
}

fn default_llm_router_url() -> String {
    "http://127.0.0.1:8080".to_string()
}

fn default_llm_api_key() -> String {
    "ai-scribe-key".to_string()
}

fn default_llm_client_id() -> String {
    "ai-scribe".to_string()
}

fn default_soap_model() -> String {
    "soap-model".to_string()
}

fn default_fast_model() -> String {
    "fast-model".to_string()
}

// Auto-detection defaults
fn default_greeting_sensitivity() -> Option<f32> {
    Some(0.7)
}

fn default_min_speech_duration_ms() -> Option<u32> {
    Some(2000)
}

// SOAP defaults
fn default_soap_detail_level() -> u8 {
    5 // Standard detail level
}

fn default_soap_format() -> String {
    "problem_based".to_string()
}

fn default_whisper_mode() -> String {
    "remote".to_string()  // Always use remote Whisper server
}

fn default_whisper_server_url() -> String {
    "http://172.16.100.45:8001".to_string()
}

fn default_whisper_server_model() -> String {
    "large-v3-turbo".to_string()
}


fn default_medplum_url() -> String {
    "http://172.16.100.45:8103".to_string()
}

fn default_medplum_client_id() -> String {
    // Empty by default - must be configured by user for their Medplum instance
    // This prevents accidental use of a development client ID in production
    String::new()
}

fn default_medplum_auto_sync() -> bool {
    true
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
            llm_router_url: default_llm_router_url(),
            llm_api_key: default_llm_api_key(),
            llm_client_id: default_llm_client_id(),
            soap_model: default_soap_model(),
            fast_model: default_fast_model(),
            medplum_server_url: default_medplum_url(),
            medplum_client_id: default_medplum_client_id(),
            medplum_auto_sync: default_medplum_auto_sync(),
            whisper_mode: default_whisper_mode(),
            whisper_server_url: default_whisper_server_url(),
            whisper_server_model: default_whisper_server_model(),
            soap_detail_level: default_soap_detail_level(),
            soap_format: default_soap_format(),
            soap_custom_instructions: String::new(),
            auto_start_enabled: false,
            greeting_sensitivity: default_greeting_sensitivity(),
            min_speech_duration_ms: default_min_speech_duration_ms(),
            debug_storage_enabled: default_debug_storage_enabled(),
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
    /// Valid whisper model names (for local mode)
    const VALID_MODELS: &'static [&'static str] = &[
        // Standard models
        "tiny", "tiny.en", "base", "base.en", "small", "small.en", "medium", "medium.en",
        // Large models
        "large", "large-v2", "large-v3", "large-v3-turbo",
        // Quantized models
        "large-v3-q5_0", "large-v3-turbo-q5_0",
        // Distil-Whisper models
        "distil-large-v3", "distil-large-v3.en",
    ];

    /// Valid output formats
    const VALID_OUTPUT_FORMATS: &'static [&'static str] = &["paragraphs", "single_paragraph"];

    /// Validate settings and return errors if any
    pub fn validate(&self) -> Vec<SettingsValidationError> {
        let mut errors = Vec::new();

        // Note: Local whisper model validation removed - app uses remote server only

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
    // Enhancement settings
    #[serde(default = "default_enhancement_enabled")]
    pub enhancement_enabled: bool,
    #[serde(default)]
    pub enhancement_model_path: Option<PathBuf>,
    // Biomarker analysis settings
    #[serde(default = "default_biomarkers_enabled")]
    pub biomarkers_enabled: bool,
    #[serde(default)]
    pub yamnet_model_path: Option<PathBuf>,
    // Audio preprocessing settings
    #[serde(default = "default_preprocessing_enabled")]
    pub preprocessing_enabled: bool,
    #[serde(default = "default_preprocessing_highpass_hz")]
    pub preprocessing_highpass_hz: u32,
    #[serde(default = "default_preprocessing_agc_target_rms")]
    pub preprocessing_agc_target_rms: f32,
    // LLM Router settings for SOAP note generation
    #[serde(default = "default_llm_router_url")]
    pub llm_router_url: String,
    #[serde(default = "default_llm_api_key")]
    pub llm_api_key: String,
    #[serde(default = "default_llm_client_id")]
    pub llm_client_id: String,
    #[serde(default = "default_soap_model")]
    pub soap_model: String,
    #[serde(default = "default_fast_model")]
    pub fast_model: String,
    // Medplum EMR settings
    #[serde(default = "default_medplum_url")]
    pub medplum_server_url: String,
    #[serde(default = "default_medplum_client_id")]
    pub medplum_client_id: String,
    #[serde(default = "default_medplum_auto_sync")]
    pub medplum_auto_sync: bool,
    // Whisper server settings (for remote transcription)
    #[serde(default = "default_whisper_mode")]
    pub whisper_mode: String,
    #[serde(default = "default_whisper_server_url")]
    pub whisper_server_url: String,
    #[serde(default = "default_whisper_server_model")]
    pub whisper_server_model: String,
    // SOAP note generation preferences (persisted)
    #[serde(default = "default_soap_detail_level")]
    pub soap_detail_level: u8,
    #[serde(default = "default_soap_format")]
    pub soap_format: String,
    #[serde(default)]
    pub soap_custom_instructions: String,
    // Auto-session detection settings
    #[serde(default)]
    pub auto_start_enabled: bool,
    #[serde(default = "default_greeting_sensitivity")]
    pub greeting_sensitivity: Option<f32>,
    #[serde(default = "default_min_speech_duration_ms")]
    pub min_speech_duration_ms: Option<u32>,
    // Debug storage (development only - stores PHI locally)
    #[serde(default = "default_debug_storage_enabled")]
    pub debug_storage_enabled: bool,
}

fn default_max_speakers() -> usize {
    10
}

fn default_similarity_threshold() -> f32 {
    0.5
}

fn default_enhancement_enabled() -> bool {
    true // GTCRN streaming enhancement enabled by default
}

fn default_biomarkers_enabled() -> bool {
    true // Biomarker analysis enabled by default
}

fn default_preprocessing_enabled() -> bool {
    false // Audio preprocessing disabled - Whisper handles raw audio well
}

fn default_preprocessing_highpass_hz() -> u32 {
    80 // 80Hz cutoff removes power hum and low-frequency rumble
}

fn default_preprocessing_agc_target_rms() -> f32 {
    0.1 // ~-20 dBFS target level for consistent Whisper input
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
            enhancement_enabled: default_enhancement_enabled(),
            enhancement_model_path: None,
            biomarkers_enabled: default_biomarkers_enabled(),
            yamnet_model_path: None,
            preprocessing_enabled: default_preprocessing_enabled(),
            preprocessing_highpass_hz: default_preprocessing_highpass_hz(),
            preprocessing_agc_target_rms: default_preprocessing_agc_target_rms(),
            llm_router_url: default_llm_router_url(),
            llm_api_key: default_llm_api_key(),
            llm_client_id: default_llm_client_id(),
            soap_model: default_soap_model(),
            fast_model: default_fast_model(),
            medplum_server_url: default_medplum_url(),
            medplum_client_id: default_medplum_client_id(),
            medplum_auto_sync: default_medplum_auto_sync(),
            whisper_mode: default_whisper_mode(),
            whisper_server_url: default_whisper_server_url(),
            whisper_server_model: default_whisper_server_model(),
            soap_detail_level: default_soap_detail_level(),
            soap_format: default_soap_format(),
            soap_custom_instructions: String::new(),
            auto_start_enabled: false,
            greeting_sensitivity: default_greeting_sensitivity(),
            min_speech_duration_ms: default_min_speech_duration_ms(),
            debug_storage_enabled: default_debug_storage_enabled(),
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

    /// Get the enhancement model file path
    pub fn get_enhancement_model_path(&self) -> Result<PathBuf> {
        if let Some(ref path) = self.enhancement_model_path {
            return Ok(path.clone());
        }
        let models_dir = Self::models_dir()?;
        Ok(models_dir.join("gtcrn_simple.onnx"))
    }

    /// Get the YAMNet model file path (for biomarker cough detection)
    pub fn get_yamnet_model_path(&self) -> Result<PathBuf> {
        if let Some(ref path) = self.yamnet_model_path {
            return Ok(path.clone());
        }
        let models_dir = Self::models_dir()?;
        Ok(models_dir.join("yamnet.onnx"))
    }

    /// Get the recordings directory for storing audio files
    pub fn get_recordings_dir(&self) -> PathBuf {
        Self::config_dir()
            .map(|d| d.join("recordings"))
            .unwrap_or_else(|_| PathBuf::from("/tmp/transcriptionapp/recordings"))
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
            llm_router_url: self.llm_router_url.clone(),
            llm_api_key: self.llm_api_key.clone(),
            llm_client_id: self.llm_client_id.clone(),
            soap_model: self.soap_model.clone(),
            fast_model: self.fast_model.clone(),
            medplum_server_url: self.medplum_server_url.clone(),
            medplum_client_id: self.medplum_client_id.clone(),
            medplum_auto_sync: self.medplum_auto_sync,
            whisper_mode: self.whisper_mode.clone(),
            whisper_server_url: self.whisper_server_url.clone(),
            whisper_server_model: self.whisper_server_model.clone(),
            soap_detail_level: self.soap_detail_level,
            soap_format: self.soap_format.clone(),
            soap_custom_instructions: self.soap_custom_instructions.clone(),
            auto_start_enabled: self.auto_start_enabled,
            greeting_sensitivity: self.greeting_sensitivity,
            min_speech_duration_ms: self.min_speech_duration_ms,
            debug_storage_enabled: self.debug_storage_enabled,
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
        self.llm_router_url = settings.llm_router_url.clone();
        self.llm_api_key = settings.llm_api_key.clone();
        self.llm_client_id = settings.llm_client_id.clone();
        self.soap_model = settings.soap_model.clone();
        self.fast_model = settings.fast_model.clone();
        self.medplum_server_url = settings.medplum_server_url.clone();
        self.medplum_client_id = settings.medplum_client_id.clone();
        self.medplum_auto_sync = settings.medplum_auto_sync;
        self.whisper_mode = settings.whisper_mode.clone();
        self.whisper_server_url = settings.whisper_server_url.clone();
        self.whisper_server_model = settings.whisper_server_model.clone();
        self.soap_detail_level = settings.soap_detail_level;
        self.soap_format = settings.soap_format.clone();
        self.soap_custom_instructions = settings.soap_custom_instructions.clone();
        // Auto-session detection settings
        self.auto_start_enabled = settings.auto_start_enabled;
        self.greeting_sensitivity = settings.greeting_sensitivity;
        self.min_speech_duration_ms = settings.min_speech_duration_ms;
        // Debug storage
        self.debug_storage_enabled = settings.debug_storage_enabled;
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
            llm_router_url: "http://192.168.1.100:4000".to_string(),
            llm_api_key: "test-api-key".to_string(),
            llm_client_id: "test-client".to_string(),
            soap_model: "soap-model".to_string(),
            fast_model: "fast-model".to_string(),
            medplum_server_url: "http://192.168.1.100:8103".to_string(),
            medplum_client_id: "test-client".to_string(),
            medplum_auto_sync: false,
            whisper_mode: "remote".to_string(),
            whisper_server_url: "http://192.168.1.100:8000".to_string(),
            whisper_server_model: "large-v3".to_string(),
            soap_detail_level: 7,
            soap_format: "comprehensive".to_string(),
            soap_custom_instructions: "Add more detail".to_string(),
            auto_start_enabled: true,
            greeting_sensitivity: Some(0.8),
            min_speech_duration_ms: Some(3000),
            debug_storage_enabled: true,
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
        assert_eq!(config.llm_router_url, "http://192.168.1.100:4000");
        assert_eq!(config.llm_api_key, "test-api-key");
        assert_eq!(config.llm_client_id, "test-client");
        assert_eq!(config.soap_model, "soap-model");
        assert_eq!(config.fast_model, "fast-model");
        assert_eq!(config.medplum_server_url, "http://192.168.1.100:8103");
        assert_eq!(config.medplum_client_id, "test-client");
        assert!(!config.medplum_auto_sync);
        assert_eq!(config.whisper_mode, "remote");
        assert_eq!(config.whisper_server_url, "http://192.168.1.100:8000");
        assert_eq!(config.whisper_server_model, "large-v3");
        assert_eq!(config.soap_detail_level, 7);
        assert_eq!(config.soap_format, "comprehensive");
        assert_eq!(config.soap_custom_instructions, "Add more detail");
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
            llm_router_url: default_llm_router_url(),
            llm_api_key: default_llm_api_key(),
            llm_client_id: default_llm_client_id(),
            soap_model: default_soap_model(),
            fast_model: default_fast_model(),
            medplum_server_url: default_medplum_url(),
            medplum_client_id: String::new(),
            medplum_auto_sync: true,
            whisper_mode: "remote".to_string(),  // Always remote
            whisper_server_url: default_whisper_server_url(),
            whisper_server_model: default_whisper_server_model(),
            soap_detail_level: default_soap_detail_level(),
            soap_format: default_soap_format(),
            soap_custom_instructions: String::new(),
            auto_start_enabled: false,
            greeting_sensitivity: Some(0.7),
            min_speech_duration_ms: Some(2000),
            debug_storage_enabled: true,
        };

        let mut config = Config::default();
        config.input_device_id = Some("old-device".to_string());
        config.update_from_settings(&settings);

        assert!(config.input_device_id.is_none());
    }

    #[test]
    fn test_llm_router_defaults() {
        let config = Config::default();
        assert_eq!(config.llm_router_url, "http://127.0.0.1:8080");
        assert_eq!(config.llm_api_key, "ai-scribe-key");
        assert_eq!(config.llm_client_id, "ai-scribe");
        assert_eq!(config.soap_model, "soap-model");
        assert_eq!(config.fast_model, "fast-model");

        let settings = Settings::default();
        assert_eq!(settings.llm_router_url, "http://127.0.0.1:8080");
        assert_eq!(settings.llm_api_key, "ai-scribe-key");
        assert_eq!(settings.llm_client_id, "ai-scribe");
        assert_eq!(settings.soap_model, "soap-model");
        assert_eq!(settings.fast_model, "fast-model");
    }

    #[test]
    fn test_medplum_defaults() {
        let config = Config::default();
        assert_eq!(config.medplum_server_url, "http://172.16.100.45:8103");
        // Client ID should be empty by default - must be configured by user
        assert!(config.medplum_client_id.is_empty());
        assert!(config.medplum_auto_sync);

        let settings = Settings::default();
        assert_eq!(settings.medplum_server_url, "http://172.16.100.45:8103");
        assert!(settings.medplum_client_id.is_empty());
        assert!(settings.medplum_auto_sync);
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

    #[test]
    fn test_preprocessing_defaults() {
        let config = Config::default();
        assert!(!config.preprocessing_enabled); // Preprocessing disabled by default
        assert_eq!(config.preprocessing_highpass_hz, 80);
        assert!((config.preprocessing_agc_target_rms - 0.1).abs() < 0.001);
    }

    #[test]
    fn test_whisper_server_defaults() {
        let config = Config::default();
        assert_eq!(config.whisper_mode, "remote");  // Always remote
        assert_eq!(config.whisper_server_url, "http://172.16.100.45:8001");
        assert_eq!(config.whisper_server_model, "large-v3-turbo");

        let settings = Settings::default();
        assert_eq!(settings.whisper_mode, "remote");  // Always remote
        assert_eq!(settings.whisper_server_url, "http://172.16.100.45:8001");
        assert_eq!(settings.whisper_server_model, "large-v3-turbo");
    }

    // Settings validation tests
    #[test]
    fn test_settings_validation_valid_defaults() {
        let settings = Settings::default();
        let errors = settings.validate();
        assert!(errors.is_empty(), "Default settings should be valid: {:?}", errors);
        assert!(settings.is_valid());
    }

    #[test]
    fn test_settings_validation_vad_threshold_valid() {
        let mut settings = Settings::default();

        // Test valid values
        settings.vad_threshold = 0.0;
        assert!(settings.validate().is_empty());

        settings.vad_threshold = 0.5;
        assert!(settings.validate().is_empty());

        settings.vad_threshold = 1.0;
        assert!(settings.validate().is_empty());
    }

    #[test]
    fn test_settings_validation_vad_threshold_invalid() {
        let mut settings = Settings::default();

        // Test invalid negative value
        settings.vad_threshold = -0.1;
        let errors = settings.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "vad_threshold"));

        // Test invalid value > 1.0
        settings.vad_threshold = 1.1;
        let errors = settings.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "vad_threshold"));
    }

    #[test]
    fn test_settings_validation_silence_to_flush_valid() {
        let mut settings = Settings::default();

        // Test valid values
        settings.silence_to_flush_ms = 100;
        assert!(settings.validate().is_empty());

        settings.silence_to_flush_ms = 5000;
        assert!(settings.validate().is_empty());
    }

    #[test]
    fn test_settings_validation_silence_to_flush_invalid() {
        let mut settings = Settings::default();

        // Test too low
        settings.silence_to_flush_ms = 50;
        let errors = settings.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "silence_to_flush_ms"));

        // Test too high
        settings.silence_to_flush_ms = 6000;
        let errors = settings.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "silence_to_flush_ms"));
    }

    #[test]
    fn test_settings_validation_max_utterance_valid() {
        let mut settings = Settings::default();
        settings.silence_to_flush_ms = 500;

        // Test valid value
        settings.max_utterance_ms = 29000;
        assert!(settings.validate().is_empty());

        settings.max_utterance_ms = 1000;
        assert!(settings.validate().is_empty());
    }

    #[test]
    fn test_settings_validation_max_utterance_exceeds_limit() {
        let mut settings = Settings::default();

        // Test exceeds Whisper 30s limit
        settings.max_utterance_ms = 30000;
        let errors = settings.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "max_utterance_ms" && e.message.contains("30s limit")));
    }

    #[test]
    fn test_settings_validation_max_utterance_less_than_silence() {
        let mut settings = Settings::default();
        settings.silence_to_flush_ms = 1000;
        settings.max_utterance_ms = 500; // Less than silence_to_flush_ms

        let errors = settings.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "max_utterance_ms" && e.message.contains("greater than silence duration")));
    }

    #[test]
    fn test_settings_validation_max_speakers_valid() {
        let mut settings = Settings::default();

        settings.max_speakers = 1;
        assert!(settings.validate().is_empty());

        settings.max_speakers = 20;
        assert!(settings.validate().is_empty());

        settings.max_speakers = 10;
        assert!(settings.validate().is_empty());
    }

    #[test]
    fn test_settings_validation_max_speakers_invalid() {
        let mut settings = Settings::default();

        // Test zero (out of range)
        settings.max_speakers = 0;
        let errors = settings.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "max_speakers"));

        // Test too high
        settings.max_speakers = 21;
        let errors = settings.validate();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.field == "max_speakers"));
    }

    #[test]
    fn test_settings_validation_multiple_errors() {
        let mut settings = Settings::default();
        settings.vad_threshold = 2.0; // Invalid
        settings.silence_to_flush_ms = 50; // Invalid
        settings.max_speakers = 0; // Invalid

        let errors = settings.validate();
        assert_eq!(errors.len(), 3);
        assert!(!settings.is_valid());
    }

    #[test]
    fn test_settings_validation_error_display() {
        let error = SettingsValidationError {
            field: "test_field".to_string(),
            message: "test message".to_string(),
        };

        assert_eq!(format!("{}", error), "test_field: test message");
    }

    #[test]
    fn test_valid_models_list() {
        // Verify all expected models are in the list
        assert!(Settings::VALID_MODELS.contains(&"tiny"));
        assert!(Settings::VALID_MODELS.contains(&"tiny.en"));
        assert!(Settings::VALID_MODELS.contains(&"base"));
        assert!(Settings::VALID_MODELS.contains(&"base.en"));
        assert!(Settings::VALID_MODELS.contains(&"small"));
        assert!(Settings::VALID_MODELS.contains(&"small.en"));
        assert!(Settings::VALID_MODELS.contains(&"medium"));
        assert!(Settings::VALID_MODELS.contains(&"medium.en"));
        assert!(Settings::VALID_MODELS.contains(&"large"));
        assert!(Settings::VALID_MODELS.contains(&"large-v2"));
        assert!(Settings::VALID_MODELS.contains(&"large-v3"));
        assert!(Settings::VALID_MODELS.contains(&"large-v3-turbo"));
        assert!(Settings::VALID_MODELS.contains(&"large-v3-q5_0"));
        assert!(Settings::VALID_MODELS.contains(&"large-v3-turbo-q5_0"));
        assert!(Settings::VALID_MODELS.contains(&"distil-large-v3"));
        assert!(Settings::VALID_MODELS.contains(&"distil-large-v3.en"));
    }

    #[test]
    fn test_valid_output_formats_list() {
        assert!(Settings::VALID_OUTPUT_FORMATS.contains(&"paragraphs"));
        assert!(Settings::VALID_OUTPUT_FORMATS.contains(&"single_paragraph"));
    }

    #[test]
    fn test_get_enhancement_model_path() {
        let config = Config::default();
        let path = config.get_enhancement_model_path().unwrap();
        assert!(path.to_string_lossy().ends_with("gtcrn_simple.onnx"));
    }

    #[test]
    fn test_get_enhancement_model_path_custom() {
        let mut config = Config::default();
        config.enhancement_model_path = Some(PathBuf::from("/custom/model.onnx"));
        let path = config.get_enhancement_model_path().unwrap();
        assert_eq!(path, PathBuf::from("/custom/model.onnx"));
    }

    #[test]
    fn test_get_yamnet_model_path() {
        let config = Config::default();
        let path = config.get_yamnet_model_path().unwrap();
        assert!(path.to_string_lossy().ends_with("yamnet.onnx"));
    }

    #[test]
    fn test_get_yamnet_model_path_custom() {
        let mut config = Config::default();
        config.yamnet_model_path = Some(PathBuf::from("/custom/yamnet.onnx"));
        let path = config.get_yamnet_model_path().unwrap();
        assert_eq!(path, PathBuf::from("/custom/yamnet.onnx"));
    }

    #[test]
    fn test_get_recordings_dir() {
        let config = Config::default();
        let path = config.get_recordings_dir();
        assert!(path.to_string_lossy().contains("recordings"));
    }

    #[test]
    fn test_soap_defaults() {
        let config = Config::default();
        assert_eq!(config.soap_detail_level, 5);
        assert_eq!(config.soap_format, "problem_based");
        assert!(config.soap_custom_instructions.is_empty());

        let settings = Settings::default();
        assert_eq!(settings.soap_detail_level, 5);
        assert_eq!(settings.soap_format, "problem_based");
        assert!(settings.soap_custom_instructions.is_empty());
    }

    #[test]
    fn test_get_diarization_model_path_custom() {
        let mut config = Config::default();
        config.diarization_model_path = Some(PathBuf::from("/custom/speaker.onnx"));
        let path = config.get_diarization_model_path().unwrap();
        assert_eq!(path, PathBuf::from("/custom/speaker.onnx"));
    }

    #[test]
    fn test_enhancement_and_biomarkers_defaults() {
        let config = Config::default();
        assert!(config.enhancement_enabled);
        assert!(config.biomarkers_enabled);
        assert!(config.enhancement_model_path.is_none());
        assert!(config.yamnet_model_path.is_none());
    }

    #[test]
    fn test_model_status_struct() {
        let status = ModelStatus {
            available: true,
            path: Some("/path/to/model".to_string()),
            error: None,
        };

        assert!(status.available);
        assert_eq!(status.path, Some("/path/to/model".to_string()));
        assert!(status.error.is_none());

        let unavailable_status = ModelStatus {
            available: false,
            path: None,
            error: Some("Model not found".to_string()),
        };

        assert!(!unavailable_status.available);
        assert!(unavailable_status.path.is_none());
        assert_eq!(unavailable_status.error, Some("Model not found".to_string()));
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = Config::default();
        let json = serde_json::to_string(&config).expect("Should serialize");
        let deserialized: Config = serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(config.whisper_model, deserialized.whisper_model);
        assert_eq!(config.language, deserialized.language);
        assert_eq!(config.vad_threshold, deserialized.vad_threshold);
        assert_eq!(config.llm_router_url, deserialized.llm_router_url);
        assert_eq!(config.llm_api_key, deserialized.llm_api_key);
        assert_eq!(config.soap_model, deserialized.soap_model);
        assert_eq!(config.medplum_server_url, deserialized.medplum_server_url);
    }

    #[test]
    fn test_settings_serialization_roundtrip() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).expect("Should serialize");
        let deserialized: Settings = serde_json::from_str(&json).expect("Should deserialize");

        assert_eq!(settings.whisper_model, deserialized.whisper_model);
        assert_eq!(settings.language, deserialized.language);
        assert_eq!(settings.vad_threshold, deserialized.vad_threshold);
    }
}
