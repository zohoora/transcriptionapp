use anyhow::{Context, Result};
use std::path::Path;
use tracing::{debug, info, warn};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::segment::Segment;

/// Whisper model sizes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhisperModel {
    Tiny,
    Base,
    Small,
    Medium,
    Large,
}

impl WhisperModel {
    pub fn filename(&self) -> &'static str {
        match self {
            Self::Tiny => "ggml-tiny.bin",
            Self::Base => "ggml-base.bin",
            Self::Small => "ggml-small.bin",
            Self::Medium => "ggml-medium.bin",
            Self::Large => "ggml-large.bin",
        }
    }

    /// Expected size range in MB (min, max) for validation
    pub fn size_range_mb(&self) -> (u64, u64) {
        match self {
            Self::Tiny => (30, 100),
            Self::Base => (100, 200),
            Self::Small => (200, 500),
            Self::Medium => (500, 1600),
            Self::Large => (1500, 4000),
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

    #[test]
    fn test_utterance_duration() {
        let utt = Utterance::new(vec![0.0; 16000], 1000, 2000);
        assert_eq!(utt.duration_ms(), 1000);
    }

    #[test]
    fn test_model_filename() {
        assert_eq!(WhisperModel::Small.filename(), "ggml-small.bin");
        assert_eq!(WhisperModel::Tiny.filename(), "ggml-tiny.bin");
    }
}
