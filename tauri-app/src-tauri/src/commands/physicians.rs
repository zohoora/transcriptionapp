//! Physician selection and room configuration commands

use crate::commands::CommandError;
use crate::physician_cache;
use crate::profile_client::{PhysicianProfile, ProfileClient};
use crate::room_config::RoomConfig;
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;
use tracing::{info, warn};

pub type SharedActivePhysician = Arc<RwLock<Option<PhysicianProfile>>>;
pub type SharedRoomConfig = Arc<RwLock<Option<RoomConfig>>>;
pub type SharedProfileClient = Arc<RwLock<Option<ProfileClient>>>;

#[tauri::command]
pub async fn get_room_config(
    room_config: State<'_, SharedRoomConfig>,
) -> Result<Option<RoomConfig>, CommandError> {
    let config = room_config.read().await;
    Ok(config.clone())
}

#[tauri::command]
pub async fn save_room_config(
    room_name: String,
    profile_server_url: String,
    room_id: Option<String>,
    profile_api_key: Option<String>,
    room_config_state: State<'_, SharedRoomConfig>,
    profile_client_state: State<'_, SharedProfileClient>,
) -> Result<(), CommandError> {
    let config = RoomConfig {
        room_name,
        profile_server_url: profile_server_url.clone(),
        room_id: room_id.clone(),
        active_physician_id: None,
        profile_api_key: profile_api_key.clone(),
    };
    config
        .save()
        .map_err(|e| CommandError::Other(format!("Failed to save room config: {e}")))?;

    // Update profile client
    let client = ProfileClient::new(&profile_server_url, profile_api_key);

    // Fire-and-forget: merge server settings if room is selected
    if room_id.is_some() {
        let merge_client = client.clone();
        let merge_room_id = room_id.clone();
        tokio::spawn(async move {
            match merge_client.merge_server_settings(merge_room_id.as_deref()).await {
                Ok(true) => info!("Room setup settings merge complete"),
                Ok(false) => {}
                Err(e) => warn!("Room setup settings merge failed: {e}"),
            }
        });
    }

    *profile_client_state.write().await = Some(client);
    *room_config_state.write().await = Some(config);

    info!("Room config saved");
    Ok(())
}

#[tauri::command]
pub async fn test_profile_server(url: String) -> Result<bool, CommandError> {
    let client = ProfileClient::new(&url, None);
    match client.health().await {
        Ok(healthy) => Ok(healthy),
        Err(e) => {
            warn!("Profile server test failed: {e}");
            Ok(false)
        }
    }
}

#[tauri::command]
pub async fn get_physicians(
    profile_client: State<'_, SharedProfileClient>,
) -> Result<Vec<PhysicianProfile>, CommandError> {
    let client = profile_client.read().await;
    if let Some(ref client) = *client {
        match client.list_physicians().await {
            Ok(profiles) => {
                // Cache for offline use
                if let Err(e) = physician_cache::cache_physicians(&profiles) {
                    warn!("Failed to cache physicians: {e}");
                }
                return Ok(profiles);
            }
            Err(e) => {
                warn!("Failed to fetch physicians from server: {e}");
            }
        }
    }
    // Fallback to cache
    physician_cache::load_cached_physicians()
        .map_err(|e| CommandError::Other(format!("Failed to load cached physicians: {e}")))
}

#[tauri::command]
pub async fn select_physician(
    physician_id: String,
    active_physician: State<'_, SharedActivePhysician>,
    profile_client: State<'_, SharedProfileClient>,
    room_config_state: State<'_, SharedRoomConfig>,
    config_state: State<'_, super::SharedPipelineState>,
) -> Result<PhysicianProfile, CommandError> {
    // Safety check: don't allow switching while recording
    {
        let pipeline = config_state.lock().map_err(|_| CommandError::LockPoisoned {
            context: "pipeline state".to_string(),
        })?;
        if pipeline.handle.is_some() {
            return Err(CommandError::Other(
                "Cannot switch physicians while recording is active".to_string(),
            ));
        }
    }

    // Fetch from server or cache
    let profile = {
        let client = profile_client.read().await;
        if let Some(ref client) = *client {
            match client.get_physician(&physician_id).await {
                Ok(p) => {
                    if let Err(e) =
                        physician_cache::cache_physician_settings(&physician_id, &p)
                    {
                        warn!("Failed to cache physician settings: {e}");
                    }
                    p
                }
                Err(e) => {
                    warn!("Failed to fetch physician from server, using cache: {e}");
                    physician_cache::load_cached_physician(&physician_id)
                        .map_err(|e| CommandError::Other(format!("Cache load failed: {e}")))?
                        .ok_or_else(|| {
                            CommandError::Other(format!("Physician not found: {physician_id}"))
                        })?
                }
            }
        } else {
            physician_cache::load_cached_physician(&physician_id)
                .map_err(|e| CommandError::Other(format!("Cache load failed: {e}")))?
                .ok_or_else(|| {
                    CommandError::Other(format!("Physician not found: {physician_id}"))
                })?
        }
    };

    // Save active physician ID to room config
    {
        let mut rc = room_config_state.write().await;
        if let Some(ref mut config) = *rc {
            config.active_physician_id = Some(physician_id.clone());
            if let Err(e) = config.save() {
                warn!("Failed to save active physician to room config: {e}");
            }
        }
    }

    // Apply physician-tier settings overlay and re-save config
    {
        let phys_settings = crate::config::PhysicianSettings::from(&profile);
        // Load fresh, apply overlay, then save (load_or_default already clamps)
        let mut config = crate::config::Config::load_or_default();
        config.settings.apply_physician(&phys_settings);
        if let Err(e) = config.save() {
            warn!("Failed to save merged config: {e}");
        }
    }

    *active_physician.write().await = Some(profile.clone());
    info!(id = %physician_id, name = %profile.name, "Physician selected (settings merged)");
    Ok(profile)
}

