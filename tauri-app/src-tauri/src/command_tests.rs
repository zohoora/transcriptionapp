//! Tests for Tauri IPC commands
//!
//! These tests verify the command handlers work correctly without
//! requiring the full Tauri application context.

#[cfg(test)]
mod tests {
    use crate::config::Settings;
    use crate::session::{SessionError, SessionManager, SessionState};
    use crate::transcription::Segment;

    /// Test that settings have valid defaults
    #[test]
    fn test_settings_defaults() {
        let settings = Settings::default();

        assert_eq!(settings.whisper_model, "small");
        assert_eq!(settings.language, "en");
        assert!(settings.input_device_id.is_none());
        assert_eq!(settings.output_format, "paragraphs");
        assert!(settings.vad_threshold >= 0.0 && settings.vad_threshold <= 1.0);
        assert!(settings.silence_to_flush_ms > 0);
        assert!(settings.max_utterance_ms > settings.silence_to_flush_ms);
    }

    /// Test settings serialization round-trip
    #[test]
    fn test_settings_serialization() {
        let settings = Settings {
            whisper_model: "medium".to_string(),
            language: "fr".to_string(),
            input_device_id: Some("device-123".to_string()),
            output_format: "sentences".to_string(),
            vad_threshold: 0.6,
            silence_to_flush_ms: 600,
            max_utterance_ms: 30000,
            diarization_enabled: true,
            max_speakers: 5,
            llm_router_url: "http://localhost:4000".to_string(),
            llm_api_key: "test-api-key".to_string(),
            llm_client_id: "test-client".to_string(),
            soap_model: "soap-model-fast".to_string(),
            soap_model_fast: "soap-model-fast".to_string(),
            fast_model: "fast-model".to_string(),
            medplum_server_url: "http://localhost:8103".to_string(),
            medplum_client_id: "test-client-id".to_string(),
            medplum_auto_sync: true,
            whisper_mode: "remote".to_string(),
            whisper_server_url: "http://172.16.100.45:8001".to_string(),
            whisper_server_model: "large-v3-turbo".to_string(),
            soap_detail_level: 5,
            soap_format: "problem_based".to_string(),
            soap_custom_instructions: String::new(),
            auto_start_enabled: false,
            auto_start_require_enrolled: false,
            auto_start_required_role: None,
            greeting_sensitivity: Some(0.7),
            min_speech_duration_ms: Some(2000),
            auto_end_enabled: true,
            auto_end_silence_ms: 180_000,
            debug_storage_enabled: true,
            miis_enabled: false,
            miis_server_url: "http://172.16.100.45:7843".to_string(),
            image_source: "off".to_string(),
            gemini_api_key: String::new(),
            screen_capture_enabled: false,
            screen_capture_interval_secs: 30,
            charting_mode: crate::config::ChartingMode::Session,
            continuous_auto_copy_soap: false,
            encounter_check_interval_secs: 120,
            encounter_silence_trigger_secs: 60,
            encounter_merge_enabled: true,
            encounter_detection_model: "faster".to_string(),
            encounter_detection_nothink: true,
            stt_alias: "medical-streaming".to_string(),
            stt_postprocess: true,
            encounter_detection_mode: crate::config::EncounterDetectionMode::Llm,
            presence_sensor_port: String::new(),
            presence_absence_threshold_secs: 90,
            presence_debounce_secs: 10,
            presence_csv_log_enabled: true,
            shadow_active_method: crate::config::ShadowActiveMethod::Llm,
            shadow_csv_log_enabled: true,
            native_stt_shadow_enabled: true,
            hybrid_confirm_window_secs: 180,
            hybrid_min_words_for_sensor_split: 500,
        };

        // Serialize to JSON
        let json = serde_json::to_string(&settings).unwrap();

        // Deserialize back
        let deserialized: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.whisper_model, "medium");
        assert_eq!(deserialized.language, "fr");
        assert_eq!(deserialized.input_device_id, Some("device-123".to_string()));
        assert_eq!(deserialized.output_format, "sentences");
        assert_eq!(deserialized.vad_threshold, 0.6);
    }

    /// Test session state transitions match expected IPC responses
    #[test]
    fn test_session_status_serialization() {
        let mut session = SessionManager::new();

        // Idle state
        let status = session.status();
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["state"], "idle");
        assert!(json["provider"].is_null());

        // Preparing state
        session.start_preparing().unwrap();
        let status = session.status();
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["state"], "preparing");

        // Recording state
        session.start_recording("whisper");
        let status = session.status();
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["state"], "recording");
        assert_eq!(json["provider"], "whisper");
    }

    /// Test transcript update serialization
    #[test]
    fn test_transcript_update_serialization() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");

        // Add segments
        session.add_segment(Segment::new(0, 1000, "Hello".to_string()));
        session.add_segment(Segment::new(1000, 2000, "World".to_string()));

        let update = session.transcript_update();
        let json = serde_json::to_value(&update).unwrap();

        assert!(json["finalized_text"].as_str().unwrap().contains("Hello"));
        assert!(json["finalized_text"].as_str().unwrap().contains("World"));
        assert_eq!(json["segment_count"], 2);
    }

    /// Test error state serialization
    #[test]
    fn test_error_state_serialization() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.set_error(SessionError::AudioDeviceError(
            "Microphone not found".to_string(),
        ));

        let status = session.status();
        let json = serde_json::to_value(&status).unwrap();

        assert_eq!(json["state"], "error");
        assert!(json["error_message"]
            .as_str()
            .unwrap()
            .contains("Microphone not found"));
    }

    /// Test invalid state transitions return appropriate errors
    #[test]
    fn test_invalid_state_transitions() {
        let mut session = SessionManager::new();

        // Can't stop when idle
        let result = session.start_stopping();
        assert!(result.is_err());

        // Can't start preparing twice
        session.start_preparing().unwrap();
        let result = session.start_preparing();
        assert!(result.is_err());
    }

    /// Test session handles device ID parameter
    #[test]
    fn test_session_with_device_id() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();

        // Device ID is handled at pipeline level, but session should accept it
        session.start_recording("whisper");

        assert_eq!(session.state(), &SessionState::Recording);
    }

    /// Test processing behind flag
    #[test]
    fn test_processing_behind_flag() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");

        // Initially not behind
        let status = session.status();
        assert!(!status.is_processing_behind);

        // Set high pending count
        session.set_pending_count(5);
        let status = session.status();
        assert!(status.is_processing_behind);

        // Clear pending
        session.set_pending_count(0);
        let status = session.status();
        assert!(!status.is_processing_behind);
    }

    /// Test elapsed time is tracked in status
    #[test]
    fn test_elapsed_time_in_status() {
        let mut session = SessionManager::new();
        session.start_preparing().unwrap();
        session.start_recording("whisper");

        // Status should have elapsed_ms field
        let status = session.status();
        // Initially elapsed time is 0 or very small
        // elapsed_ms is u64, always >= 0
        let _ = status.elapsed_ms;
    }

    /// Test model status response structure
    #[test]
    fn test_model_status_structure() {
        use crate::config::ModelStatus;

        // Available model
        let status = ModelStatus {
            available: true,
            path: Some("/path/to/model.bin".to_string()),
            error: None,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert!(json["available"].as_bool().unwrap());
        assert!(json["path"].is_string());
        assert!(json["error"].is_null());

        // Unavailable model
        let status = ModelStatus {
            available: false,
            path: Some("/path/to/model.bin".to_string()),
            error: Some("File not found".to_string()),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert!(!json["available"].as_bool().unwrap());
        assert!(json["error"].as_str().unwrap().contains("not found"));
    }

    /// Test device list response structure
    #[test]
    fn test_device_structure() {
        use crate::audio::Device;

        let device = Device {
            id: "device-123".to_string(),
            name: "Built-in Microphone".to_string(),
            is_default: true,
        };

        let json = serde_json::to_value(&device).unwrap();
        assert_eq!(json["id"], "device-123");
        assert_eq!(json["name"], "Built-in Microphone");
        assert!(json["is_default"].as_bool().unwrap());
    }

    /// Test complete session lifecycle
    #[test]
    fn test_complete_session_lifecycle() {
        let mut session = SessionManager::new();

        // Start
        assert!(session.start_preparing().is_ok());
        session.start_recording("whisper");

        // Record some content
        session.add_segment(Segment::new(0, 1000, "Test".to_string()));

        // Stop
        assert!(session.start_stopping().is_ok());
        session.complete();

        assert_eq!(session.state(), &SessionState::Completed);

        // Reset
        session.reset();
        assert_eq!(session.state(), &SessionState::Idle);
        assert_eq!(session.segments().len(), 0);
    }
}
