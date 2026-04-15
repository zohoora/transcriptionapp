//! Tauri command handlers organized by domain.
//!
//! This module re-exports all command handlers for registration in lib.rs.

mod archive;
mod audio;
mod audio_upload;
mod billing;
pub(crate) mod calibration;
mod clinical_chat;
mod continuous;
mod listening;
mod medplum;
mod images;
mod miis;
mod models;
mod ollama;
mod permissions;
pub(crate) mod physicians;
mod screenshot;
mod session;
mod settings;
mod speaker_profiles;
mod whisper_server;

// Re-export all commands for lib.rs registration
pub use archive::*;
pub use audio::*;
pub use audio_upload::*;
pub use billing::*;
pub use calibration::*;
pub use clinical_chat::*;
pub use continuous::*;
pub use listening::*;
pub use medplum::*;
pub use images::*;
pub use miis::*;
pub use models::*;
pub use ollama::*;
pub use permissions::*;
pub use physicians::*;
pub use screenshot::*;
pub use session::*;
pub use settings::*;
pub use speaker_profiles::*;
pub use whisper_server::*;

use crate::medplum::MedplumClient;
use crate::pipeline::PipelineHandle;
use crate::session::SessionManager;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tokio::sync::RwLock;

/// Structured error type for Tauri commands.
/// Provides better context than raw String errors.
///
/// Serializes to a plain string via `Display`, so the frontend
/// is unaffected by adding new variants.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Lock poisoned: {context}")]
    LockPoisoned { context: String },

    #[error("Session error: {0}")]
    Session(#[from] crate::session::SessionError),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Not running: {0}")]
    NotRunning(String),

    #[error("Already running: {0}")]
    AlreadyRunning(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("{0}")]
    Other(String),
}

impl CommandError {
    /// Convenience constructor for lock-poisoned errors.
    pub fn lock_poisoned(context: &str) -> Self {
        CommandError::LockPoisoned {
            context: context.to_string(),
        }
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        CommandError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for CommandError {
    fn from(e: serde_json::Error) -> Self {
        CommandError::Config(e.to_string())
    }
}

impl From<String> for CommandError {
    fn from(s: String) -> Self {
        CommandError::Other(s)
    }
}

impl serde::Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Device information for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// State for the running pipeline
#[derive(Default)]
pub struct PipelineState {
    pub handle: Option<PipelineHandle>,
    /// Generation counter to detect stale pipeline messages after reset
    pub generation: u64,
}

impl PipelineState {
    /// Increment generation and return the new value
    pub fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }
}

/// Shared session manager type for use in async contexts
pub type SharedSessionManager = Arc<Mutex<SessionManager>>;

/// Shared pipeline state type for use in async contexts
pub type SharedPipelineState = Arc<Mutex<PipelineState>>;

/// Shared Medplum client for EMR integration
pub type SharedMedplumClient = Arc<RwLock<Option<MedplumClient>>>;

/// Create a shared Medplum client from config
pub fn create_medplum_client() -> SharedMedplumClient {
    Arc::new(RwLock::new(None))
}

/// Parse a "YYYY-MM-DD" date string into a DateTime<Utc> (noon UTC).
pub(crate) fn parse_date(date: &str) -> Result<chrono::DateTime<chrono::Utc>, CommandError> {
    let naive_date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| CommandError::Validation(format!("Invalid date format: {}", e)))?;
    let datetime = naive_date
        .and_hms_opt(12, 0, 0)
        .ok_or_else(|| CommandError::Validation("Invalid time".into()))?;
    Ok(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(datetime, chrono::Utc))
}

/// Helper to emit session status
pub(crate) fn emit_status_arc(
    app: &AppHandle,
    session_state: &SharedSessionManager,
) -> Result<(), CommandError> {
    use tauri::Emitter;
    let status = {
        let session = session_state
            .lock()
            .map_err(|_| CommandError::lock_poisoned("session_state"))?;
        session.status()
    };
    app.emit("session_status", status)
        .map_err(|e| CommandError::Other(e.to_string()))
}

/// Helper to emit transcript update
pub(crate) fn emit_transcript_arc(
    app: &AppHandle,
    session_state: &SharedSessionManager,
) -> Result<(), CommandError> {
    use tauri::Emitter;
    let transcript = {
        let session = session_state
            .lock()
            .map_err(|_| CommandError::lock_poisoned("session_state"))?;
        session.transcript_update()
    };
    app.emit("transcript_update", transcript)
        .map_err(|e| CommandError::Other(e.to_string()))
}
