use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A transcribed segment of speech
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: Uuid,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,

    // Future-proofing (optional for POC)
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

    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }
}

/// Provider type for transcription
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    Whisper,
    Apple, // Stub for future
}

impl Default for ProviderType {
    fn default() -> Self {
        Self::Whisper
    }
}

/// A complete transcription session record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: Uuid,
    pub provider: ProviderType,
    pub language: String,
    pub input_device_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub segments: Vec<Segment>,

    // Stats
    pub total_duration_ms: u64,
    pub speech_duration_ms: u64,
    pub realtime_factor: Option<f32>,
}

impl SessionRecord {
    pub fn new(provider: ProviderType, language: String, input_device_id: String) -> Self {
        Self {
            session_id: Uuid::new_v4(),
            provider,
            language,
            input_device_id,
            started_at: Utc::now(),
            ended_at: None,
            segments: Vec::new(),
            total_duration_ms: 0,
            speech_duration_ms: 0,
            realtime_factor: None,
        }
    }

    pub fn add_segment(&mut self, segment: Segment) {
        self.speech_duration_ms += segment.duration_ms();
        self.segments.push(segment);
    }

    pub fn finalize(&mut self) {
        self.ended_at = Some(Utc::now());
        if let Some(last_segment) = self.segments.last() {
            self.total_duration_ms = last_segment.end_ms;
        }
    }

    /// Get the full transcript text with paragraph breaks
    pub fn transcript_paragraphs(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Get the full transcript as a single paragraph
    pub fn transcript_single(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_duration() {
        let seg = Segment::new(1000, 5500, "test".to_string());
        assert_eq!(seg.duration_ms(), 4500);
    }

    #[test]
    fn test_session_transcript_paragraphs() {
        let mut session = SessionRecord::new(
            ProviderType::Whisper,
            "en".to_string(),
            "default".to_string(),
        );
        session.add_segment(Segment::new(0, 1000, "Hello world.".to_string()));
        session.add_segment(Segment::new(2000, 3000, "How are you?".to_string()));

        assert_eq!(
            session.transcript_paragraphs(),
            "Hello world.\n\nHow are you?"
        );
    }

    #[test]
    fn test_session_transcript_single() {
        let mut session = SessionRecord::new(
            ProviderType::Whisper,
            "en".to_string(),
            "default".to_string(),
        );
        session.add_segment(Segment::new(0, 1000, "Hello world.".to_string()));
        session.add_segment(Segment::new(2000, 3000, "How are you?".to_string()));

        assert_eq!(session.transcript_single(), "Hello world. How are you?");
    }
}
