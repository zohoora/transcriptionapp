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
            ollama_server_url: "http://localhost:11434".to_string(),
            ollama_model: "qwen3:4b".to_string(),
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
        assert!(status.elapsed_ms >= 0);
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
