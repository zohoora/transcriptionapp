//! Speaker profile management for enrollment-based speaker recognition.
//!
//! This module provides storage and management for enrolled speaker profiles,
//! allowing the diarization system to recognize known speakers by name.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Current schema version for speaker profiles storage
const SCHEMA_VERSION: u32 = 1;

/// Speaker role in clinical encounters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerRole {
    Physician,
    Pa,       // Physician Assistant
    Rn,       // Registered Nurse
    Ma,       // Medical Assistant
    Patient,
    Other,
}

impl SpeakerRole {
    /// Get a human-readable description of the role for LLM context
    pub fn description(&self) -> &'static str {
        match self {
            SpeakerRole::Physician => "Attending/treating physician",
            SpeakerRole::Pa => "Physician assistant",
            SpeakerRole::Rn => "Registered nurse",
            SpeakerRole::Ma => "Medical assistant",
            SpeakerRole::Patient => "Patient being seen",
            SpeakerRole::Other => "Other participant",
        }
    }
}

impl Default for SpeakerRole {
    fn default() -> Self {
        SpeakerRole::Other
    }
}

/// A speaker profile with voice embedding for recognition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerProfile {
    /// Unique identifier (UUID)
    pub id: String,
    /// Display name (e.g., "Dr. Smith")
    pub name: String,
    /// Role in clinical encounters
    pub role: SpeakerRole,
    /// Custom description (e.g., "Attending physician, internal medicine")
    pub description: String,
    /// Voice embedding vector (256-dim from ECAPA-TDNN)
    pub embedding: Vec<f32>,
    /// Creation timestamp (Unix epoch seconds)
    pub created_at: i64,
    /// Last update timestamp (Unix epoch seconds)
    pub updated_at: i64,
}

impl SpeakerProfile {
    /// Create a new speaker profile with the given attributes
    pub fn new(name: String, role: SpeakerRole, description: String, embedding: Vec<f32>) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            role,
            description,
            embedding,
            created_at: now,
            updated_at: now,
        }
    }

    /// Get a formatted description for LLM context
    /// Returns something like "Dr. Smith: Attending physician, internal medicine"
    pub fn llm_description(&self) -> String {
        if self.description.is_empty() {
            format!("{}: {}", self.name, self.role.description())
        } else {
            format!("{}: {}, {}", self.name, self.role.description(), self.description)
        }
    }
}

/// Storage container with schema versioning
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpeakerProfileStore {
    schema_version: u32,
    profiles: Vec<SpeakerProfile>,
}

impl Default for SpeakerProfileStore {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            profiles: Vec::new(),
        }
    }
}

/// Manager for speaker profile persistence
#[derive(Debug)]
pub struct SpeakerProfileManager {
    store: SpeakerProfileStore,
    path: PathBuf,
}

