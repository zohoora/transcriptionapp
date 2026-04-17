//! Server-configurable data: prompt templates, billing rules, and detection thresholds.
//!
//! At startup, the client fetches these from the profile service's `/config/*` endpoints.
//! Falls back to a disk cache, then to compiled-in defaults.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{info, warn};

use crate::profile_client::ProfileClient;

// ── ConfigSource ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    Server,
    Cache,
    CompiledDefaults,
}

// ── ServerConfig (top-level container) ───────────────────────────

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub prompts: PromptTemplates,
    pub billing: BillingData,
    pub thresholds: DetectionThresholds,
    pub defaults: OperationalDefaults,
    pub version: u64,
    pub source: ConfigSource,
}

// ── ConfigVersion ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVersion {
    pub version: u64,
    pub updated_at: String,
}

// ── PromptTemplates ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptTemplates {
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub soap_base_template: String,
    #[serde(default)]
    pub soap_detail_instructions: HashMap<String, String>,
    #[serde(default)]
    pub soap_format_instructions: HashMap<String, String>,
    #[serde(default)]
    pub soap_custom_section_templates: HashMap<String, String>,
    #[serde(default)]
    pub soap_vision_template: String,
    #[serde(default)]
    pub soap_per_patient_extension: String,
    #[serde(default)]
    pub soap_single_patient_scope_template: String,
    #[serde(default)]
    pub patient_handout: String,
    #[serde(default)]
    pub encounter_detection_system: String,
    #[serde(default)]
    pub encounter_detection_sensor_departed: String,
    #[serde(default)]
    pub encounter_detection_sensor_present: String,
    #[serde(default)]
    pub clinical_content_check: String,
    #[serde(default)]
    pub multi_patient_check: String,
    #[serde(default)]
    pub multi_patient_detect: String,
    #[serde(default)]
    pub multi_patient_split: String,
    #[serde(default)]
    pub encounter_merge_system: String,
    #[serde(default)]
    pub patient_name_system: String,
    #[serde(default)]
    pub patient_name_user: String,
    #[serde(default)]
    pub greeting_detection: String,
    #[serde(default)]
    pub billing_extraction: String,
    #[serde(default)]
    pub patient_merge_correction: String,
}

