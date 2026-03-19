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
