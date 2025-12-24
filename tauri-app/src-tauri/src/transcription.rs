use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};
use uuid::Uuid;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

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

    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
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

    pub fn duration_ms(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }
}

/// Whisper transcription provider
pub struct WhisperProvider {
    ctx: WhisperContext,
    language: String,
    n_threads: i32,
}

impl WhisperProvider {
    /// Create a new WhisperProvider from a model file path
    pub fn new(model_path: &Path, language: &str, n_threads: i32) -> Result<Self> {
        // Validate model file exists and has reasonable size
        Self::validate_model(model_path)?;

        info!("Loading Whisper model from {:?}", model_path);
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().context("Invalid model path")?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to load Whisper model: {}", e))?;

        info!("Whisper model loaded successfully");

        Ok(Self {
            ctx,
            language: language.to_string(),
            n_threads,
        })
    }

    /// Validate model file
    fn validate_model(path: &Path) -> Result<()> {
        if !path.exists() {
            anyhow::bail!("Model file not found: {:?}", path);
        }

        let metadata = std::fs::metadata(path)?;
        let size_mb = metadata.len() / (1024 * 1024);

        // Loose sanity checks
        if size_mb < 30 {
            anyhow::bail!(
                "Model file too small ({}MB). Expected at least 30MB for a valid Whisper model.",
                size_mb
            );
        }
        if size_mb > 4000 {
            anyhow::bail!(
                "Model file too large ({}MB). Expected at most 4000MB for a Whisper model.",
                size_mb
            );
        }

        // Warn if unusual size
        if size_mb < 50 || size_mb > 3000 {
            warn!("Model size {}MB is unusual for a Whisper model", size_mb);
        }

        debug!("Model file validated: {}MB", size_mb);
        Ok(())
    }

    /// Transcribe an utterance with optional context
    pub fn transcribe(&self, utterance: &Utterance, context: Option<&str>) -> Result<Segment> {
        let start_time = std::time::Instant::now();

        debug!(
            "Transcribing utterance: {}ms - {}ms ({} samples)",
            utterance.start_ms,
            utterance.end_ms,
            utterance.audio.len()
        );

        // Create params
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.n_threads);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Set language
        if self.language != "auto" {
            params.set_language(Some(&self.language));
        }

        // Set context prompt if available
        if let Some(ctx) = context {
            // Use last ~50 words as context
            let context_words: Vec<&str> = ctx.split_whitespace().rev().take(50).collect();
            let context_prompt: String = context_words.into_iter().rev().collect::<Vec<_>>().join(" ");
            if !context_prompt.is_empty() {
                params.set_initial_prompt(&context_prompt);
            }
        }

        // Create state and run inference
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow::anyhow!("Failed to create Whisper state: {}", e))?;

        state
            .full(params, &utterance.audio)
            .map_err(|e| anyhow::anyhow!("Whisper inference failed: {}", e))?;

        // Extract text from all segments
        let num_segments = state.full_n_segments().map_err(|e| {
            anyhow::anyhow!("Failed to get segment count: {}", e)
        })?;

        let mut text_parts = Vec::new();
        for i in 0..num_segments {
            if let Ok(segment_text) = state.full_get_segment_text(i) {
                let trimmed = segment_text.trim();
                if !trimmed.is_empty() {
                    text_parts.push(trimmed.to_string());
                }
            }
        }

        let text = text_parts.join(" ").trim().to_string();
        let elapsed = start_time.elapsed();
        let rtf = elapsed.as_secs_f32() / (utterance.audio.len() as f32 / 16000.0);

        debug!(
            "Transcription complete in {:?} (RTF: {:.2}): \"{}\"",
            elapsed, rtf, text
        );

        Ok(Segment::new(utterance.start_ms, utterance.end_ms, text))
    }

    /// Get the language setting
    pub fn language(&self) -> &str {
        &self.language
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
