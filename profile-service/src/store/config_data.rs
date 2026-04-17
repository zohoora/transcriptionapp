use crate::error::ApiError;
use crate::types::{
    BillingData, ConfigVersion, DetectionThresholds, OperationalDefaults, PromptTemplates,
};
use chrono::Utc;
use std::path::PathBuf;
use tracing::info;

/// Store for server-configurable data: prompt templates, billing data,
/// detection thresholds, and operational defaults. Each category persists
/// to its own JSON file. A shared version counter bumps on any update for
/// client staleness checks.
pub struct ConfigDataStore {
    prompts: PromptTemplates,
    billing: BillingData,
    thresholds: DetectionThresholds,
    defaults: OperationalDefaults,
    version: u64,
    updated_at: String,
    prompts_path: PathBuf,
    billing_path: PathBuf,
    thresholds_path: PathBuf,
    defaults_path: PathBuf,
    version_path: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct VersionFile {
    version: u64,
    updated_at: String,
}

impl ConfigDataStore {
    pub fn load(data_dir: &std::path::Path) -> Result<Self, ApiError> {
        let prompts_path = data_dir.join("prompt_templates.json");
        let billing_path = data_dir.join("billing_data.json");
        let thresholds_path = data_dir.join("detection_thresholds.json");
        let defaults_path = data_dir.join("operational_defaults.json");
        let version_path = data_dir.join("config_version.json");

        let prompts = Self::load_or_default::<PromptTemplates>(&prompts_path, "prompts")?;
        let billing = Self::load_or_default::<BillingData>(&billing_path, "billing")?;
        let thresholds =
            Self::load_or_default::<DetectionThresholds>(&thresholds_path, "thresholds")?;
        let defaults =
            Self::load_or_default::<OperationalDefaults>(&defaults_path, "operational defaults")?;

        let (version, updated_at) = if version_path.exists() {
            let content = std::fs::read_to_string(&version_path)
                .map_err(|e| ApiError::Internal(format!("Failed to read config version: {e}")))?;
            let vf: VersionFile = serde_json::from_str(&content)
                .map_err(|e| ApiError::Internal(format!("Failed to parse config version: {e}")))?;
            (vf.version, vf.updated_at)
        } else {
            let now = Utc::now().to_rfc3339();
            (1, now)
        };

        let store = Self {
            prompts,
            billing,
            thresholds,
            defaults,
            version,
            updated_at,
            prompts_path,
            billing_path,
            thresholds_path,
            defaults_path,
            version_path,
        };

        // Only write files that didn't exist on disk (first-run seeding)
        if !store.prompts_path.exists() { Self::save_json(&store.prompts_path, &store.prompts)?; }
        if !store.billing_path.exists() { Self::save_json(&store.billing_path, &store.billing)?; }
        if !store.thresholds_path.exists() { Self::save_json(&store.thresholds_path, &store.thresholds)?; }
        if !store.defaults_path.exists() { Self::save_json(&store.defaults_path, &store.defaults)?; }
        if !store.version_path.exists() {
            Self::save_json(&store.version_path, &VersionFile {
                version: store.version, updated_at: store.updated_at.clone(),
            })?;
        }

        info!(
            version = store.version,
            "Loaded config data (prompts, billing, thresholds, operational defaults)"
        );
        Ok(store)
    }

    fn load_or_default<T: serde::de::DeserializeOwned + Default>(
        path: &std::path::Path,
        label: &str,
    ) -> Result<T, ApiError> {
        if path.exists() {
            let content = std::fs::read_to_string(path).map_err(|e| {
                ApiError::Internal(format!("Failed to read {label} config: {e}"))
            })?;
            serde_json::from_str(&content).map_err(|e| {
                ApiError::Internal(format!("Failed to parse {label} config: {e}"))
            })
        } else {
            info!("No {label} config found at {}, using defaults", path.display());
            Ok(T::default())
        }
    }

    fn save_json<T: serde::Serialize>(path: &std::path::Path, data: &T) -> Result<(), ApiError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ApiError::Internal(format!("Failed to create directory: {e}")))?;
        }
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| ApiError::Internal(format!("Failed to serialize: {e}")))?;
        let temp_path = path.with_extension("json.tmp");
        std::fs::write(&temp_path, &content)
            .map_err(|e| ApiError::Internal(format!("Failed to write temp file: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&temp_path, path)
            .map_err(|e| ApiError::Internal(format!("Failed to rename: {e}")))?;
        Ok(())
    }

    fn bump_version(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now().to_rfc3339();
    }

    // ── Getters ──

    pub fn get_version(&self) -> ConfigVersion {
        ConfigVersion {
            version: self.version,
            updated_at: self.updated_at.clone(),
        }
    }

    pub fn get_prompts(&self) -> PromptTemplates {
        self.prompts.clone()
    }

    pub fn get_billing(&self) -> BillingData {
        self.billing.clone()
    }

    pub fn get_thresholds(&self) -> DetectionThresholds {
        self.thresholds.clone()
    }

    pub fn get_defaults(&self) -> OperationalDefaults {
        self.defaults.clone()
    }

    // ── Updaters (full replace) ──

    pub fn update_prompts(
        &mut self,
        prompts: PromptTemplates,
    ) -> Result<PromptTemplates, ApiError> {
        self.prompts = prompts;
        self.bump_version();
        self.prompts.version = self.version;
        Self::save_json(&self.prompts_path, &self.prompts)?;
        Self::save_json(
            &self.version_path,
            &VersionFile {
                version: self.version,
                updated_at: self.updated_at.clone(),
            },
        )?;
        info!(version = self.version, "Updated prompt templates");
        Ok(self.prompts.clone())
    }

    pub fn update_billing(&mut self, billing: BillingData) -> Result<BillingData, ApiError> {
        self.billing = billing;
        self.bump_version();
        self.billing.version = self.version;
        Self::save_json(&self.billing_path, &self.billing)?;
        Self::save_json(
            &self.version_path,
            &VersionFile {
                version: self.version,
                updated_at: self.updated_at.clone(),
            },
        )?;
        info!(version = self.version, "Updated billing data");
        Ok(self.billing.clone())
    }

    pub fn update_thresholds(
        &mut self,
        thresholds: DetectionThresholds,
    ) -> Result<DetectionThresholds, ApiError> {
        self.thresholds = thresholds;
        self.bump_version();
        self.thresholds.version = self.version;
        Self::save_json(&self.thresholds_path, &self.thresholds)?;
        Self::save_json(
            &self.version_path,
            &VersionFile {
                version: self.version,
                updated_at: self.updated_at.clone(),
            },
        )?;
        info!(version = self.version, "Updated detection thresholds");
        Ok(self.thresholds.clone())
    }

    pub fn update_defaults(
        &mut self,
        defaults: OperationalDefaults,
    ) -> Result<OperationalDefaults, ApiError> {
        // Validate before mutating in-memory state so a bad PUT can't
        // stomp a valid config even transiently.
        defaults
            .validate()
            .map_err(ApiError::BadRequest)?;
        self.defaults = defaults;
        self.bump_version();
        self.defaults.version = self.version;
        Self::save_json(&self.defaults_path, &self.defaults)?;
        Self::save_json(
            &self.version_path,
            &VersionFile {
                version: self.version,
                updated_at: self.updated_at.clone(),
            },
        )?;
        info!(version = self.version, "Updated operational defaults");
        Ok(self.defaults.clone())
    }
}
