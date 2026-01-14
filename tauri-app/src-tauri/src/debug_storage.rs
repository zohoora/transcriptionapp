//! Debug Storage Module
//!
//! Provides local storage of session data for debugging purposes.
//! This module stores audio, transcripts, SOAP notes, and metadata locally.
//!
//! IMPORTANT: This module stores PHI (Protected Health Information) locally.
//! It should ONLY be enabled during development/debugging and MUST be
//! disabled in production. This is a temporary debugging feature.
//!
//! Storage location: ~/.transcriptionapp/debug/<session-id>/
//!
//! Files stored per session:
//! - audio.wav - Raw recorded audio
//! - transcript.txt - Full transcript text
//! - transcript_segments.json - Detailed segment data with timestamps
//! - soap_note.txt - Generated SOAP note
//! - metadata.json - Session metadata (timestamps, settings, etc.)

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Get the debug storage base directory
pub fn get_debug_storage_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Could not determine home directory")?;
    Ok(home.join(".transcriptionapp").join("debug"))
}

/// Get the session-specific debug directory
pub fn get_session_debug_dir(session_id: &str) -> Result<PathBuf, String> {
    let base = get_debug_storage_dir()?;
    Ok(base.join(session_id))
}

/// Ensure the debug directory exists for a session
pub fn ensure_session_dir(session_id: &str) -> Result<PathBuf, String> {
    let dir = get_session_debug_dir(session_id)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create debug directory: {}", e))?;
    Ok(dir)
}

/// Session metadata for debug storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub session_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_ms: Option<u64>,
    pub device_name: Option<String>,
    pub whisper_model: String,
    pub whisper_mode: String,
    pub language: String,
    pub diarization_enabled: bool,
    pub segment_count: usize,
    pub word_count: usize,
    pub soap_generated: bool,
    pub soap_model: Option<String>,
    pub transcript_truncated: bool,
    pub original_word_count: Option<usize>,
}

impl SessionMetadata {
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            started_at: Utc::now().to_rfc3339(),
            ended_at: None,
            duration_ms: None,
            device_name: None,
            whisper_model: String::new(),
            whisper_mode: String::new(),
            language: String::new(),
            diarization_enabled: false,
            segment_count: 0,
            word_count: 0,
            soap_generated: false,
            soap_model: None,
            transcript_truncated: false,
            original_word_count: None,
        }
    }
}

/// Transcript segment for debug storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugSegment {
    pub index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub speaker_id: Option<String>,
}

/// Debug storage manager for a session
pub struct DebugStorage {
    session_id: String,
    session_dir: PathBuf,
    metadata: SessionMetadata,
    segments: Vec<DebugSegment>,
    transcript_lines: Vec<String>,
    enabled: bool,
}

impl DebugStorage {
    /// Create a new debug storage instance for a session
    pub fn new(session_id: &str, enabled: bool) -> Result<Self, String> {
        if !enabled {
            return Ok(Self {
                session_id: session_id.to_string(),
                session_dir: PathBuf::new(),
                metadata: SessionMetadata::new(session_id),
                segments: Vec::new(),
                transcript_lines: Vec::new(),
                enabled: false,
            });
        }

        let session_dir = ensure_session_dir(session_id)?;
        info!(
            session_id = %session_id,
            dir = %session_dir.display(),
            "Debug storage initialized"
        );

        Ok(Self {
            session_id: session_id.to_string(),
            session_dir,
            metadata: SessionMetadata::new(session_id),
            segments: Vec::new(),
            transcript_lines: Vec::new(),
            enabled: true,
        })
    }

    /// Check if debug storage is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the audio file path for this session
    pub fn audio_path(&self) -> PathBuf {
        self.session_dir.join("audio.wav")
    }

    /// Update session metadata
    pub fn set_metadata(
        &mut self,
        device_name: Option<&str>,
        whisper_model: &str,
        whisper_mode: &str,
        language: &str,
        diarization_enabled: bool,
    ) {
        if !self.enabled {
            return;
        }
        self.metadata.device_name = device_name.map(|s| s.to_string());
        self.metadata.whisper_model = whisper_model.to_string();
        self.metadata.whisper_mode = whisper_mode.to_string();
        self.metadata.language = language.to_string();
        self.metadata.diarization_enabled = diarization_enabled;
    }

