use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::debug;

/// Map ISO 639-1 language code to the English name expected by the STT server.
/// Empty string means auto-detect (server determines language from audio).
pub fn iso_to_stt_language(iso: &str) -> &str {
    match iso {
        "auto" | "" => "", // Auto-detect — server determines language from audio
        "en" => "English",
        "fr" => "French",
        "fa" | "per" => "Persian",
        "es" => "Spanish",
        "de" => "German",
        "zh" => "Chinese",
        "ar" => "Arabic",
        "hi" => "Hindi",
        "pt" => "Portuguese",
        "it" => "Italian",
        "ja" => "Japanese",
        "ko" => "Korean",
        "ru" => "Russian",
        "nl" => "Dutch",
        "pl" => "Polish",
        "tr" => "Turkish",
        "sv" => "Swedish",
        "da" => "Danish",
        "fi" => "Finnish",
        "el" => "Greek",
        "cs" => "Czech",
        "ro" => "Romanian",
        "hu" => "Hungarian",
        "th" => "Thai",
        "vi" => "Vietnamese",
        "id" => "Indonesian",
        "ms" => "Malay",
        "tl" => "Filipino",
        "mk" => "Macedonian",
        "yue" => "Cantonese",
        _ => "", // Auto-detect for unknown language codes
    }
}

/// Charting mode: session-by-session or continuous all-day recording
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartingMode {
    Session,
    Continuous,
}

impl std::fmt::Display for ChartingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChartingMode::Session => write!(f, "session"),
            ChartingMode::Continuous => write!(f, "continuous"),
        }
    }
}

/// How encounters are detected in continuous mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncounterDetectionMode {
    Llm,
    Sensor,
    Shadow,
    Hybrid,
}

impl std::fmt::Display for EncounterDetectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncounterDetectionMode::Llm => write!(f, "llm"),
            EncounterDetectionMode::Sensor => write!(f, "sensor"),
            EncounterDetectionMode::Shadow => write!(f, "shadow"),
            EncounterDetectionMode::Hybrid => write!(f, "hybrid"),
        }
    }
}

/// Which detection method is "active" in shadow mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowActiveMethod {
    Llm,
    Sensor,
}

impl std::fmt::Display for ShadowActiveMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShadowActiveMethod::Llm => write!(f, "llm"),
            ShadowActiveMethod::Sensor => write!(f, "sensor"),
        }
    }
}

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
    #[serde(default = "default_soap_model_fast")]
    pub soap_model_fast: String,
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
    // STT streaming settings
    #[serde(default = "default_stt_alias")]
    pub stt_alias: String,
    #[serde(default = "default_stt_postprocess")]
    pub stt_postprocess: bool,
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
    // Speaker verification for auto-start
    #[serde(default)]
    pub auto_start_require_enrolled: bool,
    #[serde(default)]
    pub auto_start_required_role: Option<String>,
    // Auto-end session after continuous silence
    #[serde(default = "default_auto_end_enabled")]
    pub auto_end_enabled: bool,
    #[serde(default = "default_auto_end_silence_ms")]
    pub auto_end_silence_ms: u64,
    // Debug storage (development only - stores PHI locally)
    #[serde(default = "default_debug_storage_enabled")]
    pub debug_storage_enabled: bool,
    // MIIS (Medical Illustration Image Server) settings
    #[serde(default)]
    pub miis_enabled: bool,
    #[serde(default = "default_miis_server_url")]
    pub miis_server_url: String,
    // AI image generation settings
    #[serde(default = "default_image_source")]
    pub image_source: String,
    #[serde(default)]
    pub gemini_api_key: String,
    // Screen capture settings
    #[serde(default)]
    pub screen_capture_enabled: bool,
    #[serde(default = "default_screen_capture_interval_secs")]
    pub screen_capture_interval_secs: u32,
    // Continuous charting mode settings
    #[serde(default = "default_charting_mode")]
    pub charting_mode: ChartingMode,
    #[serde(default)]
    pub continuous_auto_copy_soap: bool,
    #[serde(default = "default_encounter_check_interval_secs")]
    pub encounter_check_interval_secs: u32,
    #[serde(default = "default_encounter_silence_trigger_secs")]
    pub encounter_silence_trigger_secs: u32,
    #[serde(default = "default_encounter_merge_enabled")]
    pub encounter_merge_enabled: bool,
    // Hybrid model: use a smaller/faster model for encounter detection
    #[serde(default = "default_encounter_detection_model")]
    pub encounter_detection_model: String,
    #[serde(default = "default_encounter_detection_nothink")]
    pub encounter_detection_nothink: bool,
    // Presence sensor settings (mmWave encounter detection)
    #[serde(default = "default_encounter_detection_mode")]
    pub encounter_detection_mode: EncounterDetectionMode,
    #[serde(default)]
    pub presence_sensor_port: String,
    #[serde(default)]
    pub presence_sensor_url: String,
    #[serde(default = "default_presence_absence_threshold_secs")]
    pub presence_absence_threshold_secs: u64,
    #[serde(default = "default_presence_debounce_secs")]
    pub presence_debounce_secs: u64,
    #[serde(default = "default_presence_csv_log_enabled")]
    pub presence_csv_log_enabled: bool,
    // Shadow mode settings (dual detection comparison)
    #[serde(default = "default_shadow_active_method")]
    pub shadow_active_method: ShadowActiveMethod,
    #[serde(default = "default_shadow_csv_log_enabled")]
    pub shadow_csv_log_enabled: bool,
    // Hybrid detection settings (sensor accelerates LLM confirmation)
    #[serde(default = "default_hybrid_confirm_window_secs")]
    pub hybrid_confirm_window_secs: u64,
    #[serde(default = "default_hybrid_min_words_for_sensor_split")]
    pub hybrid_min_words_for_sensor_split: usize,
    // Sleep mode settings: suppress continuous mode during off-hours
    #[serde(default = "default_sleep_mode_enabled")]
    pub sleep_mode_enabled: bool,
    #[serde(default = "default_sleep_start_hour")]
    pub sleep_start_hour: u8,
    #[serde(default = "default_sleep_end_hour")]
    pub sleep_end_hour: u8,
    // Idle encounter timeout: auto-clear stale buffers with minimal content.
    // After this many seconds with < IDLE_ENCOUNTER_MAX_WORDS, the buffer is
    // discarded as ambient noise. 0 = disabled.
    #[serde(default = "default_idle_encounter_timeout_secs")]
    pub idle_encounter_timeout_secs: u32,
    // Multi-sensor suite settings (thermal + CO2 analysis)
    #[serde(default = "default_thermal_hot_pixel_threshold_c")]
    pub thermal_hot_pixel_threshold_c: f32,
    #[serde(default = "default_co2_baseline_ppm")]
    pub co2_baseline_ppm: f32,
}

fn default_idle_encounter_timeout_secs() -> u32 {
    900 // 15 minutes — clear stale buffers accumulating ambient noise between encounters
}

fn default_thermal_hot_pixel_threshold_c() -> f32 {
    28.0 // Celsius — human body heat threshold for MLX90640
}

fn default_co2_baseline_ppm() -> f32 {
    420.0 // Outdoor/empty room CO2 baseline
}

fn default_hybrid_confirm_window_secs() -> u64 {
    180 // 3 min — allows recovery from brief sensor departures (hand wash, supplies, injections)
}