#[tauri::command]
pub async fn get_active_physician(
    active_physician: State<'_, SharedActivePhysician>,
) -> Result<Option<PhysicianProfile>, CommandError> {
    let physician = active_physician.read().await;
    Ok(physician.clone())
}

#[tauri::command]
pub async fn deselect_physician(
    active_physician: State<'_, SharedActivePhysician>,
    room_config_state: State<'_, SharedRoomConfig>,
) -> Result<(), CommandError> {
    *active_physician.write().await = None;

    // Clear from room config
    {
        let mut rc = room_config_state.write().await;
        if let Some(ref mut config) = *rc {
            config.active_physician_id = None;
            if let Err(e) = config.save() {
                warn!("Failed to clear active physician from room config: {e}");
            }
        }
    }

    info!("Physician deselected");
    Ok(())
}

/// Sync speaker profiles between local storage and profile server.
///
/// Uses **name-based matching** (case-insensitive) instead of UUID matching,
/// because local and server profiles have independently generated UUIDs.
/// When a profile exists on both sides (same name), the one with the more
/// recent `updated_at` wins. When a profile exists only on one side, it's
/// copied to the other.
///
/// Can be called as a Tauri command or directly from startup code.
pub async fn do_sync_speaker_profiles(
    client: &crate::profile_client::ProfileClient,
) -> Result<String, String> {
    // Fetch server profiles
    let server_profiles = client
        .list_speakers()
        .await
        .map_err(|e| format!("Failed to fetch speakers: {e}"))?;

    // Load local profiles
    let mut local_manager = crate::speaker_profiles::SpeakerProfileManager::load()
        .map_err(|e| format!("Failed to load local profiles: {e}"))?;
    let local_profiles = local_manager.list().to_vec();

    // Build name-based lookup (case-insensitive)
    let server_by_name: std::collections::HashMap<String, &crate::profile_client::SpeakerProfile> =
        server_profiles.iter().map(|p| (p.name.to_lowercase(), p)).collect();
    let local_by_name: std::collections::HashMap<String, &crate::speaker_profiles::SpeakerProfile> =
        local_profiles.iter().map(|p| (p.name.to_lowercase(), p)).collect();

    // Upload local-only profiles to server (name not found on server)
    let mut uploaded = 0;
    for local_profile in &local_profiles {
        let key = local_profile.name.to_lowercase();
        if !server_by_name.contains_key(&key) {
            let body = serde_json::json!({
                "name": local_profile.name,
                "role": super::speaker_profiles::role_to_string(&local_profile.role),
                "description": local_profile.description,
                "embedding": local_profile.embedding,
            });
            if let Err(e) = client.upload_speaker(&body).await {
                warn!("Failed to upload local speaker '{}' to server: {e}", local_profile.name);
            } else {
                uploaded += 1;
            }
        }
    }

    // Download server-only profiles to local (name not found locally)
    let mut added = 0;
    // Update local profiles with newer server versions (same name, server has newer updated_at)
    let mut updated = 0;
    for server_profile in &server_profiles {
        let key = server_profile.name.to_lowercase();
        if let Some(local_profile) = local_by_name.get(&key) {
            // Both exist — update local if server has newer embedding
            if server_profile.updated_at > local_profile.updated_at
                && !server_profile.embedding.is_empty()
            {
                let role = super::speaker_profiles::string_to_role(&server_profile.role);
                let replacement = crate::speaker_profiles::SpeakerProfile {
                    id: local_profile.id.clone(), // keep local ID
                    name: server_profile.name.clone(),
                    role,
                    description: server_profile.description.clone(),
                    embedding: server_profile.embedding.clone(),
                    created_at: local_profile.created_at, // keep original create time
                    updated_at: server_profile.updated_at,
                };
                if local_manager.update(replacement).is_ok() {
                    updated += 1;
                }
            }
        } else {
            // Server-only: add locally
            let role = super::speaker_profiles::string_to_role(&server_profile.role);
            let local = crate::speaker_profiles::SpeakerProfile {
                id: server_profile.id.clone(),
                name: server_profile.name.clone(),
                role,
                description: server_profile.description.clone(),
                embedding: server_profile.embedding.clone(),
                created_at: server_profile.created_at,
                updated_at: server_profile.updated_at,
            };
            if let Err(e) = local_manager.add(local) {
                warn!("Failed to add server speaker '{}' locally: {e}", server_profile.name);
            } else {
                added += 1;
            }
        }
    }

    let total = local_manager.count();
    info!(total, added, updated, uploaded, "Speaker profile sync complete");
    Ok(format!(
        "Synced {total} profiles ({added} new from server, {updated} updated, {uploaded} uploaded to server)"
    ))
}

