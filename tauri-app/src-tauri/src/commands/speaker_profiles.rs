//! Speaker profile management commands for enrollment-based speaker recognition.

use crate::config::Config;
use crate::speaker_profiles::{SpeakerProfile, SpeakerProfileManager, SpeakerRole};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

#[cfg(feature = "diarization")]
use crate::diarization::{DiarizationConfig, DiarizationProvider};

/// Speaker profile data for frontend (without embedding)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerProfileInfo {
    pub id: String,
    pub name: String,
    pub role: String,
    pub description: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&SpeakerProfile> for SpeakerProfileInfo {
    fn from(profile: &SpeakerProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
            role: role_to_string(&profile.role),
            description: profile.description.clone(),
            created_at: profile.created_at,
            updated_at: profile.updated_at,
        }
    }
}

fn role_to_string(role: &SpeakerRole) -> String {
    match role {
        SpeakerRole::Physician => "physician".to_string(),
        SpeakerRole::Pa => "pa".to_string(),
        SpeakerRole::Rn => "rn".to_string(),
        SpeakerRole::Ma => "ma".to_string(),
        SpeakerRole::Patient => "patient".to_string(),
        SpeakerRole::Other => "other".to_string(),
    }
}

fn string_to_role(s: &str) -> SpeakerRole {
    match s.to_lowercase().as_str() {
        "physician" => SpeakerRole::Physician,
        "pa" => SpeakerRole::Pa,
        "rn" => SpeakerRole::Rn,
        "ma" => SpeakerRole::Ma,
        "patient" => SpeakerRole::Patient,
        _ => SpeakerRole::Other,
    }
}

/// List all speaker profiles
#[tauri::command]
pub fn list_speaker_profiles() -> Result<Vec<SpeakerProfileInfo>, String> {
    let manager = SpeakerProfileManager::load().map_err(|e| {
        error!("Failed to load speaker profiles: {}", e);
        e.to_string()
    })?;

    let profiles: Vec<SpeakerProfileInfo> = manager.list().iter().map(|p| p.into()).collect();
    info!("Listed {} speaker profiles", profiles.len());
    Ok(profiles)
}

/// Get a single speaker profile by ID
#[tauri::command]
pub fn get_speaker_profile(profile_id: String) -> Result<SpeakerProfileInfo, String> {
    let manager = SpeakerProfileManager::load().map_err(|e| e.to_string())?;

    manager
        .get(&profile_id)
        .map(|p| p.into())
        .ok_or_else(|| format!("Speaker profile '{}' not found", profile_id))
}

/// Create a new speaker profile from audio samples
///
/// # Arguments
/// * `name` - Display name (e.g., "Dr. Smith")
/// * `role` - Role string ("physician", "pa", "rn", "ma", "patient", "other")
/// * `description` - Custom description
/// * `audio_samples` - Audio samples at 16kHz mono (5-10 seconds recommended)
#[tauri::command]
pub async fn create_speaker_profile(
    name: String,
    role: String,
    description: String,
    audio_samples: Vec<f32>,
) -> Result<SpeakerProfileInfo, String> {
    // Validate input
    if name.trim().is_empty() {
        return Err("Name cannot be empty".to_string());
    }

    if audio_samples.len() < 16000 {
        return Err("Audio sample too short. Please provide at least 1 second of audio.".to_string());
    }

    if audio_samples.len() < 80000 {
        // Less than 5 seconds
        tracing::warn!(
            "Audio sample is short ({:.1}s). 5-10 seconds recommended for better recognition.",
            audio_samples.len() as f32 / 16000.0
        );
    }

    // Extract embedding using diarization provider
    let embedding = extract_embedding_from_audio(&audio_samples)?;

    // Create profile
    let profile = SpeakerProfile::new(
        name.trim().to_string(),
        string_to_role(&role),
        description.trim().to_string(),
        embedding,
    );

    let profile_info: SpeakerProfileInfo = (&profile).into();

    // Save to storage
    let mut manager = SpeakerProfileManager::load().map_err(|e| e.to_string())?;
    manager.add(profile).map_err(|e| e.to_string())?;

    info!("Created speaker profile: {} ({})", profile_info.name, profile_info.id);
    Ok(profile_info)
}

/// Update an existing speaker profile (metadata only, not embedding)
#[tauri::command]
pub fn update_speaker_profile(
    profile_id: String,
    name: String,
    role: String,
    description: String,
) -> Result<SpeakerProfileInfo, String> {
    if name.trim().is_empty() {
        return Err("Name cannot be empty".to_string());
    }

    let mut manager = SpeakerProfileManager::load().map_err(|e| e.to_string())?;

    let existing = manager
        .get(&profile_id)
        .ok_or_else(|| format!("Speaker profile '{}' not found", profile_id))?;

    let updated = SpeakerProfile {
        id: existing.id.clone(),
        name: name.trim().to_string(),
        role: string_to_role(&role),
        description: description.trim().to_string(),
        embedding: existing.embedding.clone(),
        created_at: existing.created_at,
        updated_at: chrono::Utc::now().timestamp(),
    };

    let profile_info: SpeakerProfileInfo = (&updated).into();
    manager.update(updated).map_err(|e| e.to_string())?;

    info!("Updated speaker profile: {} ({})", profile_info.name, profile_info.id);
    Ok(profile_info)
}

