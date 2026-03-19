//! Background audio upload queue.
//!
//! Queues WAV audio files for asynchronous upload to the profile server
//! when the app is idle (not actively recording). The queue persists to
//! disk so pending uploads survive app restarts.

use crate::commands::{SharedContinuousModeState, SharedPipelineState};
use crate::profile_client::ProfileClient;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// A single pending audio upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub physician_id: String,
    pub session_id: String,
    pub audio_path: PathBuf,
    pub added_at: String,
}

/// In-memory queue backed by a JSON file on disk.
pub struct AudioUploadQueue {
    entries: VecDeque<QueueEntry>,
    queue_path: PathBuf,
}

/// Shared handle used by Tauri state management and the background task.
pub type SharedAudioUploadQueue = Arc<tokio::sync::Mutex<AudioUploadQueue>>;

impl AudioUploadQueue {
    /// Load the queue from its persistent file, or start empty.
    pub fn load() -> Self {
        let queue_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".transcriptionapp")
            .join("cache")
            .join("audio_upload_queue.json");

        let entries: VecDeque<QueueEntry> = if queue_path.exists() {
            std::fs::read_to_string(&queue_path)
                .ok()
                .and_then(|s| serde_json::from_str::<Vec<QueueEntry>>(&s).ok())
                .map(VecDeque::from)
                .unwrap_or_default()
        } else {
            VecDeque::new()
        };

        Self {
            entries,
            queue_path,
        }
    }

    /// Enqueue a new audio file for upload. Duplicate session IDs are ignored.
    pub fn add(&mut self, physician_id: String, session_id: String, audio_path: PathBuf) {
        // Don't double-queue the same session
        if self.entries.iter().any(|e| e.session_id == session_id) {
            return;
        }
        self.entries.push_back(QueueEntry {
            physician_id,
            session_id,
            audio_path,
            added_at: chrono::Utc::now().to_rfc3339(),
        });
        self.save();
    }

    /// Peek at the next entry without removing it.
    pub fn next(&self) -> Option<&QueueEntry> {
        self.entries.front()
    }

    /// Remove the first entry (after a successful upload).
    pub fn remove_first(&mut self) {
        if self.entries.pop_front().is_some() {
            self.save();
        }
    }

    /// Number of uploads still pending.
    pub fn pending_count(&self) -> usize {
        self.entries.len()
    }

    /// Persist the current queue to disk (creates parent dirs lazily).
    fn save(&self) {
        if let Some(parent) = self.queue_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.entries) {
            let _ = std::fs::write(&self.queue_path, json);
        }
    }
}

/// Returns `true` when the app is actively recording (session or continuous mode).
///
/// Checks both `PipelineState.handle.is_some()` (session mode) and
/// `SharedContinuousModeState.is_some()` (continuous mode). Each lock is
/// acquired and released immediately to avoid contention.
fn is_recording(
    pipeline_state: &SharedPipelineState,
    continuous_state: &SharedContinuousModeState,
) -> bool {
    let session_recording = pipeline_state
        .lock()
        .map(|ps| ps.handle.is_some())
        .unwrap_or(false);

    let continuous_recording = continuous_state
        .lock()
        .map(|cs| cs.is_some())
        .unwrap_or(false);

    session_recording || continuous_recording
}

/// Background task that drains the audio upload queue one item at a time.
///
/// Sleeps while recording is active so uploads don't compete for bandwidth
/// or CPU. On upload failure the entry stays in the queue and is retried
/// after a longer back-off delay.
pub async fn audio_upload_task(
    queue: SharedAudioUploadQueue,
    profile_client: Arc<RwLock<Option<ProfileClient>>>,
    pipeline_state: SharedPipelineState,
    continuous_state: SharedContinuousModeState,
) {
    loop {
        // Only upload when not recording
        if is_recording(&pipeline_state, &continuous_state) {
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            continue;
        }

        let entry = {
            let q = queue.lock().await;
            q.next().cloned()
        };

        if let Some(entry) = entry {
            // Check audio file still exists
            if !entry.audio_path.exists() {
                warn!(
                    "Audio file no longer exists, removing from queue: {:?}",
                    entry.audio_path
                );
                queue.lock().await.remove_first();
                continue;
            }

            let client = profile_client.read().await.clone();
            if let Some(client) = client {
                match client
                    .upload_audio(&entry.physician_id, &entry.session_id, &entry.audio_path)
                    .await
                {
                    Ok(()) => {
                        info!("Uploaded audio for session {}", entry.session_id);
                        queue.lock().await.remove_first();
                    }
                    Err(e) => {
                        warn!("Audio upload failed for session {}: {e}", entry.session_id);
                        // Don't remove — will retry next cycle
                        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                    }
                }
            } else {
                // No profile client configured — wait and try again later
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            }
        } else {
            // Queue empty, sleep longer
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_prevents_duplicates() {
        let mut queue = AudioUploadQueue {
            entries: VecDeque::new(),
            queue_path: PathBuf::from("/tmp/test_audio_queue.json"),
        };

        queue.add(
            "dr1".to_string(),
            "sess1".to_string(),
            PathBuf::from("/tmp/a.wav"),
        );
        queue.add(
            "dr1".to_string(),
            "sess1".to_string(),
            PathBuf::from("/tmp/b.wav"),
        );

        assert_eq!(queue.pending_count(), 1);
    }

    #[test]
    fn test_next_returns_first_entry() {
        let mut queue = AudioUploadQueue {
            entries: VecDeque::new(),
            queue_path: PathBuf::from("/tmp/test_audio_queue2.json"),
        };

        queue.add(
            "dr1".to_string(),
            "sess1".to_string(),
            PathBuf::from("/tmp/a.wav"),
        );
        queue.add(
            "dr1".to_string(),
            "sess2".to_string(),
            PathBuf::from("/tmp/b.wav"),
        );

        let next = queue.next().unwrap();
        assert_eq!(next.session_id, "sess1");
    }

    #[test]
    fn test_remove_first_advances_queue() {
        let mut queue = AudioUploadQueue {
            entries: VecDeque::new(),
            queue_path: PathBuf::from("/tmp/test_audio_queue3.json"),
        };

        queue.add(
            "dr1".to_string(),
            "sess1".to_string(),
            PathBuf::from("/tmp/a.wav"),
        );
        queue.add(
            "dr1".to_string(),
            "sess2".to_string(),
            PathBuf::from("/tmp/b.wav"),
        );

        queue.remove_first();
        assert_eq!(queue.pending_count(), 1);
        assert_eq!(queue.next().unwrap().session_id, "sess2");
    }

    #[test]
    fn test_remove_first_on_empty_is_safe() {
        let mut queue = AudioUploadQueue {
            entries: VecDeque::new(),
            queue_path: PathBuf::from("/tmp/test_audio_queue4.json"),
        };

        queue.remove_first(); // should not panic
        assert_eq!(queue.pending_count(), 0);
    }

    #[test]
    fn test_next_on_empty_returns_none() {
        let queue = AudioUploadQueue {
            entries: VecDeque::new(),
            queue_path: PathBuf::from("/tmp/test_audio_queue5.json"),
        };

        assert!(queue.next().is_none());
    }

    #[test]
    fn test_queue_entry_serialization() {
        let entry = QueueEntry {
            physician_id: "dr1".to_string(),
            session_id: "sess1".to_string(),
            audio_path: PathBuf::from("/tmp/audio.wav"),
            added_at: "2026-03-18T12:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: QueueEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.session_id, "sess1");
        assert_eq!(deserialized.physician_id, "dr1");
    }
}
