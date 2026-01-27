use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::transcription::Segment;

/// Session state enum matching the spec
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Idle,
    Preparing,
    Recording,
    Stopping,
    Completed,
    Error,
}

/// Session error types
#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub enum SessionError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Audio device error: {0}")]
    AudioDeviceError(String),
    #[error("Transcription error: {0}")]
    TranscriptionError(String),
    #[error("Invalid state transition: {0}")]
    InvalidTransition(String),
}

/// Status update sent to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatus {
    pub state: SessionState,
    pub provider: Option<String>,
    pub elapsed_ms: u64,
    pub is_processing_behind: bool,
    pub error_message: Option<String>,
    pub session_id: Option<String>,
}

/// Transcript update sent to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptUpdate {
    pub finalized_text: String,
    pub draft_text: Option<String>,
    pub segment_count: usize,
}

/// Session manager handles the transcription session lifecycle
pub struct SessionManager {
    state: SessionState,
    provider: Option<String>,
    start_time: Option<std::time::Instant>,
    segments: Vec<Segment>,
    error: Option<SessionError>,
    stop_flag: Arc<AtomicBool>,
    pending_count: usize,
    audio_file_path: Option<PathBuf>,
    /// Unique session ID for log correlation (generated on start, reused for stop)
    session_id: Option<String>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            state: SessionState::Idle,
            provider: None,
            start_time: None,
            segments: Vec::new(),
            error: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
            pending_count: 0,
            audio_file_path: None,
            session_id: None,
        }
    }

    /// Get the current session ID (for log correlation)
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Set the audio file path for recording
    pub fn set_audio_file_path(&mut self, path: PathBuf) {
        self.audio_file_path = Some(path);
    }

    /// Get the audio file path if recording was done
    pub fn audio_file_path(&self) -> Option<&PathBuf> {
        self.audio_file_path.as_ref()
    }

    /// Get current session status
    pub fn status(&self) -> SessionStatus {
        let elapsed_ms = self
            .start_time
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);

        SessionStatus {
            state: self.state.clone(),
            provider: self.provider.clone(),
            elapsed_ms,
            is_processing_behind: self.pending_count > 3,
            error_message: self.error.as_ref().map(|e| e.to_string()),
            session_id: self.session_id.clone(),
        }
    }

    /// Get current transcript update
    pub fn transcript_update(&self) -> TranscriptUpdate {
        let finalized_text = self
            .segments
            .iter()
            .map(|s| {
                // Format with speaker label and confidence if available
                if let Some(ref speaker) = s.speaker_id {
                    if let Some(confidence) = s.speaker_confidence {
                        format!("[{} ({:.0}%)]: {}", speaker, confidence * 100.0, s.text)
                    } else {
                        format!("[{}]: {}", speaker, s.text)
                    }
                } else {
                    s.text.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        TranscriptUpdate {
            finalized_text,
            draft_text: None, // Draft text would come from partial transcription
            segment_count: self.segments.len(),
        }
    }

    /// Transition to preparing state
    pub fn start_preparing(&mut self) -> Result<(), SessionError> {
        if self.state != SessionState::Idle {
            return Err(SessionError::InvalidTransition(format!(
                "Cannot start from state {:?}",
                self.state
            )));
        }

        info!("Session transitioning to Preparing");
        self.state = SessionState::Preparing;
        self.error = None;
        self.segments.clear();
        self.stop_flag.store(false, Ordering::SeqCst);
        // Generate a new session ID for log correlation
        self.session_id = Some(uuid::Uuid::new_v4().to_string());
        Ok(())
    }

    /// Transition to recording state
    pub fn start_recording(&mut self, provider: &str) {
        info!("Session transitioning to Recording with provider: {}", provider);
        self.state = SessionState::Recording;
        self.provider = Some(provider.to_string());
        self.start_time = Some(std::time::Instant::now());
    }

    /// Transition to stopping state
    pub fn start_stopping(&mut self) -> Result<Arc<AtomicBool>, SessionError> {
        if self.state != SessionState::Recording {
            return Err(SessionError::InvalidTransition(format!(
                "Cannot stop from state {:?}",
                self.state
            )));
        }

        info!("Session transitioning to Stopping");
        self.state = SessionState::Stopping;
        self.stop_flag.store(true, Ordering::SeqCst);
        Ok(self.stop_flag.clone())
    }

    /// Transition to completed state
    pub fn complete(&mut self) {
        info!("Session transitioning to Completed");
        self.state = SessionState::Completed;
    }

    /// Transition to error state
    pub fn set_error(&mut self, error: SessionError) {
        warn!("Session error: {}", error);
        self.state = SessionState::Error;
        self.error = Some(error);
    }

    /// Reset to idle state
    pub fn reset(&mut self) {
        info!("Session resetting to Idle");
        self.state = SessionState::Idle;
        self.provider = None;
        self.start_time = None;
        self.segments.clear();
        self.error = None;
        self.stop_flag.store(false, Ordering::SeqCst);
        self.pending_count = 0;
        self.audio_file_path = None;
        self.session_id = None;
    }

    /// Add a transcribed segment
    pub fn add_segment(&mut self, segment: Segment) {
        debug!("Adding segment: {}ms - {}ms", segment.start_ms, segment.end_ms);
        self.segments.push(segment);
    }

    /// Update pending count (for processing status)
    pub fn set_pending_count(&mut self, count: usize) {
        self.pending_count = count;
    }

    /// Get the stop flag for the processing thread
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }

    /// Get current state
    pub fn state(&self) -> &SessionState {
        &self.state
    }

    /// Check if recording
    pub fn is_recording(&self) -> bool {
        self.state == SessionState::Recording
    }

    /// Get all segments
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_state_transitions() {
        let mut session = SessionManager::new();
        assert_eq!(session.state(), &SessionState::Idle);

        // Start preparing
        session.start_preparing().unwrap();
        assert_eq!(session.state(), &SessionState::Preparing);

        // Start recording
        session.start_recording("whisper");
        assert_eq!(session.state(), &SessionState::Recording);

        // Stop
        session.start_stopping().unwrap();
        assert_eq!(session.state(), &SessionState::Stopping);

        // Complete
        session.complete();
        assert_eq!(session.state(), &SessionState::Completed);

        // Reset
        session.reset();
        assert_eq!(session.state(), &SessionState::Idle);
    }

    #[test]
    fn test_invalid_transition() {
        let mut session = SessionManager::new();

        // Can't stop from idle
        let result = session.start_stopping();
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_prepare_from_recording() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");

        let result = session.start_preparing();
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_prepare_from_preparing() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();

        let result = session.start_preparing();
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_stop_from_stopping() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");
        session.start_stopping().unwrap();

        let result = session.start_stopping();
        assert!(result.is_err());
    }

    #[test]
    fn test_error_state() {
        let mut session = SessionManager::new();
        session.set_error(SessionError::ModelNotFound("test.bin".to_string()));

        assert_eq!(session.state(), &SessionState::Error);

        let status = session.status();
        assert!(status.error_message.is_some());
        assert!(status.error_message.unwrap().contains("test.bin"));
    }

    #[test]
    fn test_add_segment() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");

        let segment = Segment::new(0, 1000, "Hello".to_string());
        session.add_segment(segment);

        assert_eq!(session.segments().len(), 1);
        assert_eq!(session.segments()[0].text, "Hello");
    }

    #[test]
    fn test_transcript_update() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");

        session.add_segment(Segment::new(0, 1000, "Hello".to_string()));
        session.add_segment(Segment::new(1000, 2000, "World".to_string()));

        let update = session.transcript_update();
        assert_eq!(update.segment_count, 2);
        assert!(update.finalized_text.contains("Hello"));
        assert!(update.finalized_text.contains("World"));
    }

    #[test]
    fn test_status_provider() {
        let mut session = SessionManager::new();

        // Before recording
        let status = session.status();
        assert!(status.provider.is_none());

        session.start_preparing().unwrap();
        session.start_recording("whisper");

        let status = session.status();
        assert_eq!(status.provider, Some("whisper".to_string()));
    }

    #[test]
    fn test_is_recording() {
        let mut session = SessionManager::new();
        assert!(!session.is_recording());

        session.start_preparing().unwrap();
        assert!(!session.is_recording());

        session.start_recording("whisper");
        assert!(session.is_recording());

        session.start_stopping().unwrap();
        assert!(!session.is_recording());
    }

    #[test]
    fn test_pending_count_affects_processing_behind() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");

        // Low pending count
        session.set_pending_count(2);
        let status = session.status();
        assert!(!status.is_processing_behind);

        // High pending count
        session.set_pending_count(5);
        let status = session.status();
        assert!(status.is_processing_behind);
    }

    #[test]
    fn test_stop_flag() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");

        let stop_flag = session.stop_flag();
        assert!(!stop_flag.load(Ordering::Relaxed));

        session.start_stopping().unwrap();
        assert!(stop_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn test_reset_clears_segments() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");
        session.add_segment(Segment::new(0, 1000, "Hello".to_string()));

        session.reset();

        assert_eq!(session.segments().len(), 0);
    }

    #[test]
    fn test_reset_clears_error() {
        let mut session = SessionManager::new();
        session.set_error(SessionError::AudioDeviceError("test".to_string()));

        session.reset();

        let status = session.status();
        assert!(status.error_message.is_none());
    }

    #[test]
    fn test_default_implementation() {
        let session = SessionManager::default();
        assert_eq!(session.state(), &SessionState::Idle);
    }

    #[test]
    fn test_session_error_display() {
        let e1 = SessionError::ModelNotFound("model.bin".to_string());
        assert!(e1.to_string().contains("model.bin"));

        let e2 = SessionError::AudioDeviceError("no mic".to_string());
        assert!(e2.to_string().contains("no mic"));

        let e3 = SessionError::TranscriptionError("failed".to_string());
        assert!(e3.to_string().contains("failed"));

        let e4 = SessionError::InvalidTransition("bad".to_string());
        assert!(e4.to_string().contains("bad"));
    }

    #[test]
    fn test_elapsed_time_zero_when_idle() {
        let session = SessionManager::new();
        let status = session.status();
        assert_eq!(status.elapsed_ms, 0);
    }

    #[test]
    fn test_start_preparing_clears_previous_data() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");
        session.add_segment(Segment::new(0, 1000, "Old".to_string()));
        session.complete();
        session.reset();

        // Start a new session
        session.start_preparing().unwrap();
        assert_eq!(session.segments().len(), 0);
    }
}