#[tauri::command]
pub async fn sync_speaker_profiles(
    profile_client: State<'_, SharedProfileClient>,
) -> Result<String, CommandError> {
    let client = profile_client.read().await;
    let client = client
        .as_ref()
        .ok_or_else(|| CommandError::Other("Profile server not configured".into()))?;
    do_sync_speaker_profiles(client)
        .await
        .map_err(|e| CommandError::Other(e))
}

#[tauri::command]
pub async fn create_physician(
    name: String,
    specialty: Option<String>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<serde_json::Value, CommandError> {
    let client = profile_client.read().await;
    let client = client
        .as_ref()
        .ok_or_else(|| CommandError::Other("Profile server not configured".into()))?;

    let body = serde_json::json!({
        "name": name,
        "specialty": specialty,
    });
    let resp = client
        .http_client()
        .post(&format!("{}/physicians", client.base_url()))
        .headers(client.auth_headers())
        .json(&body)
        .send()
        .await
        .map_err(|e| CommandError::Other(format!("Failed to create physician: {e}")))?;
    if !resp.status().is_success() {
        return Err(CommandError::Other(format!(
            "Server returned {}",
            resp.status()
        )));
    }
    let profile: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CommandError::Other(format!("Failed to parse response: {e}")))?;
    info!(name = %name, "Physician created");
    Ok(profile)
}

#[tauri::command]
pub async fn update_physician(
    physician_id: String,
    updates: serde_json::Value,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<serde_json::Value, CommandError> {
    let client = profile_client.read().await;
    let client = client
        .as_ref()
        .ok_or_else(|| CommandError::Other("Profile server not configured".into()))?;

    let resp = client
        .http_client()
        .put(&format!("{}/physicians/{}", client.base_url(), physician_id))
        .headers(client.auth_headers())
        .json(&updates)
        .send()
        .await
        .map_err(|e| CommandError::Other(format!("Failed to update physician: {e}")))?;
    if !resp.status().is_success() {
        return Err(CommandError::Other(format!(
            "Server returned {}",
            resp.status()
        )));
    }
    let profile: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CommandError::Other(format!("Failed to parse response: {e}")))?;
    info!(id = %physician_id, "Physician updated");
    Ok(profile)
}

#[tauri::command]
pub async fn delete_physician(
    physician_id: String,
    profile_client: State<'_, SharedProfileClient>,
    active_physician: State<'_, SharedActivePhysician>,
) -> Result<(), CommandError> {
    // Block deletion of the currently active physician
    {
        let active = active_physician.read().await;
        if let Some(ref active) = *active {
            if active.id == physician_id {
                return Err(CommandError::Other(
                    "Cannot delete the currently active physician. Deselect them first."
                        .to_string(),
                ));
            }
        }
    }

    let client = profile_client.read().await;
    let client = client
        .as_ref()
        .ok_or_else(|| CommandError::Other("Profile server not configured".into()))?;

    let resp = client
        .http_client()
        .delete(&format!("{}/physicians/{}", client.base_url(), physician_id))
        .headers(client.auth_headers())
        .send()
        .await
        .map_err(|e| CommandError::Other(format!("Failed to delete physician: {e}")))?;
    if !resp.status().is_success() {
        return Err(CommandError::Other(format!(
            "Server returned {}",
            resp.status()
        )));
    }
    info!(id = %physician_id, "Physician deleted");
    Ok(())
}

