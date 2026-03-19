use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

use crate::profile_client::PhysicianProfile;

#[derive(Debug, Serialize, Deserialize)]
struct CachedPhysicians {
    cached_at: String,
    profiles: Vec<PhysicianProfile>,
}

fn cache_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    let dir = home.join(".transcriptionapp").join("cache");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn cache_physicians(profiles: &[PhysicianProfile]) -> Result<()> {
    let path = cache_dir()?.join("physicians.json");
    let cached = CachedPhysicians {
        cached_at: chrono::Utc::now().to_rfc3339(),
        profiles: profiles.to_vec(),
    };
    let content = serde_json::to_string_pretty(&cached)?;
    std::fs::write(&path, content)?;
    info!(count = profiles.len(), "Cached physician profiles");
    Ok(())
}

pub fn load_cached_physicians() -> Result<Vec<PhysicianProfile>> {
    let path = cache_dir()?.join("physicians.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)?;
    let cached: CachedPhysicians = serde_json::from_str(&content)?;
    info!(
        count = cached.profiles.len(),
        cached_at = %cached.cached_at,
        "Loaded cached physicians"
    );
    Ok(cached.profiles)
}

pub fn cache_physician_settings(id: &str, profile: &PhysicianProfile) -> Result<()> {
    let path = cache_dir()?.join(format!("physician_{id}.json"));
    let content = serde_json::to_string_pretty(profile)?;
    std::fs::write(&path, content)?;
    Ok(())
}

pub fn load_cached_physician(id: &str) -> Result<Option<PhysicianProfile>> {
    let path = cache_dir()?.join(format!("physician_{id}.json"));
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let profile: PhysicianProfile = serde_json::from_str(&content)?;
    Ok(Some(profile))
}
