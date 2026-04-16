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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile(id: &str, name: &str) -> PhysicianProfile {
        PhysicianProfile {
            id: id.to_string(),
            name: name.to_string(),
            specialty: Some("Family Medicine".to_string()),
            soap_detail_level: Some(5),
            soap_format: Some("comprehensive".to_string()),
            soap_custom_instructions: None,
            charting_mode: Some("continuous".to_string()),
            language: None,
            image_source: Some("ai".to_string()),
            gemini_api_key: None,
            auto_start_enabled: Some(true),
            auto_start_require_enrolled: Some(false),
            auto_start_required_role: None,
            auto_end_enabled: Some(true),
            auto_end_silence_ms: Some(180_000),
            encounter_merge_enabled: Some(true),
            encounter_check_interval_secs: Some(120),
            encounter_silence_trigger_secs: Some(45),
            medplum_auto_sync: Some(false),
            diarization_enabled: Some(true),
            max_speakers: Some(10),
            medplum_practitioner_id: None,
            billing_default_visit_setting: Some("in_office".to_string()),
            billing_counselling_exhausted: Some(false),
            billing_is_hospital: Some(false),
            created_at: "2026-04-01T12:00:00Z".to_string(),
            updated_at: "2026-04-15T12:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_cached_physicians_roundtrip() {
        // Verify the on-disk format roundtrips cleanly
        let profiles = vec![
            sample_profile("p1", "Dr Alice"),
            sample_profile("p2", "Dr Bob"),
        ];
        let cached = CachedPhysicians {
            cached_at: chrono::Utc::now().to_rfc3339(),
            profiles: profiles.clone(),
        };
        let json = serde_json::to_string_pretty(&cached).unwrap();
        let parsed: CachedPhysicians = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.profiles.len(), 2);
        assert_eq!(parsed.profiles[0].name, "Dr Alice");
        assert_eq!(parsed.profiles[1].name, "Dr Bob");
    }

    #[test]
    fn test_cached_physicians_format_v1_compat() {
        // If the on-disk format is missing optional fields, they should still parse.
        // This guards against breaking older cache files when adding new fields.
        let json = r#"{
            "cached_at": "2026-04-01T12:00:00Z",
            "profiles": [
                {
                    "id": "p1",
                    "name": "Dr Alice",
                    "created_at": "2026-04-01T12:00:00Z",
                    "updated_at": "2026-04-01T12:00:00Z"
                }
            ]
        }"#;
        let parsed: CachedPhysicians = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.profiles.len(), 1);
        assert_eq!(parsed.profiles[0].id, "p1");
        // Missing fields default to None
        assert!(parsed.profiles[0].specialty.is_none());
        assert!(parsed.profiles[0].soap_detail_level.is_none());
    }

    #[test]
    fn test_individual_physician_roundtrip() {
        // Per-physician cache file format
        let profile = sample_profile("p1", "Dr Alice");
        let json = serde_json::to_string_pretty(&profile).unwrap();
        let parsed: PhysicianProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "p1");
        assert_eq!(parsed.name, "Dr Alice");
        assert_eq!(parsed.soap_detail_level, Some(5));
    }
}