impl SpeakerProfileManager {
    /// Get the default storage path for speaker profiles
    fn default_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Failed to get home directory")?;
        Ok(home.join(".transcriptionapp").join("speaker_profiles.json"))
    }

    /// Load profiles from disk or create empty store
    pub fn load() -> Result<Self> {
        let path = Self::default_path()?;
        Self::load_from_path(path)
    }

    /// Load profiles from a specific path
    pub fn load_from_path(path: PathBuf) -> Result<Self> {
        let store = if path.exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read speaker profiles from {:?}", path))?;

            let store: SpeakerProfileStore = serde_json::from_str(&content)
                .with_context(|| "Failed to parse speaker profiles JSON")?;

            // Handle schema migration if needed
            if store.schema_version != SCHEMA_VERSION {
                warn!(
                    "Speaker profiles schema version mismatch: {} vs {}, may need migration",
                    store.schema_version, SCHEMA_VERSION
                );
            }

            info!("Loaded {} speaker profiles from {:?}", store.profiles.len(), path);
            store
        } else {
            debug!("No speaker profiles file found at {:?}, using empty store", path);
            SpeakerProfileStore::default()
        };

        Ok(Self { store, path })
    }

    /// Save profiles to disk with atomic write and secure permissions
    pub fn save(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }

        let content = serde_json::to_string_pretty(&self.store)
            .context("Failed to serialize speaker profiles")?;

        // Atomic write: write to temp file, then rename
        let temp_path = self.path.with_extension("json.tmp");
        std::fs::write(&temp_path, &content)
            .with_context(|| format!("Failed to write temp file {:?}", temp_path))?;

        // Set strict permissions (600) on Unix - embeddings are somewhat sensitive
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&temp_path, permissions)
                .with_context(|| "Failed to set permissions on speaker profiles")?;
        }

        // Atomic rename
        std::fs::rename(&temp_path, &self.path)
            .with_context(|| format!("Failed to rename temp file to {:?}", self.path))?;

        info!("Saved {} speaker profiles to {:?}", self.store.profiles.len(), self.path);
        Ok(())
    }

    /// Get all speaker profiles
    pub fn list(&self) -> &[SpeakerProfile] {
        &self.store.profiles
    }

    /// Get a profile by ID
    pub fn get(&self, id: &str) -> Option<&SpeakerProfile> {
        self.store.profiles.iter().find(|p| p.id == id)
    }

    /// Add a new profile
    pub fn add(&mut self, profile: SpeakerProfile) -> Result<()> {
        // Check for duplicate names (case-insensitive)
        let name_lower = profile.name.to_lowercase();
        if self.store.profiles.iter().any(|p| p.name.to_lowercase() == name_lower) {
            anyhow::bail!("A speaker profile with name '{}' already exists", profile.name);
        }

        info!("Adding speaker profile: {} ({})", profile.name, profile.role.description());
        self.store.profiles.push(profile);
        self.save()
    }

    /// Update an existing profile
    pub fn update(&mut self, profile: SpeakerProfile) -> Result<()> {
        let idx = self.store.profiles
            .iter()
            .position(|p| p.id == profile.id)
            .context("Speaker profile not found")?;

        // Check for duplicate names (excluding current profile)
        let name_lower = profile.name.to_lowercase();
        if self.store.profiles.iter()
            .enumerate()
            .any(|(i, p)| i != idx && p.name.to_lowercase() == name_lower)
        {
            anyhow::bail!("A speaker profile with name '{}' already exists", profile.name);
        }

        info!("Updating speaker profile: {} ({})", profile.name, profile.id);
        self.store.profiles[idx] = profile;
        self.save()
    }

    /// Delete a profile by ID
    pub fn delete(&mut self, id: &str) -> Result<()> {
        let initial_len = self.store.profiles.len();
        self.store.profiles.retain(|p| p.id != id);

        if self.store.profiles.len() == initial_len {
            anyhow::bail!("Speaker profile with ID '{}' not found", id);
        }

        info!("Deleted speaker profile: {}", id);
        self.save()
    }

    /// Get profiles as a vector (for passing to diarization)
    pub fn profiles(&self) -> Vec<SpeakerProfile> {
        self.store.profiles.clone()
    }

    /// Check if any profiles exist
    pub fn has_profiles(&self) -> bool {
        !self.store.profiles.is_empty()
    }

    /// Get profile count
    pub fn count(&self) -> usize {
        self.store.profiles.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_embedding() -> Vec<f32> {
        vec![0.1; 256]
    }

    #[test]
    fn test_speaker_profile_new() {
        let profile = SpeakerProfile::new(
            "Dr. Smith".to_string(),
            SpeakerRole::Physician,
            "Internal medicine".to_string(),
            create_test_embedding(),
        );

        assert!(!profile.id.is_empty());
        assert_eq!(profile.name, "Dr. Smith");
        assert_eq!(profile.role, SpeakerRole::Physician);
        assert_eq!(profile.description, "Internal medicine");
        assert_eq!(profile.embedding.len(), 256);
        assert!(profile.created_at > 0);
        assert_eq!(profile.created_at, profile.updated_at);
    }

    #[test]
    fn test_speaker_role_description() {
        assert_eq!(SpeakerRole::Physician.description(), "Attending/treating physician");
        assert_eq!(SpeakerRole::Pa.description(), "Physician assistant");
        assert_eq!(SpeakerRole::Rn.description(), "Registered nurse");
        assert_eq!(SpeakerRole::Ma.description(), "Medical assistant");
        assert_eq!(SpeakerRole::Patient.description(), "Patient being seen");
        assert_eq!(SpeakerRole::Other.description(), "Other participant");
    }

    #[test]
    fn test_llm_description() {
        let profile = SpeakerProfile::new(
            "Dr. Smith".to_string(),
            SpeakerRole::Physician,
            "Internal medicine".to_string(),
            create_test_embedding(),
        );
        assert_eq!(
            profile.llm_description(),
            "Dr. Smith: Attending/treating physician, Internal medicine"
        );

        let profile_no_desc = SpeakerProfile::new(
            "Nurse Jane".to_string(),
            SpeakerRole::Rn,
            String::new(),
            create_test_embedding(),
        );
        assert_eq!(
            profile_no_desc.llm_description(),
            "Nurse Jane: Registered nurse"
        );
    }

    #[test]
    fn test_manager_crud_operations() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("profiles.json");

        let mut manager = SpeakerProfileManager::load_from_path(path.clone()).unwrap();
        assert_eq!(manager.count(), 0);
        assert!(!manager.has_profiles());

        // Add profile
        let profile = SpeakerProfile::new(
            "Dr. Smith".to_string(),
            SpeakerRole::Physician,
            "Cardiology".to_string(),
            create_test_embedding(),
        );
        let profile_id = profile.id.clone();
        manager.add(profile).unwrap();

        assert_eq!(manager.count(), 1);
        assert!(manager.has_profiles());
        assert!(manager.get(&profile_id).is_some());

        // Update profile
        let mut updated = manager.get(&profile_id).unwrap().clone();
        updated.description = "Internal medicine".to_string();
        updated.updated_at = chrono::Utc::now().timestamp();
        manager.update(updated).unwrap();

        assert_eq!(manager.get(&profile_id).unwrap().description, "Internal medicine");

        // Persistence check
        let manager2 = SpeakerProfileManager::load_from_path(path).unwrap();
        assert_eq!(manager2.count(), 1);
        assert_eq!(manager2.get(&profile_id).unwrap().description, "Internal medicine");

        // Delete profile
        manager.delete(&profile_id).unwrap();
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_duplicate_name_prevention() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("profiles.json");

        let mut manager = SpeakerProfileManager::load_from_path(path).unwrap();

        let profile1 = SpeakerProfile::new(
            "Dr. Smith".to_string(),
            SpeakerRole::Physician,
            String::new(),
            create_test_embedding(),
        );
        manager.add(profile1).unwrap();

        // Try to add duplicate (case-insensitive)
        let profile2 = SpeakerProfile::new(
            "dr. smith".to_string(),
            SpeakerRole::Pa,
            String::new(),
            create_test_embedding(),
        );
        let result = manager.add(profile2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let profile = SpeakerProfile::new(
            "Test User".to_string(),
            SpeakerRole::Other,
            "Test description".to_string(),
            create_test_embedding(),
        );

        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: SpeakerProfile = serde_json::from_str(&json).unwrap();

        assert_eq!(profile.id, deserialized.id);
        assert_eq!(profile.name, deserialized.name);
        assert_eq!(profile.role, deserialized.role);
        assert_eq!(profile.description, deserialized.description);
        assert_eq!(profile.embedding.len(), deserialized.embedding.len());
    }

    #[test]
    fn test_speaker_role_serialization() {
        assert_eq!(serde_json::to_string(&SpeakerRole::Physician).unwrap(), "\"physician\"");
        assert_eq!(serde_json::to_string(&SpeakerRole::Pa).unwrap(), "\"pa\"");
        assert_eq!(serde_json::to_string(&SpeakerRole::Rn).unwrap(), "\"rn\"");
        assert_eq!(serde_json::to_string(&SpeakerRole::Ma).unwrap(), "\"ma\"");
        assert_eq!(serde_json::to_string(&SpeakerRole::Patient).unwrap(), "\"patient\"");
        assert_eq!(serde_json::to_string(&SpeakerRole::Other).unwrap(), "\"other\"");

        let physician: SpeakerRole = serde_json::from_str("\"physician\"").unwrap();
        assert_eq!(physician, SpeakerRole::Physician);
    }
}
