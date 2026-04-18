//! Test-side RunContext that captures emitted events + virtualizes the clock.
//!
//! Clock: uses tokio's paused-time mode. start_utc + elapsed = virtual now.
//! Events: every emit_* pushes a CapturedEvent onto an Arc<Mutex<Vec>>.
//! LLM: Arc<dyn LlmBackend> (typically ReplayLlmBackend).
//! Pipeline: start_pipeline returns a fake PipelineHandle + a pre-loaded
//! channel of PipelineMessages derived from bundle.segments.
//! Sensor source: not plumbed through RunContext today — orchestrator
//! constructs sensor sources from Config. For tests we override config to
//! disable sensor mode (hybrid → llm-only), or will add a ctx.sensor_source
//! hook in a later phase.

use super::captured_event::CapturedEvent;
use super::replay_llm_backend::ReplayLlmBackend;
use crate::continuous_mode_events::ContinuousModeEvent;
use crate::llm_backend::LlmBackend;
use crate::pipeline::{PipelineConfig, PipelineHandle, PipelineMessage};
use crate::replay_bundle::{ReplayBundle, ReplaySegment};
use crate::run_context::RunContext;
use crate::server_config::ServerConfig;
use crate::transcription::Segment;
use chrono::{DateTime, Local, Utc};
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct RecordingRunContext {
    captured: Arc<Mutex<Vec<CapturedEvent>>>,
    server_config: Arc<ServerConfig>,
    llm: Arc<dyn LlmBackend>,
    archive_root: PathBuf,
    start_utc: DateTime<Utc>,
    start_instant: std::time::Instant,
    /// Pre-built PipelineMessages to deliver when start_pipeline is called.
    /// Wrapped in Mutex<Option> because start_pipeline consumes them once.
    pending_messages: Arc<Mutex<Option<Vec<PipelineMessage>>>>,
}

impl RecordingRunContext {
    pub fn new(
        server_config: ServerConfig,
        llm: Arc<dyn LlmBackend>,
        archive_root: PathBuf,
        start_utc: DateTime<Utc>,
        pending_messages: Vec<PipelineMessage>,
    ) -> Self {
        Self {
            captured: Arc::new(Mutex::new(Vec::new())),
            server_config: Arc::new(server_config),
            llm,
            archive_root,
            start_utc,
            start_instant: std::time::Instant::now(),
            pending_messages: Arc::new(Mutex::new(Some(pending_messages))),
        }
    }

    /// Build a context seeded from a ReplayBundle.
    pub fn from_bundle(bundle: &ReplayBundle, archive_root: PathBuf) -> Self {
        use crate::harness::policies::PromptPolicy;

        let start_utc = bundle
            .segments
            .first()
            .and_then(|s| DateTime::parse_from_rfc3339(&s.ts).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let llm: Arc<dyn LlmBackend> =
            Arc::new(ReplayLlmBackend::from_bundle(bundle, PromptPolicy::Strict));

        let pending = segments_to_messages(&bundle.segments);

        Self::new(
            compiled_default_server_config(),
            llm,
            archive_root,
            start_utc,
            pending,
        )
    }

    pub fn captured_events(&self) -> Vec<CapturedEvent> {
        self.captured.lock().expect("poisoned").clone()
    }

    fn record(&self, event_name: &str, window: Option<String>, payload: serde_json::Value) {
        let virtual_ts = self.now_utc();
        self.captured
            .lock()
            .expect("poisoned")
            .push(CapturedEvent {
                virtual_ts,
                window,
                event_name: event_name.into(),
                payload,
            });
    }
}

impl RunContext for RecordingRunContext {
    fn emit_continuous_event(&self, event: &ContinuousModeEvent) {
        let payload = serde_json::to_value(event).unwrap_or_default();
        self.record("continuous_mode_event", None, payload);
    }

    fn emit_json(&self, event_name: &str, payload: serde_json::Value) {
        self.record(event_name, None, payload);
    }

    fn emit_to_window_json(&self, window: &str, event_name: &str, payload: serde_json::Value) {
        self.record(event_name, Some(window.into()), payload);
    }

    fn server_config_snapshot(&self) -> ServerConfig {
        (*self.server_config).clone()
    }

    fn llm(&self) -> Arc<dyn LlmBackend> {
        Arc::clone(&self.llm)
    }

