use serde::{Deserialize, Serialize};
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
        }
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
        }
    }

    /// Get current transcript update
    pub fn transcript_update(&self) -> TranscriptUpdate {
        let finalized_text = self
            .segments
            .iter()
            .map(|s| s.text.as_str())
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
}