    /// Add a transcript segment
    pub fn add_segment(
        &mut self,
        index: usize,
        start_ms: u64,
        end_ms: u64,
        text: &str,
        speaker_id: Option<String>,
    ) {
        if !self.enabled {
            return;
        }

        let segment = DebugSegment {
            index,
            start_ms,
            end_ms,
            text: text.to_string(),
            speaker_id: speaker_id.clone(),
        };

        self.segments.push(segment);

        // Build transcript line with speaker info
        let line = if let Some(ref speaker) = speaker_id {
            format!("[{}] {}", speaker, text)
        } else {
            text.to_string()
        };
        self.transcript_lines.push(line);

        debug!(
            session_id = %self.session_id,
            segment_index = index,
            "Debug segment added"
        );
    }

    /// Save the transcript to file
    pub fn save_transcript(&mut self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        // Update word count
        let full_transcript = self.transcript_lines.join("\n");
        self.metadata.word_count = full_transcript.split_whitespace().count();
        self.metadata.segment_count = self.segments.len();

        // Save full transcript
        let transcript_path = self.session_dir.join("transcript.txt");
        let mut file = File::create(&transcript_path)
            .map_err(|e| format!("Failed to create transcript file: {}", e))?;
        file.write_all(full_transcript.as_bytes())
            .map_err(|e| format!("Failed to write transcript: {}", e))?;

        info!(
            session_id = %self.session_id,
            path = %transcript_path.display(),
            words = self.metadata.word_count,
            segments = self.metadata.segment_count,
            "Transcript saved to debug storage"
        );

        // Save segments JSON
        let segments_path = self.session_dir.join("transcript_segments.json");
        let segments_json = serde_json::to_string_pretty(&self.segments)
            .map_err(|e| format!("Failed to serialize segments: {}", e))?;
        let mut file = File::create(&segments_path)
            .map_err(|e| format!("Failed to create segments file: {}", e))?;
        file.write_all(segments_json.as_bytes())
            .map_err(|e| format!("Failed to write segments: {}", e))?;

        debug!(
            session_id = %self.session_id,
            path = %segments_path.display(),
            "Segments JSON saved"
        );

        Ok(())
    }

    /// Save a SOAP note to file
    pub fn save_soap_note(&mut self, soap_content: &str, model_used: &str) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        let soap_path = self.session_dir.join("soap_note.txt");
        let mut file = File::create(&soap_path)
            .map_err(|e| format!("Failed to create SOAP note file: {}", e))?;
        file.write_all(soap_content.as_bytes())
            .map_err(|e| format!("Failed to write SOAP note: {}", e))?;

        self.metadata.soap_generated = true;
        self.metadata.soap_model = Some(model_used.to_string());

        info!(
            session_id = %self.session_id,
            path = %soap_path.display(),
            model = %model_used,
            "SOAP note saved to debug storage"
        );

        Ok(())
    }

    /// Mark that transcript was truncated for LLM
    pub fn mark_transcript_truncated(&mut self, original_word_count: usize) {
        if !self.enabled {
            return;
        }
        self.metadata.transcript_truncated = true;
        self.metadata.original_word_count = Some(original_word_count);
    }

    /// Finalize the session and save metadata
    pub fn finalize(&mut self, duration_ms: u64) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        self.metadata.ended_at = Some(Utc::now().to_rfc3339());
        self.metadata.duration_ms = Some(duration_ms);

        // Save metadata
        let metadata_path = self.session_dir.join("metadata.json");
        let metadata_json = serde_json::to_string_pretty(&self.metadata)
            .map_err(|e| format!("Failed to serialize metadata: {}", e))?;
        let mut file = File::create(&metadata_path)
            .map_err(|e| format!("Failed to create metadata file: {}", e))?;
        file.write_all(metadata_json.as_bytes())
            .map_err(|e| format!("Failed to write metadata: {}", e))?;

        info!(
            session_id = %self.session_id,
            path = %metadata_path.display(),
            duration_ms = duration_ms,
            "Debug session finalized"
        );

        Ok(())
    }

    /// Get the full transcript text
    pub fn get_transcript(&self) -> String {
        self.transcript_lines.join("\n")
    }
}