fn default_hybrid_min_words_for_sensor_split() -> usize {
    500 // Minimum words for sensor timeout to force-split
}

fn default_sleep_mode_enabled() -> bool { true }
fn default_sleep_start_hour() -> u8 { 22 } // 10 PM EST
fn default_sleep_end_hour() -> u8 { 6 }    // 6 AM EST

fn default_shadow_active_method() -> ShadowActiveMethod {
    ShadowActiveMethod::Sensor
}

fn default_shadow_csv_log_enabled() -> bool {
    true
}

fn default_encounter_detection_mode() -> EncounterDetectionMode {
    EncounterDetectionMode::Hybrid
}

fn default_presence_absence_threshold_secs() -> u64 {
    180
}

fn default_presence_debounce_secs() -> u64 {
    15 // 15s — prevents false splits from brief departures (patient shifting, doctor stepping to desk)
}

fn default_presence_csv_log_enabled() -> bool {
    true
}

fn default_encounter_merge_enabled() -> bool {
    true // Auto-merge split encounters by default
}

fn default_encounter_detection_model() -> String {
    "fast-model".to_string() // ~7B model — 1.7B was insufficient (1.6% detection rate)
}

fn default_encounter_detection_nothink() -> bool {
    false // Allow thinking for better reasoning with fast-model (~7B)
}

fn default_stt_alias() -> String {
    "medical-streaming".to_string()
}

fn default_stt_postprocess() -> bool {
    true
}

fn default_charting_mode() -> ChartingMode {
    ChartingMode::Session
}

fn default_encounter_check_interval_secs() -> u32 {
    120 // 2 minutes
}

fn default_encounter_silence_trigger_secs() -> u32 {
    45 // 45 seconds — catches natural patient transitions; LLM detector validates completeness
}

fn default_screen_capture_interval_secs() -> u32 {
    30 // 30 seconds default
}

fn default_miis_server_url() -> String {
    "http://100.119.83.76:7843".to_string()
}

fn default_image_source() -> String {
    "ai".to_string()
}

fn default_auto_end_enabled() -> bool {
    true // Auto-end enabled by default
}

fn default_auto_end_silence_ms() -> u64 {
    180_000 // 3 minutes
}

fn default_debug_storage_enabled() -> bool {
    // Only enabled by default in debug builds - disabled in release/production
    cfg!(debug_assertions)
}

fn default_llm_router_url() -> String {
    // Empty by default - must be configured by user for SOAP generation
    String::new()
}

fn default_llm_api_key() -> String {
    // Empty by default - must be configured by user
    String::new()
}

fn default_llm_client_id() -> String {
    "ai-scribe".to_string()
}

fn default_soap_model() -> String {
    "soap-model-fast".to_string()
}

fn default_soap_model_fast() -> String {
    "soap-model-fast".to_string()
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
    "http://100.119.83.76:8001".to_string()
}

fn default_whisper_server_model() -> String {
    "large-v3-turbo".to_string()
}


fn default_medplum_url() -> String {
    // Empty by default - must be configured by user for EMR integration
    String::new()
}

fn default_medplum_client_id() -> String {
    // Default to the FabricScribe ClientApplication ID
    "af1464aa-e00c-4940-a32e-18d878b7911c".to_string()
}

