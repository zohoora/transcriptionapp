use crate::error::ApiError;
use crate::types::{CreatePhysicianRequest, PhysicianProfile, UpdatePhysicianRequest};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
struct PhysicianStore {
    schema_version: u32,
    profiles: Vec<PhysicianProfile>,
}

impl Default for PhysicianStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            profiles: Vec::new(),
        }
    }
}

pub struct PhysicianManager {
    store: PhysicianStore,
    path: PathBuf,
}

impl PhysicianManager {
    pub fn load(path: PathBuf) -> Result<Self, ApiError> {
        let store = if path.exists() {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| ApiError::Internal(format!("Failed to read physicians: {e}")))?;
            serde_json::from_str(&content)
                .map_err(|e| ApiError::Internal(format!("Failed to parse physicians: {e}")))?
        } else {
            PhysicianStore::default()
        };
        info!(count = store.profiles.len(), "Loaded physician profiles");
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

    pub fn list(&self) -> Vec<PhysicianProfile> {
        self.store.profiles.clone()
    }

    pub fn get(&self, id: &str) -> Result<PhysicianProfile, ApiError> {
        self.store
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("Physician not found: {id}")))
    }

    pub fn create(&mut self, req: CreatePhysicianRequest) -> Result<PhysicianProfile, ApiError> {
        let now = Utc::now().to_rfc3339();
        let profile = PhysicianProfile {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            specialty: req.specialty,
            soap_detail_level: None,
            soap_format: None,
            soap_custom_instructions: None,
            charting_mode: None,
            language: None,
            image_source: None,
            gemini_api_key: None,
            auto_start_enabled: None,
            auto_start_require_enrolled: None,
            auto_start_required_role: None,
            auto_end_enabled: None,
            auto_end_silence_ms: None,
            encounter_merge_enabled: None,
            encounter_check_interval_secs: None,
            encounter_silence_trigger_secs: None,
            medplum_auto_sync: None,
            diarization_enabled: None,
            max_speakers: None,
            medplum_practitioner_id: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.profiles.push(profile.clone());
        self.save()?;
        info!(id = %profile.id, name = %profile.name, "Created physician profile");
        Ok(profile)
    }

    pub fn update(
        &mut self,
        id: &str,
        req: UpdatePhysicianRequest,
    ) -> Result<PhysicianProfile, ApiError> {
        let profile = self
            .store
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| ApiError::NotFound(format!("Physician not found: {id}")))?;

        // Partial update — only overwrite fields that are Some in the request
        if let Some(name) = req.name {
            profile.name = name;
        }
        if req.specialty.is_some() {
            profile.specialty = req.specialty;
        }
        if req.soap_detail_level.is_some() {
            profile.soap_detail_level = req.soap_detail_level;
        }
        if req.soap_format.is_some() {
            profile.soap_format = req.soap_format;
        }
        if req.soap_custom_instructions.is_some() {
            profile.soap_custom_instructions = req.soap_custom_instructions;
        }
        if req.charting_mode.is_some() {
            profile.charting_mode = req.charting_mode;
        }
        if req.language.is_some() {
            profile.language = req.language;
        }
        if req.image_source.is_some() {
            profile.image_source = req.image_source;
        }
        if req.gemini_api_key.is_some() {
            profile.gemini_api_key = req.gemini_api_key;
        }
        if req.auto_start_enabled.is_some() {
            profile.auto_start_enabled = req.auto_start_enabled;
        }
        if req.auto_start_require_enrolled.is_some() {
            profile.auto_start_require_enrolled = req.auto_start_require_enrolled;
        }
        if req.auto_start_required_role.is_some() {
            profile.auto_start_required_role = req.auto_start_required_role;
        }
        if req.auto_end_enabled.is_some() {
            profile.auto_end_enabled = req.auto_end_enabled;
        }
        if req.auto_end_silence_ms.is_some() {
            profile.auto_end_silence_ms = req.auto_end_silence_ms;
        }
        if req.encounter_merge_enabled.is_some() {
            profile.encounter_merge_enabled = req.encounter_merge_enabled;
        }
        if req.encounter_check_interval_secs.is_some() {
            profile.encounter_check_interval_secs = req.encounter_check_interval_secs;
        }
        if req.encounter_silence_trigger_secs.is_some() {
            profile.encounter_silence_trigger_secs = req.encounter_silence_trigger_secs;
        }
        if req.medplum_auto_sync.is_some() {
            profile.medplum_auto_sync = req.medplum_auto_sync;
        }
        if req.diarization_enabled.is_some() {
            profile.diarization_enabled = req.diarization_enabled;
        }
        if req.max_speakers.is_some() {
            profile.max_speakers = req.max_speakers;
        }
        if req.medplum_practitioner_id.is_some() {
            profile.medplum_practitioner_id = req.medplum_practitioner_id;
        }

        profile.updated_at = Utc::now().to_rfc3339();
        let updated = profile.clone();
        self.save()?;
        info!(id = %id, "Updated physician profile");
        Ok(updated)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), ApiError> {
        let len_before = self.store.profiles.len();
        self.store.profiles.retain(|p| p.id != id);
        if self.store.profiles.len() == len_before {
            return Err(ApiError::NotFound(format!("Physician not found: {id}")));
        }
        self.save()?;
        info!(id = %id, "Deleted physician profile");
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.store.profiles.len()
    }
}