/// Static helper to save a SOAP note for a session that may not have debug storage active
pub fn save_soap_note_standalone(
    session_id: &str,
    soap_content: &str,
    model_used: &str,
    enabled: bool,
) -> Result<(), String> {
    if !enabled {
        return Ok(());
    }

    let session_dir = ensure_session_dir(session_id)?;
    let soap_path = session_dir.join("soap_note.txt");

    let mut file = File::create(&soap_path)
        .map_err(|e| format!("Failed to create SOAP note file: {}", e))?;
    file.write_all(soap_content.as_bytes())
        .map_err(|e| format!("Failed to write SOAP note: {}", e))?;

    info!(
        session_id = %session_id,
        path = %soap_path.display(),
        model = %model_used,
        "SOAP note saved to debug storage (standalone)"
    );

    Ok(())
}

/// List all debug sessions
pub fn list_debug_sessions() -> Result<Vec<String>, String> {
    let base = get_debug_storage_dir()?;
    if !base.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(&base).map_err(|e| format!("Failed to read debug directory: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        if entry.path().is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                sessions.push(name.to_string());
            }
        }
    }

    // Sort by name (which includes timestamp)
    sessions.sort();
    sessions.reverse(); // Most recent first

    Ok(sessions)
}

/// Get metadata for a specific debug session
pub fn get_session_metadata(session_id: &str) -> Result<SessionMetadata, String> {
    let session_dir = get_session_debug_dir(session_id)?;
    let metadata_path = session_dir.join("metadata.json");

    let content = fs::read_to_string(&metadata_path)
        .map_err(|e| format!("Failed to read metadata: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse metadata: {}", e))
}

/// Get transcript for a specific debug session
pub fn get_session_transcript(session_id: &str) -> Result<String, String> {
    let session_dir = get_session_debug_dir(session_id)?;
    let transcript_path = session_dir.join("transcript.txt");

    fs::read_to_string(&transcript_path)
        .map_err(|e| format!("Failed to read transcript: {}", e))
}

/// Get SOAP note for a specific debug session
pub fn get_session_soap_note(session_id: &str) -> Result<String, String> {
    let session_dir = get_session_debug_dir(session_id)?;
    let soap_path = session_dir.join("soap_note.txt");

    fs::read_to_string(&soap_path).map_err(|e| format!("Failed to read SOAP note: {}", e))
}

/// Clean up old debug sessions (keep last N sessions)
pub fn cleanup_old_sessions(keep_count: usize) -> Result<usize, String> {
    let sessions = list_debug_sessions()?;
    let mut deleted = 0;

    if sessions.len() <= keep_count {
        return Ok(0);
    }

    for session_id in sessions.iter().skip(keep_count) {
        let session_dir = get_session_debug_dir(session_id)?;
        if let Err(e) = fs::remove_dir_all(&session_dir) {
            warn!(
                session_id = %session_id,
                error = %e,
                "Failed to delete old debug session"
            );
        } else {
            deleted += 1;
            debug!(session_id = %session_id, "Deleted old debug session");
        }
    }

    if deleted > 0 {
        info!(deleted = deleted, "Cleaned up old debug sessions");
    }

    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_storage_disabled() {
        let storage = DebugStorage::new("test-session", false).unwrap();
        assert!(!storage.is_enabled());
    }

    #[test]
    fn test_session_metadata_new() {
        let metadata = SessionMetadata::new("test-123");
        assert_eq!(metadata.session_id, "test-123");
        assert!(metadata.ended_at.is_none());
        assert!(!metadata.soap_generated);
    }

    #[test]
    fn test_debug_segment_serialization() {
        let segment = DebugSegment {
            index: 0,
            start_ms: 1000,
            end_ms: 2000,
            text: "Hello world".to_string(),
            speaker_id: Some("Speaker 1".to_string()),
        };

        let json = serde_json::to_string(&segment).unwrap();
        assert!(json.contains("Hello world"));
        assert!(json.contains("1000"));
    }
}
