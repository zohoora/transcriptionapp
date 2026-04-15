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
        version: 0,
        source: ConfigSource::CompiledDefaults,
    }
}

// ── Cache ────────────────────────────────────────────────────────

fn cache_path() -> PathBuf {
    let base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(".transcriptionapp").join("server_config_cache.json")
}

#[derive(Serialize, Deserialize)]
struct CachedConfig {
    version: u64,
    prompts: PromptTemplates,
    billing: BillingData,
    thresholds: DetectionThresholds,
}

fn save_cache(config: &ServerConfig) -> Result<()> {
    let cached = CachedConfig {
        version: config.version,
        prompts: config.prompts.clone(),
        billing: config.billing.clone(),
        thresholds: config.thresholds.clone(),
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
                source: ConfigSource::Cache,
            });
        }
    }

    // Fetch all three in parallel
    let (prompts_result, billing_result, thresholds_result) = tokio::join!(
        client.get_config_prompts(),
        client.get_config_billing(),
        client.get_config_thresholds(),
    );

    Ok(ServerConfig {
        version: server_version.version,
        prompts: prompts_result?,
        billing: billing_result?,
        thresholds: thresholds_result?,
        source: ConfigSource::Server,
    })
}