    fn start_pipeline(
        &self,
        _config: PipelineConfig,
    ) -> Result<(PipelineHandle, mpsc::Receiver<PipelineMessage>), String> {
        let pending = self
            .pending_messages
            .lock()
            .expect("poisoned")
            .take()
            .unwrap_or_default();

        // Channel sized to hold every pre-loaded message so try_send never
        // backpressures. For small margins, +16 also leaves room for any
        // late messages the orchestrator internally mixes in (none today,
        // but defensive).
        let capacity = (pending.len() + 16).max(32);
        let (tx, rx) = mpsc::channel::<PipelineMessage>(capacity);

        for msg in pending {
            // try_send cannot fail here because capacity is sized to fit.
            if let Err(e) = tx.try_send(msg) {
                return Err(format!("ScriptedPipeline: internal try_send error: {:?}", e));
            }
        }

        // Drop the sender so the receiver naturally closes when drained.
        // The orchestrator's `rx.recv().await` returns None after the last
        // scripted message, triggering its shutdown path.
        drop(tx);

        let handle = fake_pipeline_handle();
        Ok((handle, rx))
    }

    fn archive_root(&self) -> PathBuf {
        self.archive_root.clone()
    }

    fn now_utc(&self) -> DateTime<Utc> {
        let elapsed = self.start_instant.elapsed();
        self.start_utc
            + chrono::Duration::from_std(elapsed).unwrap_or(chrono::Duration::zero())
    }

    fn now_local(&self) -> DateTime<Local> {
        self.now_utc().with_timezone(&Local)
    }

    fn sleep(&self, dur: Duration) -> impl Future<Output = ()> + Send {
        tokio::time::sleep(dur)
    }
    // raw_tauri_app() returns None via the trait default — test contexts have no AppHandle.
}

/// Convert bundle.segments into a vec of PipelineMessage::Segment(Segment).
/// Each ReplaySegment maps to a production Segment with the recorded text,
/// start/end ms, and (optional) speaker id.
fn segments_to_messages(segments: &[ReplaySegment]) -> Vec<PipelineMessage> {
    segments
        .iter()
        .map(|s| {
            let mut seg = Segment::new(s.start_ms, s.end_ms, s.text.clone());
            seg.speaker_id = s.speaker_id.clone();
            seg.speaker_confidence = s.speaker_confidence;
            PipelineMessage::Segment(seg)
        })
        .collect()
}

/// Construct a PipelineHandle with no spawned thread. stop/join are no-ops;
/// reset_biomarkers_flag returns an Arc<AtomicBool> for the orchestrator to hold.
fn fake_pipeline_handle() -> PipelineHandle {
    // PipelineHandle's fields are private. We can't construct it directly.
    // Workaround: start_pipeline with a dummy config that does nothing.
    // For tests, the dummy PipelineConfig leads to a real pipeline thread —
    // which we don't want. So we add a test-only constructor in pipeline.rs.
    PipelineHandle::for_testing()
}

/// Best-effort compiled-default ServerConfig for test contexts that don't
/// need real prompts/thresholds. If server_config.rs doesn't expose a
/// `compiled_defaults` helper, construct manually using Default where possible.
fn compiled_default_server_config() -> ServerConfig {
    use crate::server_config::{
        BillingData, ConfigSource, DetectionThresholds, OperationalDefaults, PromptTemplates,
    };
    ServerConfig {
        prompts: PromptTemplates::default(),
        billing: BillingData::default(),
        thresholds: DetectionThresholds::default(),
        defaults: OperationalDefaults::default(),
        version: 0,
        source: ConfigSource::CompiledDefaults,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trivial_ctx() -> RecordingRunContext {
        use crate::harness::policies::PromptPolicy;
        RecordingRunContext::new(
            compiled_default_server_config(),
            Arc::new(ReplayLlmBackend::for_testing(vec![], PromptPolicy::Strict)),
            std::env::temp_dir().join("harness-unit-test"),
            Utc::now(),
            vec![],
        )
    }

    #[tokio::test]
    async fn emits_are_captured() {
        let ctx = trivial_ctx();
        ctx.emit_json("test_event", serde_json::json!({"x": 1}));
        let events = ctx.captured_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_name, "test_event");
    }

    #[tokio::test]
    async fn continuous_events_capture_type_field() {
        let ctx = trivial_ctx();
        ctx.emit_continuous_event(&ContinuousModeEvent::Started);
        let events = ctx.captured_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type(), Some("started"));
    }

    #[tokio::test]
    async fn raw_tauri_app_is_none_in_tests() {
        let ctx = trivial_ctx();
        assert!(ctx.raw_tauri_app().is_none());
    }
}
