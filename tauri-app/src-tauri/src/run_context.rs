//! The runtime environment that run_continuous_mode runs inside.
//!
//! Production uses TauriRunContext (wraps AppHandle + real LLMClient + the real
//! audio pipeline). Tests use RecordingRunContext in src/harness/ which drives
//! from a replay_bundle and captures emitted events.
//!
//! This trait is the narrowest possible abstraction that makes
//! run_continuous_mode testable offline. All the orchestrator's reaches into
//! Tauri-specific state, global clocks, and outbound dependencies route
//! through methods on this trait. No other orchestrator code needs to change
//! once this seam is in place.

use crate::continuous_mode_events::ContinuousModeEvent;
use crate::llm_backend::LlmBackend;
use crate::pipeline::{PipelineConfig, PipelineHandle, PipelineMessage};
use crate::server_config::ServerConfig;
use chrono::{DateTime, Local, Utc};
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::mpsc;

// ============================================================================
// The trait
// ============================================================================

pub trait RunContext: Clone + Send + Sync + 'static {
    // ── Event emission ──────────────────────────────────────────────────

    fn emit_continuous_event(&self, event: &ContinuousModeEvent);
    fn emit_json(&self, event_name: &str, payload: serde_json::Value);
    fn emit_to_window_json(&self, window: &str, event_name: &str, payload: serde_json::Value);

    // ── Managed state snapshot ──────────────────────────────────────────

    /// Snapshot of server-configurable data (prompts, billing, thresholds, defaults).
    /// Called at continuous-mode start (once) to freeze inputs for the run's
    /// detector/consumer/flush tasks.
    fn server_config_snapshot(&self) -> ServerConfig;

    // ── Outbound dependencies ───────────────────────────────────────────

    fn llm(&self) -> Arc<dyn LlmBackend>;

    /// Start the audio → STT → segments pipeline and return a handle + the
    /// channel to consume PipelineMessages from.
    ///
    /// Production: delegates to crate::pipeline::start_pipeline. Test: returns
    /// a minimal PipelineHandle (no spawned thread) + a pre-loaded channel of
    /// recorded segments.
    fn start_pipeline(
        &self,
        config: PipelineConfig,
    ) -> Result<(PipelineHandle, mpsc::Receiver<PipelineMessage>), String>;

    // ── I/O ─────────────────────────────────────────────────────────────

    fn archive_root(&self) -> PathBuf;

    // ── Clock ───────────────────────────────────────────────────────────

    fn now_utc(&self) -> DateTime<Utc>;
    fn now_local(&self) -> DateTime<Local>;
    fn sleep(&self, dur: Duration) -> impl Future<Output = ()> + Send;

    // ── Escape hatch for not-yet-abstracted Tauri-dependent subsystems ──
    //
    // Returns Some(AppHandle) in production (TauriRunContext) and None in
    // tests (RecordingRunContext). Callers that depend on Tauri-specific
    // APIs not yet threaded through the trait (e.g. shadow_observer) can
    // gate themselves on the Some case. Test paths skip those features.
    fn raw_tauri_app(&self) -> Option<AppHandle> { None }
}

// ============================================================================
// Production impl
// ============================================================================

#[derive(Clone)]
pub struct TauriRunContext {
    pub app: AppHandle,
    pub archive_root: PathBuf,
    pub llm: Arc<dyn LlmBackend>,
}

impl TauriRunContext {
    pub fn new(app: AppHandle, archive_root: PathBuf, llm: Arc<dyn LlmBackend>) -> Self {
        Self { app, archive_root, llm }
    }
}

impl RunContext for TauriRunContext {
    fn emit_continuous_event(&self, event: &ContinuousModeEvent) {
        use tauri::Emitter;
        let _ = self.app.emit("continuous_mode_event", event);
    }

    fn emit_json(&self, event_name: &str, payload: serde_json::Value) {
        use tauri::Emitter;
        let _ = self.app.emit(event_name, payload);
    }

    fn emit_to_window_json(&self, window: &str, event_name: &str, payload: serde_json::Value) {
        use tauri::Emitter;
        let _ = self.app.emit_to(window, event_name, payload);
    }

    fn server_config_snapshot(&self) -> ServerConfig {
        use tauri::Manager;
        let shared = self.app.state::<crate::commands::physicians::SharedServerConfig>();
        let guard = shared.blocking_read();
        guard.clone()
    }

    fn llm(&self) -> Arc<dyn LlmBackend> {
        Arc::clone(&self.llm)
    }

    fn start_pipeline(
        &self,
        config: PipelineConfig,
    ) -> Result<(PipelineHandle, mpsc::Receiver<PipelineMessage>), String> {
        let (tx, rx) = mpsc::channel::<PipelineMessage>(32);
        let handle = crate::pipeline::start_pipeline(config, tx)
            .map_err(|e| e.to_string())?;
        Ok((handle, rx))
    }

    fn archive_root(&self) -> PathBuf {
        self.archive_root.clone()
    }

    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn now_local(&self) -> DateTime<Local> {
        Local::now()
    }

    fn sleep(&self, dur: Duration) -> impl Future<Output = ()> + Send {
        tokio::time::sleep(dur)
    }

    fn raw_tauri_app(&self) -> Option<AppHandle> {
        Some(self.app.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_tauri_context_is_run_context() {
        fn takes_ctx(_: impl RunContext) {}
        // TauriRunContext requires an AppHandle which is runtime-only; this
        // is a pure type-check that the impl is complete.
        let _ = |c: TauriRunContext| takes_ctx(c);
    }
}
