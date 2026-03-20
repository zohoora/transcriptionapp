use crate::error::ApiError;
use crate::types::{InfrastructureSettings, UpdateInfrastructureRequest};
use std::path::PathBuf;
use tracing::info;

pub struct InfrastructureStore {
    settings: InfrastructureSettings,
    path: PathBuf,
}

impl InfrastructureStore {
    pub fn load(path: PathBuf) -> Result<Self, ApiError> {
        let settings = if path.exists() {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| ApiError::Internal(format!("Failed to read infrastructure: {e}")))?;
            serde_json::from_str(&content)
                .map_err(|e| ApiError::Internal(format!("Failed to parse infrastructure: {e}")))?
        } else {
            InfrastructureSettings::default()
        };
        info!("Loaded infrastructure settings");
        Ok(Self { settings, path })
    }

    fn save(&self) -> Result<(), ApiError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ApiError::Internal(format!("Failed to create directory: {e}")))?;
        }
        let content = serde_json::to_string_pretty(&self.settings)
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

    pub fn get(&self) -> InfrastructureSettings {
        self.settings.clone()
    }

    pub fn update(&mut self, req: UpdateInfrastructureRequest) -> Result<InfrastructureSettings, ApiError> {
        // Merge non-None fields from request into current settings
        if req.llm_router_url.is_some() { self.settings.llm_router_url = req.llm_router_url; }
        if req.llm_api_key.is_some() { self.settings.llm_api_key = req.llm_api_key; }
        if req.llm_client_id.is_some() { self.settings.llm_client_id = req.llm_client_id; }
        if req.soap_model.is_some() { self.settings.soap_model = req.soap_model; }
        if req.soap_model_fast.is_some() { self.settings.soap_model_fast = req.soap_model_fast; }
        if req.fast_model.is_some() { self.settings.fast_model = req.fast_model; }
        if req.whisper_server_url.is_some() { self.settings.whisper_server_url = req.whisper_server_url; }
        if req.whisper_server_model.is_some() { self.settings.whisper_server_model = req.whisper_server_model; }
        if req.stt_alias.is_some() { self.settings.stt_alias = req.stt_alias; }
        if req.stt_postprocess.is_some() { self.settings.stt_postprocess = req.stt_postprocess; }
        if req.medplum_server_url.is_some() { self.settings.medplum_server_url = req.medplum_server_url; }
        if req.medplum_client_id.is_some() { self.settings.medplum_client_id = req.medplum_client_id; }
        if req.miis_server_url.is_some() { self.settings.miis_server_url = req.miis_server_url; }
        if req.whisper_mode.is_some() { self.settings.whisper_mode = req.whisper_mode; }
        if req.encounter_detection_model.is_some() { self.settings.encounter_detection_model = req.encounter_detection_model; }
        if req.encounter_detection_nothink.is_some() { self.settings.encounter_detection_nothink = req.encounter_detection_nothink; }

        self.save()?;
        info!("Updated infrastructure settings");
        Ok(self.settings.clone())
    }
}
