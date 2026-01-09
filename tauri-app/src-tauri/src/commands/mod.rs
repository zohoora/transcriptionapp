//! Tauri command handlers organized by domain.
//!
//! This module re-exports all command handlers for registration in lib.rs.

mod audio;
mod listening;
mod medplum;
mod models;
mod ollama;
mod permissions;
mod session;
mod settings;
mod whisper_server;

// Re-export all commands for lib.rs registration
pub use audio::*;
pub use listening::*;
pub use medplum::*;
pub use models::*;
pub use ollama::*;
pub use permissions::*;
pub use session::*;
pub use settings::*;
pub use whisper_server::*;

use crate::medplum::MedplumClient;
use crate::pipeline::PipelineHandle;
use crate::session::SessionManager;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tokio::sync::RwLock;

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

/// Helper to emit session status
pub(crate) fn emit_status_arc(
    app: &AppHandle,
    session_state: &SharedSessionManager,
) -> Result<(), String> {
    use tauri::Emitter;
    let status = {
        let session = session_state.lock().map_err(|e| e.to_string())?;
        session.status()
    };
    app.emit("session_status", status).map_err(|e| e.to_string())
}

/// Helper to emit transcript update
pub(crate) fn emit_transcript_arc(
    app: &AppHandle,
    session_state: &SharedSessionManager,
) -> Result<(), String> {
    use tauri::Emitter;
    let transcript = {
        let session = session_state.lock().map_err(|e| e.to_string())?;
        session.transcript_update()
    };
    app.emit("transcript_update", transcript)
        .map_err(|e| e.to_string())
}
