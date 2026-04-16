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

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_urls(primary: &str, fallbacks: Option<Vec<String>>) -> RoomConfig {
        RoomConfig {
            room_name: "Test Room".to_string(),
            profile_server_url: primary.to_string(),
            room_id: None,
            active_physician_id: None,
            profile_api_key: None,
            fallback_server_urls: fallbacks,
        }
    }

    #[test]
    fn test_all_server_urls_primary_only() {
        let config = config_with_urls("http://primary:8090", None);
        assert_eq!(config.all_server_urls(), vec!["http://primary:8090"]);
    }

    #[test]
    fn test_all_server_urls_with_fallbacks() {
        let config = config_with_urls(
            "http://tailscale:8090",
            Some(vec!["http://lan:8090".to_string(), "http://localhost:8090".to_string()]),
        );
        assert_eq!(
            config.all_server_urls(),
            vec!["http://tailscale:8090", "http://lan:8090", "http://localhost:8090"]
        );
    }

    #[test]
    fn test_all_server_urls_primary_first() {
        // Order matters: primary should always be probed first
        let config = config_with_urls(
            "http://first",
            Some(vec!["http://second".to_string(), "http://third".to_string()]),
        );
        let urls = config.all_server_urls();
        assert_eq!(urls[0], "http://first");
        assert_eq!(urls[1], "http://second");
        assert_eq!(urls[2], "http://third");
    }

    #[test]
    fn test_all_server_urls_empty_fallback_vec() {
        // Edge case: explicit empty fallback list should still return primary
        let config = config_with_urls("http://primary:8090", Some(vec![]));
        assert_eq!(config.all_server_urls(), vec!["http://primary:8090"]);
    }

    #[test]
    fn test_serde_roundtrip_with_fallbacks() {
        let config = config_with_urls(
            "http://primary:8090",
            Some(vec!["http://lan:8090".to_string()]),
        );
        let json = serde_json::to_string(&config).unwrap();
        let parsed: RoomConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.all_server_urls(), config.all_server_urls());
    }

    #[test]
    fn test_serde_skips_none_fallbacks() {
        // skip_serializing_if = "Option::is_none" means absent field stays absent
        let config = config_with_urls("http://primary:8090", None);
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("fallback_server_urls"));
    }

    #[test]
    fn test_serde_default_when_field_missing() {
        // Backward compat: old configs without fallback_server_urls should still parse
        let json = r#"{"room_name":"Old","profile_server_url":"http://x:8090"}"#;
        let config: RoomConfig = serde_json::from_str(json).unwrap();
        assert!(config.fallback_server_urls.is_none());
        assert_eq!(config.all_server_urls(), vec!["http://x:8090"]);
    }
}
