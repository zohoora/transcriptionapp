//! Typed event emission for continuous mode.
//!
//! Replaces inline `serde_json::json!({...})` calls with a typed enum that
//! serializes to identical JSON. This ensures compile-time field name checking
//! and eliminates typos in event payloads.

use serde::Serialize;

/// All events emitted on the `"continuous_mode_event"` channel.
///
/// Serialized with `#[serde(tag = "type")]` so the JSON always contains a
/// `"type"` field matching the variant name in snake_case.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContinuousModeEvent {
    Error {
        error: String,
    },
    Started,
    Stopped,
    Checking,
    SensorStatus {
        connected: bool,
        state: String,
    },
    IdleBufferCleared {
        word_count: usize,
        buffer_age_secs: i64,
    },
    EncounterDetected {
        session_id: String,
        word_count: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        patient_name: Option<String>,
    },
    SoapGenerated {
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        patient_count: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        recovered: Option<bool>,
    },
    SoapFailed {
        session_id: String,
        error: String,
    },
    EncounterMerged {
        #[serde(skip_serializing_if = "Option::is_none")]
        kept_session_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        merged_into_session_id: Option<String>,
        removed_session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Speech detected by VAD but no transcription segments produced
    TranscriptionStalled {
        speech_secs: u64,
    },
    SleepStarted {
        resume_at: String,
    },
    SleepEnded,
    ShadowDecision {
        shadow_method: String,
        outcome: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        buffer_words: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sensor_state: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
}

impl ContinuousModeEvent {
    /// Emit this event on the `"continuous_mode_event"` channel (Tauri path).
    pub fn emit(&self, app: &tauri::AppHandle) {
        use tauri::Emitter;
        let _ = app.emit("continuous_mode_event", self);
    }

    /// Emit this event via a RunContext. Used by run_continuous_mode so the
    /// same call site works in both production (TauriRunContext forwards to
    /// app.emit) and tests (RecordingRunContext records the event in a Vec).
    pub fn emit_via<C: crate::run_context::RunContext>(&self, ctx: &C) {
        ctx.emit_continuous_event(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_error() {
        let event = ContinuousModeEvent::Error {
            error: "something broke".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "error");
        assert_eq!(json["error"], "something broke");
        assert_eq!(json.as_object().unwrap().len(), 2);
    }

    #[test]
    fn serialize_started() {
        let event = ContinuousModeEvent::Started;
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "started");
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    #[test]
    fn serialize_stopped() {
        let event = ContinuousModeEvent::Stopped;
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "stopped");
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    #[test]
    fn serialize_checking() {
        let event = ContinuousModeEvent::Checking;
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "checking");
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    #[test]
    fn serialize_sensor_status() {
        let event = ContinuousModeEvent::SensorStatus {
            connected: true,
            state: "present".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "sensor_status");
        assert_eq!(json["connected"], true);
        assert_eq!(json["state"], "present");
        assert_eq!(json.as_object().unwrap().len(), 3);
    }

    #[test]
    fn serialize_idle_buffer_cleared() {
        let event = ContinuousModeEvent::IdleBufferCleared {
            word_count: 42,
            buffer_age_secs: 600,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "idle_buffer_cleared");
        assert_eq!(json["word_count"], 42);
        assert_eq!(json["buffer_age_secs"], 600);
        assert_eq!(json.as_object().unwrap().len(), 3);
    }

    #[test]
    fn serialize_encounter_detected_with_name() {
        let event = ContinuousModeEvent::EncounterDetected {
            session_id: "sess-123".into(),
            word_count: 500,
            patient_name: Some("John Doe".into()),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "encounter_detected");
        assert_eq!(json["session_id"], "sess-123");
        assert_eq!(json["word_count"], 500);
        assert_eq!(json["patient_name"], "John Doe");
        assert_eq!(json.as_object().unwrap().len(), 4);
    }

    #[test]
    fn serialize_encounter_detected_without_name() {
        let event = ContinuousModeEvent::EncounterDetected {
            session_id: "sess-456".into(),
            word_count: 300,
            patient_name: None,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "encounter_detected");
        assert_eq!(json["session_id"], "sess-456");
        assert_eq!(json["word_count"], 300);
        // patient_name should be absent (skip_serializing_if = None)
        assert!(json.get("patient_name").is_none());
        assert_eq!(json.as_object().unwrap().len(), 3);
    }

    #[test]
    fn serialize_soap_generated_with_patient_count() {
        let event = ContinuousModeEvent::SoapGenerated {
            session_id: "sess-789".into(),
            patient_count: Some(2),
            recovered: None,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "soap_generated");
        assert_eq!(json["session_id"], "sess-789");
        assert_eq!(json["patient_count"], 2);
        assert!(json.get("recovered").is_none());
        assert_eq!(json.as_object().unwrap().len(), 3);
    }

    #[test]
    fn serialize_soap_generated_recovered() {
        let event = ContinuousModeEvent::SoapGenerated {
            session_id: "sess-orphan".into(),
            patient_count: None,
            recovered: Some(true),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "soap_generated");
        assert_eq!(json["session_id"], "sess-orphan");
        assert!(json.get("patient_count").is_none());
        assert_eq!(json["recovered"], true);
        assert_eq!(json.as_object().unwrap().len(), 3);
    }

    #[test]
    fn serialize_soap_generated_minimal() {
        let event = ContinuousModeEvent::SoapGenerated {
            session_id: "sess-flush".into(),
            patient_count: None,
            recovered: None,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "soap_generated");
        assert_eq!(json["session_id"], "sess-flush");
        assert!(json.get("patient_count").is_none());
        assert!(json.get("recovered").is_none());
        assert_eq!(json.as_object().unwrap().len(), 2);
    }

    #[test]
    fn serialize_soap_failed() {
        let event = ContinuousModeEvent::SoapFailed {
            session_id: "sess-fail".into(),
            error: "timeout_120s".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "soap_failed");
        assert_eq!(json["session_id"], "sess-fail");
        assert_eq!(json["error"], "timeout_120s");
        assert_eq!(json.as_object().unwrap().len(), 3);
    }

    #[test]
    fn serialize_encounter_merged_with_kept() {
        let event = ContinuousModeEvent::EncounterMerged {
            kept_session_id: Some("prev-sess".into()),
            merged_into_session_id: None,
            removed_session_id: "curr-sess".into(),
            reason: Some("small orphan (150 words) with sensor present".into()),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "encounter_merged");
        assert_eq!(json["kept_session_id"], "prev-sess");
        assert!(json.get("merged_into_session_id").is_none());
        assert_eq!(json["removed_session_id"], "curr-sess");
        assert_eq!(
            json["reason"],
            "small orphan (150 words) with sensor present"
        );
        assert_eq!(json.as_object().unwrap().len(), 4);
    }

    #[test]
    fn serialize_encounter_merged_with_merged_into() {
        let event = ContinuousModeEvent::EncounterMerged {
            kept_session_id: None,
            merged_into_session_id: Some("prev-sess".into()),
            removed_session_id: "curr-sess".into(),
            reason: None,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "encounter_merged");
        assert!(json.get("kept_session_id").is_none());
        assert_eq!(json["merged_into_session_id"], "prev-sess");
        assert_eq!(json["removed_session_id"], "curr-sess");
        assert!(json.get("reason").is_none());
        assert_eq!(json.as_object().unwrap().len(), 3);
    }

    #[test]
    fn serialize_sleep_started() {
        let event = ContinuousModeEvent::SleepStarted {
            resume_at: "2026-04-01T10:00:00Z".into(),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "sleep_started");
        assert_eq!(json["resume_at"], "2026-04-01T10:00:00Z");
    }

    #[test]
    fn serialize_sleep_ended() {
        let event = ContinuousModeEvent::SleepEnded;
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "sleep_ended");
        assert_eq!(json.as_object().unwrap().len(), 1);
    }

    #[test]
    fn serialize_shadow_decision_sensor() {
        let event = ContinuousModeEvent::ShadowDecision {
            shadow_method: "sensor".into(),
            outcome: "would_split".into(),
            buffer_words: Some(1500),
            sensor_state: Some("absent".into()),
            confidence: None,
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "shadow_decision");
        assert_eq!(json["shadow_method"], "sensor");
        assert_eq!(json["outcome"], "would_split");
        assert_eq!(json["buffer_words"], 1500);
        assert_eq!(json["sensor_state"], "absent");
        assert!(json.get("confidence").is_none());
        assert_eq!(json.as_object().unwrap().len(), 5);
    }

    #[test]
    fn serialize_shadow_decision_llm() {
        let event = ContinuousModeEvent::ShadowDecision {
            shadow_method: "llm".into(),
            outcome: "would_not_split".into(),
            buffer_words: Some(800),
            sensor_state: None,
            confidence: Some(0.85),
        };
        let json: serde_json::Value = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "shadow_decision");
        assert_eq!(json["shadow_method"], "llm");
        assert_eq!(json["outcome"], "would_not_split");
        assert_eq!(json["buffer_words"], 800);
        assert!(json.get("sensor_state").is_none());
        assert_eq!(json["confidence"], 0.85);
        assert_eq!(json.as_object().unwrap().len(), 5);
    }
}