fn default_medplum_auto_sync() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            whisper_model: "small".to_string(),
            language: "auto".to_string(),
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
            soap_model_fast: default_soap_model_fast(),
            fast_model: default_fast_model(),
            medplum_server_url: default_medplum_url(),
            medplum_client_id: default_medplum_client_id(),
            medplum_auto_sync: default_medplum_auto_sync(),
            whisper_mode: default_whisper_mode(),
            whisper_server_url: default_whisper_server_url(),
            whisper_server_model: default_whisper_server_model(),
            stt_alias: default_stt_alias(),
            stt_postprocess: default_stt_postprocess(),
            soap_detail_level: default_soap_detail_level(),
            soap_format: default_soap_format(),
            soap_custom_instructions: String::new(),
            auto_start_enabled: false,
            greeting_sensitivity: default_greeting_sensitivity(),
            min_speech_duration_ms: default_min_speech_duration_ms(),
            auto_start_require_enrolled: false,
            auto_start_required_role: None,
            auto_end_enabled: default_auto_end_enabled(),
            auto_end_silence_ms: default_auto_end_silence_ms(),
            debug_storage_enabled: default_debug_storage_enabled(),
            miis_enabled: false,
            miis_server_url: default_miis_server_url(),
            image_source: default_image_source(),
            gemini_api_key: String::new(),
            screen_capture_enabled: false,
            screen_capture_interval_secs: default_screen_capture_interval_secs(),
            charting_mode: default_charting_mode(),
            continuous_auto_copy_soap: false,
            encounter_check_interval_secs: default_encounter_check_interval_secs(),
            encounter_silence_trigger_secs: default_encounter_silence_trigger_secs(),
            encounter_merge_enabled: default_encounter_merge_enabled(),
            encounter_detection_model: default_encounter_detection_model(),
            encounter_detection_nothink: default_encounter_detection_nothink(),
            encounter_detection_mode: default_encounter_detection_mode(),
            presence_sensor_port: String::new(),
            presence_sensor_url: String::new(),
            presence_absence_threshold_secs: default_presence_absence_threshold_secs(),
            presence_debounce_secs: default_presence_debounce_secs(),
            presence_csv_log_enabled: default_presence_csv_log_enabled(),
            shadow_active_method: default_shadow_active_method(),
            shadow_csv_log_enabled: default_shadow_csv_log_enabled(),
            hybrid_confirm_window_secs: default_hybrid_confirm_window_secs(),
            hybrid_min_words_for_sensor_split: default_hybrid_min_words_for_sensor_split(),
            sleep_mode_enabled: default_sleep_mode_enabled(),
            sleep_start_hour: default_sleep_start_hour(),
            sleep_end_hour: default_sleep_end_hour(),
            idle_encounter_timeout_secs: default_idle_encounter_timeout_secs(),
            thermal_hot_pixel_threshold_c: default_thermal_hot_pixel_threshold_c(),
            co2_baseline_ppm: default_co2_baseline_ppm(),
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

        // Validate screen capture interval (10-60 seconds)
        if self.screen_capture_interval_secs < 10 || self.screen_capture_interval_secs > 60 {
            errors.push(SettingsValidationError {
                field: "screen_capture_interval_secs".to_string(),
                message: format!(
                    "Screen capture interval {}s is out of range. Must be between 10 and 60 seconds",
                    self.screen_capture_interval_secs
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

        // --- Cross-field and range constraints ---

        // SOAP detail level must be 1-10
        if self.soap_detail_level < 1 || self.soap_detail_level > 10 {
            errors.push(SettingsValidationError {
                field: "soap_detail_level".to_string(),
                message: format!(
                    "SOAP detail level {} is out of range. Must be between 1 and 10",
                    self.soap_detail_level
                ),
            });
        }

        // Auto-end silence must be at least 10 seconds if enabled to prevent accidental sessions
        if self.auto_end_enabled && self.auto_end_silence_ms > 0 && self.auto_end_silence_ms < 10_000 {
            errors.push(SettingsValidationError {
                field: "auto_end_silence_ms".to_string(),
                message: format!(
                    "Auto-end silence {}ms is too short. Must be at least 10000ms (10 seconds) to avoid premature session ends",
                    self.auto_end_silence_ms
                ),
            });
        }

        // Greeting sensitivity must be 0.0-1.0
        if let Some(sensitivity) = self.greeting_sensitivity {
            if !(0.0..=1.0).contains(&sensitivity) {
                errors.push(SettingsValidationError {
                    field: "greeting_sensitivity".to_string(),
                    message: format!(
                        "Greeting sensitivity {} is out of range. Must be between 0.0 and 1.0",
                        sensitivity
                    ),
                });
            }
        }

        // Encounter check interval must be at least 30 seconds
        if self.encounter_check_interval_secs > 0 && self.encounter_check_interval_secs < 30 {
            errors.push(SettingsValidationError {
                field: "encounter_check_interval_secs".to_string(),
                message: format!(
                    "Encounter check interval {}s is too frequent. Must be at least 30 seconds",
                    self.encounter_check_interval_secs
                ),
            });
        }

        // Encounter silence trigger must be at least 10 seconds
        if self.encounter_silence_trigger_secs > 0 && self.encounter_silence_trigger_secs < 10 {
            errors.push(SettingsValidationError {
                field: "encounter_silence_trigger_secs".to_string(),
                message: format!(
                    "Encounter silence trigger {}s is too short. Must be at least 10 seconds",
                    self.encounter_silence_trigger_secs
                ),
            });
        }

        // Sensor-only mode requires a sensor URL or port (shadow/hybrid fall back to LLM gracefully)
        if self.encounter_detection_mode == EncounterDetectionMode::Sensor
            && self.presence_sensor_port.is_empty()
            && self.presence_sensor_url.is_empty()
        {
            errors.push(SettingsValidationError {
                field: "presence_sensor_port".to_string(),
                message: "Sensor mode requires a presence sensor URL or serial port to be configured".to_string(),
            });
        }

        // SOAP format must be a known value
        if self.soap_format != "problem_based" && self.soap_format != "comprehensive" {
            errors.push(SettingsValidationError {
                field: "soap_format".to_string(),
                message: format!(
                    "Unknown SOAP format '{}'. Must be 'problem_based' or 'comprehensive'",
                    self.soap_format
                ),
            });
        }

        errors
    }

    /// Check if settings are valid
    pub fn is_valid(&self) -> bool {
        self.validate().is_empty()
    }
}

// ---------------------------------------------------------------------------
// Phase C: Settings tier classification and physician-tier extraction
// ---------------------------------------------------------------------------

/// Classification tier for each settings field
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsTier {
    Infrastructure,
    Room,
    Physician,
}

/// Physician-tier settings overlay.
///
/// All fields are `Option` so that `None` means "use the room/infrastructure default".
/// When serialized, absent fields are omitted entirely.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicianSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_detail_level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_custom_instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charting_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gemini_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_require_enrolled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_start_required_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_end_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_end_silence_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_merge_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_check_interval_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_silence_trigger_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medplum_auto_sync: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diarization_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_speakers: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medplum_practitioner_id: Option<String>,
}

impl From<&crate::profile_client::PhysicianProfile> for PhysicianSettings {
    fn from(p: &crate::profile_client::PhysicianProfile) -> Self {
        Self {
            soap_detail_level: p.soap_detail_level,
            soap_format: p.soap_format.clone(),
            soap_custom_instructions: p.soap_custom_instructions.clone(),
            charting_mode: p.charting_mode.clone(),
            language: p.language.clone(),
            image_source: p.image_source.clone(),
            gemini_api_key: p.gemini_api_key.clone(),
            auto_start_enabled: p.auto_start_enabled,
            auto_start_require_enrolled: p.auto_start_require_enrolled,
            auto_start_required_role: p.auto_start_required_role.clone(),
            auto_end_enabled: p.auto_end_enabled,
            auto_end_silence_ms: p.auto_end_silence_ms,
            output_format: None,
            encounter_merge_enabled: p.encounter_merge_enabled,
            encounter_check_interval_secs: p.encounter_check_interval_secs,
            encounter_silence_trigger_secs: p.encounter_silence_trigger_secs,
            medplum_auto_sync: p.medplum_auto_sync,
            diarization_enabled: p.diarization_enabled,
            max_speakers: p.max_speakers,
            medplum_practitioner_id: p.medplum_practitioner_id.clone(),
        }
    }
}

/// Infrastructure-tier settings overlay (clinic-wide shared settings).
/// All fields `Option` — `None` means "no server value, keep local default".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InfrastructureOverlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_router_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soap_model_fast: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_server_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_server_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_postprocess: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medplum_server_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medplum_client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub miis_server_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_detection_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_detection_nothink: Option<bool>,
}

/// Room-tier settings overlay (per-room hardware/behavior).
/// All fields `Option` — `None` means "no server value, keep local/infrastructure default".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoomOverlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_detection_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_sensor_port: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_sensor_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_absence_threshold_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_debounce_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal_hot_pixel_threshold_c: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub co2_baseline_ppm: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hybrid_confirm_window_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hybrid_min_words_for_sensor_split: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_encounter_timeout_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_capture_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_capture_interval_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_active_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_csv_log_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_csv_log_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vad_threshold: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silence_to_flush_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_utterance_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greeting_sensitivity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_speech_duration_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_storage_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_device_id: Option<String>,
}

impl Settings {
    /// Extract physician-tier settings from the current flat settings
    pub fn physician(&self) -> PhysicianSettings {
        PhysicianSettings {
            soap_detail_level: Some(self.soap_detail_level),
            soap_format: Some(self.soap_format.clone()),
            soap_custom_instructions: if self.soap_custom_instructions.is_empty() { None } else { Some(self.soap_custom_instructions.clone()) },
            charting_mode: Some(self.charting_mode.to_string()),
            language: Some(self.language.clone()),
            image_source: Some(self.image_source.clone()),
            gemini_api_key: if self.gemini_api_key.is_empty() { None } else { Some(self.gemini_api_key.clone()) },
            auto_start_enabled: Some(self.auto_start_enabled),
            auto_start_require_enrolled: Some(self.auto_start_require_enrolled),
            auto_start_required_role: self.auto_start_required_role.clone(),
            auto_end_enabled: Some(self.auto_end_enabled),
            auto_end_silence_ms: Some(self.auto_end_silence_ms),
            output_format: Some(self.output_format.clone()),
            encounter_merge_enabled: Some(self.encounter_merge_enabled),
            encounter_check_interval_secs: Some(self.encounter_check_interval_secs),
            encounter_silence_trigger_secs: Some(self.encounter_silence_trigger_secs),
            medplum_auto_sync: Some(self.medplum_auto_sync),
            diarization_enabled: Some(self.diarization_enabled),
            max_speakers: Some(self.max_speakers),
            medplum_practitioner_id: None,
        }
    }

