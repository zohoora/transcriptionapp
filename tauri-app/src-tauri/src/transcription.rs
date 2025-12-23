use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A transcribed segment of speech
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: Uuid,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub speaker_id: Option<String>,
    pub avg_log_prob: Option<f32>,
    pub no_speech_prob: Option<f32>,
}

impl Segment {
    pub fn new(start_ms: u64, end_ms: u64, text: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            start_ms,
            end_ms,
            text,
            speaker_id: None,
            avg_log_prob: None,
            no_speech_prob: None,
        }
    }
}

/// Utterance ready for transcription
#[derive(Debug, Clone)]
pub struct Utterance {
    pub audio: Vec<f32>,
    pub start_ms: u64,
    pub end_ms: u64,
}

impl Utterance {
    pub fn new(audio: Vec<f32>, start_ms: u64, end_ms: u64) -> Self {
        Self {
            audio,
            start_ms,
            end_ms,
        }
    }
}

// TODO: Implement WhisperProvider for actual transcription
// This is placeholder for M2 integration

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_creation() {
        let segment = Segment::new(0, 1000, "test".to_string());
        assert_eq!(segment.start_ms, 0);
        assert_eq!(segment.end_ms, 1000);
        assert_eq!(segment.text, "test");
    }
}
