use crate::error::ApiError;
use crate::types::{CreateSpeakerRequest, SpeakerProfile, UpdateSpeakerRequest};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
struct SpeakerStore {
    schema_version: u32,
    profiles: Vec<SpeakerProfile>,
}

impl Default for SpeakerStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            profiles: Vec::new(),
        }
    }
}

pub struct SpeakerManager {
    store: SpeakerStore,
    path: PathBuf,
}

impl SpeakerManager {
    pub fn load(path: PathBuf) -> Result<Self, ApiError> {
        let store = if path.exists() {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| ApiError::Internal(format!("Failed to read speakers: {e}")))?;
            serde_json::from_str(&content)
                .map_err(|e| ApiError::Internal(format!("Failed to parse speakers: {e}")))?
        } else {
            SpeakerStore::default()
        };
        info!(count = store.profiles.len(), "Loaded speaker profiles");
        Ok(Self { store, path })
    }

    fn save(&self) -> Result<(), ApiError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ApiError::Internal(format!("Failed to create directory: {e}")))?;
        }
        let content = serde_json::to_string_pretty(&self.store)
            .map_err(|e| ApiError::Internal(format!("Failed to serialize: {e}")))?;
        let temp_path = self.path.with_extension("json.tmp");
        std::fs::write(&temp_path, &content)
            .map_err(|e| ApiError::Internal(format!("Failed to write temp file: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&temp_path, &self.path)
            .map_err(|e| ApiError::Internal(format!("Failed to rename: {e}")))?;
        Ok(())
    }

    pub fn list(&self) -> Vec<SpeakerProfile> {
        self.store.profiles.clone()
    }

    pub fn get(&self, id: &str) -> Result<SpeakerProfile, ApiError> {
        self.store
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("Speaker not found: {id}")))
    }

    pub fn create(&mut self, req: CreateSpeakerRequest) -> Result<SpeakerProfile, ApiError> {
        let now = Utc::now().timestamp();
        let profile = SpeakerProfile {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            role: req.role,
            description: req.description,
            embedding: req.embedding,
            created_at: now,
            updated_at: now,
        };
        self.store.profiles.push(profile.clone());
        self.save()?;
        info!(id = %profile.id, name = %profile.name, "Created speaker profile");
        Ok(profile)
    }

    pub fn update(
        &mut self,
        id: &str,
        req: UpdateSpeakerRequest,
    ) -> Result<SpeakerProfile, ApiError> {
        let profile = self
            .store
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| ApiError::NotFound(format!("Speaker not found: {id}")))?;

        if let Some(name) = req.name {
            profile.name = name;
        }
        if let Some(role) = req.role {
            profile.role = role;
        }
        if let Some(desc) = req.description {
            profile.description = desc;
        }
        if let Some(emb) = req.embedding {
            profile.embedding = emb;
        }
        profile.updated_at = Utc::now().timestamp();
        let updated = profile.clone();
        self.save()?;
        info!(id = %id, "Updated speaker profile");
        Ok(updated)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), ApiError> {
        let len_before = self.store.profiles.len();
        self.store.profiles.retain(|p| p.id != id);
        if self.store.profiles.len() == len_before {
            return Err(ApiError::NotFound(format!("Speaker not found: {id}")));
        }
        self.save()?;
        info!(id = %id, "Deleted speaker profile");
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.store.profiles.len()
    }
}
