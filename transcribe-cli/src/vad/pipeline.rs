use std::collections::VecDeque;
use tracing::{debug, trace};
use voice_activity_detector::VoiceActivityDetector;

use crate::transcription::Utterance;

/// VAD configuration parameters
#[derive(Debug, Clone)]
pub struct VadConfig {
    /// Speech probability threshold (0.0 - 1.0)
    pub vad_threshold: f32,
    /// Pre-roll samples (audio before speech detection)
    pub pre_roll_samples: usize,
    /// Minimum speech duration in samples (ignore shorter sounds)
    pub min_speech_samples: usize,
    /// Silence duration to flush utterance in samples
    pub silence_to_flush_samples: usize,
    /// Maximum utterance length in samples (Whisper's 30s limit safety)
    pub max_utterance_samples: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            vad_threshold: 0.5,
            pre_roll_samples: 4800,       // 300ms at 16kHz
            min_speech_samples: 4000,     // 250ms at 16kHz
            silence_to_flush_samples: 8000, // 500ms at 16kHz
            max_utterance_samples: 400000,  // 25s at 16kHz
        }
    }
}

impl VadConfig {
    /// Create config from millisecond values
    pub fn from_ms(
        vad_threshold: f32,
        pre_roll_ms: u32,
        min_speech_ms: u32,
        silence_to_flush_ms: u32,
        max_utterance_ms: u32,
    ) -> Self {
        const SAMPLES_PER_MS: usize = 16; // 16kHz
        Self {
            vad_threshold,
            pre_roll_samples: pre_roll_ms as usize * SAMPLES_PER_MS,
            min_speech_samples: min_speech_ms as usize * SAMPLES_PER_MS,
            silence_to_flush_samples: silence_to_flush_ms as usize * SAMPLES_PER_MS,
            max_utterance_samples: max_utterance_ms as usize * SAMPLES_PER_MS,
        }
    }
}

/// VAD-gated audio pipeline
///
/// Controls when inference runs based on voice activity detection.
/// Critical: VAD controls inference, not audio storage. The audio clock always advances.
pub struct VadGatedPipeline {
    /// Audio clock: count of 16kHz samples processed
    /// IMPORTANT: Represents the END of the most recently processed chunk
    audio_clock_samples: u64,

    /// VAD state
    is_speech_active: bool,
    silence_samples: u64,

    /// Speech accumulator
    speech_buffer: Vec<f32>,
    speech_start_samples: u64,

    /// Pre-roll buffer (contains samples BEFORE current chunk)
    pre_roll_buffer: VecDeque<f32>,

    /// Configuration
    config: VadConfig,

    /// Output queue
    transcription_queue: VecDeque<Utterance>,
}

impl VadGatedPipeline {
    pub fn new() -> Self {
        Self::with_config(VadConfig::default())
    }

    pub fn with_config(config: VadConfig) -> Self {
        Self {
            audio_clock_samples: 0,
            is_speech_active: false,
            silence_samples: 0,
            speech_buffer: Vec::new(),
            speech_start_samples: 0,
            pre_roll_buffer: VecDeque::with_capacity(config.pre_roll_samples),
            config,
            transcription_queue: VecDeque::new(),
        }
    }

    /// Advance the audio clock by the given number of samples
    pub fn advance_audio_clock(&mut self, samples: usize) {
        self.audio_clock_samples += samples as u64;
    }

    /// Get timestamp at START of current chunk
    /// audio_clock is at END, so subtract chunk length
    fn chunk_start_samples(&self, chunk_len: usize) -> u64 {
        self.audio_clock_samples.saturating_sub(chunk_len as u64)
    }

