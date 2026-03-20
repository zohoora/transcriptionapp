use crate::error::ApiError;
use crate::types::{CreateRoomRequest, Room, UpdateRoomRequest};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
struct RoomStore {
    schema_version: u32,
    rooms: Vec<Room>,
}

impl Default for RoomStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            rooms: Vec::new(),
        }
    }
}

pub struct RoomManager {
    store: RoomStore,
    path: PathBuf,
}

impl RoomManager {
    pub fn load(path: PathBuf) -> Result<Self, ApiError> {
        let store = if path.exists() {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| ApiError::Internal(format!("Failed to read rooms: {e}")))?;
            serde_json::from_str(&content)
                .map_err(|e| ApiError::Internal(format!("Failed to parse rooms: {e}")))?
        } else {
            RoomStore::default()
        };
        info!(count = store.rooms.len(), "Loaded rooms");
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

    pub fn list(&self) -> Vec<Room> {
        self.store.rooms.clone()
    }

    pub fn get(&self, id: &str) -> Result<Room, ApiError> {
        self.store
            .rooms
            .iter()
            .find(|r| r.id == id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("Room not found: {id}")))
    }

    pub fn create(&mut self, req: CreateRoomRequest) -> Result<Room, ApiError> {
        let now = Utc::now().to_rfc3339();
        let room = Room {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            description: req.description,
            // Room-tier settings default to None (use infrastructure defaults)
            encounter_detection_mode: None,
            presence_sensor_port: None,
            presence_sensor_url: None,
            presence_absence_threshold_secs: None,
            presence_debounce_secs: None,
            thermal_hot_pixel_threshold_c: None,
            co2_baseline_ppm: None,
            hybrid_confirm_window_secs: None,
            hybrid_min_words_for_sensor_split: None,
            screen_capture_enabled: None,
            screen_capture_interval_secs: None,
            shadow_active_method: None,
            shadow_csv_log_enabled: None,
            presence_csv_log_enabled: None,
            vad_threshold: None,
            silence_to_flush_ms: None,
            max_utterance_ms: None,
            greeting_sensitivity: None,
            min_speech_duration_ms: None,
            whisper_model: None,
            debug_storage_enabled: None,
            input_device_id: None,
            created_at: now.clone(),
            updated_at: now,
        };
        self.store.rooms.push(room.clone());
        self.save()?;
        info!(id = %room.id, name = %room.name, "Created room");
        Ok(room)
    }

    pub fn update(&mut self, id: &str, req: UpdateRoomRequest) -> Result<Room, ApiError> {
        let room = self
            .store
            .rooms
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| ApiError::NotFound(format!("Room not found: {id}")))?;

        if let Some(name) = req.name {
            room.name = name;
        }
        if req.description.is_some() {
            room.description = req.description;
        }
        // Room-tier settings (merge non-None fields)
        if req.encounter_detection_mode.is_some() { room.encounter_detection_mode = req.encounter_detection_mode; }
        if req.presence_sensor_port.is_some() { room.presence_sensor_port = req.presence_sensor_port; }
        if req.presence_sensor_url.is_some() { room.presence_sensor_url = req.presence_sensor_url; }
        if req.presence_absence_threshold_secs.is_some() { room.presence_absence_threshold_secs = req.presence_absence_threshold_secs; }
        if req.presence_debounce_secs.is_some() { room.presence_debounce_secs = req.presence_debounce_secs; }
        if req.thermal_hot_pixel_threshold_c.is_some() { room.thermal_hot_pixel_threshold_c = req.thermal_hot_pixel_threshold_c; }
        if req.co2_baseline_ppm.is_some() { room.co2_baseline_ppm = req.co2_baseline_ppm; }
        if req.hybrid_confirm_window_secs.is_some() { room.hybrid_confirm_window_secs = req.hybrid_confirm_window_secs; }
        if req.hybrid_min_words_for_sensor_split.is_some() { room.hybrid_min_words_for_sensor_split = req.hybrid_min_words_for_sensor_split; }
        if req.screen_capture_enabled.is_some() { room.screen_capture_enabled = req.screen_capture_enabled; }
        if req.screen_capture_interval_secs.is_some() { room.screen_capture_interval_secs = req.screen_capture_interval_secs; }
        if req.shadow_active_method.is_some() { room.shadow_active_method = req.shadow_active_method; }
        if req.shadow_csv_log_enabled.is_some() { room.shadow_csv_log_enabled = req.shadow_csv_log_enabled; }
        if req.presence_csv_log_enabled.is_some() { room.presence_csv_log_enabled = req.presence_csv_log_enabled; }
        if req.vad_threshold.is_some() { room.vad_threshold = req.vad_threshold; }
        if req.silence_to_flush_ms.is_some() { room.silence_to_flush_ms = req.silence_to_flush_ms; }
        if req.max_utterance_ms.is_some() { room.max_utterance_ms = req.max_utterance_ms; }
        if req.greeting_sensitivity.is_some() { room.greeting_sensitivity = req.greeting_sensitivity; }
        if req.min_speech_duration_ms.is_some() { room.min_speech_duration_ms = req.min_speech_duration_ms; }
        if req.whisper_model.is_some() { room.whisper_model = req.whisper_model; }
        if req.debug_storage_enabled.is_some() { room.debug_storage_enabled = req.debug_storage_enabled; }
        if req.input_device_id.is_some() { room.input_device_id = req.input_device_id; }

        room.updated_at = Utc::now().to_rfc3339();
        let updated = room.clone();
        self.save()?;
        info!(id = %id, "Updated room");
        Ok(updated)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), ApiError> {
        let len_before = self.store.rooms.len();
        self.store.rooms.retain(|r| r.id != id);
        if self.store.rooms.len() == len_before {
            return Err(ApiError::NotFound(format!("Room not found: {id}")));
        }
        self.save()?;
        info!(id = %id, "Deleted room");
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.store.rooms.len()
    }
}
