//! Server sync context for uploading session data to the profile service.
//!
//! Provides fire-and-forget async helpers for syncing session metadata,
//! transcripts, SOAP notes, and auxiliary files (pipeline logs, replay bundles,
//! screenshots) to the centralized profile server.

use tracing::{info, warn};

use crate::local_archive;
use crate::profile_client::ProfileClient;

/// Context for syncing session data to the profile server.
/// Threaded through continuous mode and session commands.
#[derive(Clone)]
pub struct ServerSyncContext {
    pub physician_id: Option<String>,
    pub physician_name: Option<String>,
    pub room_name: Option<String>,
    pub client: Option<ProfileClient>,
}

impl ServerSyncContext {
    pub fn empty() -> Self {
        Self {
            physician_id: None,
            physician_name: None,
            room_name: None,
            client: None,
        }
    }

    /// Build from Tauri shared state locks. Acquires reads briefly, then drops.
    pub async fn from_state(
        active_physician: &crate::commands::SharedActivePhysician,
        room_config_state: &crate::commands::SharedRoomConfig,
        profile_client_state: &crate::commands::SharedProfileClient,
    ) -> Self {
        let physician = active_physician.read().await;
        let room_config = room_config_state.read().await;
        let client = profile_client_state.read().await;
        Self {
            physician_id: physician.as_ref().map(|p| p.id.clone()),
            physician_name: physician.as_ref().map(|p| p.name.clone()),
            room_name: room_config.as_ref().map(|rc| rc.room_name.clone()),
            client: client.clone(),
        }
    }