    /// Process a chunk of 16kHz audio through VAD
    ///
    /// Note: Call advance_audio_clock BEFORE calling this method!
    pub fn process_chunk(&mut self, audio: &[f32], vad: &mut VoiceActivityDetector) {
        let chunk_len = audio.len();
        let chunk_start = self.chunk_start_samples(chunk_len);

        // Run VAD prediction
        let speech_prob = vad.predict(audio.iter().copied());
        let is_speech = speech_prob > self.config.vad_threshold;

        trace!(
            "VAD chunk at {}ms: speech={}, buffer_len={}",
            chunk_start / 16,
            is_speech,
            self.speech_buffer.len()
        );

        // CRITICAL: Check max utterance length FIRST
        if self.is_speech_active && self.speech_buffer.len() >= self.config.max_utterance_samples {
            debug!(
                "Max utterance length reached at {}ms, forcing flush",
                self.audio_clock_samples / 16
            );
            self.flush_utterance();

            // Restart immediately (speech still active)
            self.is_speech_active = true;
            // Subtract pre-roll from chunk start (same rule as normal start)
            self.speech_start_samples = chunk_start.saturating_sub(self.pre_roll_buffer.len() as u64);
            self.speech_buffer.extend(self.pre_roll_buffer.iter());
        }

        match (self.is_speech_active, is_speech) {
            // Transition: silence -> speech
            (false, true) => {
                self.is_speech_active = true;
                self.silence_samples = 0;

                // Start time = chunk start minus pre-roll
                self.speech_start_samples = chunk_start.saturating_sub(self.pre_roll_buffer.len() as u64);

                self.speech_buffer.clear();
                self.speech_buffer.extend(self.pre_roll_buffer.iter());
                self.speech_buffer.extend(audio.iter());

                debug!(
                    "Speech started at {}ms (with {}ms pre-roll)",
                    self.speech_start_samples / 16,
                    self.pre_roll_buffer.len() / 16
                );
            }

            // Continuing speech
            (true, true) => {
                self.speech_buffer.extend(audio.iter());
                self.silence_samples = 0;
            }

            // Transition: speech -> silence
            (true, false) => {
                self.speech_buffer.extend(audio.iter());
                self.silence_samples += chunk_len as u64;

                if self.silence_samples >= self.config.silence_to_flush_samples as u64 {
                    debug!(
                        "Silence threshold reached at {}ms, flushing utterance",
                        self.audio_clock_samples / 16
                    );
                    self.flush_utterance();
                }
            }

            // Continuing silence
            (false, false) => {
                // Nothing to accumulate
            }
        }

        // Update pre-roll buffer AFTER processing
        self.pre_roll_buffer.extend(audio.iter().copied());
        while self.pre_roll_buffer.len() > self.config.pre_roll_samples {
            self.pre_roll_buffer.pop_front();
        }
    }

    /// Flush the current utterance to the transcription queue
    fn flush_utterance(&mut self) {
        if self.speech_buffer.len() < self.config.min_speech_samples {
            debug!(
                "Ignoring short utterance: {} samples (min: {})",
                self.speech_buffer.len(),
                self.config.min_speech_samples
            );
            self.speech_buffer.clear();
            self.is_speech_active = false;
            self.silence_samples = 0;
            return;
        }

        let start_ms = self.speech_start_samples / 16;
        let end_ms = start_ms + (self.speech_buffer.len() as u64 / 16);

        debug!(
            "Flushing utterance: {}ms - {}ms ({} samples)",
            start_ms,
            end_ms,
            self.speech_buffer.len()
        );

        let utterance = Utterance::new(
            std::mem::take(&mut self.speech_buffer),
            start_ms,
            end_ms,
        );

        self.transcription_queue.push_back(utterance);
        self.is_speech_active = false;
        self.silence_samples = 0;
    }

    /// Force flush any pending speech (called on session stop)
    pub fn force_flush(&mut self) {
        if self.is_speech_active && !self.speech_buffer.is_empty() {
            debug!("Force flushing remaining speech buffer");
            // Temporarily lower min speech threshold for final flush
            let original_min = self.config.min_speech_samples;
            self.config.min_speech_samples = 0;
            self.flush_utterance();
            self.config.min_speech_samples = original_min;
        }
    }

    /// Get the next utterance ready for transcription
    pub fn pop_utterance(&mut self) -> Option<Utterance> {
        self.transcription_queue.pop_front()
    }

    /// Check if there are utterances ready for transcription
    pub fn has_pending_utterances(&self) -> bool {
        !self.transcription_queue.is_empty()
    }

    /// Get the number of pending utterances
    pub fn pending_count(&self) -> usize {
        self.transcription_queue.len()
    }

    /// Get current audio clock in milliseconds
    pub fn audio_clock_ms(&self) -> u64 {
        self.audio_clock_samples / 16
    }

    /// Check if currently detecting speech
    pub fn is_speech_active(&self) -> bool {
        self.is_speech_active
    }
}

impl Default for VadGatedPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_vad() -> VoiceActivityDetector {
        VoiceActivityDetector::builder()
            .sample_rate(16000)
            .chunk_size(512usize)
            .build()
            .expect("Failed to create VAD")
    }

    #[test]
    fn test_vad_config_from_ms() {
        let config = VadConfig::from_ms(0.5, 300, 250, 500, 25000);
        assert_eq!(config.pre_roll_samples, 4800);
        assert_eq!(config.min_speech_samples, 4000);
        assert_eq!(config.silence_to_flush_samples, 8000);
        assert_eq!(config.max_utterance_samples, 400000);
    }

    #[test]
    fn test_pipeline_initial_state() {
        let pipeline = VadGatedPipeline::new();
        assert_eq!(pipeline.audio_clock_ms(), 0);
        assert!(!pipeline.is_speech_active());
        assert!(!pipeline.has_pending_utterances());
    }

    #[test]
    fn test_pipeline_audio_clock_advances() {
        let mut pipeline = VadGatedPipeline::new();
        pipeline.advance_audio_clock(16000); // 1 second
        assert_eq!(pipeline.audio_clock_ms(), 1000);
    }

    #[test]
    fn test_force_flush_empty() {
        let mut pipeline = VadGatedPipeline::new();
        pipeline.force_flush();
        assert!(!pipeline.has_pending_utterances());
    }
}