/// Re-enroll a speaker profile with new audio samples
#[tauri::command]
pub async fn reenroll_speaker_profile(
    profile_id: String,
    audio_samples: Vec<f32>,
) -> Result<SpeakerProfileInfo, String> {
    if audio_samples.len() < 16000 {
        return Err("Audio sample too short. Please provide at least 1 second of audio.".to_string());
    }

    // Extract new embedding
    let embedding = extract_embedding_from_audio(&audio_samples)?;

    let mut manager = SpeakerProfileManager::load().map_err(|e| e.to_string())?;

    let existing = manager
        .get(&profile_id)
        .ok_or_else(|| format!("Speaker profile '{}' not found", profile_id))?;

    let updated = SpeakerProfile {
        id: existing.id.clone(),
        name: existing.name.clone(),
        role: existing.role.clone(),
        description: existing.description.clone(),
        embedding,
        created_at: existing.created_at,
        updated_at: chrono::Utc::now().timestamp(),
    };

    let profile_info: SpeakerProfileInfo = (&updated).into();
    manager.update(updated).map_err(|e| e.to_string())?;

    info!("Re-enrolled speaker profile: {} ({})", profile_info.name, profile_info.id);
    Ok(profile_info)
}

/// Delete a speaker profile
#[tauri::command]
pub fn delete_speaker_profile(profile_id: String) -> Result<(), String> {
    let mut manager = SpeakerProfileManager::load().map_err(|e| e.to_string())?;
    manager.delete(&profile_id).map_err(|e| e.to_string())?;
    info!("Deleted speaker profile: {}", profile_id);
    Ok(())
}

/// Extract voice embedding from audio samples
#[cfg(feature = "diarization")]
fn extract_embedding_from_audio(audio: &[f32]) -> Result<Vec<f32>, String> {
    // Load config to get model path
    let config = Config::load_or_default();
    let model_path = config
        .get_diarization_model_path()
        .map_err(|e| format!("Failed to get diarization model path: {}", e))?;

    if !model_path.exists() {
        return Err(format!(
            "Speaker embedding model not found at {:?}. Please download it first.",
            model_path
        ));
    }

    // Create diarization provider for embedding extraction
    let diarization_config = DiarizationConfig {
        model_path,
        max_speakers: 1, // Not used for embedding extraction
        similarity_threshold: 0.5,
        min_similarity: 0.5,
        min_audio_samples: 8000,
        min_energy_threshold: -10.0,
        n_threads: 2,
    };

    let mut provider = DiarizationProvider::new(diarization_config)
        .map_err(|e| format!("Failed to initialize diarization provider: {}", e))?;

    // Extract embedding
    provider
        .extract_embedding(audio)
        .map_err(|e| format!("Failed to extract voice embedding: {}", e))
}

/// Stub when diarization feature is not enabled
#[cfg(not(feature = "diarization"))]
fn extract_embedding_from_audio(_audio: &[f32]) -> Result<Vec<f32>, String> {
    Err("Speaker enrollment requires the diarization feature to be enabled".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_conversion() {
        assert!(matches!(string_to_role("physician"), SpeakerRole::Physician));
        assert!(matches!(string_to_role("PHYSICIAN"), SpeakerRole::Physician));
        assert!(matches!(string_to_role("pa"), SpeakerRole::Pa));
        assert!(matches!(string_to_role("rn"), SpeakerRole::Rn));
        assert!(matches!(string_to_role("ma"), SpeakerRole::Ma));
        assert!(matches!(string_to_role("patient"), SpeakerRole::Patient));
        assert!(matches!(string_to_role("other"), SpeakerRole::Other));
        assert!(matches!(string_to_role("unknown"), SpeakerRole::Other));
    }

    #[test]
    fn test_role_to_string() {
        assert_eq!(role_to_string(&SpeakerRole::Physician), "physician");
        assert_eq!(role_to_string(&SpeakerRole::Pa), "pa");
        assert_eq!(role_to_string(&SpeakerRole::Rn), "rn");
        assert_eq!(role_to_string(&SpeakerRole::Ma), "ma");
        assert_eq!(role_to_string(&SpeakerRole::Patient), "patient");
        assert_eq!(role_to_string(&SpeakerRole::Other), "other");
    }

    #[test]
    fn test_speaker_profile_info_conversion() {
        let profile = SpeakerProfile::new(
            "Dr. Smith".to_string(),
            SpeakerRole::Physician,
            "Internal medicine".to_string(),
            vec![0.1; 256],
        );

        let info: SpeakerProfileInfo = (&profile).into();
        assert_eq!(info.name, "Dr. Smith");
        assert_eq!(info.role, "physician");
        assert_eq!(info.description, "Internal medicine");
    }
}