#[tauri::command]
pub async fn get_rooms(
    profile_client: State<'_, SharedProfileClient>,
) -> Result<Vec<crate::profile_client::Room>, CommandError> {
    let client = profile_client.read().await;
    if let Some(ref client) = *client {
        client
            .list_rooms()
            .await
            .map_err(|e| CommandError::Other(format!("Failed to fetch rooms: {e}")))
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
pub async fn create_room(
    name: String,
    description: Option<String>,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<crate::profile_client::Room, CommandError> {
    let client = profile_client.read().await;
    let client = client
        .as_ref()
        .ok_or_else(|| CommandError::Other("Profile server not configured".into()))?;
    client
        .create_room(&name, description.as_deref())
        .await
        .map_err(|e| CommandError::Other(format!("Failed to create room: {e}")))
}

#[tauri::command]
pub async fn update_room(
    room_id: String,
    updates: serde_json::Value,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<crate::profile_client::Room, CommandError> {
    let client = profile_client.read().await;
    let client = client
        .as_ref()
        .ok_or_else(|| CommandError::Other("Profile server not configured".into()))?;
    let room = client
        .update_room(&room_id, &updates)
        .await
        .map_err(|e| CommandError::Other(format!("Failed to update room: {e}")))?;
    info!(id = %room_id, "Room updated");
    Ok(room)
}

#[tauri::command]
pub async fn delete_room(
    room_id: String,
    profile_client: State<'_, SharedProfileClient>,
    room_config_state: State<'_, SharedRoomConfig>,
) -> Result<(), CommandError> {
    // Block deletion of the currently active room
    {
        let rc = room_config_state.read().await;
        if let Some(ref config) = *rc {
            if config.room_id.as_deref() == Some(&room_id) {
                return Err(CommandError::Other(
                    "Cannot delete the currently active room. Switch rooms first.".to_string(),
                ));
            }
        }
    }

    let client = profile_client.read().await;
    let client = client
        .as_ref()
        .ok_or_else(|| CommandError::Other("Profile server not configured".into()))?;
    client
        .delete_room(&room_id)
        .await
        .map_err(|e| CommandError::Other(format!("Failed to delete room: {e}")))?;
    info!(id = %room_id, "Room deleted");
    Ok(())
}

/// Re-fetch server settings (infra + room) and merge into local config.
/// Returns the merged settings for the frontend to reload.
#[tauri::command]
pub async fn sync_settings_from_server(
    profile_client: State<'_, SharedProfileClient>,
    room_config: State<'_, SharedRoomConfig>,
) -> Result<crate::config::Settings, CommandError> {
    let client = profile_client.read().await;
    let client = client
        .as_ref()
        .ok_or_else(|| CommandError::Other("Profile server not configured".into()))?;

    let room_id = {
        let rc = room_config.read().await;
        rc.as_ref().and_then(|rc| rc.room_id.clone())
    };

    client
        .merge_server_settings(room_id.as_deref())
        .await
        .map_err(|e| CommandError::Other(format!("Settings merge failed: {e}")))?;

    let config = crate::config::Config::load_or_default();
    Ok(config.to_settings())
}

/// Push infrastructure-tier settings from local config to server
#[tauri::command]
pub async fn sync_infrastructure_settings(
    settings: crate::config::InfrastructureOverlay,
    profile_client: State<'_, SharedProfileClient>,
) -> Result<(), CommandError> {
    let client = profile_client.read().await;
    let client = client
        .as_ref()
        .ok_or_else(|| CommandError::Other("Profile server not configured".into()))?;
    client
        .update_infrastructure(&settings)
        .await
        .map_err(|e| CommandError::Other(format!("Failed to sync infrastructure: {e}")))?;
    info!("Infrastructure settings synced to server");
    Ok(())
}

/// Push room-tier settings from local config to server
#[tauri::command]
pub async fn sync_room_settings(
    settings: crate::config::RoomOverlay,
    profile_client: State<'_, SharedProfileClient>,
    room_config: State<'_, SharedRoomConfig>,
) -> Result<(), CommandError> {
    let client = profile_client.read().await;
    let client = client
        .as_ref()
        .ok_or_else(|| CommandError::Other("Profile server not configured".into()))?;

    let rc = room_config.read().await;
    let room_id = rc
        .as_ref()
        .and_then(|rc| rc.room_id.clone())
        .ok_or_else(|| CommandError::Other("No room ID configured".into()))?;

    // Serialize the overlay as a JSON value for the generic update_room method
    let updates = serde_json::to_value(&settings)
        .map_err(|e| CommandError::Other(format!("Failed to serialize room settings: {e}")))?;
    client
        .update_room(&room_id, &updates)
        .await
        .map_err(|e| CommandError::Other(format!("Failed to sync room settings: {e}")))?;
    info!(room_id = %room_id, "Room settings synced to server");
    Ok(())
}