    /// Overlay physician settings onto the current settings (non-None fields win)
    pub fn apply_physician(&mut self, phys: &PhysicianSettings) {
        if let Some(v) = phys.soap_detail_level { self.soap_detail_level = v; }
        if let Some(ref v) = phys.soap_format { self.soap_format = v.clone(); }
        if let Some(ref v) = phys.soap_custom_instructions { self.soap_custom_instructions = v.clone(); }
        if let Some(ref v) = phys.charting_mode {
            if v == "continuous" { self.charting_mode = ChartingMode::Continuous; }
            else { self.charting_mode = ChartingMode::Session; }
        }
        if let Some(ref v) = phys.language { self.language = v.clone(); }
        if let Some(ref v) = phys.image_source { self.image_source = v.clone(); }
        if let Some(ref v) = phys.gemini_api_key { self.gemini_api_key = v.clone(); }
        if let Some(v) = phys.auto_start_enabled { self.auto_start_enabled = v; }
        if let Some(v) = phys.auto_start_require_enrolled { self.auto_start_require_enrolled = v; }
        if phys.auto_start_required_role.is_some() { self.auto_start_required_role = phys.auto_start_required_role.clone(); }
        if let Some(v) = phys.auto_end_enabled { self.auto_end_enabled = v; }
        if let Some(v) = phys.auto_end_silence_ms { self.auto_end_silence_ms = v; }
        if let Some(ref v) = phys.output_format { self.output_format = v.clone(); }
        if let Some(v) = phys.encounter_merge_enabled { self.encounter_merge_enabled = v; }
        if let Some(v) = phys.encounter_check_interval_secs { self.encounter_check_interval_secs = v; }
        if let Some(v) = phys.encounter_silence_trigger_secs { self.encounter_silence_trigger_secs = v; }
        if let Some(v) = phys.medplum_auto_sync { self.medplum_auto_sync = v; }
        if let Some(v) = phys.diarization_enabled { self.diarization_enabled = v; }
        if let Some(v) = phys.max_speakers { self.max_speakers = v; }
    }

    /// Extract infrastructure-tier settings from current flat settings
    pub fn infrastructure(&self) -> InfrastructureOverlay {
        InfrastructureOverlay {
            llm_router_url: if self.llm_router_url.is_empty() { None } else { Some(self.llm_router_url.clone()) },
            llm_api_key: if self.llm_api_key.is_empty() { None } else { Some(self.llm_api_key.clone()) },
            llm_client_id: Some(self.llm_client_id.clone()),
            soap_model: Some(self.soap_model.clone()),
            soap_model_fast: Some(self.soap_model_fast.clone()),
            fast_model: Some(self.fast_model.clone()),
            whisper_server_url: if self.whisper_server_url.is_empty() { None } else { Some(self.whisper_server_url.clone()) },
            whisper_server_model: Some(self.whisper_server_model.clone()),
            stt_alias: Some(self.stt_alias.clone()),
            stt_postprocess: Some(self.stt_postprocess),
            medplum_server_url: if self.medplum_server_url.is_empty() { None } else { Some(self.medplum_server_url.clone()) },
            medplum_client_id: Some(self.medplum_client_id.clone()),
            miis_server_url: if self.miis_server_url.is_empty() { None } else { Some(self.miis_server_url.clone()) },
            whisper_mode: Some(self.whisper_mode.clone()),
            encounter_detection_model: Some(self.encounter_detection_model.clone()),
            encounter_detection_nothink: Some(self.encounter_detection_nothink),
        }
    }

    /// Overlay infrastructure settings onto current settings (non-None fields win)
    pub fn apply_infrastructure(&mut self, infra: &InfrastructureOverlay) {
        if let Some(ref v) = infra.llm_router_url { self.llm_router_url = v.clone(); }
        if let Some(ref v) = infra.llm_api_key { self.llm_api_key = v.clone(); }
        if let Some(ref v) = infra.llm_client_id { self.llm_client_id = v.clone(); }
        if let Some(ref v) = infra.soap_model { self.soap_model = v.clone(); }
        if let Some(ref v) = infra.soap_model_fast { self.soap_model_fast = v.clone(); }
        if let Some(ref v) = infra.fast_model { self.fast_model = v.clone(); }
        if let Some(ref v) = infra.whisper_server_url { self.whisper_server_url = v.clone(); }
        if let Some(ref v) = infra.whisper_server_model { self.whisper_server_model = v.clone(); }
        if let Some(ref v) = infra.stt_alias { self.stt_alias = v.clone(); }
        if let Some(v) = infra.stt_postprocess { self.stt_postprocess = v; }
        if let Some(ref v) = infra.medplum_server_url { self.medplum_server_url = v.clone(); }
        if let Some(ref v) = infra.medplum_client_id { self.medplum_client_id = v.clone(); }
        if let Some(ref v) = infra.miis_server_url { self.miis_server_url = v.clone(); }
        if let Some(ref v) = infra.whisper_mode { self.whisper_mode = v.clone(); }
        if let Some(ref v) = infra.encounter_detection_model { self.encounter_detection_model = v.clone(); }
        if let Some(v) = infra.encounter_detection_nothink { self.encounter_detection_nothink = v; }
    }

    /// Extract room-tier settings from current flat settings
    pub fn room(&self) -> RoomOverlay {
        RoomOverlay {
            encounter_detection_mode: Some(self.encounter_detection_mode.to_string()),
            presence_sensor_port: if self.presence_sensor_port.is_empty() { None } else { Some(self.presence_sensor_port.clone()) },
            presence_sensor_url: if self.presence_sensor_url.is_empty() { None } else { Some(self.presence_sensor_url.clone()) },
            presence_absence_threshold_secs: Some(self.presence_absence_threshold_secs),
            presence_debounce_secs: Some(self.presence_debounce_secs),
            thermal_hot_pixel_threshold_c: Some(self.thermal_hot_pixel_threshold_c),
            co2_baseline_ppm: Some(self.co2_baseline_ppm),
            hybrid_confirm_window_secs: Some(self.hybrid_confirm_window_secs),
            hybrid_min_words_for_sensor_split: Some(self.hybrid_min_words_for_sensor_split),
            idle_encounter_timeout_secs: Some(self.idle_encounter_timeout_secs),
            screen_capture_enabled: Some(self.screen_capture_enabled),
            screen_capture_interval_secs: Some(self.screen_capture_interval_secs),
            shadow_active_method: Some(self.shadow_active_method.to_string()),
            shadow_csv_log_enabled: Some(self.shadow_csv_log_enabled),
            presence_csv_log_enabled: Some(self.presence_csv_log_enabled),
            vad_threshold: Some(self.vad_threshold),
            silence_to_flush_ms: Some(self.silence_to_flush_ms),
            max_utterance_ms: Some(self.max_utterance_ms),
            greeting_sensitivity: self.greeting_sensitivity,
            min_speech_duration_ms: self.min_speech_duration_ms,
            whisper_model: Some(self.whisper_model.clone()),
            debug_storage_enabled: Some(self.debug_storage_enabled),
            input_device_id: self.input_device_id.clone(),
        }
    }