    /// Fire-and-forget: upload session metadata+transcript+auxiliary files to server.
    /// Also schedules a delayed re-sync (30s) to catch late-written files
    /// (SOAP generation events in pipeline_log, replay_bundle from build_and_reset).
    pub fn sync_session(&self, session_id: &str, date: &str) {
        let Some(ref phys_id) = self.physician_id else { return };
        let Some(ref client) = self.client else { return };
        let phys_id = phys_id.clone();
        let client = client.clone();
        let sid = session_id.to_string();
        let date = date.to_string();

        // Immediate sync: core data + whatever aux files exist now
        let phys_id2 = phys_id.clone();
        let client2 = client.clone();
        let sid2 = sid.clone();
        let date2 = date.clone();
        tauri::async_runtime::spawn(async move {
            // Upload core session data (metadata + transcript + SOAP)
            if let Ok(details) = local_archive::get_session(&sid, &date) {
                if let Ok(body) = serde_json::to_value(&details) {
                    if let Err(e) = client.upload_session(&phys_id, &sid, &body).await {
                        warn!("Server sync failed (upload_session {}): {e}", sid);
                    }
                }
            }

            Self::upload_aux_files(&client, &phys_id, &sid, &date).await;
        });

        // Delayed re-sync: catches late-written files (SOAP events, replay bundle)
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            info!("Delayed re-sync for session {}", sid2);

            // Re-upload SOAP (may have been generated after initial sync)
            if let Ok(details) = local_archive::get_session(&sid2, &date2) {
                if details.soap_note.is_some() {
                    if let Ok(body) = serde_json::to_value(&details) {
                        if let Err(e) = client2.upload_session(&phys_id2, &sid2, &body).await {
                            warn!("Delayed re-sync failed (upload_session {}): {e}", sid2);
                        }
                    }
                }
            }

            Self::upload_aux_files(&client2, &phys_id2, &sid2, &date2).await;
        });
    }

    /// Fire-and-forget: delete a merged-away session from the server and re-upload the surviving session.
    pub fn sync_merge(&self, deleted_session_id: &str, surviving_session_id: &str, date: &str) {
        let Some(ref phys_id) = self.physician_id else { return };
        let Some(ref client) = self.client else { return };
        let phys_id = phys_id.clone();
        let client = client.clone();
        let deleted = deleted_session_id.to_string();
        let surviving = surviving_session_id.to_string();
        let date = date.to_string();

        tauri::async_runtime::spawn(async move {
            // Delete the merged-away session from server
            if let Err(e) = client.delete_session(&phys_id, &deleted).await {
                warn!("Server sync: failed to delete merged session {}: {e}", deleted);
            } else {
                info!("Server sync: deleted merged session {} from server", deleted);
            }

            // Re-upload the surviving (merged) session with updated data
            if let Ok(details) = local_archive::get_session(&surviving, &date) {
                if let Ok(body) = serde_json::to_value(&details) {
                    if let Err(e) = client.upload_session(&phys_id, &surviving, &body).await {
                        warn!("Server sync: failed to re-upload merged session {}: {e}", surviving);
                    }
                }
            }
        });
    }

    /// Upload auxiliary files (pipeline_log, replay_bundle, segments) and day_log.
    async fn upload_aux_files(
        client: &crate::profile_client::ProfileClient,
        phys_id: &str,
        session_id: &str,
        date: &str,
    ) {
        if let Ok(session_dir) = Self::local_session_dir(session_id, date) {
            for filename in &["pipeline_log.jsonl", "replay_bundle.json", "segments.jsonl", "billing.json"] {
                let path = session_dir.join(filename);
                if path.exists() {
                    match std::fs::read(&path) {
                        Ok(data) => {
                            if let Err(e) = client.upload_session_file(phys_id, session_id, filename, data).await {
                                warn!("Server sync failed ({} for {}): {e}", filename, session_id);
                            }
                        }
                        Err(e) => warn!("Failed to read {}: {e}", path.display()),
                    }
                }
            }
            // Upload screenshots
            let screenshots_dir = session_dir.join("screenshots");
            if screenshots_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&screenshots_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().map_or(false, |e| e == "jpg") {
                            let fname = format!("screenshots/{}", path.file_name().unwrap_or_default().to_string_lossy());
                            match std::fs::read(&path) {
                                Ok(data) => {
                                    if let Err(e) = client.upload_session_file(phys_id, session_id, &fname, data).await {
                                        warn!("Server sync failed ({} for {}): {e}", fname, session_id);
                                    }
                                }
                                Err(e) => warn!("Failed to read screenshot {}: {e}", path.display()),
                            }
                        }
                    }
                }
            }
        }

        if let Ok(date_dir) = Self::local_date_dir(date) {
            let day_log_path = date_dir.join("day_log.jsonl");
            if day_log_path.exists() {
                match std::fs::read(&day_log_path) {
                    Ok(data) => {
                        if let Err(e) = client.upload_day_log(phys_id, date, data).await {
                            warn!("Server sync failed (day_log for {}): {e}", date);
                        }
                    }
                    Err(e) => warn!("Failed to read day_log: {e}"),
                }
            }
        }
    }

    /// Resolve the local session directory from session_id + date string.
    /// Delegates to local_archive which also validates session_id against path traversal.
    fn local_session_dir(session_id: &str, date_str: &str) -> Result<std::path::PathBuf, String> {
        local_archive::get_session_dir_from_str(session_id, date_str)
    }

    /// Resolve the local date directory from a date string.
    fn local_date_dir(date_str: &str) -> Result<std::path::PathBuf, String> {
        local_archive::get_date_dir_from_str(date_str)
    }

    /// Fire-and-forget: upload SOAP note to server.
    pub fn sync_soap(&self, session_id: &str, soap_content: &str, detail_level: u8, format: &str) {
        let Some(ref phys_id) = self.physician_id else { return };
        let Some(ref client) = self.client else { return };
        let phys_id = phys_id.clone();
        let client = client.clone();
        let sid = session_id.to_string();
        let soap = soap_content.to_string();
        let dl = detail_level;
        let fmt = format.to_string();
        tauri::async_runtime::spawn(async move {
            let body = serde_json::json!({
                "content": soap,
                "detail_level": dl,
                "format": fmt,
            });
            if let Err(e) = client.update_soap(&phys_id, &sid, &body).await {
                warn!("Server sync failed (update_soap {}): {e}", sid);
            }
        });
    }

    /// Enrich metadata with physician/room fields.
    pub fn enrich_metadata(&self, metadata: &mut local_archive::ArchiveMetadata) {
        metadata.physician_id = self.physician_id.clone();
        metadata.physician_name = self.physician_name.clone();
        metadata.room_name = self.room_name.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_context_has_no_client() {
        let ctx = ServerSyncContext::empty();
        assert!(ctx.physician_id.is_none());
        assert!(ctx.physician_name.is_none());
        assert!(ctx.room_name.is_none());
        assert!(ctx.client.is_none());
    }

    #[test]
    fn test_enrich_metadata_overwrites_fields() {
        let ctx = ServerSyncContext {
            physician_id: Some("phys-1".to_string()),
            physician_name: Some("Dr Test".to_string()),
            room_name: Some("Room 6".to_string()),
            client: None,
        };
        let mut metadata = local_archive::ArchiveMetadata::new("test-session");
        // Enrich should set the multi-user fields
        ctx.enrich_metadata(&mut metadata);
        assert_eq!(metadata.physician_id, Some("phys-1".to_string()));
        assert_eq!(metadata.physician_name, Some("Dr Test".to_string()));
        assert_eq!(metadata.room_name, Some("Room 6".to_string()));
    }

    #[test]
    fn test_enrich_metadata_clears_fields_when_empty_context() {
        let ctx = ServerSyncContext::empty();
        let mut metadata = local_archive::ArchiveMetadata::new("test-session");
        // Pre-populate to verify they get cleared
        metadata.physician_id = Some("old-id".to_string());
        metadata.physician_name = Some("Old Name".to_string());
        metadata.room_name = Some("Old Room".to_string());

        ctx.enrich_metadata(&mut metadata);

        assert_eq!(metadata.physician_id, None);
        assert_eq!(metadata.physician_name, None);
        assert_eq!(metadata.room_name, None);
    }

    #[test]
    fn test_clone_preserves_all_fields() {
        let ctx = ServerSyncContext {
            physician_id: Some("phys-1".to_string()),
            physician_name: Some("Dr Test".to_string()),
            room_name: Some("Room 6".to_string()),
            client: None,
        };
        let cloned = ctx.clone();
        assert_eq!(cloned.physician_id, ctx.physician_id);
        assert_eq!(cloned.physician_name, ctx.physician_name);
        assert_eq!(cloned.room_name, ctx.room_name);
    }

    #[tokio::test]
    async fn test_sync_session_silently_drops_when_no_client() {
        // Without a client, sync should be a no-op (not panic)
        let ctx = ServerSyncContext::empty();
        ctx.sync_session("nonexistent-session", "2026-04-15");
        // No assertion needed — just verifying it doesn't panic
        // The fire-and-forget task will silently exit when client is None
    }

    #[tokio::test]
    async fn test_sync_session_silently_drops_when_no_physician() {
        // Without a physician_id, sync should also be a no-op
        let ctx = ServerSyncContext {
            physician_id: None,
            physician_name: None,
            room_name: None,
            client: None,
        };
        ctx.sync_session("nonexistent-session", "2026-04-15");
    }
}