// ── BillingData ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BillingData {
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub ohip_codes: Vec<OhipCodeEntry>,
    #[serde(default)]
    pub diagnostic_codes: Vec<DiagnosticCodeEntry>,
    #[serde(default)]
    pub exclusion_groups: Vec<ExclusionGroupEntry>,
    #[serde(default)]
    pub visit_type_mappings: HashMap<String, VisitTypeMappingEntry>,
    #[serde(default)]
    pub procedure_type_mappings: HashMap<String, String>,
    #[serde(default)]
    pub condition_type_mappings: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub companion_rules: Vec<CompanionRule>,
    #[serde(default)]
    pub time_rates: Vec<TimeRate>,
    #[serde(default)]
    pub counselling_unit_thresholds: Vec<u64>,
    #[serde(default)]
    pub code_implied_diagnostics: HashMap<String, String>,
    #[serde(default)]
    pub tray_fee_qualifying_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OhipCodeEntry {
    pub code: String,
    pub description: String,
    pub ffs_rate_cents: u32,
    pub basket: String,
    pub shadow_pct: u8,
    pub category: String,
    pub after_hours_eligible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_per_year: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticCodeEntry {
    pub code: String,
    pub description: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExclusionGroupEntry {
    pub name: String,
    pub codes: Vec<String>,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitTypeMappingEntry {
    pub code: String,
    #[serde(default)]
    pub quantity_from_duration: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exhausted_alternative: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionRule {
    pub trigger_code: String,
    pub companion_code: String,
    #[serde(default)]
    pub condition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRate {
    pub code: String,
    #[serde(default)]
    pub description: String,
    pub rate_per_15min_cents: u32,
    pub settings: Vec<String>,
}

// ── DetectionThresholds ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionThresholds {
    #[serde(default)]
    pub version: u64,
    #[serde(default = "default_3000")]
    pub force_check_word_threshold: usize,
    #[serde(default = "default_5000")]
    pub force_split_word_threshold: usize,
    #[serde(default = "default_3")]
    pub force_split_consecutive_limit: u32,
    #[serde(default = "default_25000")]
    pub absolute_word_cap: usize,
    #[serde(default = "default_100")]
    pub min_words_for_clinical_check: usize,
    #[serde(default = "default_90")]
    pub screenshot_stale_grace_secs: i64,
    #[serde(default = "default_2500")]
    pub multi_patient_check_word_threshold: usize,
    #[serde(default = "default_500")]
    pub multi_patient_split_min_words: usize,
    #[serde(default = "default_085")]
    pub confidence_base_short: f64,
    #[serde(default = "default_070")]
    pub confidence_base_long: f64,
    #[serde(default = "default_20")]
    pub confidence_age_threshold_mins: i64,
    #[serde(default = "default_005")]
    pub confidence_merge_back_increment: f64,
    #[serde(default = "default_099")]
    pub confidence_max: f64,
    #[serde(default = "default_300")]
    pub soap_generation_timeout_secs: u64,
    #[serde(default = "default_300")]
    pub billing_extraction_timeout_secs: u64,
    #[serde(default = "default_500")]
    pub merge_excerpt_words: usize,
    #[serde(default = "default_200")]
    pub idle_encounter_max_words: usize,
    #[serde(default = "default_100")]
    pub min_split_word_floor: usize,
    #[serde(default = "default_14f")]
    pub daily_hour_limit: f32,
    #[serde(default = "default_240f")]
    pub monthly_hour_limit: f32,
    #[serde(default = "default_28")]
    pub monthly_window_days: u32,
    // ── Category A extensions (Phase 3) ──
    #[serde(default = "default_mp_detect_word_threshold")]
    pub multi_patient_detect_word_threshold: usize,
    #[serde(default = "default_vision_skip_streak_k")]
    pub vision_skip_streak_k: usize,
    #[serde(default = "default_vision_skip_call_cap")]
    pub vision_skip_call_cap: usize,
    #[serde(default = "default_gemini_generation_timeout_secs")]
    pub gemini_generation_timeout_secs: u64,
}

fn default_3000() -> usize { 3000 }
fn default_5000() -> usize { 5000 }
fn default_3() -> u32 { 3 }
fn default_25000() -> usize { 25000 }
fn default_100() -> usize { 100 }
fn default_90() -> i64 { 90 }
fn default_2500() -> usize { 2500 }
fn default_500() -> usize { 500 }
fn default_085() -> f64 { 0.85 }
fn default_070() -> f64 { 0.70 }
fn default_20() -> i64 { 20 }
fn default_005() -> f64 { 0.05 }
fn default_099() -> f64 { 0.99 }
fn default_300() -> u64 { 300 }
fn default_200() -> usize { 200 }
fn default_14f() -> f32 { 14.0 }
fn default_240f() -> f32 { 240.0 }
fn default_28() -> u32 { 28 }
fn default_mp_detect_word_threshold() -> usize { 500 }
fn default_vision_skip_streak_k() -> usize { 5 }
fn default_vision_skip_call_cap() -> usize { 30 }
fn default_gemini_generation_timeout_secs() -> u64 { 45 }

impl Default for DetectionThresholds {
    fn default() -> Self {
        Self {
            version: 0,
            force_check_word_threshold: 3000,
            force_split_word_threshold: 5000,
            force_split_consecutive_limit: 3,
            absolute_word_cap: 25000,
            min_words_for_clinical_check: 100,
            screenshot_stale_grace_secs: 90,
            multi_patient_check_word_threshold: 2500,
            multi_patient_split_min_words: 500,
            confidence_base_short: 0.85,
            confidence_base_long: 0.70,
            confidence_age_threshold_mins: 20,
            confidence_merge_back_increment: 0.05,
            confidence_max: 0.99,
            soap_generation_timeout_secs: 300,
            billing_extraction_timeout_secs: 300,
            merge_excerpt_words: 500,
            idle_encounter_max_words: 200,
            min_split_word_floor: 100,
            daily_hour_limit: 14.0,
            monthly_hour_limit: 240.0,
            monthly_window_days: 28,
            multi_patient_detect_word_threshold: 500,
            vision_skip_streak_k: 5,
            vision_skip_call_cap: 30,
            gemini_generation_timeout_secs: 45,
        }
    }
}

// ── OperationalDefaults ──────────────────────────────────────────

/// Server-configurable operational defaults (Phase 3).
///
/// Mirrors `profile_service::types::OperationalDefaults` — same field names,
/// types, and compiled defaults. Clients trust the server response and DO NOT
/// validate locally (defense in depth: the server validates on PUT).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationalDefaults {
    #[serde(default)]
    pub version: u64,

    // ── Sleep mode ──
    #[serde(default = "default_sleep_start_hour")]
    pub sleep_start_hour: u8,
    #[serde(default = "default_sleep_end_hour")]
    pub sleep_end_hour: u8,

    // ── Sensor baselines ──
    #[serde(default = "default_thermal_hot_pixel_threshold_c")]
    pub thermal_hot_pixel_threshold_c: f32,
    #[serde(default = "default_co2_baseline_ppm")]
    pub co2_baseline_ppm: f32,

    // ── Encounter detection timing ──
    #[serde(default = "default_encounter_check_interval_secs")]
    pub encounter_check_interval_secs: u32,
    #[serde(default = "default_encounter_silence_trigger_secs")]
    pub encounter_silence_trigger_secs: u32,

    // ── LLM model aliases ──
    #[serde(default = "default_soap_model")]
    pub soap_model: String,
    #[serde(default = "default_soap_model_fast")]
    pub soap_model_fast: String,
    #[serde(default = "default_fast_model")]
    pub fast_model: String,
    #[serde(default = "default_encounter_detection_model")]
    pub encounter_detection_model: String,
}

fn default_sleep_start_hour() -> u8 { 22 }
fn default_sleep_end_hour() -> u8 { 6 }
fn default_thermal_hot_pixel_threshold_c() -> f32 { 28.0 }
fn default_co2_baseline_ppm() -> f32 { 420.0 }
fn default_encounter_check_interval_secs() -> u32 { 120 }
fn default_encounter_silence_trigger_secs() -> u32 { 45 }
fn default_soap_model() -> String { "soap-model-fast".to_string() }
fn default_soap_model_fast() -> String { "soap-model-fast".to_string() }
fn default_fast_model() -> String { "fast-model".to_string() }
fn default_encounter_detection_model() -> String { "fast-model".to_string() }

impl Default for OperationalDefaults {
    fn default() -> Self {
        Self {
            version: 0,
            sleep_start_hour: default_sleep_start_hour(),
            sleep_end_hour: default_sleep_end_hour(),
            thermal_hot_pixel_threshold_c: default_thermal_hot_pixel_threshold_c(),
            co2_baseline_ppm: default_co2_baseline_ppm(),
            encounter_check_interval_secs: default_encounter_check_interval_secs(),
            encounter_silence_trigger_secs: default_encounter_silence_trigger_secs(),
            soap_model: default_soap_model(),
            soap_model_fast: default_soap_model_fast(),
            fast_model: default_fast_model(),
            encounter_detection_model: default_encounter_detection_model(),
        }
    }
}

// ── Compiled defaults ────────────────────────────────────────────

/// Return a ServerConfig with all compiled-in defaults.
/// Used as ultimate fallback when server is unreachable and no cache exists.
pub fn compiled_defaults() -> ServerConfig {
    ServerConfig {
        prompts: PromptTemplates::default(),
        billing: BillingData::default(),
        thresholds: DetectionThresholds::default(),
        defaults: OperationalDefaults::default(),
        version: 0,
        source: ConfigSource::CompiledDefaults,
    }
}

// ── Cache ────────────────────────────────────────────────────────

fn cache_path() -> PathBuf {
    let base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(".transcriptionapp").join("server_config_cache.json")
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct CachedConfig {
    version: u64,
    prompts: PromptTemplates,
    billing: BillingData,
    thresholds: DetectionThresholds,
    #[serde(default)]
    defaults: OperationalDefaults,
}

fn save_cache(config: &ServerConfig) -> Result<()> {
    let cached = CachedConfig {
        version: config.version,
        prompts: config.prompts.clone(),
        billing: config.billing.clone(),
        thresholds: config.thresholds.clone(),
        defaults: config.defaults.clone(),
    };
    let path = cache_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string(&cached)?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, &content)?;
    std::fs::rename(&temp, &path)?;
    Ok(())
}

fn load_cache() -> Option<ServerConfig> {
    let path = cache_path();
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    let cached: CachedConfig = serde_json::from_str(&content).ok()?;
    Some(ServerConfig {
        version: cached.version,
        prompts: cached.prompts,
        billing: cached.billing,
        thresholds: cached.thresholds,
        defaults: cached.defaults,
        source: ConfigSource::Cache,
    })
}

// ── Fetch from server ────────────────────────────────────────────

/// Load server config: try server first, fall back to cache, then compiled defaults.
pub async fn load_server_config(client: &ProfileClient) -> ServerConfig {
    // Try fetching from server
    match fetch_from_server(client).await {
        Ok(config) => {
            info!(version = config.version, "Loaded server config from profile service");
            if let Err(e) = save_cache(&config) {
                warn!("Failed to cache server config: {e}");
            }
            config
        }
        Err(e) => {
            warn!("Failed to fetch server config: {e}");
            // Try cache
            if let Some(cached) = load_cache() {
                info!(version = cached.version, "Using cached server config");
                cached
            } else {
                info!("No server config cache, using compiled defaults");
                compiled_defaults()
            }
        }
    }
}

async fn fetch_from_server(client: &ProfileClient) -> Result<ServerConfig> {
    // Check if we have a cache and if it's still current
    let server_version = client.get_config_version().await?;

    if let Some(cached) = load_cache() {
        if cached.version >= server_version.version {
            return Ok(ServerConfig {
                version: cached.version,
                prompts: cached.prompts,
                billing: cached.billing,
                thresholds: cached.thresholds,
                defaults: cached.defaults,
                source: ConfigSource::Cache,
            });
        }
    }

    // Fetch all four in parallel
    let (prompts_result, billing_result, thresholds_result, defaults_result) = tokio::join!(
        client.get_config_prompts(),
        client.get_config_billing(),
        client.get_config_thresholds(),
        client.get_config_defaults(),
    );

    // Defaults fetch falls back to compiled defaults on error (matches the
    // precedent set by whole-config fallback: never let one missing payload
    // brick the app). Prompts/billing/thresholds still hard-fail because they
    // pre-date Phase 3 and a sudden miss there would signal a real regression.
    let defaults = defaults_result.unwrap_or_else(|e| {
        warn!("Failed to fetch config defaults, using compiled defaults: {e}");
        OperationalDefaults::default()
    });

    Ok(ServerConfig {
        version: server_version.version,
        prompts: prompts_result?,
        billing: billing_result?,
        thresholds: thresholds_result?,
        defaults,
        source: ConfigSource::Server,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compiled_defaults_has_correct_source() {
        let config = compiled_defaults();
        assert!(matches!(config.source, ConfigSource::CompiledDefaults));
        assert_eq!(config.version, 0);
    }

    #[test]
    fn test_compiled_defaults_has_sane_thresholds() {
        let config = compiled_defaults();
        // These are documented production values — sanity check they survive refactors
        assert_eq!(config.thresholds.force_check_word_threshold, 3000);
        assert_eq!(config.thresholds.force_split_word_threshold, 5000);
        assert_eq!(config.thresholds.absolute_word_cap, 25000);
        assert_eq!(config.thresholds.confidence_base_short, 0.85);
        assert_eq!(config.thresholds.confidence_base_long, 0.70);
    }

    #[test]
    fn test_cached_config_roundtrip() {
        // The on-disk cache format must roundtrip cleanly so a saved cache
        // can be read back after an app restart
        let original = CachedConfig {
            version: 42,
            prompts: PromptTemplates::default(),
            billing: BillingData::default(),
            thresholds: DetectionThresholds::default(),
            defaults: OperationalDefaults::default(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: CachedConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 42);
    }

    #[test]
    fn test_cached_config_back_compat() {
        // Old caches that lack newer threshold fields should still parse
        // (DetectionThresholds uses #[serde(default = "default_N")] on each field)
        let json = r#"{
            "version": 1,
            "prompts": {},
            "billing": {},
            "thresholds": {}
        }"#;
        let parsed: CachedConfig = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.version, 1);
        // Defaults should be applied
        assert_eq!(parsed.thresholds.force_check_word_threshold, 3000);
    }

    #[test]
    fn test_config_source_variants() {
        // Ensure all three sources are distinguishable in code
        let server = ServerConfig {
            source: ConfigSource::Server,
            ..compiled_defaults()
        };
        let cache = ServerConfig {
            source: ConfigSource::Cache,
            ..compiled_defaults()
        };
        let defaults = compiled_defaults();
        assert!(matches!(server.source, ConfigSource::Server));
        assert!(matches!(cache.source, ConfigSource::Cache));
        assert!(matches!(defaults.source, ConfigSource::CompiledDefaults));
    }

    // ── OperationalDefaults (Phase 3) ────────────────────────────

    #[test]
    fn test_operational_defaults_back_compat() {
        // Deserializing `{}` must populate every field with its compiled default —
        // guarantees old caches + older server responses still parse after field additions.
        let parsed: OperationalDefaults =
            serde_json::from_str("{}").expect("parses empty object");
        assert_eq!(parsed, OperationalDefaults::default());
    }

    #[test]
    fn test_cached_config_roundtrip_with_defaults() {
        // Full 4-section roundtrip — must survive serde without losing fields.
        // Neither PromptTemplates, BillingData, nor DetectionThresholds derive
        // PartialEq (they carry large nested maps/vecs), so we field-compare
        // on the one section we're actually exercising in this test — the
        // new OperationalDefaults — and sanity-check the version.
        let original_defaults = OperationalDefaults {
            version: 7,
            sleep_start_hour: 21,
            sleep_end_hour: 7,
            thermal_hot_pixel_threshold_c: 29.5,
            co2_baseline_ppm: 450.0,
            encounter_check_interval_secs: 90,
            encounter_silence_trigger_secs: 30,
            soap_model: "custom-soap".to_string(),
            soap_model_fast: "custom-soap-fast".to_string(),
            fast_model: "custom-fast".to_string(),
            encounter_detection_model: "custom-detect".to_string(),
        };
        let original = CachedConfig {
            version: 7,
            prompts: PromptTemplates::default(),
            billing: BillingData::default(),
            thresholds: DetectionThresholds::default(),
            defaults: original_defaults.clone(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: CachedConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 7);
        assert_eq!(parsed.defaults, original_defaults);
        // Sanity-check one threshold field survives (guards against serde
        // accidentally dropping the `thresholds` section entirely).
        assert_eq!(parsed.thresholds.force_check_word_threshold, 3000);
        assert_eq!(parsed.thresholds.multi_patient_detect_word_threshold, 500);
    }

    #[test]
    fn test_cached_config_back_compat_missing_defaults() {
        // Caches written before Phase 3 landed don't have a `defaults` key.
        // CachedConfig#defaults uses `#[serde(default)]` so old caches still load.
        let json = r#"{
            "version": 1,
            "prompts": {},
            "billing": {},
            "thresholds": {}
        }"#;
        let parsed: CachedConfig = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.version, 1);
        // Per-field equality — OperationalDefaults derives PartialEq, so this is clean.
        assert_eq!(parsed.defaults, OperationalDefaults::default());
    }

    #[test]
    fn test_compiled_defaults_has_sane_operational_defaults() {
        // Locks in the 10 compiled operational-default values. If any drifts,
        // this test flips red so a rebuild can't silently change behavior.
        let config = compiled_defaults();
        assert_eq!(config.defaults.sleep_start_hour, 22);
        assert_eq!(config.defaults.sleep_end_hour, 6);
        assert_eq!(config.defaults.thermal_hot_pixel_threshold_c, 28.0);
        assert_eq!(config.defaults.co2_baseline_ppm, 420.0);
        assert_eq!(config.defaults.encounter_check_interval_secs, 120);
        assert_eq!(config.defaults.encounter_silence_trigger_secs, 45);
        assert_eq!(config.defaults.soap_model, "soap-model-fast");
        assert_eq!(config.defaults.soap_model_fast, "soap-model-fast");
        assert_eq!(config.defaults.fast_model, "fast-model");
        assert_eq!(config.defaults.encounter_detection_model, "fast-model");
    }

    #[test]
    fn test_detection_thresholds_back_compat_cat_a_extensions() {
        // Existing Phase 2 thresholds already use per-field serde defaults; the
        // new Cat A fields must do the same so old caches + older server
        // responses don't break after we extend the struct.
        let parsed: DetectionThresholds =
            serde_json::from_str("{}").expect("parses empty object");
        assert_eq!(parsed.multi_patient_detect_word_threshold, 500);
        assert_eq!(parsed.vision_skip_streak_k, 5);
        assert_eq!(parsed.vision_skip_call_cap, 30);
        assert_eq!(parsed.gemini_generation_timeout_secs, 45);
        // And an existing field — defense against accidental renames collapsing
        // back-compat for the whole struct.
        assert_eq!(parsed.force_check_word_threshold, 3000);
    }

    #[test]
    fn test_compiled_defaults_has_sane_cat_a_extensions() {
        let config = compiled_defaults();
        assert_eq!(config.thresholds.multi_patient_detect_word_threshold, 500);
        assert_eq!(config.thresholds.vision_skip_streak_k, 5);
        assert_eq!(config.thresholds.vision_skip_call_cap, 30);
        assert_eq!(config.thresholds.gemini_generation_timeout_secs, 45);
    }
}