    /// Overlay room settings onto current settings (non-None fields win)
    pub fn apply_room(&mut self, room: &RoomOverlay) {
        if let Some(ref v) = room.encounter_detection_mode {
            self.encounter_detection_mode = match v.as_str() {
                "llm" => EncounterDetectionMode::Llm,
                "sensor" => EncounterDetectionMode::Sensor,
                "shadow" => EncounterDetectionMode::Shadow,
                _ => EncounterDetectionMode::Hybrid,
            };
        }
        if let Some(ref v) = room.presence_sensor_port { self.presence_sensor_port = v.clone(); }
        if let Some(ref v) = room.presence_sensor_url { self.presence_sensor_url = v.clone(); }
        if let Some(v) = room.presence_absence_threshold_secs { self.presence_absence_threshold_secs = v; }
        if let Some(v) = room.presence_debounce_secs { self.presence_debounce_secs = v; }
        if let Some(v) = room.thermal_hot_pixel_threshold_c { self.thermal_hot_pixel_threshold_c = v; }
        if let Some(v) = room.co2_baseline_ppm { self.co2_baseline_ppm = v; }
        if let Some(v) = room.hybrid_confirm_window_secs { self.hybrid_confirm_window_secs = v; }
        if let Some(v) = room.hybrid_min_words_for_sensor_split { self.hybrid_min_words_for_sensor_split = v; }
        if let Some(v) = room.idle_encounter_timeout_secs { self.idle_encounter_timeout_secs = v; }
        if let Some(v) = room.screen_capture_enabled { self.screen_capture_enabled = v; }
        if let Some(v) = room.screen_capture_interval_secs { self.screen_capture_interval_secs = v; }
        if let Some(ref v) = room.shadow_active_method {
            self.shadow_active_method = match v.as_str() {
                "sensor" => ShadowActiveMethod::Sensor,
                _ => ShadowActiveMethod::Llm,
            };
        }
        if let Some(v) = room.shadow_csv_log_enabled { self.shadow_csv_log_enabled = v; }
        if let Some(v) = room.presence_csv_log_enabled { self.presence_csv_log_enabled = v; }
        if let Some(v) = room.vad_threshold { self.vad_threshold = v; }
        if let Some(v) = room.silence_to_flush_ms { self.silence_to_flush_ms = v; }
        if let Some(v) = room.max_utterance_ms { self.max_utterance_ms = v; }
        if room.greeting_sensitivity.is_some() { self.greeting_sensitivity = room.greeting_sensitivity; }
        if room.min_speech_duration_ms.is_some() { self.min_speech_duration_ms = room.min_speech_duration_ms; }
        if let Some(ref v) = room.whisper_model { self.whisper_model = v.clone(); }
        if let Some(v) = room.debug_storage_enabled { self.debug_storage_enabled = v; }
        if room.input_device_id.is_some() { self.input_device_id = room.input_device_id.clone(); }
    }

    /// Get the tier classification for each setting field name
    pub fn tier_map() -> std::collections::HashMap<&'static str, SettingsTier> {
        let mut m = std::collections::HashMap::new();
        // Infrastructure
        for field in &["llm_router_url", "llm_api_key", "llm_client_id", "soap_model", "soap_model_fast",
                       "fast_model", "whisper_server_url", "whisper_server_model", "stt_alias", "stt_postprocess",
                       "medplum_server_url", "medplum_client_id", "miis_server_url", "whisper_mode",
                       "encounter_detection_model", "encounter_detection_nothink"] {
            m.insert(*field, SettingsTier::Infrastructure);
        }
        // Room
        for field in &["input_device_id", "encounter_detection_mode", "presence_sensor_port",
                       "presence_sensor_url", "presence_absence_threshold_secs", "presence_debounce_secs",
                       "thermal_hot_pixel_threshold_c", "co2_baseline_ppm", "hybrid_confirm_window_secs",
                       "hybrid_min_words_for_sensor_split", "screen_capture_enabled", "screen_capture_interval_secs",
                       "shadow_active_method", "shadow_csv_log_enabled", "presence_csv_log_enabled",
                       "vad_threshold", "silence_to_flush_ms", "max_utterance_ms", "greeting_sensitivity",
                       "min_speech_duration_ms", "whisper_model", "debug_storage_enabled"] {
            m.insert(*field, SettingsTier::Room);
        }
        // Physician
        for field in &["soap_custom_instructions", "soap_detail_level", "soap_format", "charting_mode",
                       "language", "image_source", "gemini_api_key", "auto_start_enabled",
                       "auto_start_require_enrolled", "auto_start_required_role", "auto_end_enabled",
                       "auto_end_silence_ms", "output_format", "encounter_merge_enabled",
                       "encounter_check_interval_secs", "encounter_silence_trigger_secs",
                       "medplum_auto_sync", "diarization_enabled", "max_speakers"] {
            m.insert(*field, SettingsTier::Physician);
        }
        m
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
    /// All frontend-visible settings, flattened into the same JSON level
    #[serde(flatten)]
    pub settings: Settings,
    // Config-only fields below (not exposed to frontend)
    #[serde(default)]
    pub model_path: Option<PathBuf>,
    #[serde(default)]
    pub diarization_model_path: Option<PathBuf>,
    #[serde(default = "default_similarity_threshold")]
    pub speaker_similarity_threshold: f32,
    #[serde(default = "default_enhancement_enabled")]
    pub enhancement_enabled: bool,
    #[serde(default)]
    pub enhancement_model_path: Option<PathBuf>,
    #[serde(default = "default_biomarkers_enabled")]
    pub biomarkers_enabled: bool,
    #[serde(default)]
    pub yamnet_model_path: Option<PathBuf>,
    #[serde(default = "default_preprocessing_enabled")]
    pub preprocessing_enabled: bool,
    #[serde(default = "default_preprocessing_highpass_hz")]
    pub preprocessing_highpass_hz: u32,
    #[serde(default = "default_preprocessing_agc_target_rms")]
    pub preprocessing_agc_target_rms: f32,
}

impl std::ops::Deref for Config {
    type Target = Settings;
    fn deref(&self) -> &Settings {
        &self.settings
    }
}

