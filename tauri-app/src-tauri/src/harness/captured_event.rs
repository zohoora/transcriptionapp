//! Event capture type for RecordingRunContext.
//!
//! Every emit_* call on a RecordingRunContext appends a CapturedEvent to an
//! internal Vec; the harness reads the Vec after the run to feed the event
//! comparator.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedEvent {
    /// Virtual time of emission (ctx.now_utc at the emit point).
    pub virtual_ts: DateTime<Utc>,
    /// Window target — Some for window-scoped emits, None for app-level.
    pub window: Option<String>,
    /// Event name ("continuous_mode_event", "continuous_transcript_preview", etc).
    pub event_name: String,
    /// Event payload as JSON.
    pub payload: serde_json::Value,
}

impl CapturedEvent {
    /// For continuous_mode_event payloads, extract the "type" field.
    /// Returns None for other event types.
    pub fn event_type(&self) -> Option<&str> {
        self.payload.get("type").and_then(|v| v.as_str())
    }
}
