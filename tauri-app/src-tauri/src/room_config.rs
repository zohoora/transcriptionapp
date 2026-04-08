use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomConfig {
    pub room_name: String,
    pub profile_server_url: String,
    #[serde(default)]
    pub room_id: Option<String>,
    #[serde(default)]
    pub active_physician_id: Option<String>,
    #[serde(default)]
    pub profile_api_key: Option<String>,
    /// Additional server URLs to try if the primary is unreachable (e.g. LAN IP fallback).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_server_urls: Option<Vec<String>>,
}

impl RoomConfig {
    /// Return all server URLs: primary first, then fallbacks.
    pub fn all_server_urls(&self) -> Vec<String> {
        let mut urls = vec![self.profile_server_url.clone()];
        if let Some(ref fallbacks) = self.fallback_server_urls {
            urls.extend(fallbacks.iter().cloned());
        }
        urls
    }
}

impl RoomConfig {
    fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        Ok(home.join(".transcriptionapp").join("room_config.json"))
    }

    pub fn load() -> Result<Option<Self>> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        let config: Self = serde_json::from_str(&content)?;
        info!(room = %config.room_name, "Loaded room config");
        Ok(Some(config))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        let temp_path = path.with_extension("json.tmp");
        std::fs::write(&temp_path, &content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&temp_path, &path)?;
        info!(room = %self.room_name, "Saved room config");
        Ok(())
    }
}