impl std::ops::DerefMut for Config {
    fn deref_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }
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
            settings: Settings::default(),
            model_path: None,
            diarization_model_path: None,
            speaker_similarity_threshold: 0.5,
            enhancement_enabled: default_enhancement_enabled(),
            enhancement_model_path: None,
            biomarkers_enabled: default_biomarkers_enabled(),
            yamnet_model_path: None,
            preprocessing_enabled: default_preprocessing_enabled(),
            preprocessing_highpass_hz: default_preprocessing_highpass_hz(),
            preprocessing_agc_target_rms: default_preprocessing_agc_target_rms(),
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

    /// Load config from file or return default.
    /// Clamps out-of-range values to safe defaults to prevent runtime errors.
    pub fn load_or_default() -> Self {
        let mut config = match Self::load() {
            Ok(config) => config,
            Err(e) => {
                debug!("Failed to load config, using default: {}", e);
                Self::default()
            }
        };
        config.clamp_values();

        // Migrate miis_enabled → image_source for backward compatibility
        // If user had miis_enabled=true and no explicit image_source in their config,
        // serde default gives "ai" — override to "miis" to preserve their preference
        if config.miis_enabled && config.image_source == "ai" {
            config.image_source = "miis".to_string();
        }

        config
    }

    /// Clamp values that could cause runtime errors to safe ranges.
    /// This is a safety net for manually edited config files or corrupt state.
    fn clamp_values(&mut self) {
        // speaker_similarity_threshold must be 0.0-1.0 (cosine similarity)
        self.speaker_similarity_threshold = self.speaker_similarity_threshold.clamp(0.0, 1.0);

        // auto_end_silence_ms must be >= 10s if enabled, or 0 to disable
        if self.auto_end_silence_ms > 0 && self.auto_end_silence_ms < 10_000 {
            debug!("Clamping auto_end_silence_ms from {} to 10000", self.auto_end_silence_ms);
            self.auto_end_silence_ms = 10_000;
        }

        // SOAP detail level must be 1-10
        self.soap_detail_level = self.soap_detail_level.clamp(1, 10);

        // vad_threshold must be 0.0-1.0
        self.vad_threshold = self.vad_threshold.clamp(0.0, 1.0);

        // encounter_check_interval_secs must be at least 30
        if self.encounter_check_interval_secs < 30 {
            self.encounter_check_interval_secs = 30;
        }

        // Presence sensor: absence threshold 10-600 seconds
        self.presence_absence_threshold_secs = self.presence_absence_threshold_secs.clamp(10, 600);

        // Presence sensor: debounce 1-60 seconds
        self.presence_debounce_secs = self.presence_debounce_secs.clamp(1, 60);

        // Hybrid: confirm window 30-600 seconds
        self.hybrid_confirm_window_secs = self.hybrid_confirm_window_secs.clamp(30, 600);

        // Hybrid: min words 100-5000
        self.hybrid_min_words_for_sensor_split = self.hybrid_min_words_for_sensor_split.clamp(100, 5000);

        // Sleep mode: hours must be valid (0-23)
        self.sleep_start_hour = self.sleep_start_hour.min(23);
        self.sleep_end_hour = self.sleep_end_hour.min(23);

        // Idle encounter timeout: 0 (disabled) or 300-3600 seconds (5 min - 1 hour)
        if self.idle_encounter_timeout_secs > 0 {
            self.idle_encounter_timeout_secs = self.idle_encounter_timeout_secs.clamp(300, 3600);
        }

        // Thermal: hot pixel threshold 20-40°C
        self.thermal_hot_pixel_threshold_c = self.thermal_hot_pixel_threshold_c.clamp(20.0, 40.0);

        // CO2: baseline 300-600 ppm
        self.co2_baseline_ppm = self.co2_baseline_ppm.clamp(300.0, 600.0);

        // image_source must be "off", "miis", or "ai"
        if !["off", "miis", "ai"].contains(&self.image_source.as_str()) {
            self.image_source = "off".to_string();
        }
    }

    /// Load config from file (with clamping for safety)
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let mut config = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            serde_json::from_str(&content)?
        } else {
            Self::default()
        };
        config.clamp_values();
        Ok(config)
    }

    /// Save config to file with atomic write and strict permissions
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;

        // Atomic write: write to temp file, then rename
        let temp_path = path.with_extension("json.tmp");
        std::fs::write(&temp_path, &content)?;

        // Set strict permissions (600) on Unix - file contains API keys
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&temp_path, permissions)?;
        }

        // Atomic rename (on same filesystem)
        std::fs::rename(&temp_path, &path)?;
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
        self.settings.clone()
    }

    /// Update from frontend Settings
    pub fn update_from_settings(&mut self, settings: &Settings) {
        self.settings = settings.clone();
        // Clamp values to safe ranges after applying settings
        self.clamp_values();
    }

    /// Snapshot of pipeline-relevant config fields for replay bundle logging.
    /// Captures all settings that affect encounter detection, merge, sensor, and SOAP behavior.
    pub fn replay_snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "encounter_detection_mode": self.encounter_detection_mode.to_string(),
            "encounter_detection_model": self.encounter_detection_model,
            "encounter_detection_nothink": self.encounter_detection_nothink,
            "encounter_check_interval_secs": self.encounter_check_interval_secs,
            "encounter_silence_trigger_secs": self.encounter_silence_trigger_secs,
            "encounter_merge_enabled": self.encounter_merge_enabled,
            "hybrid_confirm_window_secs": self.hybrid_confirm_window_secs,
            "hybrid_min_words_for_sensor_split": self.hybrid_min_words_for_sensor_split,
            "presence_sensor_port": self.presence_sensor_port,
            "presence_sensor_url": self.presence_sensor_url,
            "presence_absence_threshold_secs": self.presence_absence_threshold_secs,
            "presence_debounce_secs": self.presence_debounce_secs,
            "soap_model": self.soap_model,
            "soap_model_fast": self.soap_model_fast,
            "soap_detail_level": self.soap_detail_level,
            "soap_format": self.soap_format,
            "soap_custom_instructions": self.soap_custom_instructions,
            "idle_encounter_timeout_secs": self.idle_encounter_timeout_secs,
            "screen_capture_enabled": self.screen_capture_enabled,
            "screen_capture_interval_secs": self.screen_capture_interval_secs,
            "fast_model": self.fast_model,
            "shadow_active_method": self.shadow_active_method.to_string(),
            "thermal_hot_pixel_threshold_c": self.thermal_hot_pixel_threshold_c,
            "co2_baseline_ppm": self.co2_baseline_ppm,
            "sleep_mode_enabled": self.sleep_mode_enabled,
            "sleep_start_hour": self.sleep_start_hour,
            "sleep_end_hour": self.sleep_end_hour,
        })
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
        assert_eq!(config.language, "auto");
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
            soap_model: "soap-model-fast".to_string(),
            soap_model_fast: "soap-model-fast".to_string(),
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
            auto_start_require_enrolled: false,
            auto_start_required_role: None,
            greeting_sensitivity: Some(0.8),
            min_speech_duration_ms: Some(3000),
            auto_end_enabled: true,
            auto_end_silence_ms: 180_000, // 3 minutes
            debug_storage_enabled: true,
            miis_enabled: false,
            miis_server_url: "http://172.16.100.45:7843".to_string(),
            image_source: "off".to_string(),
            gemini_api_key: String::new(),
            screen_capture_enabled: false,
            screen_capture_interval_secs: 30,
            charting_mode: ChartingMode::Session,
            continuous_auto_copy_soap: false,
            encounter_check_interval_secs: 120,
            encounter_silence_trigger_secs: 60,
            encounter_merge_enabled: true,
            encounter_detection_model: default_encounter_detection_model(),
            encounter_detection_nothink: default_encounter_detection_nothink(),
            encounter_detection_mode: default_encounter_detection_mode(),
            presence_sensor_port: String::new(),
            presence_sensor_url: String::new(),
            presence_absence_threshold_secs: default_presence_absence_threshold_secs(),
            presence_debounce_secs: default_presence_debounce_secs(),
            presence_csv_log_enabled: default_presence_csv_log_enabled(),
            shadow_active_method: default_shadow_active_method(),
            shadow_csv_log_enabled: default_shadow_csv_log_enabled(),
            hybrid_confirm_window_secs: default_hybrid_confirm_window_secs(),
            hybrid_min_words_for_sensor_split: default_hybrid_min_words_for_sensor_split(),
            sleep_mode_enabled: default_sleep_mode_enabled(),
            sleep_start_hour: default_sleep_start_hour(),
            sleep_end_hour: default_sleep_end_hour(),
            idle_encounter_timeout_secs: default_idle_encounter_timeout_secs(),
            thermal_hot_pixel_threshold_c: default_thermal_hot_pixel_threshold_c(),
            co2_baseline_ppm: default_co2_baseline_ppm(),
            stt_alias: "medical-streaming".to_string(),
            stt_postprocess: true,
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
        assert_eq!(config.soap_model, "soap-model-fast");
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
            soap_model_fast: default_soap_model_fast(),
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
            auto_start_require_enrolled: false,
            auto_start_required_role: None,
            greeting_sensitivity: Some(0.7),
            min_speech_duration_ms: Some(2000),
            auto_end_enabled: true,
            auto_end_silence_ms: 180_000,
            debug_storage_enabled: true,
            miis_enabled: false,
            miis_server_url: default_miis_server_url(),
            image_source: default_image_source(),
            gemini_api_key: String::new(),
            screen_capture_enabled: false,
            screen_capture_interval_secs: 30,
            charting_mode: default_charting_mode(),
            continuous_auto_copy_soap: false,
            encounter_check_interval_secs: default_encounter_check_interval_secs(),
            encounter_silence_trigger_secs: default_encounter_silence_trigger_secs(),
            encounter_merge_enabled: default_encounter_merge_enabled(),
            encounter_detection_model: default_encounter_detection_model(),
            encounter_detection_nothink: default_encounter_detection_nothink(),
            encounter_detection_mode: default_encounter_detection_mode(),
            presence_sensor_port: String::new(),
            presence_sensor_url: String::new(),
            presence_absence_threshold_secs: default_presence_absence_threshold_secs(),
            presence_debounce_secs: default_presence_debounce_secs(),
            presence_csv_log_enabled: default_presence_csv_log_enabled(),
            shadow_active_method: default_shadow_active_method(),
            shadow_csv_log_enabled: default_shadow_csv_log_enabled(),
            hybrid_confirm_window_secs: default_hybrid_confirm_window_secs(),
            hybrid_min_words_for_sensor_split: default_hybrid_min_words_for_sensor_split(),
            sleep_mode_enabled: default_sleep_mode_enabled(),
            sleep_start_hour: default_sleep_start_hour(),
            sleep_end_hour: default_sleep_end_hour(),
            idle_encounter_timeout_secs: default_idle_encounter_timeout_secs(),
            thermal_hot_pixel_threshold_c: default_thermal_hot_pixel_threshold_c(),
            co2_baseline_ppm: default_co2_baseline_ppm(),
            stt_alias: default_stt_alias(),
            stt_postprocess: default_stt_postprocess(),
        };

        let mut config = Config::default();
        config.input_device_id = Some("old-device".to_string());
        config.update_from_settings(&settings);

        assert!(config.input_device_id.is_none());
    }

    #[test]
    fn test_llm_router_defaults() {
        let config = Config::default();
        // LLM router URL and API key are empty by default - must be configured by user
        assert!(config.llm_router_url.is_empty());
        assert!(config.llm_api_key.is_empty());
        assert_eq!(config.llm_client_id, "ai-scribe");
        assert_eq!(config.soap_model, "soap-model-fast");
        assert_eq!(config.fast_model, "fast-model");

        let settings = Settings::default();
        assert!(settings.llm_router_url.is_empty());
        assert!(settings.llm_api_key.is_empty());
        assert_eq!(settings.llm_client_id, "ai-scribe");
        assert_eq!(settings.soap_model, "soap-model-fast");
        assert_eq!(settings.fast_model, "fast-model");
    }

    #[test]
    fn test_medplum_defaults() {
        let config = Config::default();
        // Medplum server URL is empty by default, client ID has a default
        assert!(config.medplum_server_url.is_empty());
        assert_eq!(config.medplum_client_id, "af1464aa-e00c-4940-a32e-18d878b7911c");
        assert!(config.medplum_auto_sync);

        let settings = Settings::default();
        assert!(settings.medplum_server_url.is_empty());
        assert_eq!(settings.medplum_client_id, "af1464aa-e00c-4940-a32e-18d878b7911c");
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
        assert_eq!(config.whisper_server_url, "http://100.119.83.76:8001");
        assert_eq!(config.whisper_server_model, "large-v3-turbo");
        assert_eq!(config.stt_alias, "medical-streaming");
        assert!(config.stt_postprocess);

        let settings = Settings::default();
        assert_eq!(settings.whisper_mode, "remote");  // Always remote
        assert_eq!(settings.whisper_server_url, "http://100.119.83.76:8001");
        assert_eq!(settings.whisper_server_model, "large-v3-turbo");
        assert_eq!(settings.stt_alias, "medical-streaming");
        assert!(settings.stt_postprocess);
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

    #[test]
    fn test_presence_sensor_defaults() {
        let config = Config::default();
        assert_eq!(config.encounter_detection_mode, EncounterDetectionMode::Hybrid);
        assert!(config.presence_sensor_port.is_empty());
        assert!(config.presence_sensor_url.is_empty());
        assert_eq!(config.presence_absence_threshold_secs, 180);
        assert_eq!(config.presence_debounce_secs, 15);
        assert!(config.presence_csv_log_enabled);

        let settings = Settings::default();
        assert_eq!(settings.encounter_detection_mode, EncounterDetectionMode::Hybrid);
        assert!(settings.presence_sensor_port.is_empty());
        assert!(settings.presence_sensor_url.is_empty());
        assert_eq!(settings.presence_absence_threshold_secs, 180);
        assert_eq!(settings.presence_debounce_secs, 15);
        assert!(settings.presence_csv_log_enabled);
    }

    #[test]
    fn test_presence_sensor_clamping() {
        let mut config = Config::default();

        // Out-of-range absence threshold should be clamped
        config.presence_absence_threshold_secs = 5;
        config.clamp_values();
        assert_eq!(config.presence_absence_threshold_secs, 10);

        config.presence_absence_threshold_secs = 1000;
        config.clamp_values();
        assert_eq!(config.presence_absence_threshold_secs, 600);

        // Out-of-range debounce should be clamped
        config.presence_debounce_secs = 0;
        config.clamp_values();
        assert_eq!(config.presence_debounce_secs, 1);

        config.presence_debounce_secs = 100;
        config.clamp_values();
        assert_eq!(config.presence_debounce_secs, 60);
    }

    #[test]
    fn test_old_config_without_sensor_fields_loads_with_defaults() {
        // Simulate an old config JSON without sensor fields
        // Includes required Settings fields (whisper_model, language, etc.) plus
        // diarization_enabled/max_speakers which Settings requires without serde(default)
        let json = r#"{
            "schema_version": 1,
            "whisper_model": "small",
            "language": "en",
            "output_format": "paragraphs",
            "vad_threshold": 0.5,
            "silence_to_flush_ms": 500,
            "max_utterance_ms": 25000,
            "diarization_enabled": false,
            "max_speakers": 10
        }"#;

        let config: Config = serde_json::from_str(json).expect("Should deserialize old config");
        assert_eq!(config.encounter_detection_mode, EncounterDetectionMode::Hybrid);
        assert!(config.presence_sensor_port.is_empty());
        assert_eq!(config.presence_absence_threshold_secs, 180);
        assert_eq!(config.presence_debounce_secs, 15);
        assert!(config.presence_csv_log_enabled);
    }

    #[test]
    fn test_presence_sensor_settings_roundtrip() {
        let mut config = Config::default();
        config.encounter_detection_mode = EncounterDetectionMode::Sensor;
        config.presence_sensor_port = "/dev/cu.usbserial-2110".to_string();
        config.presence_sensor_url = "http://172.16.100.37".to_string();
        config.presence_absence_threshold_secs = 120;
        config.presence_debounce_secs = 15;
        config.presence_csv_log_enabled = false;

        let settings = config.to_settings();
        assert_eq!(settings.encounter_detection_mode, EncounterDetectionMode::Sensor);
        assert_eq!(settings.presence_sensor_port, "/dev/cu.usbserial-2110");
        assert_eq!(settings.presence_sensor_url, "http://172.16.100.37");
        assert_eq!(settings.presence_absence_threshold_secs, 120);
        assert_eq!(settings.presence_debounce_secs, 15);
        assert!(!settings.presence_csv_log_enabled);

        let mut config2 = Config::default();
        config2.update_from_settings(&settings);
        assert_eq!(config2.encounter_detection_mode, EncounterDetectionMode::Sensor);
        assert_eq!(config2.presence_sensor_port, "/dev/cu.usbserial-2110");
        assert_eq!(config2.presence_sensor_url, "http://172.16.100.37");
        assert_eq!(config2.presence_absence_threshold_secs, 120);
        assert_eq!(config2.presence_debounce_secs, 15);
        assert!(!config2.presence_csv_log_enabled);
    }

    #[test]
    fn test_hybrid_detection_mode_round_trip() {
        let mut config = Config::default();
        config.encounter_detection_mode = EncounterDetectionMode::Hybrid;
        config.hybrid_confirm_window_secs = 120;
        config.hybrid_min_words_for_sensor_split = 300;

        let settings = config.to_settings();
        assert_eq!(settings.encounter_detection_mode, EncounterDetectionMode::Hybrid);
        assert_eq!(settings.hybrid_confirm_window_secs, 120);
        assert_eq!(settings.hybrid_min_words_for_sensor_split, 300);

        // Serialize and deserialize
        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.encounter_detection_mode, EncounterDetectionMode::Hybrid);
        assert_eq!(deserialized.hybrid_confirm_window_secs, 120);
        assert_eq!(deserialized.hybrid_min_words_for_sensor_split, 300);

        // Round-trip through update_from_settings
        let mut config2 = Config::default();
        config2.update_from_settings(&deserialized);
        assert_eq!(config2.encounter_detection_mode, EncounterDetectionMode::Hybrid);
        assert_eq!(config2.hybrid_confirm_window_secs, 120);
        assert_eq!(config2.hybrid_min_words_for_sensor_split, 300);
    }

    #[test]
    fn test_hybrid_config_clamping() {
        let mut config = Config::default();

        // Below minimum
        config.hybrid_confirm_window_secs = 5;
        config.hybrid_min_words_for_sensor_split = 10;
        config.clamp_values();
        assert_eq!(config.hybrid_confirm_window_secs, 30);
        assert_eq!(config.hybrid_min_words_for_sensor_split, 100);

        // Above maximum
        config.hybrid_confirm_window_secs = 9999;
        config.hybrid_min_words_for_sensor_split = 99999;
        config.clamp_values();
        assert_eq!(config.hybrid_confirm_window_secs, 600);
        assert_eq!(config.hybrid_min_words_for_sensor_split, 5000);

        // Within range — unchanged
        config.hybrid_confirm_window_secs = 180;
        config.hybrid_min_words_for_sensor_split = 500;
        config.clamp_values();
        assert_eq!(config.hybrid_confirm_window_secs, 180);
        assert_eq!(config.hybrid_min_words_for_sensor_split, 500);
    }

    #[test]
    fn test_hybrid_mode_no_sensor_port_is_valid() {
        let mut settings = Settings::default();
        settings.encounter_detection_mode = EncounterDetectionMode::Hybrid;
        settings.presence_sensor_port = String::new();
        // Hybrid mode should be valid without sensor port (graceful fallback to LLM)
        let port_errors: Vec<_> = settings.validate().into_iter()
            .filter(|e| e.field == "presence_sensor_port")
            .collect();
        assert!(port_errors.is_empty(), "Hybrid mode should not require sensor port");
    }

    #[test]
    fn test_hybrid_defaults() {
        let config = Config::default();
        assert_eq!(config.encounter_detection_mode, EncounterDetectionMode::Hybrid);
        assert_eq!(config.hybrid_confirm_window_secs, 180);
        assert_eq!(config.hybrid_min_words_for_sensor_split, 500);
    }

    #[test]
    fn test_hybrid_mode_display() {
        assert_eq!(EncounterDetectionMode::Hybrid.to_string(), "hybrid");
    }

    #[test]
    fn test_idle_encounter_timeout_clamping() {
        let mut config = Config::default();

        // Default is 900s (15 min)
        assert_eq!(config.idle_encounter_timeout_secs, 900);

        // 0 means disabled — should stay 0
        config.idle_encounter_timeout_secs = 0;
        config.clamp_values();
        assert_eq!(config.idle_encounter_timeout_secs, 0);

        // Below minimum (300s) — clamped up
        config.idle_encounter_timeout_secs = 60;
        config.clamp_values();
        assert_eq!(config.idle_encounter_timeout_secs, 300);

        // Above maximum (3600s) — clamped down
        config.idle_encounter_timeout_secs = 7200;
        config.clamp_values();
        assert_eq!(config.idle_encounter_timeout_secs, 3600);

        // Within range — unchanged
        config.idle_encounter_timeout_secs = 900;
        config.clamp_values();
        assert_eq!(config.idle_encounter_timeout_secs, 900);
    }

    // ========================================================================
    // replay_snapshot tests
    // ========================================================================

    #[test]
    fn test_replay_snapshot_returns_object() {
        let config = Config::default();
        let snapshot = config.replay_snapshot();
        assert!(snapshot.is_object(), "replay_snapshot should return a JSON object");
    }

    #[test]
    fn test_replay_snapshot_contains_expected_keys() {
        let config = Config::default();
        let snapshot = config.replay_snapshot();
        let obj = snapshot.as_object().unwrap();
        let expected_keys = vec![
            "encounter_detection_mode",
            "encounter_detection_model",
            "encounter_detection_nothink",
            "encounter_check_interval_secs",
            "encounter_silence_trigger_secs",
            "encounter_merge_enabled",
            "hybrid_confirm_window_secs",
            "hybrid_min_words_for_sensor_split",
            "presence_sensor_port",
            "presence_sensor_url",
            "presence_absence_threshold_secs",
            "presence_debounce_secs",
            "soap_model",
            "soap_model_fast",
            "soap_detail_level",
            "soap_format",
            "screen_capture_enabled",
            "screen_capture_interval_secs",
            "fast_model",
            "shadow_active_method",
        ];
        for key in &expected_keys {
            assert!(obj.contains_key(*key), "snapshot missing key: {}", key);
        }
    }

    #[test]
    fn test_replay_snapshot_reflects_config_values() {
        let mut config = Config::default();
        config.encounter_detection_model = "custom-model".to_string();
        config.soap_detail_level = 8;
        config.encounter_check_interval_secs = 90;

        let snapshot = config.replay_snapshot();
        let obj = snapshot.as_object().unwrap();
        assert_eq!(obj["encounter_detection_model"], "custom-model");
        assert_eq!(obj["soap_detail_level"], 8);
        assert_eq!(obj["encounter_check_interval_secs"], 90);
    }
}
