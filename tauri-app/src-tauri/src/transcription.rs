use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::biomarkers::VocalBiomarkers;

/// A transcribed segment of speech
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub id: Uuid,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub speaker_id: Option<String>,
    /// Speaker identification confidence (0.0-1.0)
    pub speaker_confidence: Option<f32>,
    /// Vocal biomarkers (vitality, stability)
    pub vocal_biomarkers: Option<VocalBiomarkers>,
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
            speaker_confidence: None,
            vocal_biomarkers: None,
            avg_log_prob: None,
            no_speech_prob: None,
        }
    }

    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }
}

/// Utterance ready for transcription
#[derive(Debug, Clone)]
pub struct Utterance {
    pub id: Uuid,
    pub audio: Vec<f32>,
    pub start_ms: u64,
    pub end_ms: u64,
}

impl Utterance {
    pub fn new(audio: Vec<f32>, start_ms: u64, end_ms: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            audio,
            start_ms,
            end_ms,
        }
    }

    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Property-based tests
    proptest! {
        #[test]
        fn prop_segment_duration_never_negative(start in 0u64..u64::MAX/2, end in 0u64..u64::MAX/2) {
            let segment = Segment::new(start, end, "test".to_string());
            // duration_ms uses saturating_sub, so should never be negative
            let duration = segment.duration_ms();
            prop_assert!(duration <= end.max(start));
        }

        #[test]
        fn prop_segment_duration_correct_when_end_greater(start in 0u64..1_000_000, delta in 0u64..1_000_000) {
            let end = start + delta;
            let segment = Segment::new(start, end, "test".to_string());
            prop_assert_eq!(segment.duration_ms(), delta);
        }

        #[test]
        fn prop_utterance_duration_never_negative(start in 0u64..u64::MAX/2, end in 0u64..u64::MAX/2) {
            let utt = Utterance::new(vec![], start, end);
            let duration = utt.duration_ms();
            prop_assert!(duration <= end.max(start));
        }

        #[test]
        fn prop_segment_preserves_text(text in ".*") {
            let segment = Segment::new(0, 100, text.clone());
            prop_assert_eq!(segment.text, text);
        }

        #[test]
        fn prop_segment_preserves_timestamps(start in 0u64..1_000_000, end in 0u64..1_000_000) {
            let segment = Segment::new(start, end, "test".to_string());
            prop_assert_eq!(segment.start_ms, start);
            prop_assert_eq!(segment.end_ms, end);
        }

        #[test]
        fn prop_utterance_preserves_audio(audio in proptest::collection::vec(-1.0f32..1.0, 0..10000)) {
            let utt = Utterance::new(audio.clone(), 0, 100);
            prop_assert_eq!(utt.audio.len(), audio.len());
            for (a, b) in utt.audio.iter().zip(audio.iter()) {
                prop_assert!((a - b).abs() < f32::EPSILON);
            }
        }

        #[test]
        fn prop_each_segment_has_unique_uuid(count in 1usize..100) {
            let segments: Vec<Segment> = (0..count)
                .map(|i| Segment::new(i as u64, i as u64 + 100, format!("seg {}", i)))
                .collect();

            // All UUIDs should be unique
            let mut ids: Vec<_> = segments.iter().map(|s| s.id).collect();
            ids.sort();
            ids.dedup();
            prop_assert_eq!(ids.len(), segments.len());
        }
    }

    #[test]
    fn test_segment_creation() {
        let segment = Segment::new(0, 1000, "test".to_string());
        assert_eq!(segment.start_ms, 0);
        assert_eq!(segment.end_ms, 1000);
        assert_eq!(segment.text, "test");
    }

    #[test]
    fn test_utterance_duration() {
        let utt = Utterance::new(vec![0.0; 16000], 1000, 2000);
        assert_eq!(utt.duration_ms(), 1000);
    }

    #[test]
    fn test_segment_duration() {
        let segment = Segment::new(500, 2500, "hello".to_string());
        assert_eq!(segment.duration_ms(), 2000);
    }

    #[test]
    fn test_segment_duration_zero() {
        let segment = Segment::new(1000, 1000, "".to_string());
        assert_eq!(segment.duration_ms(), 0);
    }

    #[test]
    fn test_segment_duration_saturating() {
        // If end < start (shouldn't happen but let's be safe)
        let segment = Segment::new(2000, 1000, "".to_string());
        assert_eq!(segment.duration_ms(), 0);
    }

    #[test]
    fn test_segment_uuid_unique() {
        let s1 = Segment::new(0, 100, "one".to_string());
        let s2 = Segment::new(0, 100, "one".to_string());
        assert_ne!(s1.id, s2.id);
    }

    #[test]
    fn test_segment_optional_fields() {
        let segment = Segment::new(0, 100, "test".to_string());
        assert!(segment.speaker_id.is_none());
        assert!(segment.avg_log_prob.is_none());
        assert!(segment.no_speech_prob.is_none());
    }

    #[test]
    fn test_utterance_creation() {
        let audio = vec![0.1, 0.2, 0.3];
        let utt = Utterance::new(audio.clone(), 0, 100);
        assert_eq!(utt.audio.len(), 3);
        assert_eq!(utt.start_ms, 0);
        assert_eq!(utt.end_ms, 100);
    }

    #[test]
    fn test_utterance_duration_saturating() {
        // Edge case: end before start
        let utt = Utterance::new(vec![], 1000, 500);
        assert_eq!(utt.duration_ms(), 0);
    }

    #[test]
    fn test_utterance_empty_audio() {
        let utt = Utterance::new(vec![], 0, 0);
        assert!(utt.audio.is_empty());
        assert_eq!(utt.duration_ms(), 0);
    }

    #[test]
    fn test_utterance_large_audio() {
        // 1 second of 16kHz audio
        let audio = vec![0.0f32; 16000];
        let utt = Utterance::new(audio, 0, 1000);
        assert_eq!(utt.audio.len(), 16000);
        assert_eq!(utt.duration_ms(), 1000);
    }

    #[test]
    fn test_segment_empty_text() {
        let segment = Segment::new(0, 100, "".to_string());
        assert!(segment.text.is_empty());
    }

    #[test]
    fn test_segment_with_unicode() {
        let segment = Segment::new(0, 100, "Hello, world!".to_string());
        assert_eq!(segment.text, "Hello, world!");
    }

    #[test]
    fn test_segment_long_text() {
        let long_text = "word ".repeat(1000);
        let segment = Segment::new(0, 60000, long_text.clone());
        assert_eq!(segment.text.len(), long_text.len());
    }

    #[test]
    fn test_segment_clone() {
        let s1 = Segment::new(0, 100, "test".to_string());
        let s2 = s1.clone();
        assert_eq!(s1.id, s2.id);
        assert_eq!(s1.text, s2.text);
        assert_eq!(s1.start_ms, s2.start_ms);
    }

    #[test]
    fn test_utterance_clone() {
        let u1 = Utterance::new(vec![0.1, 0.2], 0, 100);
        let u2 = u1.clone();
        assert_eq!(u1.audio, u2.audio);
        assert_eq!(u1.start_ms, u2.start_ms);
    }
}
